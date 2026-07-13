use super::*;

use std::io;
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use fc_game::types::Survivor;

use crate::client::ClientConn;
use crate::protocol::{ClientMsg, ServerMsg};
use crate::world_manager::InviteBook;

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

/// Decrements the shared active-connection counter when a handler thread
/// exits, on every exit path (normal return or panic unwind).
pub(crate) struct ConnGuard(pub(crate) Arc<AtomicUsize>);

impl Drop for ConnGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
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
