//! Routes an authenticated login to that account's own persistent world,
//! spawning a fresh `sim_loop` thread (reused unmodified from `server.rs`)
//! on first use and evicting it once nobody's connected for a while — see
//! `ServerConfig::idle_shutdown`. Guest connections (`Hello`, no account)
//! never touch this module; they keep joining the single shared world exactly
//! as before.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::game::types::CENTRAL_MIGRANTS_PER_ACCOUNT;
use crate::net::protocol::ServerMsg;
use crate::net::server::{join, sim_loop, ServerConfig, ToServer};

/// Base directory for per-account world saves, one `{account_id}.bin` file
/// each. Overridable via `FC_ACCOUNT_WORLDS_DIR`, mainly so tests can point
/// at a throwaway directory instead of the real one.
pub const DEFAULT_WORLDS_DIR: &str = "/var/lib/frozen-city/accounts";

/// Map key for the one central world in `worlds`. Account rowids from SQLite
/// start at 1, so a negative key can never collide with a real account.
const CENTRAL_KEY: i64 = -1;

/// How long `enter_central` waits for a sim thread to answer a
/// CountOwned/ExtractMigrants query before giving up. Generous — a sim thread
/// answers between two 200 ms ticks — but bounded, so one wedged world thread
/// can't hang login connections forever (the entry lock serializes them).
const MIGRATE_REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// Why `enter_central` refused, mapped to a client-visible `AuthFailed`
/// reason by the caller (`server::route_first_msg`).
pub enum CentralError {
    /// The account's personal world hasn't finished the Tunnel (and the
    /// account has no settlers already living in the central world).
    NotGraduated,
    /// A needed world couldn't be spawned or didn't answer.
    Capacity,
}

/// Why `visit_friend` refused, mapped to a client-visible `AuthFailed`
/// reason by the caller.
pub enum VisitError {
    /// No standing invite from that host for this visitor (or it expired).
    NoInvite,
    /// The host isn't currently online in their personal world — the default
    /// policy is that a world without its owner admits no visitors.
    HostOffline,
    /// A needed world couldn't be spawned or didn't answer.
    Capacity,
}

/// How long an `Invite` keeps admitting the invited account's `VisitFriend`.
/// Long enough to comfortably switch worlds (and survive a reconnect or
/// two); short enough that an invite isn't a permanent key to the city.
const INVITE_TTL: Duration = Duration::from_secs(15 * 60);

/// Standing invites: (host account, visitor account) → when issued. Written
/// by the central world's sim thread (an `Invite` command), read by
/// `visit_friend` when the visitor dials in. Deliberately NOT persisted —
/// a server restart voiding pending invites is fine.
#[derive(Default)]
pub struct InviteBook {
    map: Mutex<HashMap<(i64, i64), std::time::Instant>>,
}

impl InviteBook {
    /// Record (or refresh) an invite from `host` to `visitor`.
    pub fn invite(&self, host: i64, visitor: i64) {
        let mut map = self.map.lock().unwrap();
        let now = std::time::Instant::now();
        // Opportunistic cleanup so the map never grows unbounded on a
        // long-running server full of never-used invites.
        map.retain(|_, at| now.duration_since(*at) < INVITE_TTL);
        map.insert((host, visitor), now);
    }

    /// Is there a fresh invite from `host` to `visitor`? Not consumed on
    /// read: the visitor may need it again for a reconnect a minute later.
    pub fn valid(&self, host: i64, visitor: i64) -> bool {
        let map = self.map.lock().unwrap();
        map.get(&(host, visitor))
            .is_some_and(|at| at.elapsed() < INVITE_TTL)
    }
}

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

fn worlds_dir() -> String {
    std::env::var("FC_ACCOUNT_WORLDS_DIR").unwrap_or_else(|_| DEFAULT_WORLDS_DIR.to_string())
}

fn account_save_path(account_id: i64) -> String {
    format!("{}/{account_id}.bin", worlds_dir())
}

fn central_save_path() -> String {
    format!("{}/central.bin", worlds_dir())
}

struct WorldHandle {
    tx: Sender<ToServer>,
    shutdown: Arc<AtomicBool>,
    thread: thread::JoinHandle<()>,
}

