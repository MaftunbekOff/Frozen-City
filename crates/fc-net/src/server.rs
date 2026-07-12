//! Authoritative game server. Runs the simulation on its own thread at a fixed
//! 5 Hz tick and hands the local (in-process) player a plain channel pair —
//! one unified code path for singleplayer, host and join.
//!
//! A single TCP port speaks three protocols, told apart by the first bytes of
//! each connection: the native length-prefixed frame protocol, browser
//! WebSockets ("GET " + `Upgrade: websocket`), and plain HTTP GET for the
//! static web build (index.html + wasm) so a dedicated server is also the web
//! host.

use std::collections::{HashMap, VecDeque};
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tungstenite::Message;

use fc_game::sim;
use fc_game::types::{
    GamePhase, GameState, Mission, PlayerCommand, PlayerInfo, Ping, Role, Survivor, Tech, TICK_MS,
};
use crate::accounts;
use crate::client::ClientConn;
use crate::persist;
use crate::protocol::{
    decode, encode, read_frame, write_frame, ClientMsg, FriendInfo, Included, ServerMsg,
    ShowcaseEntry, MAX_FRAME, TILES_EVERY_N_TICKS,
};
use crate::world_manager::InviteBook;

/// Directory the built-in HTTP server serves the web build from.
const WEB_ROOT: &str = "web";

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

/// True on processes (extra region servers) where account login and the
/// central world are switched off — see `ACCOUNTS_DISABLED_REASON`.
fn accounts_disabled() -> bool {
    std::env::var("FC_DISABLE_ACCOUNTS").is_ok_and(|v| !v.is_empty() && v != "0")
}

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

/// In-process registration throttle: at most this many new accounts per
/// sliding minute, across all connections. bcrypt hashing is also CPU-heavy,
/// so this doubles as a hash-flood guard.
const MAX_REGISTRATIONS_PER_MINUTE: u32 = 5;

fn register_throttled() -> bool {
    static WINDOW: std::sync::OnceLock<Mutex<(Instant, u32)>> = std::sync::OnceLock::new();
    let window = WINDOW.get_or_init(|| Mutex::new((Instant::now(), 0)));
    let mut w = window.lock().unwrap();
    let now = Instant::now();
    if now.duration_since(w.0) >= Duration::from_secs(60) {
        *w = (now, 0);
    }
    w.1 += 1;
    w.1 > MAX_REGISTRATIONS_PER_MINUTE
}

/// Mint a fresh, unguessable 64-bit token from the OS CSPRNG. Used for
/// session/reconnect tokens: unlike a seeded PRNG stream (which can be
/// inverted from a single observed output), each draw here is independent
/// and can't be predicted from a token an attacker sniffs off the wire.
fn fresh_token() -> u64 {
    let mut b = [0u8; 8];
    getrandom::getrandom(&mut b).expect("OS CSPRNG unavailable");
    u64::from_le_bytes(b)
}

pub struct ServerConfig {
    /// Bind a TCP listener on this port; `None` = local-only (singleplayer).
    pub port: Option<u16>,
    pub seed: u64,
    pub win_days: u32,
    /// Keep running with zero clients (dedicated server mode).
    pub persistent: bool,
    /// Print events and day changes to stdout (dedicated server mode).
    pub verbose: bool,
    /// Where to load/save this world when `persistent`. `None` uses the
    /// single shared-world default (`persist::load`/`save`, i.e.
    /// `FC_WORLD_SAVE`); `Some(path)` is a per-account world spawned by
    /// `WorldManager`, one file per account.
    pub save_path: Option<String>,
    /// When set, a world with zero connected clients saves and exits once
    /// this much time has passed since it last had any — instead of running
    /// forever. `None` for the single shared world (always stays up);
    /// `Some(_)` for per-account worlds, so an abandoned account's thread
    /// doesn't run forever.
    pub idle_shutdown: Option<Duration>,
    /// This world is THE central world (the Global World through the Tunnel):
    /// a fresh one is created with `sim::new_game_central` instead of
    /// `sim::new_game`, and the flag is re-asserted on every load so the
    /// world can never silently degrade into an ordinary survival map.
    pub central: bool,
    /// For per-account personal worlds: the account that OWNS this world.
    /// Only that account's connections get the Owner role; anyone else (an
    /// invited visitor) is a Guest no matter who joins first. `None` for the
    /// shared guest world (first-joiner rule as ever) and the central world.
    pub owner_account: Option<i64>,
    /// Present only on the central world: where an `Invite` records the
    /// standing permission that `WorldManager::visit_friend` later checks.
    pub invites: Option<Arc<InviteBook>>,
    /// Present only on the central world: lets an `Invite` deliver
    /// `ServerMsg::Invited` cross-world, to the target account's PERSONAL
    /// world connection (not just one currently in the central world) —
    /// see `WorldManager::deliver_to_account`.
    pub world_manager: Option<Arc<crate::world_manager::WorldManager>>,
}

/// On a persistent server, a finished world (won or lost) restarts with a
/// fresh map after this long, keeping the connected players.
const WORLD_RESET_AFTER: Duration = Duration::from_secs(45);

/// How often a persistent server writes its world to disk. Bounds how much
/// progress a hard kill (crash, OOM, `RuntimeMaxSec`) can lose; a graceful
/// stop (SIGTERM) saves immediately instead of waiting for this.
const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(20);

pub enum ToServer {
    Join {
        name: String,
        /// Session token from a previous `Welcome`, to resume the same player
        /// identity (reconnect) instead of joining as a fresh player.
        token: Option<u64>,
        /// Account this connection authenticated as (`None` for guests) —
        /// recorded on the joining `PlayerInfo` so the central world can tie
        /// players to the settlers they own.
        account: Option<i64>,
        out: Sender<ServerMsg>,
        id_back: Sender<u64>,
    },
    Msg {
        client: u64,
        msg: ClientMsg,
    },
    Leave {
        client: u64,
    },
    /// Central world only: how many settlers `account` currently owns there.
    /// `world_manager` asks before migrating more, so re-entry tops the group
    /// up to the cap instead of stacking a fresh group per visit.
    CountOwned {
        account: i64,
        reply: Sender<usize>,
    },
    /// Personal world only: remove up to `max` survivors for migration
    /// through the Tunnel. Replies `None` when this world hasn't graduated
    /// (the Tunnel isn't finished — nobody leaves through a hole that isn't
    /// there), `Some(survivors)` otherwise, possibly empty.
    ExtractMigrants {
        max: usize,
        reply: Sender<Option<Vec<Survivor>>>,
    },
    /// Central world only: settle migrated survivors as `account`'s, arriving
    /// with the display name `name` (for the event log).
    InjectMigrants {
        account: i64,
        name: String,
        survivors: Vec<Survivor>,
    },
    /// Personal world only: is the owning account currently connected here?
    /// Gate for `VisitFriend` — a world without its owner admits no visitors.
    OwnerOnline {
        owner: i64,
        reply: Sender<bool>,
    },
    /// Deliver `msg` to every currently-connected client whose session
    /// authenticated as `account`, in WHICHEVER world receives this —
    /// cross-world push (e.g. `ServerMsg::Invited` reaching a target's own
    /// personal world, not just one they happen to be in centrally right
    /// now). A no-op if that account isn't connected here. Boxed: `ServerMsg`
    /// embeds a full `GameState` in some variants, and this is the only
    /// `ToServer` variant that would otherwise blow up every other variant's
    /// size (they're all small control fields/channels).
    DeliverServerMsg {
        account: i64,
        msg: Box<ServerMsg>,
    },
}

