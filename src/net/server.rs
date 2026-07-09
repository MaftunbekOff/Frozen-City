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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use tungstenite::Message;

use crate::game::rng::Rng;
use crate::game::sim;
use crate::game::types::{GamePhase, GameState, PlayerCommand, PlayerInfo, TICK_MS};
use crate::net::client::ClientConn;
use crate::net::protocol::{
    read_frame, write_frame, ClientMsg, ServerMsg, TILES_EVERY_N_TICKS,
};

/// Directory the built-in HTTP server serves the web build from.
const WEB_ROOT: &str = "web";

pub struct ServerConfig {
    /// Bind a TCP listener on this port; `None` = local-only (singleplayer).
    pub port: Option<u16>,
    pub seed: u64,
    pub win_days: u32,
    /// Keep running with zero clients (dedicated server mode).
    pub persistent: bool,
    /// Print events and day changes to stdout (dedicated server mode).
    pub verbose: bool,
}

/// On a persistent server, a finished world (won or lost) restarts with a
/// fresh map after this long, keeping the connected players.
const WORLD_RESET_AFTER: Duration = Duration::from_secs(45);

pub enum ToServer {
    Join {
        name: String,
        /// Session token from a previous `Welcome`, to resume the same player
        /// identity (reconnect) instead of joining as a fresh player.
        token: Option<u64>,
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
}

#[derive(Clone)]
pub struct ServerHandle {
    pub to_server: Sender<ToServer>,
    pub shutdown: Arc<AtomicBool>,
    pub addr: Option<SocketAddr>,
}

impl ServerHandle {
    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }
}

