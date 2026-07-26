//! End-to-end test for V0.6's "full loop" acceptance criterion (ROADMAP.md
//! V0.6 "Natija mezonlari": "To'liq tsikl e2e testi: missiya -> Tunnel -> hub
//! -> taklif -> mehmon binoni quradi -> voqeada mehmon X qurdi ko'rinadi"),
//! over the real wire, plus V0.5's "Shaxsiy olam <-> hub o'tish < 5 soniya"
//! transition-time criterion measured along the way.
//!
//! The personal-world side is crafted to be "a few commands from graduating"
//! (mission-completion trick from `central_world_e2e.rs`'s save-crafting
//! pattern, taken one step further: instead of setting `graduated = true`
//! directly, the Tunnel is left one stage short with resources stocked so
//! `InvestTunnel` actually finishes it live) rather than grinding a full
//! playthrough — this keeps the test fast while still exercising the real
//! `InvestTunnel` -> graduation -> `EnterCentral` -> `Invite` -> `VisitFriend`
//! -> build -> attribution chain over the wire, unlike `central_world_e2e.rs`
//! (which starts already graduated) or `visit_e2e.rs` (which focuses on the
//! guest-permission matrix, not the transition itself).
//!
//! Deliberately a single `#[test]`: it sets `FC_ACCOUNTS_DB` and
//! `FC_ACCOUNT_WORLDS_DIR`, process-wide env vars, and cargo runs `#[test]`
//! fns in one binary on separate threads by default.

use std::time::{Duration, Instant};

use frozen_city::game::sim;
use frozen_city::game::types::{
    BuildingKind, GamePhase, GameState, PlayerCommand, TunnelState, TUNNEL_STAGES,
};
use frozen_city::net::client::{self, ClientConn};
use frozen_city::net::persist;
use frozen_city::net::protocol::{ClientMsg, ServerMsg};
use frozen_city::net::server::{self, ServerConfig};
use frozen_city::net::world_manager::WorldManager;

fn seed_accounts(db_path: &std::path::Path) {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE accounts (
            id INTEGER PRIMARY KEY,
            telegram_id INTEGER UNIQUE NOT NULL,
            telegram_username TEXT,
            display_username TEXT UNIQUE NOT NULL,
            first_name TEXT NOT NULL,
            last_name TEXT NOT NULL,
            birth_date TEXT NOT NULL,
            login TEXT UNIQUE NOT NULL,
            password_hash TEXT NOT NULL,
            created_at TEXT NOT NULL
        );",
    )
    .unwrap();
    // Id 1 = Traveler (near-graduated, finishes the Tunnel live below).
    // Id 2 = Buddy (already graduated, meets Traveler in central and later
    // visits Traveler's personal world).
    for (telegram_id, name, login, pw) in [
        (1i64, "Traveler", "fc400001", "pw-traveler"),
        (2i64, "Buddy", "fc400002", "pw-buddy"),
    ] {
        let hash = bcrypt::hash(pw, bcrypt::DEFAULT_COST).unwrap();
        conn.execute(
            "INSERT INTO accounts
                (telegram_id, telegram_username, display_username, first_name,
                 last_name, birth_date, login, password_hash, created_at)
             VALUES (?1, ?2, ?3, ?3, 'Karimov', '2000-01-01', ?4, ?5, '2026-01-01T00:00:00')",
            rusqlite::params![telegram_id, name, name, login, hash],
        )
        .unwrap();
    }
}

fn recv_welcome(conn: &ClientConn) -> (u64, GameState) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match conn.recv_timeout(Duration::from_millis(500)) {
            Ok(ServerMsg::Welcome { player_id, state, .. }) => return (player_id, state),
            Ok(other) => panic!("expected Welcome, got {other:?}"),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(e) => panic!("connection died before Welcome: {e:?}"),
        }
    }
    panic!("no Welcome within 10s");
}