#[derive(Clone)]
pub struct ServerHandle {
    pub to_server: Sender<ToServer>,
    pub shutdown: Arc<AtomicBool>,
    pub addr: Option<SocketAddr>,
    /// The sim thread, so a caller can block until it has actually finished
    /// (and, for a persistent server, written its final save) instead of
    /// racing it: `stop()` only flips a flag the sim thread polls, it doesn't
    /// wait for the thread to notice.
    sim_thread: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
}

impl ServerHandle {
    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    /// Blocks until the sim thread has exited. Idempotent — a second call
    /// (or a call on a handle that was never joined) is a no-op.
    pub fn join(&self) {
        if let Some(t) = self.sim_thread.lock().unwrap().take() {
            let _ = t.join();
        }
    }
}

pub fn start(config: ServerConfig) -> io::Result<ServerHandle> {
    start_inner(config, None)
}

/// Like `start`, but a successful `Login` on this listener routes to that
/// account's own persistent world via `world_manager` instead of the shared
/// one — used only by the dedicated production server (`main.rs`). Every
/// other caller (co-op host/join, all tests) keeps using `start`, where
/// `Login` still just authenticates and joins the single shared world,
/// unchanged.
pub fn start_with_accounts(
    config: ServerConfig,
    world_manager: Arc<crate::world_manager::WorldManager>,
) -> io::Result<ServerHandle> {
    start_inner(config, Some(world_manager))
}

fn start_inner(
    config: ServerConfig,
    world_manager: Option<Arc<crate::world_manager::WorldManager>>,
) -> io::Result<ServerHandle> {
    let (to_server_tx, to_server_rx) = channel::<ToServer>();
    let shutdown = Arc::new(AtomicBool::new(false));

    let addr = if let Some(port) = config.port {
        let listener = TcpListener::bind(("0.0.0.0", port))?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let tx = to_server_tx.clone();
        let flag = shutdown.clone();
        let wm = world_manager.clone();
        thread::Builder::new()
            .name("fc-acceptor".into())
            .spawn(move || accept_loop(listener, tx, flag, wm))
            .expect("spawn acceptor");
        Some(addr)
    } else {
        None
    };

    let flag = shutdown.clone();
    let sim_thread = thread::Builder::new()
        .name("fc-sim".into())
        .spawn(move || sim_loop(config, to_server_rx, flag))
        .expect("spawn sim");

    Ok(ServerHandle {
        to_server: to_server_tx,
        shutdown,
        addr,
        sim_thread: Arc::new(Mutex::new(Some(sim_thread))),
    })
}

/// Create an in-process connection to a running server (local player).
pub fn connect_local(handle: &ServerHandle, name: String) -> ClientConn {
    let (out_tx, out_rx) = channel::<ServerMsg>(); // server -> client
    let (in_tx, in_rx) = channel::<ClientMsg>(); // client -> server
    let to_server = handle.to_server.clone();
    thread::Builder::new()
        .name("fc-local-pump".into())
        .spawn(move || {
            let (id_tx, id_rx) = channel::<u64>();
            if to_server
                .send(ToServer::Join {
                    name,
                    // The in-process local player never reconnects.
                    token: None,
                    account: None,
                    out: out_tx,
                    id_back: id_tx,
                })
                .is_err()
            {
                return;
            }
            let Ok(id) = id_rx.recv() else { return };
            for msg in in_rx {
                if to_server.send(ToServer::Msg { client: id, msg }).is_err() {
                    return;
                }
            }
            let _ = to_server.send(ToServer::Leave { client: id });
        })
        .expect("spawn local pump");
    ClientConn::Channels {
        tx: in_tx,
        rx: out_rx,
    }
}

/// Hard cap on concurrent accepted connections (native, WebSocket and plain
/// HTTP all share this accept path), so a connection flood can't spawn
/// unbounded OS threads and exhaust memory/handles.
// Per-process: when several regions run side by side on one box, each gets
// its own independent 128-connection budget, not a shared one.
const MAX_CONNECTIONS: usize = 128;

/// Decrements the shared active-connection counter when a handler thread
/// exits, on every exit path (normal return or panic unwind).
struct ConnGuard(Arc<AtomicUsize>);

impl Drop for ConnGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

