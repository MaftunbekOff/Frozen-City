//! Authoritative game server. Runs the simulation on its own thread at a fixed
//! 5 Hz tick, accepts TCP clients, and hands the local (in-process) player a
//! plain channel pair — one unified code path for singleplayer, host and join.

use std::collections::HashMap;
use std::io;
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::game::types::{GameState, PlayerCommand, TICK_MS};
use crate::game::sim;
use crate::net::client::ClientConn;
use crate::net::protocol::{read_frame, write_frame, ClientMsg, ServerMsg};

/// How often full tile data rides along with the snapshot (every Nth tick).
const TILES_EVERY_N_TICKS: u64 = 5;

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
    ClientConn {
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

fn handle_socket(mut stream: TcpStream, to_server: Sender<ToServer>) {
    let _ = stream.set_nodelay(true);
    // The very first frame must be Hello; give the client 10 s for it.
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let name = match read_frame::<_, ClientMsg>(&mut stream) {
        Ok(ClientMsg::Hello { name }) => sanitize_name(&name),
        _ => return,
    };
    let _ = stream.set_read_timeout(None);

    let (out_tx, out_rx) = channel::<ServerMsg>();
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
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break 'outer,
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
