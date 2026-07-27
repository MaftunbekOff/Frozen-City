//! Routes an authenticated login to that account's own persistent world,
//! spawning a fresh `sim_loop` thread (reused unmodified from `server.rs`)
//! on first use and keeping it ticking in the background — production,
//! survival needs, weather, births and deaths — long after the owner
//! disconnects, only evicting it after a long, mostly-just-a-backstop
//! stretch of nobody connecting at all — see `ServerConfig::idle_shutdown`.
//! Guest connections (`Hello`, no account) never touch this module; they
//! keep joining the single shared world exactly as before.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use fc_game::types::CENTRAL_MIGRANTS_PER_ACCOUNT;
use crate::protocol::ServerMsg;
use crate::server::{join, sim_loop, ServerConfig, ToServer};

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
/// saves and its thread exits. Deliberately long, not short: the colony is
/// meant to keep living — production, hunger/thirst, weather, births and
/// deaths all keep ticking — while its owner is away, the same way the
/// single shared world always has, rather than freezing the moment the last
/// client disconnects and only resuming on the next login. A short timeout
/// here (this used to be 5 minutes) is exactly what made the world pause.
/// Not `None` (never evict): an account that never logs back in again should
/// eventually stop costing a thread, so this is a long-but-finite backstop
/// rather than a true "always on" guarantee — 200 accounts idle forever
/// would otherwise mean 200 permanent threads for good.
const IDLE_SHUTDOWN: Duration = Duration::from_secs(30 * 24 * 3600);

/// Hard cap on simultaneously spawned per-account worlds — mirrors
/// `server::MAX_CONNECTIONS`'s per-process budget, bounding worst-case
/// thread/memory use regardless of how many accounts exist.
const MAX_ACCOUNT_WORLDS: usize = 200;

fn worlds_dir() -> String {
    std::env::var("FC_ACCOUNT_WORLDS_DIR").unwrap_or_else(|_| DEFAULT_WORLDS_DIR.to_string())
}