pub struct WorldManager {
    worlds: Mutex<HashMap<i64, WorldHandle>>,
    /// Serializes `enter_central`'s count→extract→inject handshake so two
    /// simultaneous entries by the same account can't both see "0 settlers"
    /// and migrate a double group past the per-account cap. Entries are a
    /// few channel round-trips (~ms); a global lock is fine at this scale.
    central_entry: Mutex<()>,
    /// Standing friend-world invites; written by the central world's sim
    /// thread, checked by `visit_friend`.
    invites: Arc<InviteBook>,
    seed: u64,
    win_days: u32,
    verbose: bool,
}

impl WorldManager {
    pub fn new(seed: u64, win_days: u32, verbose: bool) -> Arc<WorldManager> {
        Arc::new(WorldManager {
            worlds: Mutex::new(HashMap::new()),
            central_entry: Mutex::new(()),
            invites: Arc::new(InviteBook::default()),
            seed,
            win_days,
            verbose,
        })
    }

    /// Admits `visitor` into `host`'s personal world as a guest — only while
    /// a fresh invite stands and the host is online there (the default
    /// no-owner-no-entry policy). The world is never spawned FOR a visit: a
    /// not-running world means the host isn't in it.
    pub fn visit_friend(
        self: &Arc<Self>,
        host: i64,
        visitor: i64,
        name: String,
        token: Option<u64>,
    ) -> Result<(Sender<ToServer>, u64, Receiver<ServerMsg>), VisitError> {
        if !self.invites.valid(host, visitor) {
            return Err(VisitError::NoInvite);
        }
        let tx = {
            let worlds = self.worlds.lock().unwrap();
            match worlds.get(&host) {
                Some(handle) => handle.tx.clone(),
                None => return Err(VisitError::HostOffline),
            }
        };
        let (reply_tx, reply_rx) = channel();
        if tx
            .send(ToServer::OwnerOnline {
                owner: host,
                reply: reply_tx,
            })
            .is_err()
        {
            return Err(VisitError::HostOffline);
        }
        match reply_rx.recv_timeout(MIGRATE_REPLY_TIMEOUT) {
            Ok(true) => {}
            Ok(false) => return Err(VisitError::HostOffline),
            Err(_) => return Err(VisitError::Capacity),
        }
        let (id, out_rx) = join(&tx, name, token, Some(visitor)).ok_or(VisitError::Capacity)?;
        Ok((tx, id, out_rx))
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
        match join(&tx, name.clone(), token, Some(account_id)) {
            Some((id, out_rx)) => Some((tx, id, out_rx)),
            None => {
                self.worlds.lock().unwrap().remove(&account_id);
                let tx = self.get_or_spawn(account_id)?;
                let (id, out_rx) = join(&tx, name, token, Some(account_id))?;
                Some((tx, id, out_rx))
            }
        }
    }

