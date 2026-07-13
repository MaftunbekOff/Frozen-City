use super::*;

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::accounts;
use crate::protocol::{ClientMsg, ServerMsg};

/// Sent back as `ServerMsg::AuthFailed` when a `Login` first message doesn't
/// match an account — deliberately generic (never "unknown login" vs "wrong
/// password") so it can't be used to enumerate registered logins.
const AUTH_FAILED_REASON: &str = "Noto'g'ri login yoki parol.";

/// Sent back as `ServerMsg::AuthFailed` when `Login` is otherwise valid but
/// `WorldManager` is already at `MAX_ACCOUNT_WORLDS` and this account has no
/// world running yet — a distinct reason from `AUTH_FAILED_REASON` is safe
/// here (unlike login/password, capacity isn't enumerable).
const SERVER_FULL_REASON: &str = "Server hozircha to'la, birozdan so'ng qayta urinib ko'ring.";

/// Sent back as `ServerMsg::AuthFailed` for an `EnterCentral` from an account
/// that hasn't finished the Tunnel in its personal world (and has no settlers
/// already living in the central world from an earlier graduation).
const NOT_GRADUATED_REASON: &str =
    "Global Olamga o'tish uchun avval shaxsiy olamda Tunnelni qurib bitiring.";

/// Sent back as `ServerMsg::AuthFailed` for a `Login`/`EnterCentral` on a
/// process that has accounts disabled (`FC_DISABLE_ACCOUNTS`, set on the
/// extra region servers): each process has its own `WorldManager`, so letting
/// accounts in there would silently fork one account's "single" personal
/// world into per-region copies — accounts live on the main region only.
const ACCOUNTS_DISABLED_REASON: &str =
    "Akkaunt bilan kirish faqat asosiy regionda ishlaydi. Asosiy regionni tanlang.";

/// Sent back as `ServerMsg::AuthFailed` for a `VisitFriend` with no standing
/// invite (or an expired one).
const NO_INVITE_REASON: &str =
    "Bu olamga taklifingiz yo'q yoki muddati o'tgan. Global Olamda yangi taklif so'rang.";

/// Sent back as `ServerMsg::AuthFailed` for a `VisitFriend` while the host
/// isn't online in their own world (the default owner-present-only policy).
const HOST_OFFLINE_REASON: &str = "Do'stingiz hozir o'z olamida emas — u kirganda qayta urining.";

/// Sent back as `ServerMsg::AuthFailed` when in-client registration is
/// throttled — a flood guard, not a player-behavior signal.
const REGISTER_THROTTLED_REASON: &str =
    "Hozir ro'yxatdan o'tish band, bir daqiqadan so'ng qayta urining.";

/// Hard cap on concurrent accepted connections (native, WebSocket and plain
/// HTTP all share this accept path), so a connection flood can't spawn
/// unbounded OS threads and exhaust memory/handles.
// Per-process: when several regions run side by side on one box, each gets
// its own independent 128-connection budget, not a shared one.
const MAX_CONNECTIONS: usize = 128;

pub(crate) fn accept_loop(
    listener: TcpListener,
    to_server: Sender<ToServer>,
    shutdown: Arc<AtomicBool>,
    world_manager: Option<Arc<crate::world_manager::WorldManager>>,
) {
    let active = Arc::new(AtomicUsize::new(0));
    loop {
        if shutdown.load(Ordering::SeqCst) {
            return;
        }
        match listener.accept() {
            Ok((stream, _peer)) => {
                if active.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
                    // Over the cap: close the socket without spawning a
                    // handler, rather than let threads grow unbounded.
                    drop(stream);
                    continue;
                }
                active.fetch_add(1, Ordering::Relaxed);
                let tx = to_server.clone();
                let wm = world_manager.clone();
                let counted = active.clone();
                let spawned = thread::Builder::new()
                    .name("fc-conn".into())
                    .spawn(move || {
                        let _guard = ConnGuard(counted);
                        handle_socket(stream, tx, wm)
                    });
                if spawned.is_err() {
                    // Thread failed to spawn, so no guard exists to release
                    // the slot we already reserved.
                    active.fetch_sub(1, Ordering::Relaxed);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(150));
            }
            Err(_) => return,
        }
    }
}

