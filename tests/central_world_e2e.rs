//! End-to-end test for the central world (the Global World through the
//! Tunnel), over the real wire protocol: a graduated account's `EnterCentral`
//! migrates settlers out of its personal world into the one shared central
//! world; an ungraduated account is refused; re-entry doesn't duplicate the
//! group; ownership gates commands; and the central world survives a full
//! server restart. Same harness style as `account_world_e2e.rs`.
//!
//! Deliberately a single `#[test]`: it sets `FC_ACCOUNTS_DB` and
//! `FC_ACCOUNT_WORLDS_DIR`, process-wide env vars, and cargo runs `#[test]`
//! fns in one binary on separate threads by default.

use std::time::{Duration, Instant};

use frozen_city::game::sim;
use frozen_city::game::types::{
    BuildingKind, GameState, PlayerCommand, CENTRAL_MIGRANTS_PER_ACCOUNT,
};
use frozen_city::net::client::{self, ClientConn};
use frozen_city::net::persist;
use frozen_city::net::protocol::{ClientMsg, ServerMsg};
use frozen_city::net::server::{self, ServerConfig};
use frozen_city::net::world_manager::WorldManager;

fn seed_two_accounts(db_path: &std::path::Path) {
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
    // Ids 1 (Aziz, graduated below) and 2 (Vali, never graduated).
    for (telegram_id, name, login, pw) in [
        (1i64, "Aziz", "fc111111", "pw-aziz"),
        (2i64, "Vali", "fc222222", "pw-vali"),
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

fn recv_auth_failed(conn: &ClientConn) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match conn.recv_timeout(Duration::from_millis(500)) {
            Ok(ServerMsg::AuthFailed { reason }) => return reason,
            Ok(other) => panic!("expected AuthFailed, got {other:?}"),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            // The server closes the connection right after AuthFailed; if the
            // frame raced the close we may only observe the drop. Treat a
            // clean death before any Welcome as the refusal it is.
            Err(_) => return String::new(),
        }
    }
    panic!("no AuthFailed within 10s");
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

fn enter_central_msg(login: &str, password: &str) -> ClientMsg {
    ClientMsg::EnterCentral {
        login: login.to_string(),
        password: password.to_string(),
        token: None,
    }
}

fn login_msg(login: &str, password: &str) -> ClientMsg {
    ClientMsg::Login {
        login: login.to_string(),
        password: password.to_string(),
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
            central: false, // the shared guest world; WorldManager owns the central one
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
fn tunnel_migration_ownership_and_restart() {
    let db_dir = std::env::temp_dir().join(format!("fc-central-e2e-db-{}", std::process::id()));
    let worlds_dir = std::env::temp_dir().join(format!("fc-central-e2e-worlds-{}", std::process::id()));
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
    seed_two_accounts(&db_path);

    // Aziz's personal world exists on disk and has finished the Tunnel; the
    // server will load exactly this when his account world spawns. Use the
    // bootstrapped starting state (furnace already lit, full population)
    // since this test cares about a settled personal world with more than
    // a fresh leader-only opening, not the furnace-building intro itself.
    let mut aziz_world = sim::new_game_bootstrapped(777, 12);
    aziz_world.graduated = true;
    let aziz_start_pop = aziz_world.survivors.len();
    assert!(aziz_start_pop > CENTRAL_MIGRANTS_PER_ACCOUNT);
    persist::save_at(&aziz_world, worlds_dir.join("1.bin").to_str().unwrap()).unwrap();

    let wm1 = WorldManager::new(9001, 12, false);
    let handle1 = start(9001, wm1.clone());
    let addr1 = addr_of(&handle1);

    // --- An ungraduated account is refused at the Tunnel. ---
    let vali = client::connect_tcp_with(&addr1, enter_central_msg("fc222222", "pw-vali"))
        .expect("vali dials");
    let reason = recv_auth_failed(&vali);
    assert!(
        reason.is_empty() || reason.contains("Tunnel"),
        "refusal should say the Tunnel is unfinished: {reason:?}"
    );

    // --- A graduated account walks through with a settler group. ---
    let aziz = client::connect_tcp_with(&addr1, enter_central_msg("fc111111", "pw-aziz"))
        .expect("aziz dials");
    let (aziz_pid, central) = recv_welcome(&aziz);
    assert!(central.central, "EnterCentral must land in the central world");
    assert_eq!(
        central.owned_settlers(1),
        CENTRAL_MIGRANTS_PER_ACCOUNT,
        "first entry brings a full settler group: {:?}",
        central.survivors
    );
    assert_eq!(
        central.player(aziz_pid).and_then(|p| p.account),
        Some(1),
        "the central player must be tied to the account"
    );
    assert!(central.owner_id.is_none(), "the central world belongs to no one");

    // --- Ownership over the wire: build, then assign an owned settler. ---
    let spot = (0..64u8)
        .flat_map(|y| (0..64u8).map(move |x| (x, y)))
        .find(|&(x, y)| central.can_place(BuildingKind::Sawmill, x, y).is_ok())
        .expect("a sawmill spot in the central world");
    aziz.send(ClientMsg::Cmd(PlayerCommand::Place {
        kind: BuildingKind::Sawmill,
        x: spot.0,
        y: spot.1,
        facing: 0,
    }));
    let with_sawmill = wait_state(&aziz, |s| {
        s.buildings.iter().any(|b| b.kind == BuildingKind::Sawmill)
    });
    let sawmill = with_sawmill
        .buildings
        .iter()
        .find(|b| b.kind == BuildingKind::Sawmill)
        .unwrap()
        .id;
    let my_settler = with_sawmill
        .survivors
        .iter()
        .find(|s| s.owner == Some(1))
        .unwrap()
        .id;
    aziz.send(ClientMsg::Cmd(PlayerCommand::AssignSurvivor {
        survivor: my_settler,
        building: Some(sawmill),
    }));
    wait_state(&aziz, |s| {
        s.survivors
            .iter()
            .any(|x| x.id == my_settler && x.assigned_building == Some(sawmill))
    });

    // --- The personal world actually lost the migrated settlers. ---
    let aziz_home = client::connect_tcp_with(&addr1, login_msg("fc111111", "pw-aziz"))
        .expect("aziz logs into his personal world");
    let (_, home_state) = recv_welcome(&aziz_home);
    assert!(!home_state.central);
    assert_eq!(
        home_state.survivors.len(),
        aziz_start_pop - CENTRAL_MIGRANTS_PER_ACCOUNT,
        "migrated settlers must be gone from the personal world"
    );
    drop(aziz_home);

    // --- Re-entry tops up, never duplicates: the cap holds. ---
    drop(aziz);
    let aziz2 = client::connect_tcp_with(&addr1, enter_central_msg("fc111111", "pw-aziz"))
        .expect("aziz re-enters");
    let (_, central2) = recv_welcome(&aziz2);
    assert_eq!(
        central2.owned_settlers(1),
        CENTRAL_MIGRANTS_PER_ACCOUNT,
        "re-entering must not stack a second settler group"
    );
    drop(aziz2);

    // --- Full restart: the central world (settlers, building) persists. ---
    handle1.stop();
    handle1.join();
    wm1.stop_all();
    wm1.join_all();
    assert!(worlds_dir.join("central.bin").exists(), "central world has its own save");

    let wm2 = WorldManager::new(4242, 12, false);
    let handle2 = start(4242, wm2.clone());
    let addr2 = addr_of(&handle2);
    let aziz3 = client::connect_tcp_with(&addr2, enter_central_msg("fc111111", "pw-aziz"))
        .expect("aziz enters after restart");
    let (_, central3) = recv_welcome(&aziz3);
    assert!(central3.central);
    assert_eq!(
        central3.owned_settlers(1),
        CENTRAL_MIGRANTS_PER_ACCOUNT,
        "settlers must survive a server restart"
    );
    assert!(
        central3.buildings.iter().any(|b| b.kind == BuildingKind::Sawmill),
        "the central sawmill must survive a server restart"
    );

    handle2.stop();
    handle2.join();
    wm2.stop_all();
    wm2.join_all();

    std::fs::remove_dir_all(&db_dir).ok();
    std::fs::remove_dir_all(&worlds_dir).ok();
    std::env::remove_var("FC_ACCOUNTS_DB");
    std::env::remove_var("FC_ACCOUNT_WORLDS_DIR");
}