fn accept_loop(
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
enum FirstMsgOutcome {
    Joined(Sender<ToServer>, Option<(u64, Receiver<ServerMsg>)>),
    Refused(&'static str),
    Drop,
}

fn route_first_msg(
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

/// The native desktop protocol: length-prefixed bincode frames.
fn handle_native(
    mut stream: TcpStream,
    to_server: Sender<ToServer>,
    world_manager: Option<Arc<crate::world_manager::WorldManager>>,
) {
    // The very first frame must be Hello, Login or EnterCentral (the 10 s
    // timeout is already set). `target` is whichever world this connection
    // ends up in — the shared world for a guest `Hello`, or (when
    // authenticated and a `world_manager` is wired up) that account's own
    // world or the central one — and is reused for every message this
    // connection sends afterward, not just the initial join.
    let (target, joined) = match read_frame::<_, ClientMsg>(&mut stream) {
        Ok(msg) => match route_first_msg(msg, &to_server, &world_manager) {
            FirstMsgOutcome::Joined(target, joined) => (target, joined),
            FirstMsgOutcome::Refused(reason) => {
                let _ = write_frame(
                    &mut stream,
                    &ServerMsg::AuthFailed {
                        reason: reason.to_string(),
                    },
                );
                return;
            }
            FirstMsgOutcome::Drop => return,
        },
        Err(_) => return,
    };
    let _ = stream.set_read_timeout(None);

    let Some((id, out_rx)) = joined else {
        return;
    };

    // Writer thread: serialize server messages onto the socket. When the
    // server drops this client's sender, shut the socket down so the blocking
    // reader below unblocks and cleans up.
    let Ok(write_stream) = stream.try_clone() else {
        let _ = target.send(ToServer::Leave { client: id });
        return;
    };
    // A stalled client (e.g. suspended tab, dead NAT) would otherwise let its
    // outbound channel grow forever since the writer blocks indefinitely.
    // Bound the write so a stuck send errors out, the writer thread exits and
    // drops the receiver, and the sim thread reaps the client.
    let _ = write_stream.set_write_timeout(Some(Duration::from_secs(30)));
    thread::Builder::new()
        .name("fc-conn-writer".into())
        .spawn(move || {
            let mut w = io::BufWriter::new(write_stream);
            for msg in out_rx {
                if write_frame(&mut w, &msg).is_err() {
                    break;
                }
            }
            if let Ok(s) = w.into_inner() {
                let _ = s.shutdown(Shutdown::Both);
            }
        })
        .ok();

    while let Ok(msg) = read_frame::<_, ClientMsg>(&mut stream) {
        if target.send(ToServer::Msg { client: id, msg }).is_err() {
            break;
        }
    }
    let _ = target.send(ToServer::Leave { client: id });
}

/// Register with the sim thread; returns the client id and snapshot receiver.
/// `pub(crate)` so `world_manager` can join a connection onto a per-account
/// world's channel the same way this module joins one onto the shared world.
pub(crate) fn join(
    to_server: &Sender<ToServer>,
    name: String,
    token: Option<u64>,
    account: Option<i64>,
) -> Option<(u64, Receiver<ServerMsg>)> {
    let (out_tx, out_rx) = channel::<ServerMsg>();
    let (id_tx, id_rx) = channel::<u64>();
    to_server
        .send(ToServer::Join {
            name,
            token,
            account,
            out: out_tx,
            id_back: id_tx,
        })
        .ok()?;
    let id = id_rx.recv().ok()?;
    Some((id, out_rx))
}

/// A browser said "GET ": read the request head, then either upgrade to a
/// WebSocket or serve the static web build.
fn handle_http(
    mut stream: TcpStream,
    to_server: Sender<ToServer>,
    world_manager: Option<Arc<crate::world_manager::WorldManager>>,
) {
    // Read byte-by-byte so nothing past the head is consumed (the bytes that
    // follow the upgrade response are WebSocket frames).
    let mut head = Vec::with_capacity(512);
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") && head.len() < 8192 {
        match stream.read(&mut byte) {
            Ok(1) => head.push(byte[0]),
            _ => return,
        }
    }
    let text = String::from_utf8_lossy(&head).to_string();
    let path = text
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_string();
    let lower = text.to_ascii_lowercase();
    let is_upgrade = lower.contains("upgrade: websocket");
    if is_upgrade {
        serve_websocket(stream, head, to_server, world_manager);
    } else {
        // Serve the precompressed .gz sibling when the client accepts gzip —
        // the assetless wasm is ~66 MB raw vs ~15 MB gzipped, which matters a
        // lot on mobile web.
        let accepts_gzip = lower
            .lines()
            .any(|l| l.starts_with("accept-encoding:") && l.contains("gzip"));
        serve_static(stream, &path, accepts_gzip);
    }
}

/// Feeds the already-consumed request head back to tungstenite, then the live
/// socket. Lets the sniffing above coexist with tungstenite's handshake.
struct PrefixedStream {
    prefix: Vec<u8>,
    pos: usize,
    inner: TcpStream,
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

/// WebSocket clients get a single service thread (this one): reads use a
/// short timeout so queued snapshots can be written between frames.
fn serve_websocket(
    stream: TcpStream,
    head: Vec<u8>,
    to_server: Sender<ToServer>,
    world_manager: Option<Arc<crate::world_manager::WorldManager>>,
) {
    let prefixed = PrefixedStream {
        prefix: head,
        pos: 0,
        inner: stream,
    };
    // Cap WebSocket message/frame sizes at the same MAX_FRAME the native
    // length-prefixed protocol enforces (protocol.rs) — the default 64 MiB
    // tungstenite limit would otherwise let a browser client push far more
    // than a native client ever could.
    // WebSocketConfig is #[non_exhaustive], so it can't be built with a struct
    // literal from outside tungstenite — start from default and set the caps.
    let mut ws_config = tungstenite::protocol::WebSocketConfig::default();
    ws_config.max_message_size = Some(MAX_FRAME as usize);
    ws_config.max_frame_size = Some(MAX_FRAME as usize);
    let Ok(mut ws) = tungstenite::accept_with_config(prefixed, Some(ws_config)) else {
        return;
    };

    // First frame must be Hello, Login or EnterCentral (still under the 10 s
    // read timeout). `target` is whichever world this connection ends up in —
    // see `route_first_msg` — and is reused for every message this connection
    // sends afterward, not just the initial join.
    let (target, joined) = loop {
        match ws.read() {
            Ok(Message::Binary(b)) => match decode::<ClientMsg>(&b, MAX_FRAME as usize) {
                Ok(msg) => match route_first_msg(msg, &to_server, &world_manager) {
                    FirstMsgOutcome::Joined(target, joined) => break (target, joined),
                    FirstMsgOutcome::Refused(reason) => {
                        if let Ok(bytes) = encode(&ServerMsg::AuthFailed {
                            reason: reason.to_string(),
                        }) {
                            let _ = ws.send(Message::Binary(bytes.into()));
                        }
                        return;
                    }
                    FirstMsgOutcome::Drop => return,
                },
                Err(_) => return,
            },
            Ok(Message::Ping(_) | Message::Pong(_)) => continue,
            _ => return,
        }
    };

    let Some((id, out_rx)) = joined else {
        return;
    };
    let _ = ws
        .get_mut()
        .inner
        .set_read_timeout(Some(Duration::from_millis(20)));
    // Same write-timeout robustness fix as the native path: a stalled
    // WebSocket write should error out instead of letting the outbound queue
    // grow unbounded.
    let _ = ws
        .get_mut()
        .inner
        .set_write_timeout(Some(Duration::from_secs(30)));

    'session: loop {
        match ws.read() {
            Ok(Message::Binary(b)) => {
                if let Ok(msg) = decode::<ClientMsg>(&b, MAX_FRAME as usize) {
                    if target.send(ToServer::Msg { client: id, msg }).is_err() {
                        break 'session;
                    }
                }
            }
            Ok(Message::Close(_)) => break 'session,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if e.kind() == io::ErrorKind::WouldBlock
                    || e.kind() == io::ErrorKind::TimedOut => {}
            Err(_) => break 'session,
        }
        loop {
            match out_rx.try_recv() {
                Ok(msg) => {
                    let Ok(bytes) = encode(&msg) else {
                        break 'session;
                    };
                    if ws.send(Message::Binary(bytes.into())).is_err() {
                        break 'session;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break 'session,
            }
        }
    }
    let _ = ws.close(None);
    let _ = target.send(ToServer::Leave { client: id });
}

/// Minimal static file server for the web build (`web/` next to the binary).
fn serve_static(mut stream: TcpStream, path: &str, accepts_gzip: bool) {
    let rel = path.split('?').next().unwrap_or("/");
    let rel = if rel == "/" { "/index.html" } else { rel };
    let candidate = PathBuf::from(WEB_ROOT).join(rel.trim_start_matches('/'));
    // No path traversal: every component must be a plain name (no "..", no
    // roots), which keeps the resolved path inside WEB_ROOT.
    let safe = candidate
        .components()
        .all(|c| matches!(c, Component::Normal(_)));
    // Prefer a precompressed `.gz` sibling (produced by build-web.sh) when the
    // client accepts gzip — turns a 66 MB wasm transfer into ~15 MB.
    let gz = PathBuf::from(format!("{}.gz", candidate.to_string_lossy()));
    let (body, gzipped) = if safe && accepts_gzip {
        match std::fs::read(&gz) {
            Ok(b) => (Some(b), true),
            Err(_) => (std::fs::read(&candidate).ok(), false),
        }
    } else if safe {
        (std::fs::read(&candidate).ok(), false)
    } else {
        (None, false)
    };
    let response = match body {
        Some(body) => {
            let mime = content_type(&candidate);
            let enc = if gzipped { "Content-Encoding: gzip\r\n" } else { "" };
            // The wasm/js bundles are not content-hashed, so cache briefly
            // with revalidation rather than forever (avoids a stale build
            // after a redeploy against the server-authoritative wire
            // format). A plain string check, not `Path::starts_with` (which
            // compares whole components — "pkg-webgpu" would never match a
            // "pkg" prefix that way).
            let cache = if rel.trim_start_matches('/').starts_with("pkg-") {
                "Cache-Control: public, max-age=3600\r\n"
            } else {
                "Cache-Control: no-cache\r\n"
            };
            let mut r = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\n{enc}{cache}Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .into_bytes();
            r.extend_from_slice(&body);
            r
        }
        None => {
            let msg = "Frozen City server. Web build not found — run build-web.sh first.";
            format!(
                "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{msg}",
                msg.len()
            )
            .into_bytes()
        }
    };
    let _ = stream.write_all(&response);
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Both);
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}

fn sanitize_name(name: &str) -> String {
    let cleaned: String = name.chars().filter(|c| !c.is_control()).take(24).collect();
    if cleaned.trim().is_empty() {
        "Survivor".to_string()
    } else {
        cleaned.trim().to_string()
    }
}

/// Disconnected-player sessions (kept for reconnect) are capped at this many
/// entries, oldest evicted first, to bound memory on a long-running public
/// server.
const MAX_SESSIONS: usize = 128;

/// Upper bound on client/control messages processed per sim-loop pass before
/// yielding to the tick check, so no single flooding connection can starve
/// simulation for the whole shared world by keeping the drain loop busy.
const MAX_DRAIN_PER_PASS: u32 = 4096;

/// Per-connection message-rate limiter: a 1 s sliding window with generous
/// caps so normal play (and the e2e tests) are unaffected, while a
/// misbehaving or malicious client can't flood the sim thread.
struct RateLimiter {
    window_start: Instant,
    cmd: u32,
    chat: u32,
    ping: u32,
    cursor: u32,
    /// Last time `RefreshShowcase` was allowed — a separate, longer-period
    /// throttle (not the 1 s sliding window above): showcase reads a
    /// friend's save file from disk per entry, so it's capped independently
    /// at roughly human "opened the panel" cadence, not command-flood scale.
    last_showcase: Option<Instant>,
}

impl RateLimiter {
    fn new(now: Instant) -> Self {
        RateLimiter {
            window_start: now,
            cmd: 0,
            chat: 0,
            ping: 0,
            cursor: 0,
            last_showcase: None,
        }
    }

    fn reset_if_elapsed(&mut self, now: Instant) {
        if now.duration_since(self.window_start) >= Duration::from_secs(1) {
            self.window_start = now;
            self.cmd = 0;
            self.chat = 0;
            self.ping = 0;
            self.cursor = 0;
        }
    }

    fn allow_cmd(&mut self, now: Instant) -> bool {
        self.reset_if_elapsed(now);
        self.cmd += 1;
        self.cmd <= 30
    }

    fn allow_chat(&mut self, now: Instant) -> bool {
        self.reset_if_elapsed(now);
        self.chat += 1;
        self.chat <= 4
    }

    fn allow_ping(&mut self, now: Instant) -> bool {
        self.reset_if_elapsed(now);
        self.ping += 1;
        self.ping <= 6
    }

    /// Cursor presence is high-frequency but still bounded well above the
    /// client's own ~8/s cadence, so a flood can't monopolise the sim thread.
    fn allow_cursor(&mut self, now: Instant) -> bool {
        self.reset_if_elapsed(now);
        self.cursor += 1;
        self.cursor <= 60
    }

    /// At most one `RefreshShowcase` per [`SHOWCASE_COOLDOWN`], independent
    /// of the 1 s command window (see `last_showcase`'s doc comment).
    fn allow_showcase(&mut self, now: Instant) -> bool {
        if self.last_showcase.is_some_and(|last| now.duration_since(last) < SHOWCASE_COOLDOWN) {
            return false;
        }
        self.last_showcase = Some(now);
        true
    }
}

/// Minimum spacing between `RefreshShowcase` requests from one connection —
/// each entry is a friend's save file read from disk, so this is a
/// human-interaction-scale cap, not a command-flood one.
const SHOWCASE_COOLDOWN: Duration = Duration::from_secs(5);

/// How far (Chebyshev, in tiles) a nearby-chat bubble carries. Roughly the
/// on-screen neighborhood at the default zoom — close enough to feel local,
/// wide enough that two players talking don't have to stand on one tile.
const LOCAL_CHAT_RADIUS: f32 = 12.0;

/// The freshest friends list for `account`, with online-in-central flags when
/// this world IS the central one (elsewhere the server has no global view and
/// reports everyone offline — see `FriendInfo::online_central`).
fn social_for(state: &GameState, account: i64) -> ServerMsg {
    let friends = accounts::friends_list(account)
        .into_iter()
        .map(|(fid, fname)| FriendInfo {
            account: fid,
            name: fname,
            online_central: state.central && state.players.iter().any(|p| p.account == Some(fid)),
        })
        .collect();
    ServerMsg::Social { friends }
}

/// `ServerMsg::Showcase` for `account` (V0.5 "hub activities v1"): one entry
/// per friend, read on demand from that friend's personal-world save file —
/// fine at this scale (a handful of friends, a rare click), not something the
/// tick loop ever does. A friend with no save yet (never played, or the file
/// simply isn't there) is silently skipped rather than fabricating a row.
/// When `state` IS the central world, each entry's `central_contribution` is
/// filled in for free from the ledger already sitting in memory; elsewhere
/// (a personal world has no global view) it's `None`.
fn showcase_for(state: &GameState, account: i64) -> ServerMsg {
    let entries = accounts::friends_list(account)
        .into_iter()
        .filter_map(|(fid, fname)| {
            let path = crate::world_manager::account_save_path(fid);
            let friend_state = persist::load_at(&path)?;
            Some(ShowcaseEntry {
                account: fid,
                name: fname,
                days_survived: friend_state.day(),
                population: friend_state.survivors.len() as u32,
                buildings: friend_state.buildings.len() as u32,
                graduated: friend_state.graduated,
                central_contribution: state.central.then(|| state.ledger_for(fid)),
            })
        })
        .collect();
    ServerMsg::Showcase { entries }
}

/// A private, transient system line to one client, reusing the Bubble channel
/// (`player_id: 0` marks system text, same convention as `ChatLine`). Used
/// for feedback that must not enter the shared world snapshot ("friend not
/// found", "invite sent").
fn system_bubble(text: &str) -> ServerMsg {
    ServerMsg::Bubble {
        player_id: 0,
        name: "System".to_string(),
        color: 0,
        text: text.to_string(),
    }
}

/// Common client-departure bookkeeping, shared by an explicit `Leave` and the
/// broadcast dead-client cleanup. Drops the connection, stashes the
/// departing player's stats (name/color/built/demolished) under their
/// session token so a later `Hello` carrying that token can reconnect as the
/// same player, then removes them from the live roster. No-op if the
/// connection id is unknown (already cleaned up).
#[allow(clippy::too_many_arguments)]
fn disconnect(
    client_id: u64,
    clients: &mut HashMap<u64, Sender<ServerMsg>>,
    player_of: &mut HashMap<u64, u64>,
    token_of: &mut HashMap<u64, u64>,
    sessions: &mut HashMap<u64, PlayerInfo>,
    session_order: &mut VecDeque<u64>,
    limiters: &mut HashMap<u64, RateLimiter>,
    state: &mut GameState,
) {
    clients.remove(&client_id);
    limiters.remove(&client_id);
    let Some(player_id) = player_of.remove(&client_id) else {
        return;
    };
    let token = token_of.remove(&client_id);
    let saved_info = state.player(player_id).cloned();
    if let (Some(token), Some(info)) = (token, saved_info) {
        if !sessions.contains_key(&token) {
            session_order.push_back(token);
        }
        sessions.insert(token, info);
        while sessions.len() > MAX_SESSIONS {
            let Some(oldest) = session_order.pop_front() else {
                break;
            };
            sessions.remove(&oldest);
        }
    }
    sim::player_left(state, player_id);
}

/// `pub(crate)` so `world_manager` can spawn one `sim_loop` per account world,
/// reusing the exact same tick/persistence/session logic as the single
/// shared world instead of duplicating it.
pub(crate) fn sim_loop(config: ServerConfig, rx: Receiver<ToServer>, shutdown: Arc<AtomicBool>) {
    // A persistent (dedicated) server resumes its last save instead of
    // starting fresh, so the periodic systemd restart and a deploy's
    // stop/start don't wipe every player's city. Anything else (singleplayer,
    // host/join) is a throwaway in-memory world, same as before.
    let fresh = || {
        if config.central {
            sim::new_game_central(config.seed)
        } else {
            sim::new_game(config.seed, config.win_days)
        }
    };
    let mut state = if config.persistent {
        match &config.save_path {
            Some(path) => persist::load_at(path),
            None => persist::load(),
        }
        .unwrap_or_else(fresh)
    } else {
        fresh()
    };
    // Re-assert on every boot: a central world stays central even if its save
    // predates the flag or was ever written without it.
    if config.central {
        state.central = true;
    }
    let mut clients: HashMap<u64, Sender<ServerMsg>> = HashMap::new();
    // Connection id (key of `clients`, the `client` field on `ToServer`) is
    // decoupled from player id: a connection is a socket, a player is a
    // persistent identity that can outlive one and survive a reconnect.
    let mut next_client_id: u64 = 1;
    let mut next_player_id: u64 = 1;
    // Session tokens must be unguessable: a sequential counter would let anyone
    // hijack a disconnected player by trying Hello{token: 1, 2, 3, ...}, and a
    // seeded PRNG stream (even one seeded from wall-clock entropy) is invertible
    // from a single observed output, letting an attacker who sniffs one token
    // predict every subsequent one. Each token is instead drawn independently
    // from the OS CSPRNG via `fresh_token()`.
    let mut player_of: HashMap<u64, u64> = HashMap::new(); // connection id -> player id
    let mut token_of: HashMap<u64, u64> = HashMap::new(); // connection id -> session token
    let mut sessions: HashMap<u64, PlayerInfo> = HashMap::new(); // session token -> saved info of a disconnected player
    let mut session_order: VecDeque<u64> = VecDeque::new(); // insertion order of `sessions`, for eviction
    let mut limiters: HashMap<u64, RateLimiter> = HashMap::new(); // connection id -> rate limiter
    let mut pending: Vec<(u64, PlayerCommand)> = Vec::new();
    let mut ever_joined = false;
    // When `config.idle_shutdown` is set, tracks how long `clients` has been
    // continuously empty — reset to `None` the moment anyone joins.
    let mut idle_since: Option<Instant> = None;
    let mut printed_events: u64 = 0;
    let mut printed_chat: u64 = 0;
    let mut game_over_since: Option<Instant> = None;
    // What the last broadcast tick's `State` message carried, so the next
    // one can skip these quiet-most-ticks collections when nothing changed
    // (see `protocol::Included`). `events`/`chat` reuse the monotonic
    // counters already kept for the verbose console log below instead of
    // cloning the (capped but text-heavy) logs just to compare them.
    let mut last_sent_events: u64 = 0;
    let mut last_sent_chat: u64 = 0;
    let mut last_sent_pings: Vec<Ping> = Vec::new();
    let mut last_sent_missions: Vec<Mission> = Vec::new();
    let mut last_sent_techs: Vec<Tech> = Vec::new();

    let tick_dur = Duration::from_millis(TICK_MS);
    let mut next_tick = Instant::now() + tick_dur;
    let mut last_save = Instant::now();

    if config.verbose && config.port.is_some() {
        println!(
            "[server] Frozen City dedicated server up (seed {}, survive {} days)",
            config.seed, config.win_days
        );
    }

    'outer: loop {
        // Drain control/client messages, but never more than a bounded batch
        // per pass so a flood can't postpone the tick indefinitely.
        let mut drained = 0u32;
        loop {
            if drained >= MAX_DRAIN_PER_PASS {
                break;
            }
            let recv = rx.try_recv();
            if recv.is_ok() {
                drained += 1;
            }
            match recv {
                Ok(ToServer::CountOwned { account, reply }) => {
                    let _ = reply.send(state.owned_settlers(account));
                }
                Ok(ToServer::ExtractMigrants { max, reply }) => {
                    let migrants = if state.graduated {
                        Some(sim::extract_migrants(&mut state, max))
                    } else {
                        None
                    };
                    let _ = reply.send(migrants);
                }
                Ok(ToServer::InjectMigrants { account, name, survivors }) => {
                    sim::inject_migrants(&mut state, account, &name, survivors);
                }
                Ok(ToServer::OwnerOnline { owner, reply }) => {
                    let online = state.players.iter().any(|p| p.account == Some(owner));
                    let _ = reply.send(online);
                }
                Ok(ToServer::DeliverServerMsg { account, msg }) => {
                    let targets: Vec<u64> = state
                        .players
                        .iter()
                        .filter(|p| p.account == Some(account))
                        .map(|p| p.id)
                        .collect();
                    if !targets.is_empty() {
                        for (cid, out) in &clients {
                            if player_of.get(cid).is_some_and(|p| targets.contains(p)) {
                                let _ = out.send((*msg).clone());
                            }
                        }
                    }
                }
                Ok(ToServer::Join { name, token, account, out, id_back }) => {
                    let client_id = next_client_id;
                    next_client_id += 1;

                    // A token that still points at a stashed, disconnected
                    // player's info is a reconnect: resume that identity
                    // (and its built/demolished stats) instead of joining
                    // fresh.
                    let reconnect = token.and_then(|t| sessions.remove(&t).map(|info| (t, info)));
                    let (player_id, session_token, reconnected) =
                        if let Some((t, mut saved)) = reconnect {
                            let player_id = saved.id;
                            session_order.retain(|&tok| tok != t);
                            // A stashed session may predate the account field
                            // (or the player first joined as a guest); the
                            // authenticated connection is the fresher truth.
                            if account.is_some() {
                                saved.account = account;
                            }
                            sim::player_rejoined(&mut state, saved);
                            // Rotate the token on every reconnect: the old one
                            // was sent once in plaintext, so a sniffed token is
                            // dead the moment the real client reconnects.
                            (player_id, fresh_token(), true)
                        } else {
                            let player_id = next_player_id;
                            next_player_id += 1;
                            let session_token = fresh_token();
                            sim::player_joined_as(&mut state, player_id, &name, account);
                            (player_id, session_token, false)
                        };

                    // In an account-owned personal world, authority follows
                    // the OWNING ACCOUNT, not join order: an invited visitor
                    // who happens to connect first must never seize the
                    // Owner role (or keep a stale owner_id claim).
                    if let Some(owner_acc) = config.owner_account {
                        let is_owner = account == Some(owner_acc);
                        if let Some(p) = state.players.iter_mut().find(|p| p.id == player_id) {
                            p.role = if is_owner { Role::Owner } else { Role::Guest };
                        }
                        if is_owner {
                            state.owner_id = Some(player_id);
                        } else if state.owner_id == Some(player_id) {
                            state.owner_id = None;
                        }
                    }

                    player_of.insert(client_id, player_id);
                    token_of.insert(client_id, session_token);
                    limiters.insert(client_id, RateLimiter::new(Instant::now()));

                    let _ = out.send(ServerMsg::Welcome {
                        player_id,
                        token: session_token,
                        state: state.clone(),
                    });
                    // Account sessions get their friends list right away, so
                    // the social panel is populated without an extra request.
                    // Same for their current visit policy (V0.6
                    // "owner-offline entry"), so the settings toggle can show
                    // the right state without a round-trip.
                    if let Some(acc) = account {
                        let _ = out.send(social_for(&state, acc));
                        let _ = out.send(ServerMsg::VisitPolicy {
                            allow_offline: accounts::visit_policy(acc),
                        });
                    }
                    clients.insert(client_id, out);
                    let _ = id_back.send(client_id);
                    ever_joined = true;
                    idle_since = None;
                    if config.verbose {
                        if reconnected {
                            println!(
                                "[server] {} reconnected (#{} as player {})",
                                name, client_id, player_id
                            );
                        } else {
                            println!(
                                "[server] {} connected (#{} as player {})",
                                name, client_id, player_id
                            );
                        }
                    }
                }
                Ok(ToServer::Msg { client, msg }) => {
                    // A message for a connection we've already torn down (an
                    // in-flight frame that raced the disconnect) must be dropped,
                    // never attributed to a fabricated player id.
                    let Some(&pid) = player_of.get(&client) else {
                        continue;
                    };
                    let now = Instant::now();
                    // A missing limiter denies (fail closed); every live
                    // connection has one, inserted on Join.
                    let allowed = match &msg {
                        ClientMsg::Cmd(_) => limiters
                            .get_mut(&client)
                            .is_some_and(|l| l.allow_cmd(now)),
                        ClientMsg::Chat { .. } => limiters
                            .get_mut(&client)
                            .is_some_and(|l| l.allow_chat(now)),
                        ClientMsg::Ping { .. } => limiters
                            .get_mut(&client)
                            .is_some_and(|l| l.allow_ping(now)),
                        ClientMsg::Cursor { .. } => limiters
                            .get_mut(&client)
                            .is_some_and(|l| l.allow_cursor(now)),
                        // First-frame-only messages (consumed before `join()`,
                        // never reach here); a later one is a protocol
                        // violation, not a command.
                        ClientMsg::Hello { .. }
                        | ClientMsg::Login { .. }
                        | ClientMsg::EnterCentral { .. }
                        | ClientMsg::Register { .. }
                        | ClientMsg::VisitFriend { .. } => false,
                        // Social traffic shares the chat budget: all of it is
                        // human-scale (a click or a said line), and Add/Remove
                        // hit the accounts DB, which a flood must not.
                        ClientMsg::ChatLocal { .. }
                        | ClientMsg::AddFriend { .. }
                        | ClientMsg::RemoveFriend { .. }
                        | ClientMsg::RefreshSocial
                        | ClientMsg::Invite { .. } => limiters
                            .get_mut(&client)
                            .is_some_and(|l| l.allow_chat(now)),
                        // Low-frequency owner-only admin actions: never rate
                        // limited (they're structurally rare — one click each
                        // — and gating them behind the cmd/chat/ping caps
                        // would let a flood on those channels starve a kick).
                        ClientMsg::SetGuestPermission { .. } | ClientMsg::Kick { .. } => true,
                        // Showcase reads a save file per friend from disk —
                        // its own dedicated cooldown (see `allow_showcase`),
                        // not the 1 s chat/cmd budget.
                        ClientMsg::RefreshShowcase => limiters
                            .get_mut(&client)
                            .is_some_and(|l| l.allow_showcase(now)),
                        // Rare owner-only setting change; same reasoning as
                        // SetGuestPermission/Kick above.
                        ClientMsg::SetVisitPolicy { .. } => true,
                    };
                    if allowed {
                        match msg {
                            ClientMsg::Cmd(cmd) => pending.push((pid, cmd)),
                            ClientMsg::Cursor { x, y } => sim::set_cursor(&mut state, pid, x, y),
                            ClientMsg::Chat { text } => sim::push_chat(&mut state, pid, &text),
                            ClientMsg::Ping { x, y } => sim::add_ping(&mut state, pid, x, y),
                            ClientMsg::Hello { .. }
                            | ClientMsg::Login { .. }
                            | ClientMsg::EnterCentral { .. }
                            | ClientMsg::Register { .. }
                            | ClientMsg::VisitFriend { .. } => {}
                            ClientMsg::ChatLocal { text } => {
                                let text = sim::sanitize_public_text(&text);
                                if !text.trim().is_empty() {
                                    if let Some(speaker) = state.player(pid).cloned() {
                                        let bubble = ServerMsg::Bubble {
                                            player_id: pid,
                                            name: speaker.name.clone(),
                                            color: speaker.color,
                                            text,
                                        };
                                        for (cid, out) in &clients {
                                            let Some(&other) = player_of.get(cid) else {
                                                continue;
                                            };
                                            // Deliver within earshot; when a
                                            // position is unknown (a player
                                            // who hasn't moved yet) err on
                                            // the side of delivering.
                                            let near = other == pid
                                                || match (
                                                    speaker.cursor,
                                                    state.player(other).and_then(|p| p.cursor),
                                                ) {
                                                    (Some((sx, sy)), Some((ox, oy))) => {
                                                        (sx - ox).abs().max((sy - oy).abs())
                                                            <= LOCAL_CHAT_RADIUS
                                                    }
                                                    _ => true,
                                                };
                                            if near {
                                                let _ = out.send(bubble.clone());
                                            }
                                        }
                                    }
                                }
                            }
                            ClientMsg::AddFriend { name } => {
                                if let Some(acc) = state.player_account(pid) {
                                    let feedback = match accounts::friends_add(acc, &name) {
                                        Ok((_, fname)) => {
                                            format!("{fname} do'stlar ro'yxatiga qo'shildi.")
                                        }
                                        Err(accounts::FriendError::NotFound) => {
                                            "Bunday ismli o'yinchi topilmadi.".to_string()
                                        }
                                        Err(accounts::FriendError::SelfAdd) => {
                                            "O'zingizni qo'sha olmaysiz.".to_string()
                                        }
                                        Err(accounts::FriendError::Io) => {
                                            "Server do'stlar ro'yxatini yangilay olmadi.".to_string()
                                        }
                                    };
                                    if let Some(out) = clients.get(&client) {
                                        let _ = out.send(system_bubble(&feedback));
                                        let _ = out.send(social_for(&state, acc));
                                    }
                                }
                            }
                            ClientMsg::RemoveFriend { account: friend } => {
                                if let Some(acc) = state.player_account(pid) {
                                    let _ = accounts::friends_remove(acc, friend);
                                    if let Some(out) = clients.get(&client) {
                                        let _ = out.send(social_for(&state, acc));
                                    }
                                }
                            }
                            ClientMsg::RefreshSocial => {
                                if let Some(acc) = state.player_account(pid) {
                                    if let Some(out) = clients.get(&client) {
                                        let _ = out.send(social_for(&state, acc));
                                    }
                                }
                            }
                            ClientMsg::Invite { account: target } => {
                                // Only in the central world (that's where
                                // people meet), only by account sessions,
                                // never to yourself.
                                let host_acc = state.player_account(pid);
                                if let (true, Some(host_acc), Some(book)) =
                                    (state.central, host_acc, config.invites.as_ref())
                                {
                                    if host_acc != target {
                                        let targets: Vec<u64> = state
                                            .players
                                            .iter()
                                            .filter(|p| p.account == Some(target))
                                            .map(|p| p.id)
                                            .collect();
                                        let host_name = state
                                            .player(pid)
                                            .map(|p| p.name.clone())
                                            .unwrap_or_default();
                                        let invited_msg = ServerMsg::Invited {
                                            host: host_acc,
                                            host_name: host_name.clone(),
                                        };
                                        // Deliver to the target if they're
                                        // right here in the central world...
                                        for (cid, out) in &clients {
                                            if player_of.get(cid).is_some_and(|p| targets.contains(p)) {
                                                let _ = out.send(invited_msg.clone());
                                            }
                                        }
                                        // ...AND, regardless, to their own
                                        // PERSONAL world too (V0.6 "guest
                                        // without onboarding" — a friend
                                        // doesn't have to be standing in the
                                        // hub at this exact moment to be
                                        // invited into it).
                                        if let Some(wm) = config.world_manager.as_ref() {
                                            wm.deliver_to_account(target, invited_msg);
                                        }
                                        book.invite(host_acc, target);
                                        if let Some(out) = clients.get(&client) {
                                            let feedback = if targets.is_empty() {
                                                "Taklif yuborildi — do'stingiz hozir Global Olamda emas, lekin o'z olamiga kirganda ko'radi."
                                            } else {
                                                "Taklif yuborildi — o'z olamingizga qaytsangiz, do'stingiz kira oladi."
                                            };
                                            let _ = out.send(system_bubble(feedback));
                                        }
                                    }
                                }
                            }
                            ClientMsg::SetGuestPermission { perm } => {
                                if state.is_owner(pid) {
                                    sim::set_guest_permission(&mut state, perm);
                                }
                            }
                            ClientMsg::Kick { target } => {
                                // Owner-only; never self, never another owner.
                                if state.is_owner(pid)
                                    && target != pid
                                    && !state.is_owner(target)
                                {
                                    // Reverse-lookup the connection serving
                                    // this player, if it's still live.
                                    let kc = player_of
                                        .iter()
                                        .find(|(_, &p)| p == target)
                                        .map(|(&c, _)| c);
                                    if let Some(kc) = kc {
                                        // Drop the connection directly rather
                                        // than via `disconnect()`: a kicked
                                        // player must not get a saved
                                        // reconnect session (which would let
                                        // them silently rejoin) or a "left"
                                        // event (kick_player below pushes its
                                        // own "removed by the owner" event).
                                        // Dropping the Sender here ends the
                                        // writer thread, which shuts the
                                        // socket down and unblocks the
                                        // reader thread to clean up its own
                                        // `Leave` (a no-op by then since the
                                        // connection id is already gone).
                                        clients.remove(&kc);
                                        player_of.remove(&kc);
                                        token_of.remove(&kc);
                                        limiters.remove(&kc);
                                    }
                                    sim::kick_player(&mut state, target);
                                }
                            }
                            ClientMsg::RefreshShowcase => {
                                if let Some(acc) = state.player_account(pid) {
                                    if let Some(out) = clients.get(&client) {
                                        let _ = out.send(showcase_for(&state, acc));
                                    }
                                }
                            }
                            ClientMsg::SetVisitPolicy { allow_offline } => {
                                if let Some(acc) = state.player_account(pid) {
                                    if accounts::set_visit_policy(acc, allow_offline).is_ok() {
                                        if let Some(out) = clients.get(&client) {
                                            let _ = out.send(ServerMsg::VisitPolicy { allow_offline });
                                        }
                                    } else if let Some(out) = clients.get(&client) {
                                        let _ = out.send(system_bubble(
                                            "Sozlamani saqlab bo'lmadi — keyinroq qayta urining.",
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(ToServer::Leave { client }) => {
                    if clients.contains_key(&client) {
                        disconnect(
                            client,
                            &mut clients,
                            &mut player_of,
                            &mut token_of,
                            &mut sessions,
                            &mut session_order,
                            &mut limiters,
                            &mut state,
                        );
                        if config.verbose {
                            println!("[server] client #{} disconnected", client);
                        }
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break 'outer,
            }
        }

        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        if ever_joined && clients.is_empty() && !config.persistent {
            break;
        }
        // Per-account worlds (`idle_shutdown: Some(_)`) save and exit after
        // sitting with nobody connected for that long, instead of running
        // forever — `WorldManager::get_or_spawn` respawns them from disk the
        // next time that account logs in. The single shared world always has
        // `idle_shutdown: None` and is unaffected.
        if let Some(idle_dur) = config.idle_shutdown {
            if clients.is_empty() {
                let since = *idle_since.get_or_insert_with(Instant::now);
                if Instant::now().duration_since(since) >= idle_dur {
                    if config.persistent {
                        if let Err(e) = save_world(&config, &state) {
                            if config.verbose {
                                eprintln!("[server] idle-eviction save failed: {e}");
                            }
                        }
                    }
                    break;
                }
            } else {
                idle_since = None;
            }
        }

        let now = Instant::now();

        if config.persistent && now.duration_since(last_save) >= AUTOSAVE_INTERVAL {
            last_save = now;
            if let Err(e) = save_world(&config, &state) {
                if config.verbose {
                    eprintln!("[server] world autosave failed: {e}");
                }
            }
        }

        if now >= next_tick {
            let day_before = state.day();
            // Drop commands queued by a player who has since left or been
            // kicked: `can_issue` trusts pids absent from the roster, so an
            // unapplied command from a departed guest must not slip through
            // with unrestricted permission.
            pending.retain(|(pid, _)| state.player(*pid).is_some());
            for (pid, cmd) in pending.drain(..) {
                sim::apply_command(&mut state, pid, &cmd);
            }
            sim::tick(&mut state);

            // A dead (or victorious) persistent world starts over, so the
            // public server never sits in a game-over screen for hours.
            if config.persistent {
                if state.phase == GamePhase::Running {
                    game_over_since = None;
                } else {
                    let since = *game_over_since.get_or_insert(now);
                    if now.duration_since(since) >= WORLD_RESET_AFTER {
                        game_over_since = None;
                        let seed = state.rng ^ 0xA5A5_5A5A_D00D_FEED ^ state.tick;
                        let players = state.players.clone();
                        // Carry ownership and the owner-set guest policy across
                        // the reset, otherwise a ViewOnly-locked world silently
                        // reopens to Build for everyone each game cycle.
                        let guest_perm = state.guest_perm;
                        let owner_id = state.owner_id;
                        // Graduation is a permanent achievement, not a
                        // property of one map: it's what admits this world's
                        // account to the central world, and must not expire
                        // just because the next survival run started before
                        // (or without) the player stepping through.
                        let graduated = state.graduated;
                        state = sim::new_game(seed, config.win_days);
                        state.players = players;
                        state.guest_perm = guest_perm;
                        state.owner_id = owner_id;
                        state.graduated = graduated;
                        printed_events = 0;
                        printed_chat = 0;
                        // `total_events`/`total_chat` also restart at 0 on a fresh
                        // `new_game`, so the last-sent counters must follow —
                        // otherwise a coincidental match after the reset would
                        // wrongly tell a connected client "no new events/chat"
                        // while it's still holding the previous world's log.
                        last_sent_events = 0;
                        last_sent_chat = 0;
                        sim::push_event(
                            &mut state,
                            "A new expedition arrives - the city rises again.",
                        );
                        if config.verbose {
                            println!("[server] world reset (seed {seed})");
                        }
                    }
                }
            }

            if config.verbose {
                if state.day() != day_before {
                    println!(
                        "[server] day {} — pop {}, wood {:.0}, coal {:.0}, food {:.0}, {:.1} C",
                        state.day(),
                        state.survivors.len(),
                        state.stock.wood,
                        state.stock.coal,
                        state.stock.food,
                        state.temperature()
                    );
                }
                while printed_events < state.total_events {
                    let missed = (state.total_events - printed_events) as usize;
                    let start = state.events.len().saturating_sub(missed);
                    for ev in &state.events[start..] {
                        println!("[server] day {}: {}", ev.day, ev.text);
                    }
                    printed_events = state.total_events;
                }
                while printed_chat < state.total_chat {
                    let missed = (state.total_chat - printed_chat) as usize;
                    let start = state.chat.len().saturating_sub(missed);
                    for line in &state.chat[start..] {
                        println!("[server] chat {}: {}", line.name, line.text);
                    }
                    printed_chat = state.total_chat;
                }
            }

            // Broadcast the snapshot; tiles ride along only every Nth tick, and
            // the other quiet-most-ticks collections (events/chat/pings/
            // missions/techs) only when they actually changed — see
            // `protocol::Included`. With nobody connected (a persistent,
            // currently-empty region) there's nothing to send it to, so skip
            // the clone/serialize/send work entirely — the sim keeps ticking
            // (and autosaving) above regardless. `state.tick`'s tile cadence
            // still advances on its normal schedule either way, so the moment
            // a client joins mid-cycle it sees exactly the tile/no-tile
            // pattern it would have if the broadcast had never stopped.
            if !clients.is_empty() {
                let included = Included {
                    tiles: state.tick % TILES_EVERY_N_TICKS == 0,
                    events: state.total_events != last_sent_events,
                    chat: state.total_chat != last_sent_chat,
                    pings: state.pings != last_sent_pings,
                    missions: state.missions != last_sent_missions,
                    techs: state.techs != last_sent_techs,
                };
                last_sent_events = state.total_events;
                last_sent_chat = state.total_chat;
                if included.pings {
                    last_sent_pings = state.pings.clone();
                }
                if included.missions {
                    last_sent_missions = state.missions.clone();
                }
                if included.techs {
                    last_sent_techs = state.techs.clone();
                }
                let mut wire = state.clone();
                if !included.tiles {
                    wire.tiles = Vec::new();
                }
                if !included.events {
                    wire.events = Vec::new();
                }
                if !included.chat {
                    wire.chat = Vec::new();
                }
                if !included.pings {
                    wire.pings = Vec::new();
                }
                if !included.missions {
                    wire.missions = Vec::new();
                }
                if !included.techs {
                    wire.techs = Vec::new();
                }
                let mut dead: Vec<u64> = Vec::new();
                for (id, out) in &clients {
                    let msg = ServerMsg::State {
                        state: wire.clone(),
                        included,
                    };
                    if out.send(msg).is_err() {
                        dead.push(*id);
                    }
                }
                for id in dead {
                    disconnect(
                        id,
                        &mut clients,
                        &mut player_of,
                        &mut token_of,
                        &mut sessions,
                        &mut session_order,
                        &mut limiters,
                        &mut state,
                    );
                }
            }

            next_tick += tick_dur;
            // If we fell far behind (debugger, laptop sleep), resync.
            if now > next_tick + Duration::from_secs(1) {
                next_tick = now + tick_dur;
            }
        }

        let wait = next_tick.saturating_duration_since(Instant::now());
        if !wait.is_zero() {
            thread::sleep(wait.min(Duration::from_millis(50)));
        }
    }
    if config.persistent {
        if let Err(e) = save_world(&config, &state) {
            if config.verbose {
                eprintln!("[server] final world save on shutdown failed: {e}");
            }
        }
    }
    shutdown.store(true, Ordering::SeqCst);
}

/// Saves to this config's world: the per-account path when set, otherwise
/// the single shared-world default (`persist::save`, i.e. `FC_WORLD_SAVE`).
fn save_world(config: &ServerConfig, state: &GameState) -> io::Result<()> {
    match &config.save_path {
        Some(path) => persist::save_at(state, path),
        None => persist::save(state),
    }
}