fn handle_socket(
    stream: TcpStream,
    to_server: Sender<ToServer>,
    world_manager: Option<Arc<crate::world_manager::WorldManager>>,
) {
    // Sockets accepted from a non-blocking listener inherit non-blocking mode
    // on Windows; everything below expects a blocking socket.
    if stream.set_nonblocking(false).is_err() {
        return;
    }
    let _ = stream.set_nodelay(true);
    // Give the client 10 s to reveal which protocol it speaks.
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let Some(probe) = peek4(&stream) else { return };
    if &probe == b"GET " {
        handle_http(stream, to_server, world_manager);
    } else {
        handle_native(stream, to_server, world_manager);
    }
}

/// Peek the first 4 bytes without consuming them.
fn peek4(stream: &TcpStream) -> Option<[u8; 4]> {
    let mut buf = [0u8; 4];
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match stream.peek(&mut buf) {
            Ok(n) if n >= 4 => return Some(buf),
            Ok(0) => return None,
            Ok(_) => {
                if Instant::now() > deadline {
                    return None;
                }
                thread::sleep(Duration::from_millis(5));
            }
            Err(_) => return None,
        }
    }
}

/// Where a connection's very first message routed it: into a world (guest
/// shared world, an account's personal world, or the central world), refused
/// with a reason the client should see, or a protocol violation to drop
/// silently. Shared by the native and WebSocket paths, which only differ in
/// how they write the `AuthFailed` back.
pub(crate) enum FirstMsgOutcome {
    Joined(Sender<ToServer>, Option<(u64, Receiver<ServerMsg>)>),
    Refused(&'static str),
    Drop,
}

pub(crate) fn route_first_msg(
    msg: ClientMsg,
    to_server: &Sender<ToServer>,
    world_manager: &Option<Arc<crate::world_manager::WorldManager>>,
) -> FirstMsgOutcome {
    use crate::world_manager::{CentralError, VisitError};
    match msg {
        ClientMsg::Hello { name, token } => {
            let name = sanitize_name(&name);
            FirstMsgOutcome::Joined(to_server.clone(), join(to_server, name, token, None))
        }
        ClientMsg::Register {
            login,
            password,
            name,
        } => {
            if accounts_disabled() {
                return FirstMsgOutcome::Refused(ACCOUNTS_DISABLED_REASON);
            }
            if register_throttled() {
                return FirstMsgOutcome::Refused(REGISTER_THROTTLED_REASON);
            }
            match accounts::register_account(&login, &password, &name) {
                Ok((account_id, display_name)) => {
                    // A successful registration signs straight in, exactly
                    // like a `Login` would have.
                    let name = sanitize_name(&display_name);
                    match world_manager {
                        Some(wm) => match wm.join_account(account_id, name, None) {
                            Some((target, id, out_rx)) => {
                                FirstMsgOutcome::Joined(target, Some((id, out_rx)))
                            }
                            None => FirstMsgOutcome::Refused(SERVER_FULL_REASON),
                        },
                        None => FirstMsgOutcome::Joined(
                            to_server.clone(),
                            join(to_server, name, None, Some(account_id)),
                        ),
                    }
                }
                Err(accounts::RegisterError::Taken) => {
                    FirstMsgOutcome::Refused("Bu login yoki ism allaqachon band.")
                }
                Err(accounts::RegisterError::Invalid(why)) => FirstMsgOutcome::Refused(why),
                Err(accounts::RegisterError::Io) => {
                    FirstMsgOutcome::Refused("Server ro'yxatdan o'tkaza olmadi — keyinroq urining.")
                }
            }
        }
        ClientMsg::VisitFriend {
            login,
            password,
            host,
            token,
        } => {
            if accounts_disabled() {
                return FirstMsgOutcome::Refused(ACCOUNTS_DISABLED_REASON);
            }
            match accounts::authenticate(&login, &password) {
                Some((account_id, display_name)) => {
                    let name = sanitize_name(&display_name);
                    match world_manager {
                        Some(wm) => match wm.visit_friend(host, account_id, name, token) {
                            Ok((target, id, out_rx)) => {
                                FirstMsgOutcome::Joined(target, Some((id, out_rx)))
                            }
                            Err(VisitError::NoInvite) => FirstMsgOutcome::Refused(NO_INVITE_REASON),
                            Err(VisitError::HostOffline) => {
                                FirstMsgOutcome::Refused(HOST_OFFLINE_REASON)
                            }
                            Err(VisitError::Capacity) => {
                                FirstMsgOutcome::Refused(SERVER_FULL_REASON)
                            }
                        },
                        None => FirstMsgOutcome::Refused(ACCOUNTS_DISABLED_REASON),
                    }
                }
                None => FirstMsgOutcome::Refused(AUTH_FAILED_REASON),
            }
        }
        ClientMsg::Login {
            login,
            password,
            token,
        } => {
            if accounts_disabled() {
                return FirstMsgOutcome::Refused(ACCOUNTS_DISABLED_REASON);
            }
            match accounts::authenticate(&login, &password) {
                Some((account_id, display_name)) => {
                    let name = sanitize_name(&display_name);
                    match world_manager {
                        Some(wm) => match wm.join_account(account_id, name, token) {
                            Some((target, id, out_rx)) => {
                                FirstMsgOutcome::Joined(target, Some((id, out_rx)))
                            }
                            None => FirstMsgOutcome::Refused(SERVER_FULL_REASON),
                        },
                        // No world manager (co-op host, tests): authenticate
                        // but land in the shared world, as before.
                        None => FirstMsgOutcome::Joined(
                            to_server.clone(),
                            join(to_server, name, token, Some(account_id)),
                        ),
                    }
                }
                None => FirstMsgOutcome::Refused(AUTH_FAILED_REASON),
            }
        }
        ClientMsg::EnterCentral {
            login,
            password,
            token,
        } => {
            if accounts_disabled() {
                return FirstMsgOutcome::Refused(ACCOUNTS_DISABLED_REASON);
            }
            match accounts::authenticate(&login, &password) {
                Some((account_id, display_name)) => {
                    let name = sanitize_name(&display_name);
                    match world_manager {
                        Some(wm) => match wm.enter_central(account_id, name, token) {
                            Ok((target, id, out_rx)) => {
                                FirstMsgOutcome::Joined(target, Some((id, out_rx)))
                            }
                            Err(CentralError::NotGraduated) => {
                                FirstMsgOutcome::Refused(NOT_GRADUATED_REASON)
                            }
                            Err(CentralError::Capacity) => {
                                FirstMsgOutcome::Refused(SERVER_FULL_REASON)
                            }
                        },
                        // Only the dedicated server has a central world.
                        None => FirstMsgOutcome::Refused(ACCOUNTS_DISABLED_REASON),
                    }
                }
                None => FirstMsgOutcome::Refused(AUTH_FAILED_REASON),
            }
        }
        _ => FirstMsgOutcome::Drop,
    }
}

/// Feeds the already-consumed request head back to tungstenite, then the live
/// socket. Lets the sniffing above coexist with tungstenite's handshake.
pub(crate) struct PrefixedStream {
    pub(crate) prefix: Vec<u8>,
    pub(crate) pos: usize,
    pub(crate) inner: TcpStream,
}

impl Read for PrefixedStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos < self.prefix.len() {
            let n = (self.prefix.len() - self.pos).min(buf.len());
            buf[..n].copy_from_slice(&self.prefix[self.pos..self.pos + n]);
            self.pos += n;
            return Ok(n);
        }
        self.inner.read(buf)
    }
}

impl Write for PrefixedStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
