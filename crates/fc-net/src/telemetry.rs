//! Lightweight, non-blocking playtest telemetry (server-side, native only).
//!
//! Ends the "flying blind on players" problem. Until now the server kept NO
//! record of who plays, for how long, or how far they get — every "acceptance"
//! number came from bots, never a human. Two events per connection (a session
//! start, and an end that carries a progress snapshot) are enough to derive:
//!
//! - **DAU / concurrency** — distinct accounts per day, peak simultaneous
//!   players over time;
//! - **session length** — how long a sitting actually lasts;
//! - **drop-off day** — the in-game DAY a player quits on (the single most
//!   useful retention signal for a survival game: "everyone quits on day 3"
//!   tells you exactly where the fun dies);
//! - **the vision funnel** — how many accounts reach the Tunnel, graduate, and
//!   enter the Global World.
//!
//! Design mirrors [`crate::persist`] / [`crate::accounts`]: a process-wide
//! singleton configured by ONE env var, `FC_TELEMETRY_PATH`.
//!
//! - **Unset** (tests, singleplayer, `--host`) → a zero-cost no-op that opens
//!   no file. This is why nothing else in the codebase — `ServerConfig`,
//!   `server::start`, every test's config literal — had to change: telemetry
//!   is genuinely global, so it reads its own env instead of being threaded
//!   through twenty call sites.
//! - **Set** (production systemd units) → a single background thread owns the
//!   file; every `sim_loop` (the shared guest world, each per-account world,
//!   and the central world) just drops events into one unbounded channel and
//!   never blocks a tick on disk I/O — the same off-thread discipline the
//!   showcase reads use.
//!
//! One line of JSON per event (JSONL): append-only, self-describing, trivial
//! to analyse offline (`bot/analyze_telemetry.py`) or tail live.

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use fc_game::types::{GamePhase, GameState};

/// Which kind of world a session happened in — a top-level analysis dimension
/// (guest churn vs. account retention vs. hub traffic look nothing alike).
pub const WORLD_SHARED: &str = "shared_guest";
pub const WORLD_PERSONAL: &str = "personal";
pub const WORLD_CENTRAL: &str = "central";

/// One telemetry record. A tagged enum so each JSONL line is self-describing:
/// `{"event":"session_start",...}` / `{"event":"session_end",...}`. Fields are
/// intentionally flat and repeated across variants (no shared header object) so
/// the analyzer can read any line in isolation.
#[derive(Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    SessionStart {
        /// Unix seconds (wall clock). Server-side only, so `SystemTime` is fine
        /// — this never has to be deterministic like the sim's own clock.
        ts: u64,
        world: &'static str,
        /// Account id for a personal world, `-1` for the central world, `0` for
        /// the shared guest world — the stable key to group a world's sessions.
        world_key: i64,
        /// Per-world player id (not stable across worlds; `account` is the
        /// cross-world identity when present).
        player: u64,
        /// `None` for guests (no account needed to play).
        account: Option<i64>,
        name: String,
        /// True when this resumes a dropped session (reconnect) rather than a
        /// genuinely new sitting — so the analyzer can avoid double-counting.
        reconnect: bool,
    },
    SessionEnd {
        ts: u64,
        world: &'static str,
        world_key: i64,
        player: u64,
        account: Option<i64>,
        name: String,
        /// Wall-clock length of the sitting, seconds.
        duration_s: u64,
        // --- progress snapshot at the moment of leaving ---
        /// In-game day reached. THE drop-off signal for a survival game.
        day: u32,
        phase: &'static str,
        /// Finished the Tunnel (reached the Global World) — the funnel summit.
        graduated: bool,
        buildings: usize,
        population: usize,
        missions_done: usize,
        missions_total: usize,
        tunnel_stage: u8,
        /// Stockpile, floored to whole units (raw sim values are `f32`).
        wood: i64,
        coal: i64,
        food: i64,
    },
}