/// `pub(crate)` so `server.rs` can read a friend's personal-world save
/// on-demand for `ServerMsg::Showcase`, without spawning (or otherwise
/// disturbing) that account's world — a plain read of whatever's on disk.
pub(crate) fn account_save_path(account_id: i64) -> String {
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

    /// Admits `visitor` into `host`'s personal world as a guest — a fresh
    /// invite must always stand. Beyond that, the default policy is
    /// no-owner-no-entry (a world without its owner currently connected
    /// admits nobody, and isn't spawned just to be visited); but if `host`
    /// has opted into `allow_offline_guests` (V0.6 "owner-offline entry",
    /// `accounts::visit_policy`), a visit is admitted regardless — lazily
    /// spawning the world if it isn't already running. Owner-role pinning
    /// (`ServerConfig::owner_account`, checked on every `Join`) still applies
    /// either way, so an offline-admitted visitor can never seize ownership.
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
        let allow_offline = crate::accounts::visit_policy(host);
        let existing = {
            let worlds = self.worlds.lock().unwrap();
            worlds.get(&host).map(|h| h.tx.clone())
        };
        let tx = match existing {
            Some(tx) => {
                let (reply_tx, reply_rx) = channel();
                if tx
                    .send(ToServer::OwnerOnline { owner: host, reply: reply_tx })
                    .is_err()
                {
                    // Stale handle (evicted between lookup and send): treat
                    // exactly like "not currently spawned" below.
                    None
                } else {
                    match reply_rx.recv_timeout(MIGRATE_REPLY_TIMEOUT) {
                        Ok(true) => Some(tx),
                        Ok(false) if allow_offline => Some(tx),
                        Ok(false) => return Err(VisitError::HostOffline),
                        Err(_) => return Err(VisitError::Capacity),
                    }
                }
            }
            None => None,
        };
        let tx = match tx {
            Some(tx) => tx,
            None if allow_offline => {
                // Not currently spawned at all: lazily spawn it (from disk,
                // same path `join_account` would use) so an offline-admitting
                // host's world is still visitable.
                self.get_or_spawn(host).ok_or(VisitError::Capacity)?
            }
            None => return Err(VisitError::HostOffline),
        };
        let (id, out_rx) = join(&tx, name, token, Some(visitor)).ok_or(VisitError::Capacity)?;
        Ok((tx, id, out_rx))
    }

    /// Pushes `msg` to `account`'s PERSONAL world, if it's currently spawned
    /// (i.e. some connection is or recently was active there) and that
    /// account is actually connected to it right now — a no-op otherwise
    /// (never spawns a world just to deliver into it: a personal world only
    /// exists here while someone's using it). Used for cross-world delivery
    /// (`ServerMsg::Invited` reaching a target's own world, not just one
    /// they happen to be in centrally at the moment) — see
    /// `ToServer::DeliverServerMsg`.
    pub fn deliver_to_account(&self, account: i64, msg: ServerMsg) {
        let tx = {
            let worlds = self.worlds.lock().unwrap();
            worlds.get(&account).map(|h| h.tx.clone())
        };
        if let Some(tx) = tx {
            let _ = tx.send(ToServer::DeliverServerMsg { account, msg: Box::new(msg) });
        }
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

    /// V0.18 — the way BACK (`ClientMsg::ReturnHome`): brings `account_id`'s
    /// settlers out of the central world, settles them in that account's own
    /// personal world, and joins it. The exact mirror of [`Self::enter_central`],
    /// and deliberately forgiving where that one is strict:
    ///
    /// * No graduation check. Having settlers centrally IS the proof, and an
    ///   account with none there is simply going home alone — which is what
    ///   joining your own world already means, so it can never fail for that.
    /// * A central world that isn't running, or answers nothing, is not fatal
    ///   either: there is then nobody to bring back, and the join proceeds.
    ///   The one thing that must never happen is settlers being extracted and
    ///   then lost because the personal world couldn't take them — so the
    ///   extraction only happens once that world is confirmed spawned, and
    ///   anyone who doesn't fit (`MAX_POPULATION`, or the personal world
    ///   currently sitting Won/Lost — see `sim::inject_returnees`) is put
    ///   straight back.
    pub fn return_home(
        self: &Arc<Self>,
        account_id: i64,
        name: String,
        token: Option<u64>,
    ) -> Option<(Sender<ToServer>, u64, Receiver<ServerMsg>)> {
        // Serialized against `enter_central` on the same lock: an account
        // must never be migrating both ways at once.
        let _entry = self.central_entry.lock().unwrap();
        let personal = self.get_or_spawn(account_id)?;
        // Only touch the central world if it's ALREADY running — spawning the
        // shared world just to check whether someone left people there would
        // be a lot of work to discover an empty list.
        let central = {
            let worlds = self.worlds.lock().unwrap();
            worlds.get(&CENTRAL_KEY).map(|h| h.tx.clone())
        };
        if let Some(central) = central {
            let (reply_tx, reply_rx) = channel();
            if central
                .send(ToServer::ExtractSettlers { account: account_id, reply: reply_tx })
                .is_ok()
            {
                if let Ok(settlers) = reply_rx.recv_timeout(MIGRATE_REPLY_TIMEOUT) {
                    if !settlers.is_empty() {
                        let sent = settlers.len();
                        let (n_tx, n_rx) = channel();
                        let settlers_for_home = settlers.clone();
                        if personal
                            .send(ToServer::InjectReturnees {
                                survivors: settlers_for_home,
                                reply: n_tx,
                            })
                            .is_ok()
                        {
                            let settled = n_rx.recv_timeout(MIGRATE_REPLY_TIMEOUT).unwrap_or(0);
                            // Anyone the home colony had no room for goes
                            // straight back to the shared city rather than
                            // vanishing between two worlds.
                            if settled < sent {
                                let leftover: Vec<_> = settlers.into_iter().skip(settled).collect();
                                let _ = central.send(ToServer::InjectMigrants {
                                    account: account_id,
                                    name: name.clone(),
                                    survivors: leftover,
                                });
                            }
                        } else {
                            // The personal world died between spawn and send:
                            // put everyone back where they came from.
                            let _ = central.send(ToServer::InjectMigrants {
                                account: account_id,
                                name: name.clone(),
                                survivors: settlers,
                            });
                        }
                    }
                }
            }
        }
        match join(&personal, name.clone(), token, Some(account_id)) {
            Some((id, out_rx)) => Some((personal, id, out_rx)),
            None => {
                // Same stale-handle fallback `join_account` uses.
                self.worlds.lock().unwrap().remove(&account_id);
                let tx = self.get_or_spawn(account_id)?;
                let (id, out_rx) = join(&tx, name, token, Some(account_id))?;
                Some((tx, id, out_rx))
            }
        }
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
            // Only the central world needs to reach OUT to another world
            // (delivering `Invited` to the target's personal world); a
            // personal world never originates cross-world pushes itself.
            world_manager: central.then(|| self.clone()),
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