fn wait_state(conn: &ClientConn, mut pred: impl FnMut(&GameState) -> bool) -> GameState {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match conn.recv_timeout(Duration::from_millis(500)) {
            Ok(ServerMsg::State { state, .. }) => {
                if pred(&state) {
                    return state;
                }
            }
            Ok(_) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(e) => panic!("connection died: {e:?}"),
        }
    }
    panic!("condition not met within 10s");
}

fn wait_invited(conn: &ClientConn) -> (i64, String) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match conn.recv_timeout(Duration::from_millis(500)) {
            Ok(ServerMsg::Invited { host, host_name }) => return (host, host_name),
            Ok(_) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(e) => panic!("connection died before Invited: {e:?}"),
        }
    }
    panic!("no Invited within 10s");
}

fn some_tent_spot(state: &GameState) -> (u8, u8) {
    (0..64u8)
        .flat_map(|y| (0..64u8).map(move |x| (x, y)))
        .find(|&(x, y)| state.can_place(BuildingKind::Tent, x, y).is_ok())
        .expect("some valid tent spot")
}

fn login_msg(login: &str, password: &str) -> ClientMsg {
    ClientMsg::Login {
        login: login.to_string(),
        password: password.to_string(),
        token: None,
    }
}

fn enter_central_msg(login: &str, password: &str) -> ClientMsg {
    ClientMsg::EnterCentral {
        login: login.to_string(),
        password: password.to_string(),
        token: None,
    }
}

fn visit_msg(login: &str, password: &str, host: i64) -> ClientMsg {
    ClientMsg::VisitFriend {
        login: login.to_string(),
        password: password.to_string(),
        host,
        token: None,
    }
}

fn start(seed: u64, wm: std::sync::Arc<WorldManager>) -> server::ServerHandle {
    server::start_with_accounts(
        ServerConfig {
            port: Some(0), // ephemeral port
            seed,
            win_days: 12,
            persistent: true,
            verbose: false,
            save_path: None,
            idle_shutdown: None,
            central: false,
            owner_account: None,
            invites: None,
            world_manager: None,
        },
        wm,
    )
    .expect("server starts")
}

fn addr_of(handle: &server::ServerHandle) -> String {
    format!("127.0.0.1:{}", handle.addr.expect("bound addr").port())
}