impl Event {
    /// A join: fresh sitting or a reconnect resuming a dropped one.
    pub fn session_start(
        world: &'static str,
        world_key: i64,
        player: u64,
        account: Option<i64>,
        name: String,
        reconnect: bool,
    ) -> Event {
        Event::SessionStart { ts: now_unix(), world, world_key, player, account, name, reconnect }
    }

    /// A departure, snapshotting how far this world had progressed. Read the
    /// snapshot straight off `state` so callers (the two `disconnect` sites)
    /// don't have to assemble a dozen fields by hand.
    pub fn session_end(
        world: &'static str,
        world_key: i64,
        player: u64,
        account: Option<i64>,
        name: String,
        duration_s: u64,
        state: &GameState,
    ) -> Event {
        Event::SessionEnd {
            ts: now_unix(),
            world,
            world_key,
            player,
            account,
            name,
            duration_s,
            day: state.day(),
            phase: phase_str(state.phase),
            graduated: state.graduated,
            buildings: state.buildings.len(),
            population: state.survivors.len(),
            missions_done: state.missions.iter().filter(|m| m.done).count(),
            missions_total: state.missions.len(),
            tunnel_stage: state.tunnel.stage,
            wood: state.stock.wood as i64,
            coal: state.stock.coal as i64,
            food: state.stock.food as i64,
        }
    }
}

fn phase_str(p: GamePhase) -> &'static str {
    match p {
        GamePhase::Running => "running",
        GamePhase::Won => "won",
        GamePhase::Lost => "lost",
    }
}

/// Current wall-clock time in Unix seconds (0 if the clock is before the epoch,
/// which can't happen in practice).
pub fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The process-wide sink, initialised once from `FC_TELEMETRY_PATH`. `None`
/// (env unset/empty, or the writer thread failed to spawn) means telemetry is
/// disabled and every `record` call is a cheap no-op.
fn sink() -> Option<&'static Sender<Event>> {
    static SINK: OnceLock<Option<Sender<Event>>> = OnceLock::new();
    SINK.get_or_init(init_sink).as_ref()
}

fn init_sink() -> Option<Sender<Event>> {
    let path = std::env::var("FC_TELEMETRY_PATH").ok().filter(|s| !s.is_empty())?;
    let (tx, rx) = channel::<Event>();
    match std::thread::Builder::new()
        .name("fc-telemetry".into())
        .spawn(move || writer_loop(&path, rx))
    {
        Ok(_) => Some(tx),
        Err(e) => {
            eprintln!("[telemetry] writer thread spawn failed: {e}; telemetry disabled");
            None
        }
    }
}

/// Owns the file for the process's lifetime, appending one JSON line per event
/// and flushing each (volume is a couple of events per connection, so per-line
/// durability costs nothing and survives a hard kill up to the last in-flight
/// event). Any I/O error is logged once and the loop keeps draining so senders
/// — the sim threads — never block or back up on a bad disk.
fn writer_loop(path: &str, rx: Receiver<Event>) {
    let mut file = match OpenOptions::new().create(true).append(true).open(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[telemetry] cannot open {path}: {e}; events dropped");
            for _ in rx {} // drain forever so `record` stays a cheap no-op
            return;
        }
    };
    let mut warned = false;
    for ev in rx {
        match serde_json::to_string(&ev) {
            Ok(mut line) => {
                line.push('\n');
                if let Err(e) = file.write_all(line.as_bytes()).and_then(|()| file.flush()) {
                    if !warned {
                        eprintln!("[telemetry] write to {path} failed: {e} (further errors silenced)");
                        warned = true;
                    }
                }
            }
            Err(e) => eprintln!("[telemetry] serialize failed: {e}"),
        }
    }
}

/// Fire-and-forget: hand an event to the writer thread. No-op when telemetry is
/// disabled, and never blocks a sim tick (unbounded channel, off-thread I/O).
pub fn record(ev: Event) {
    if let Some(tx) = sink() {
        let _ = tx.send(ev);
    }
}
