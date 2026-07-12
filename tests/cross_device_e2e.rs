//! End-to-end test for V0.4's "cross-device" acceptance criterion (ROADMAP.md
//! V0.4 "Natija mezonlari": "Brauzerda boshlagan o'yinchi desktopdan o'sha
//! shahriga kiradi"): one account, signing in from what simulates two
//! different clients/devices in sequence (a fresh TCP connection each time,
//! no shared token — exactly what a browser session and a later desktop
//! session would look like), must land in the SAME persistent world and see
//! the same city, including across a full server restart. Same harness style
//! as `account_world_e2e.rs` / `central_world_e2e.rs`.
//!
//! Deliberately a single `#[test]`: it sets `FC_ACCOUNTS_DB` and
//! `FC_ACCOUNT_WORLDS_DIR`, process-wide env vars, and cargo runs `#[test]`
//! fns in one binary on separate threads by default — a second test setting
//! its own paths here would race this one (same reasoning as the existing
//! account/central e2e tests).

use std::time::{Duration, Instant};

use frozen_city::game::types::{BuildingKind, GameState, PlayerCommand};
use frozen_city::net::client::{self, ClientConn};
use frozen_city::net::protocol::{ClientMsg, ServerMsg};
use frozen_city::net::server::{self, ServerConfig};
use frozen_city::net::world_manager::WorldManager;

fn seed_account(db_path: &std::path::Path) {
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
    let hash = bcrypt::hash("pw-nomad", bcrypt::DEFAULT_COST).unwrap();
    conn.execute(
        "INSERT INTO accounts
            (telegram_id, telegram_username, display_username, first_name,
             last_name, birth_date, login, password_hash, created_at)
         VALUES (1, 'tguser', 'Nomad', 'Nomad', 'Karimov', '2000-01-01',
                 'fc555555', ?1, '2026-01-01T00:00:00')",
        [&hash],
    )
    .unwrap();
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

fn some_tent_spot(state: &GameState) -> (u8, u8) {
    (0..64u8)
        .flat_map(|y| (0..64u8).map(move |x| (x, y)))
        .find(|&(x, y)| state.can_place(BuildingKind::Tent, x, y).is_ok())
        .expect("some valid tent spot")
}

/// Every "device" dials in fresh, no token — exactly what a browser tab and
/// a later, unrelated desktop launch would each look like on the wire; only
/// the login/password (typed on each device) ties them to the same account.
fn login_msg() -> ClientMsg {
    ClientMsg::Login {
        login: "fc555555".to_string(),
        password: "pw-nomad".to_string(),
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
fn same_account_different_devices_shares_one_world() {
    let db_dir = std::env::temp_dir().join(format!("fc-cross-device-e2e-db-{}", std::process::id()));
    let worlds_dir =
        std::env::temp_dir().join(format!("fc-cross-device-e2e-worlds-{}", std::process::id()));
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
    seed_account(&db_path);

    let wm1 = WorldManager::new(7001, 12, false);
    let handle1 = start(7001, wm1.clone());
    let addr1 = addr_of(&handle1);

    // --- "Browser" (device #1): a fresh connection, logs in, builds. ---
    let device1 = client::connect_tcp_with(&addr1, login_msg()).expect("device 1 connects");
    let (player_id, state1) = recv_welcome(&device1);
    assert!(
        state1.buildings.iter().all(|b| b.kind != BuildingKind::Tent),
        "a brand new account world should start with just the Furnace: {:?}",
        state1.buildings
    );
    let spot = some_tent_spot(&state1);
    device1.send(ClientMsg::Cmd(PlayerCommand::Place {
        kind: BuildingKind::Tent,
        x: spot.0,
        y: spot.1,
    }));
    let after_build = wait_state(&device1, |s| {
        s.buildings
            .iter()
            .any(|b| b.kind == BuildingKind::Tent && b.owner == Some(player_id))
    });
    let tent_count_after_device1 = after_build.buildings.len();
    // Device #1 disconnects (closes the tab).
    drop(device1);

    // --- "Desktop" (device #2): a wholly independent fresh connection
    // (different socket, no token — simulating a different machine), same
    // login/password. Must land in the SAME world and see the SAME tent. ---
    let device2 = client::connect_tcp_with(&addr1, login_msg()).expect("device 2 connects");
    let (_player_id_2, state2) = recv_welcome(&device2);
    assert!(
        state2
            .buildings
            .iter()
            .any(|b| b.kind == BuildingKind::Tent),
        "device 2 must see the tent device 1 built: {:?}",
        state2.buildings
    );
    assert_eq!(
        state2.buildings.len(),
        tent_count_after_device1,
        "no duplication/reset across devices"
    );
    // Continuity is by ACCOUNT, not by a shared per-world player id: both
    // connections joined the account's one persistent world (proven above by
    // seeing the same building set), which is the cross-device contract.

    // Device #2 keeps building — a second tent — to prove the world is truly
    // live and shared, not a stale read-only snapshot.
    let spot2 = some_tent_spot(&state2);
    device2.send(ClientMsg::Cmd(PlayerCommand::Place {
        kind: BuildingKind::Tent,
        x: spot2.0,
        y: spot2.1,
    }));
    let after_second_build = wait_state(&device2, |s| {
        s.buildings.iter().filter(|b| b.kind == BuildingKind::Tent).count() == 2
    });
    drop(device2);

    // --- Full server restart: the SAME account, from a THIRD fresh
    // connection, must still see both tents (state persisted, not merely
    // shared in-memory between the two live connections above). ---
    handle1.stop();
    handle1.join();
    wm1.stop_all();
    wm1.join_all();

    let wm2 = WorldManager::new(9999, 12, false);
    let handle2 = start(9999, wm2.clone());
    let addr2 = addr_of(&handle2);

    let device3 = client::connect_tcp_with(&addr2, login_msg()).expect("device 3 connects post-restart");
    let (_player_id_3, state3) = recv_welcome(&device3);
    assert_eq!(
        state3.buildings.iter().filter(|b| b.kind == BuildingKind::Tent).count(),
        2,
        "both tents (built from two different 'devices') must survive a full server restart: {:?}",
        state3.buildings
    );
    assert_eq!(state3.buildings.len(), after_second_build.buildings.len());

    handle2.stop();
    handle2.join();
    wm2.stop_all();
    wm2.join_all();

    std::fs::remove_dir_all(&db_dir).ok();
    std::fs::remove_dir_all(&worlds_dir).ok();
    std::env::remove_var("FC_ACCOUNTS_DB");
    std::env::remove_var("FC_ACCOUNT_WORLDS_DIR");
}