#[test]
fn missions_to_tunnel_to_hub_to_invite_to_guest_build() {
    let db_dir = std::env::temp_dir().join(format!("fc-full-cycle-e2e-db-{}", std::process::id()));
    let worlds_dir =
        std::env::temp_dir().join(format!("fc-full-cycle-e2e-worlds-{}", std::process::id()));
    std::fs::create_dir_all(&db_dir).unwrap();
    std::fs::remove_dir_all(&worlds_dir).ok();
    std::fs::create_dir_all(&worlds_dir).unwrap();
    let db_path = db_dir.join("accounts.db");
    // SAFETY: this test binary's only test function, so nothing else in the
    // process reads/writes these environment variables concurrently.
    unsafe {
        std::env::set_var("FC_ACCOUNTS_DB", &db_path);
        std::env::set_var("FC_ACCOUNT_WORLDS_DIR", &worlds_dir);
    }
    seed_accounts(&db_path);

    // Traveler's personal world: all missions done (Tunnel unlocked), one
    // stage short of graduating, with the resources stocked so a handful of
    // real `InvestTunnel` commands over the wire finish it — the "few
    // commands" crafting trick from `central_world_e2e.rs`, extended so the
    // graduation itself happens live instead of being preset.
    let mut traveler_world = sim::new_game(321, 12);
    for m in traveler_world.missions.iter_mut() {
        m.done = true;
    }
    traveler_world.tunnel = TunnelState {
        unlocked: true,
        stage: TUNNEL_STAGES - 1,
        progress: 0.0,
    };
    traveler_world.stock.wood = 1000.0;
    traveler_world.stock.coal = 1000.0;
    persist::save_at(&traveler_world, worlds_dir.join("1.bin").to_str().unwrap()).unwrap();

    // Buddy: already graduated (not the focus of this test; just needs to be
    // in central to receive Traveler's invite later).
    let mut buddy_world = sim::new_game(654, 12);
    buddy_world.graduated = true;
    persist::save_at(&buddy_world, worlds_dir.join("2.bin").to_str().unwrap()).unwrap();

    let wm = WorldManager::new(800, 12, false);
    let handle = start(800, wm.clone());
    let addr = addr_of(&handle);

    // --- Step 1: Traveler logs into their personal (near-graduated) world. ---
    let traveler = client::connect_tcp_with(&addr, login_msg("fc400001", "pw-traveler"))
        .expect("traveler connects");
    let (_traveler_pid, traveler_state) = recv_welcome(&traveler);
    assert!(!traveler_state.central);
    assert!(!traveler_state.graduated, "not graduated yet — that's the point");
    assert!(traveler_state.tunnel.unlocked);
    assert_eq!(traveler_state.tunnel.stage, TUNNEL_STAGES - 1);

    // --- Step 2: finish the Tunnel with real `InvestTunnel` commands
    // (TUNNEL_INVESTS_PER_STAGE contributions complete the final stage).
    // Keep sending InvestTunnel until graduated, bounded by a wall-clock
    // deadline (not just an attempt count) so a slow CI box doesn't flake. ---
    let mut graduated_state = traveler_state;
    let deadline = Instant::now() + Duration::from_secs(15);
    while !graduated_state.graduated && Instant::now() < deadline {
        traveler.send(ClientMsg::Cmd(PlayerCommand::InvestTunnel));
        graduated_state = wait_state(&traveler, |_| true);
    }
    assert!(
        graduated_state.graduated,
        "the Tunnel must finish via real InvestTunnel commands within the deadline"
    );
    assert!(
        graduated_state
            .events
            .iter()
            .any(|e| {
                e.text.contains("Tunnel")
                    || e.text.contains("Global World")
                    || e.text.contains("Victory")
            }),
        "graduation must be visible in the event feed: {:?}",
        graduated_state.events
    );
    drop(traveler);

    // --- Step 3: EnterCentral — measure the personal -> central transition
    // time (V0.5 criterion: < 5s). This covers dial + auth + migrate +
    // Welcome, i.e. everything a client actually waits on when the player
    // clicks "Enter the Global World". ---
    let transition_start = Instant::now();
    let traveler_central = client::connect_tcp_with(&addr, enter_central_msg("fc400001", "pw-traveler"))
        .expect("traveler enters central");
    let (_traveler_central_pid, central_state) = recv_welcome(&traveler_central);
    let transition_elapsed = transition_start.elapsed();
    assert!(central_state.central, "must land in the central world");
    assert!(
        transition_elapsed < Duration::from_secs(5),
        "personal -> central transition took {transition_elapsed:?}, must be < 5s (V0.5 criterion)"
    );
    println!(
        "[full_cycle_e2e] personal->central transition: {:?}",
        transition_elapsed
    );

    // --- Step 4: Buddy joins central too, Traveler invites them. ---
    let buddy_central = client::connect_tcp_with(&addr, enter_central_msg("fc400002", "pw-buddy"))
        .expect("buddy enters central");
    let (_buddy_central_pid, buddy_central_state) = recv_welcome(&buddy_central);
    assert!(
        buddy_central_state.central,
        "buddy's EnterCentral must land in the central world; roster: {:?}",
        buddy_central_state.players
    );
    // Wait until Traveler's view of central shows Buddy present, with a
    // roster dump on failure (diagnosing WHICH world each ended up in).
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last_roster: Vec<(u64, Option<i64>)> = Vec::new();
    let mut saw_buddy = false;
    while Instant::now() < deadline {
        match traveler_central.recv_timeout(Duration::from_millis(500)) {
            Ok(ServerMsg::State { state, .. }) => {
                last_roster = state.players.iter().map(|p| (p.id, p.account)).collect();
                if state.players.iter().any(|p| p.account == Some(2)) {
                    saw_buddy = true;
                    break;
                }
            }
            Ok(_) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(e) => panic!("traveler's central connection died: {e:?}"),
        }
    }
    assert!(
        saw_buddy,
        "traveler never saw buddy (account 2) join central; traveler's last roster: {last_roster:?}; \
         buddy's welcome roster: {:?}",
        buddy_central_state
            .players
            .iter()
            .map(|p| (p.id, p.account))
            .collect::<Vec<_>>()
    );
    traveler_central.send(ClientMsg::Invite { account: 2 });
    let (invited_host, invited_host_name) = wait_invited(&buddy_central);
    assert_eq!(invited_host, 1);
    assert_eq!(invited_host_name, "Traveler");
    drop(buddy_central);
    drop(traveler_central);

    // --- Step 5: Traveler goes home (must be online there for the guest
    // path's default owner-present policy), Buddy visits. ---
    let traveler_home = client::connect_tcp_with(&addr, login_msg("fc400001", "pw-traveler"))
        .expect("traveler goes home");
    let (traveler_home_pid, traveler_home_state) = recv_welcome(&traveler_home);
    assert!(!traveler_home_state.central);
    assert!(traveler_home_state.graduated);

    // Finishing the Tunnel LIVE left this world in `GamePhase::Won` (the
    // graduation victory), and `sim::apply_command` freezes ALL commands
    // while the phase isn't `Running` — so a guest arriving right now could
    // not build anything. On a persistent server the world auto-resets to a
    // fresh map after `WORLD_RESET_AFTER` (45s), keeping connections and
    // (per the roadmap) the permanent `graduated` flag. That freeze window
    // is real product behavior for the full journey, so the test waits it
    // out (bounded) rather than papering over it — see the quirk note in
    // the report: the crafted-graduated-save tests (`central_world_e2e`,
    // `visit_e2e`) never hit this because they leave phase = Running.
    let reset_deadline = Instant::now() + Duration::from_secs(70);
    let mut home_running = traveler_home_state.phase == GamePhase::Running;
    while !home_running && Instant::now() < reset_deadline {
        match traveler_home.recv_timeout(Duration::from_millis(500)) {
            Ok(ServerMsg::State { state, .. }) => {
                home_running = state.phase == GamePhase::Running;
            }
            Ok(_) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(e) => panic!("home connection died awaiting the post-graduation reset: {e:?}"),
        }
    }
    assert!(
        home_running,
        "the personal world must auto-reset to Running within ~70s of graduating \
         (WORLD_RESET_AFTER is 45s) so guests can actually build"
    );

    let buddy_visit = client::connect_tcp_with(&addr, visit_msg("fc400002", "pw-buddy", 1))
        .expect("buddy visits traveler's personal world");
    let (buddy_pid, buddy_visit_state) = recv_welcome(&buddy_visit);
    assert!(buddy_visit_state.is_owner(traveler_home_pid));
    assert!(!buddy_visit_state.is_owner(buddy_pid));

    // --- Step 6: guest builds; attribution shows up in the event feed. ---
    let spot = some_tent_spot(&buddy_visit_state);
    buddy_visit.send(ClientMsg::Cmd(PlayerCommand::Place {
        kind: BuildingKind::Tent,
        x: spot.0,
        y: spot.1,
        facing: 0,
    }));
    let with_guest_tent = wait_state(&traveler_home, |s| {
        s.buildings
            .iter()
            .any(|b| b.kind == BuildingKind::Tent && b.owner == Some(buddy_pid))
    });
    assert!(
        with_guest_tent
            .events
            .iter()
            .any(|e| e.text.contains("Buddy") && e.text.contains("building")),
        "the guest's build must show up attributed to their name in the event feed \
         (V0.8 wording: '<name> started building a <kind>.'): {:?}",
        with_guest_tent.events
    );

    handle.stop();
    handle.join();
    wm.stop_all();
    wm.join_all();

    std::fs::remove_dir_all(&db_dir).ok();
    std::fs::remove_dir_all(&worlds_dir).ok();
    std::env::remove_var("FC_ACCOUNTS_DB");
    std::env::remove_var("FC_ACCOUNT_WORLDS_DIR");
}
