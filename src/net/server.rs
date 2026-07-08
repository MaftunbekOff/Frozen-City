//! Authoritative game server. Runs the simulation on its own thread at a fixed
//! 5 Hz tick and hands the local (in-process) player a plain channel pair —
//! one unified code path for singleplayer, host and join.
//!
//! A single TCP port speaks three protocols, told apart by the first bytes of
//! each connection: the native length-prefixed frame protocol, browser
//! WebSockets ("GET " + `Upgrade: websocket`), and plain HTTP GET for the
//! static web build (index.html + wasm) so a dedicated server is also the web
//! host.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use tungstenite::Message;

use crate::game::sim;
use crate::game::types::{GamePhase, PlayerCommand, TICK_MS};
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
    let name = match read_frame::<_, ClientMsg>(&mut stream) {
        Ok(ClientMsg::Hello { name }) => sanitize_name(&name),
        _ => return,
    };
    let _ = stream.set_read_timeout(None);

    let Some((id, out_rx)) = join(&to_server, name) else {
        return;
    };

    // Writer thread: serialize server messages onto the socket. When the
    // server drops this client's sender, shut the socket down so the blocking
    // reader below unblocks and cleans up.
    let Ok(write_stream) = stream.try_clone() else {
        let _ = to_server.send(ToServer::Leave { client: id });
        return;
    };
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
fn join(to_server: &Sender<ToServer>, name: String) -> Option<(u64, Receiver<ServerMsg>)> {
    let (out_tx, out_rx) = channel::<ServerMsg>();
    let (id_tx, id_rx) = channel::<u64>();
    to_server
        .send(ToServer::Join {
            name,
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
    let is_upgrade = text.to_ascii_lowercase().contains("upgrade: websocket");
    if is_upgrade {
        serve_websocket(stream, head, to_server);
    } else {
        serve_static(stream, &path);
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
    let name = loop {
        match ws.read() {
            Ok(Message::Binary(b)) => match bincode::deserialize::<ClientMsg>(&b) {
                Ok(ClientMsg::Hello { name }) => break sanitize_name(&name),
                _ => return,
            },
            Ok(Message::Ping(_) | Message::Pong(_)) => continue,
            _ => return,
        }
    };

    let Some((id, out_rx)) = join(&to_server, name) else {
        return;
    };
    let _ = ws
        .get_mut()
        .inner
        .set_read_timeout(Some(Duration::from_millis(20)));

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
fn serve_static(mut stream: TcpStream, path: &str) {
    let rel = path.split('?').next().unwrap_or("/");
    let rel = if rel == "/" { "/index.html" } else { rel };
    let candidate = PathBuf::from(WEB_ROOT).join(rel.trim_start_matches('/'));
    // No path traversal: every component must be a plain name (no "..", no
    // roots), which keeps the resolved path inside WEB_ROOT.
    let safe = candidate
        .components()
        .all(|c| matches!(c, Component::Normal(_)));
    let body = if safe {
        std::fs::read(&candidate).ok()
    } else {
        None
    };
    let response = match body {
        Some(body) => {
            let mime = content_type(&candidate);
            let mut r = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
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

fn sim_loop(config: ServerConfig, rx: Receiver<ToServer>, shutdown: Arc<AtomicBool>) {
    let mut state = sim::new_game(config.seed, config.win_days);
    let mut clients: HashMap<u64, Sender<ServerMsg>> = HashMap::new();
    let mut next_client_id: u64 = 1;
    let mut pending: Vec<(u64, PlayerCommand)> = Vec::new();
    let mut ever_joined = false;
    let mut printed_events: u64 = 0;
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
        // Drain control/client messages.
        loop {
            match rx.try_recv() {
                Ok(ToServer::Join { name, out, id_back }) => {
                    let id = next_client_id;
                    next_client_id += 1;
                    sim::player_joined(&mut state, id, &name);
                    let _ = out.send(ServerMsg::Welcome {
                        player_id: id,
                        state: state.clone(),
                    });
                    clients.insert(id, out);
                    let _ = id_back.send(id);
                    ever_joined = true;
                    if config.verbose {
                        println!("[server] {} connected (#{})", name, id);
                    }
                }
                Ok(ToServer::Msg { client, msg }) => match msg {
                    ClientMsg::Cmd(cmd) => pending.push((client, cmd)),
                    ClientMsg::Cursor { x, y } => sim::set_cursor(&mut state, client, x, y),
                    ClientMsg::Hello { .. } => {}
                },
                Ok(ToServer::Leave { client }) => {
                    if clients.remove(&client).is_some() {
                        sim::player_left(&mut state, client);
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
                        state = sim::new_game(seed, config.win_days);
                        state.players = players;
                        printed_events = 0;
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
                clients.remove(&id);
                sim::player_left(&mut state, id);
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