    /// Admits `account_id` into the central world (the Global World through
    /// the Tunnel), migrating survivors out of its personal world on the way
    /// in until the account's settler group is at its cap — so a first entry
    /// brings a full group and later re-entries top it back up (or migrate
    /// nothing). Requires a graduated personal world unless the account
    /// already has settlers living centrally (graduation is spent the moment
    /// its settlers exist there).
    pub fn enter_central(
        self: &Arc<Self>,
        account_id: i64,
        name: String,
        token: Option<u64>,
    ) -> Result<(Sender<ToServer>, u64, Receiver<ServerMsg>), CentralError> {
        let _entry = self.central_entry.lock().unwrap();

        let central = self.get_or_spawn_central().ok_or(CentralError::Capacity)?;
        let owned = match self.ask_central_owned(&central, account_id) {
            Some(n) => n,
            // Stale handle (evicted/crashed between lookup and ask): drop it
            // and retry once against a fresh spawn, same pattern as
            // `join_account`'s stale-Sender fallback.
            None => {
                self.worlds.lock().unwrap().remove(&CENTRAL_KEY);
                let fresh = self.get_or_spawn_central().ok_or(CentralError::Capacity)?;
                self.ask_central_owned(&fresh, account_id)
                    .ok_or(CentralError::Capacity)?
            }
        };
        // Re-resolve after the possible respawn above.
        let central = self.get_or_spawn_central().ok_or(CentralError::Capacity)?;

        let want = CENTRAL_MIGRANTS_PER_ACCOUNT.saturating_sub(owned);
        if want > 0 {
            let personal = self.get_or_spawn(account_id).ok_or(CentralError::Capacity)?;
            let (reply_tx, reply_rx) = channel();
            if personal
                .send(ToServer::ExtractMigrants {
                    max: want,
                    reply: reply_tx,
                })
                .is_err()
            {
                return Err(CentralError::Capacity);
            }
            match reply_rx.recv_timeout(MIGRATE_REPLY_TIMEOUT) {
                // Not graduated: only fatal when the account has no settlers
                // centrally yet. With settlers already there (their personal
                // world reset after an earlier graduation), entry stands and
                // there's simply nothing eligible to migrate.
                Ok(None) => {
                    if owned == 0 {
                        return Err(CentralError::NotGraduated);
                    }
                }
                Ok(Some(migrants)) => {
                    if !migrants.is_empty()
                        && central
                            .send(ToServer::InjectMigrants {
                                account: account_id,
                                name: name.clone(),
                                survivors: migrants,
                            })
                            .is_err()
                    {
                        // Central died between the count and now; the migrants
                        // are already out of the personal world and would be
                        // lost silently. Give up loudly instead.
                        return Err(CentralError::Capacity);
                    }
                }
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {
                    return Err(CentralError::Capacity);
                }
            }
        }

        let (id, out_rx) =
            join(&central, name, token, Some(account_id)).ok_or(CentralError::Capacity)?;
        Ok((central, id, out_rx))
    }

    /// One CountOwned round-trip against the central world; `None` when the
    /// handle is stale (send or reply channel already closed) or the reply
    /// timed out.
    fn ask_central_owned(&self, central: &Sender<ToServer>, account_id: i64) -> Option<usize> {
        let (reply_tx, reply_rx) = channel();
        central
            .send(ToServer::CountOwned {
                account: account_id,
                reply: reply_tx,
            })
            .ok()?;
        reply_rx.recv_timeout(MIGRATE_REPLY_TIMEOUT).ok()
    }

    fn get_or_spawn(self: &Arc<Self>, account_id: i64) -> Option<Sender<ToServer>> {
        self.spawn_world(account_id, account_save_path(account_id), false)
    }

    fn get_or_spawn_central(self: &Arc<Self>) -> Option<Sender<ToServer>> {
        self.spawn_world(CENTRAL_KEY, central_save_path(), true)
    }

    fn spawn_world(
        self: &Arc<Self>,
        key: i64,
        save_path: String,
        central: bool,
    ) -> Option<Sender<ToServer>> {
        let mut worlds = self.worlds.lock().unwrap();
        if let Some(handle) = worlds.get(&key) {
            return Some(handle.tx.clone());
        }
        // The central world is exempt from the cap: it's the one shared
        // destination the cap exists to protect capacity FOR — refusing to
        // spawn it because 200 personal worlds are busy would lock everyone
        // out of the Global World exactly when the server is liveliest.
        if !central && worlds.len() >= MAX_ACCOUNT_WORLDS {
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
            save_path: Some(save_path),
            idle_shutdown: Some(IDLE_SHUTDOWN),
            central,
            // A personal world is owned by its account: only that account's
            // connections ever hold the Owner role there, no matter who
            // (an invited visitor) joins first. The central world has no owner.
            owner_account: (!central).then_some(key),
            // Only the central world issues invites (that's where people
            // meet); it needs the shared book to write them into.
            invites: central.then(|| self.invites.clone()),
        };
        let this = self.clone();
        let flag_for_thread = world_shutdown.clone();
        let thread = thread::Builder::new()
            .name(format!("fc-world-{key}"))
            .spawn(move || {
                sim_loop(config, rx, flag_for_thread);
                // Idle-eviction or a shutdown broadcast both end with
                // `sim_loop` returning here. Either way, drop the stale
                // entry so the next login (or `join_account`'s stale-Sender
                // fallback) respawns fresh from disk instead of reusing a
                // channel nobody is receiving on anymore.
                this.worlds.lock().unwrap().remove(&key);
            })
            .expect("spawn account world");
        worlds.insert(
            key,
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