pub fn start(config: ServerConfig) -> io::Result<ServerHandle> {
    let (to_server_tx, to_server_rx) = channel::<ToServer>();
    let shutdown = Arc::new(AtomicBool::new(false));

    let addr = if let Some(port) = config.port {
        let listener = TcpListener::bind(("0.0.0.0", port))?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let tx = to_server_tx.clone();
        let flag = shutdown.clone();
        thread::Builder::new()
            .name("fc-acceptor".into())
            .spawn(move || accept_loop(listener, tx, flag))
            .expect("spawn acceptor");
        Some(addr)
    } else {
        None
    };

    let flag = shutdown.clone();
    thread::Builder::new()
        .name("fc-sim".into())
        .spawn(move || sim_loop(config, to_server_rx, flag))
        .expect("spawn sim");

    Ok(ServerHandle {
        to_server: to_server_tx,
        shutdown,
        addr,
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

fn accept_loop(listener: TcpListener, to_server: Sender<ToServer>, shutdown: Arc<AtomicBool>) {
    loop {
        if shutdown.load(Ordering::SeqCst) {
            return;
        }
        match listener.accept() {
            Ok((stream, _peer)) => {
                let tx = to_server.clone();
                thread::Builder::new()
                    .name("fc-conn".into())
                    .spawn(move || handle_socket(stream, tx))
                    .ok();
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(150));
            }
            Err(_) => return,
        }
    }
}

fn handle_socket(stream: TcpStream, to_server: Sender<ToServer>) {
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
        handle_http(stream, to_server);
    } else {
        handle_native(stream, to_server);
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

/// The native desktop protocol: length-prefixed bincode frames.
fn handle_native(mut stream: TcpStream, to_server: Sender<ToServer>) {
    // The very first frame must be Hello (the 10 s timeout is already set).
    let (name, token) = match read_frame::<_, ClientMsg>(&mut stream) {
        Ok(ClientMsg::Hello { name, token }) => (sanitize_name(&name), token),
        _ => return,
    };
    let _ = stream.set_read_timeout(None);

    let Some((id, out_rx)) = join(&to_server, name, token) else {
        return;
    };

    // Writer thread: serialize server messages onto the socket. When the
    // server drops this client's sender, shut the socket down so the blocking
    // reader below unblocks and cleans up.
    let Ok(write_stream) = stream.try_clone() else {
        let _ = to_server.send(ToServer::Leave { client: id });
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

    loop {
        match read_frame::<_, ClientMsg>(&mut stream) {
            Ok(msg) => {
                if to_server.send(ToServer::Msg { client: id, msg }).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let _ = to_server.send(ToServer::Leave { client: id });
}

/// Register with the sim thread; returns the client id and snapshot receiver.
fn join(
    to_server: &Sender<ToServer>,
    name: String,
    token: Option<u64>,
) -> Option<(u64, Receiver<ServerMsg>)> {
    let (out_tx, out_rx) = channel::<ServerMsg>();
    let (id_tx, id_rx) = channel::<u64>();
    to_server
        .send(ToServer::Join {
            name,
            token,
            out: out_tx,
            id_back: id_tx,
        })
        .ok()?;
    let id = id_rx.recv().ok()?;
    Some((id, out_rx))
}

/// A browser said "GET ": read the request head, then either upgrade to a
/// WebSocket or serve the static web build.
fn handle_http(mut stream: TcpStream, to_server: Sender<ToServer>) {
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
        serve_websocket(stream, head, to_server);
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
fn serve_websocket(stream: TcpStream, head: Vec<u8>, to_server: Sender<ToServer>) {
    let prefixed = PrefixedStream {
        prefix: head,
        pos: 0,
        inner: stream,
    };
    let Ok(mut ws) = tungstenite::accept(prefixed) else {
        return;
    };

    // First frame must be Hello (still under the 10 s read timeout).
    let (name, token) = loop {
        match ws.read() {
            Ok(Message::Binary(b)) => match bincode::deserialize::<ClientMsg>(&b) {
                Ok(ClientMsg::Hello { name, token }) => break (sanitize_name(&name), token),
                _ => return,
            },
            Ok(Message::Ping(_) | Message::Pong(_)) => continue,
            _ => return,
        }
    };

    let Some((id, out_rx)) = join(&to_server, name, token) else {
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
                if let Ok(msg) = bincode::deserialize::<ClientMsg>(&b) {
                    if to_server.send(ToServer::Msg { client: id, msg }).is_err() {
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
                    let Ok(bytes) = bincode::serialize(&msg) else {
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
    let _ = to_server.send(ToServer::Leave { client: id });
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
}

impl RateLimiter {
    fn new(now: Instant) -> Self {
        RateLimiter {
            window_start: now,
            cmd: 0,
            chat: 0,
            ping: 0,
            cursor: 0,
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

fn sim_loop(config: ServerConfig, rx: Receiver<ToServer>, shutdown: Arc<AtomicBool>) {
    let mut state = sim::new_game(config.seed, config.win_days);
    let mut clients: HashMap<u64, Sender<ServerMsg>> = HashMap::new();
    // Connection id (key of `clients`, the `client` field on `ToServer`) is
    // decoupled from player id: a connection is a socket, a player is a
    // persistent identity that can outlive one and survive a reconnect.
    let mut next_client_id: u64 = 1;
    let mut next_player_id: u64 = 1;
    // Session tokens must be unguessable: a sequential counter would let anyone
    // hijack a disconnected player by trying Hello{token: 1, 2, 3, ...}. Seed a
    // private SplitMix64 stream from wall-clock entropy (kept separate from the
    // deterministic game RNG in GameState) and draw a full 64-bit token each join.
    let mut token_rng = Rng::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15)
            ^ config.seed.rotate_left(32),
    );
    let mut player_of: HashMap<u64, u64> = HashMap::new(); // connection id -> player id
    let mut token_of: HashMap<u64, u64> = HashMap::new(); // connection id -> session token
    let mut sessions: HashMap<u64, PlayerInfo> = HashMap::new(); // session token -> saved info of a disconnected player
    let mut session_order: VecDeque<u64> = VecDeque::new(); // insertion order of `sessions`, for eviction
    let mut limiters: HashMap<u64, RateLimiter> = HashMap::new(); // connection id -> rate limiter
    let mut pending: Vec<(u64, PlayerCommand)> = Vec::new();
    let mut ever_joined = false;
    let mut printed_events: u64 = 0;
    let mut printed_chat: u64 = 0;
    let mut game_over_since: Option<Instant> = None;

    let tick_dur = Duration::from_millis(TICK_MS);
    let mut next_tick = Instant::now() + tick_dur;

    if config.verbose {
        if let Some(_) = config.port {
            println!(
                "[server] Frozen City dedicated server up (seed {}, survive {} days)",
                config.seed, config.win_days
            );
        }
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
                Ok(ToServer::Join { name, token, out, id_back }) => {
                    let client_id = next_client_id;
                    next_client_id += 1;

                    // A token that still points at a stashed, disconnected
                    // player's info is a reconnect: resume that identity
                    // (and its built/demolished stats) instead of joining
                    // fresh.
                    let reconnect = token.and_then(|t| sessions.remove(&t).map(|info| (t, info)));
                    let (player_id, session_token, reconnected) =
                        if let Some((t, saved)) = reconnect {
                            let player_id = saved.id;
                            session_order.retain(|&tok| tok != t);
                            sim::player_rejoined(&mut state, saved);
                            // Rotate the token on every reconnect: the old one
                            // was sent once in plaintext, so a sniffed token is
                            // dead the moment the real client reconnects.
                            (player_id, token_rng.next_u64(), true)
                        } else {
                            let player_id = next_player_id;
                            next_player_id += 1;
                            let session_token = token_rng.next_u64();
                            sim::player_joined(&mut state, player_id, &name);
                            (player_id, session_token, false)
                        };

                    player_of.insert(client_id, player_id);
                    token_of.insert(client_id, session_token);
                    limiters.insert(client_id, RateLimiter::new(Instant::now()));

                    let _ = out.send(ServerMsg::Welcome {
                        player_id,
                        token: session_token,
                        state: state.clone(),
                    });
                    clients.insert(client_id, out);
                    let _ = id_back.send(client_id);
                    ever_joined = true;
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
                        ClientMsg::Hello { .. } => false,
                        // Low-frequency owner-only admin actions: never rate
                        // limited (they're structurally rare — one click each
                        // — and gating them behind the cmd/chat/ping caps
                        // would let a flood on those channels starve a kick).
                        ClientMsg::SetGuestPermission { .. } | ClientMsg::Kick { .. } => true,
                    };
                    if allowed {
                        match msg {
                            ClientMsg::Cmd(cmd) => pending.push((pid, cmd)),
                            ClientMsg::Cursor { x, y } => sim::set_cursor(&mut state, pid, x, y),
                            ClientMsg::Chat { text } => sim::push_chat(&mut state, pid, &text),
                            ClientMsg::Ping { x, y } => sim::add_ping(&mut state, pid, x, y),
                            ClientMsg::Hello { .. } => {}
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

        let now = Instant::now();
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
                        state = sim::new_game(seed, config.win_days);
                        state.players = players;
                        state.guest_perm = guest_perm;
                        state.owner_id = owner_id;
                        printed_events = 0;
                        printed_chat = 0;
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

            // Broadcast the snapshot; tiles ride along every Nth tick only.
            let include_tiles = state.tick % TILES_EVERY_N_TICKS == 0;
            let mut wire = state.clone();
            if !include_tiles {
                wire.tiles = Vec::new();
            }
            let mut dead: Vec<u64> = Vec::new();
            for (id, out) in &clients {
                let msg = ServerMsg::State {
                    state: wire.clone(),
                    tiles_included: include_tiles,
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
    shutdown.store(true, Ordering::SeqCst);
}
