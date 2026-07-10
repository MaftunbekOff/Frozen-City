//! Routes an authenticated login to that account's own persistent world,
//! spawning a fresh `sim_loop` thread (reused unmodified from `server.rs`)
//! on first use and evicting it once nobody's connected for a while — see
//! `ServerConfig::idle_shutdown`. Guest connections (`Hello`, no account)
//! never touch this module; they keep joining the single shared world exactly
//! as before.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::net::protocol::ServerMsg;
use crate::net::server::{join, sim_loop, ServerConfig, ToServer};

/// Base directory for per-account world saves, one `{account_id}.bin` file
/// each. Overridable via `FC_ACCOUNT_WORLDS_DIR`, mainly so tests can point
/// at a throwaway directory instead of the real one.
pub const DEFAULT_WORLDS_DIR: &str = "/var/lib/frozen-city/accounts";

/// How long an account's world keeps running with nobody connected before it
/// saves and its thread exits. Long enough to comfortably outlast a
/// reconnect (page reload, brief network blip) without keeping every
/// account's sim ticking (and cloning its state every tick, see
/// `sim_loop`'s broadcast) forever after they've logged off.
const IDLE_SHUTDOWN: Duration = Duration::from_secs(300);

/// Hard cap on simultaneously spawned per-account worlds — mirrors
/// `server::MAX_CONNECTIONS`'s per-process budget, bounding worst-case
/// thread/memory use regardless of how many accounts exist.
const MAX_ACCOUNT_WORLDS: usize = 200;

fn account_save_path(account_id: i64) -> String {
    let dir =
        std::env::var("FC_ACCOUNT_WORLDS_DIR").unwrap_or_else(|_| DEFAULT_WORLDS_DIR.to_string());
    format!("{dir}/{account_id}.bin")
}

struct WorldHandle {
    tx: Sender<ToServer>,
    shutdown: Arc<AtomicBool>,
    thread: thread::JoinHandle<()>,
}

pub struct WorldManager {
    worlds: Mutex<HashMap<i64, WorldHandle>>,
    seed: u64,
    win_days: u32,
    verbose: bool,
}

impl WorldManager {
    pub fn new(seed: u64, win_days: u32, verbose: bool) -> Arc<WorldManager> {
        Arc::new(WorldManager {
            worlds: Mutex::new(HashMap::new()),
            seed,
            win_days,
            verbose,
        })
    }

    /// Joins `account_id`'s world as `name`, spawning it on first use.
    /// Returns the world's own `Sender` alongside the usual `join` result so
    /// the caller can keep sending this connection's later messages to the
    /// same world (not the shared one). `None` only when the server is at
    /// `MAX_ACCOUNT_WORLDS` and this account has no world already running.
    pub fn join_account(
        self: &Arc<Self>,
        account_id: i64,
        name: String,
        token: Option<u64>,
    ) -> Option<(Sender<ToServer>, u64, Receiver<ServerMsg>)> {
        let tx = self.get_or_spawn(account_id)?;
        // The world we just looked up may have idle-evicted itself between
        // us reading the map and this send landing — its receiver would
        // then already be dropped, and `join` fails. Treat that as a stale
        // entry: drop it and spawn a fresh one instead of failing the
        // connection outright.
        match join(&tx, name.clone(), token) {
            Some((id, out_rx)) => Some((tx, id, out_rx)),
            None => {
                self.worlds.lock().unwrap().remove(&account_id);
                let tx = self.get_or_spawn(account_id)?;
                let (id, out_rx) = join(&tx, name, token)?;
                Some((tx, id, out_rx))
            }
        }
    }

    fn get_or_spawn(self: &Arc<Self>, account_id: i64) -> Option<Sender<ToServer>> {
        let mut worlds = self.worlds.lock().unwrap();
        if let Some(handle) = worlds.get(&account_id) {
            return Some(handle.tx.clone());
        }
        if worlds.len() >= MAX_ACCOUNT_WORLDS {
            return None;
        }
        let (tx, rx) = channel::<ToServer>();
        // Each account world gets its own private shutdown flag — never the
        // process-wide one `server::start` hands the shared world. `sim_loop`
        // stores `true` into its own flag right before returning (its
        // "I'm done" signal); sharing one flag across independently-evicting
        // worlds would let the first idle-evicted world stop every other
        // world too, including the always-on shared one.
        let world_shutdown = Arc::new(AtomicBool::new(false));
        let config = ServerConfig {
            port: None,
            seed: self.seed,
            win_days: self.win_days,
            persistent: true,
            verbose: self.verbose,
            save_path: Some(account_save_path(account_id)),
            idle_shutdown: Some(IDLE_SHUTDOWN),
        };
        let this = self.clone();
        let flag_for_thread = world_shutdown.clone();
        let thread = thread::Builder::new()
            .name(format!("fc-world-{account_id}"))
            .spawn(move || {
                sim_loop(config, rx, flag_for_thread);
                // Idle-eviction or a shutdown broadcast both end with
                // `sim_loop` returning here. Either way, drop the stale
                // entry so the next login (or `join_account`'s stale-Sender
                // fallback) respawns fresh from disk instead of reusing a
                // channel nobody is receiving on anymore.
                this.worlds.lock().unwrap().remove(&account_id);
            })
            .expect("spawn account world");
        worlds.insert(
            account_id,
            WorldHandle {
                tx: tx.clone(),
                shutdown: world_shutdown,
                thread,
            },
        );
        Some(tx)
    }

    /// Signals every currently-running account world to save and exit.
    /// Non-blocking (mirrors `ServerHandle::stop`) — call `join_all` after to
    /// actually wait for them, e.g. from a SIGTERM handler followed by the
    /// same shutdown sequence the single shared world already uses.
    pub fn stop_all(&self) {
        let worlds = self.worlds.lock().unwrap();
        for handle in worlds.values() {
            handle.shutdown.store(true, Ordering::SeqCst);
        }
    }

    /// Blocks until every account world thread that was running at call time
    /// has exited (and, since each is `persistent`, written its final save).
    /// Must run to completion before the process exits — an un-joined thread
    /// is simply killed the moment `main()` returns, mid-save or not.
    pub fn join_all(&self) {
        let handles: Vec<WorldHandle> = self.worlds.lock().unwrap().drain().map(|(_, h)| h).collect();
        for handle in handles {
            let _ = handle.thread.join();
        }
    }
}
