//! End-to-end test for `WorldManager`: two different accounts logging into a
//! `start_with_accounts` server land in two separate, isolated worlds (not
//! the single shared one `Hello`/`start` uses), and each account's world
//! survives a shutdown+restart at its own save file — same harness style as
//! `persist_e2e.rs` (disk round trip) and `account_login_e2e.rs` (Login
//! wire flow), exercising both together.
//!
//! Deliberately a single `#[test]`: it sets `FC_ACCOUNTS_DB` and
//! `FC_ACCOUNT_WORLDS_DIR`, process-wide env vars `net::accounts`/
//! `net::world_manager` read, and cargo runs `#[test]` fns in one binary on
//! separate threads by default — a second test setting its own paths here
//! would race this one (same reasoning as the two files above).

use std::time::{Duration, Instant};

use frozen_city::game::types::{BuildingKind, GameState, PlayerCommand};
use frozen_city::net::client::{self, ClientConn};
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
    // Inserted in this order with no explicit `id`, so SQLite's rowid
    // assigns them 1 and 2 in turn — `account_save_path` in
    // `world_manager.rs` keys per-account files on exactly this id.
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
            central: false,
            owner_account: None,
            invites: None,
        },
        wm,
    )
    .expect("server starts")
}

fn addr_of(handle: &server::ServerHandle) -> String {
    format!("127.0.0.1:{}", handle.addr.expect("bound addr").port())
}

#[test]
fn each_account_gets_its_own_isolated_and_persistent_world() {
    let db_dir = std::env::temp_dir().join(format!("fc-account-world-e2e-db-{}", std::process::id()));
    let worlds_dir =
        std::env::temp_dir().join(format!("fc-account-world-e2e-worlds-{}", std::process::id()));
    std::fs::create_dir_all(&db_dir).unwrap();
    std::fs::remove_dir_all(&worlds_dir).ok();
    let db_path = db_dir.join("accounts.db");
    // SAFETY: this test binary's only test function, so nothing else in the
    // process reads/writes these environment variables concurrently.
    unsafe {
        std::env::set_var("FC_ACCOUNTS_DB", &db_path);
        std::env::set_var("FC_ACCOUNT_WORLDS_DIR", &worlds_dir);
    }
    seed_two_accounts(&db_path);

    let wm1 = WorldManager::new(9001, 12, false);
    let handle1 = start(9001, wm1.clone());
    let addr1 = addr_of(&handle1);

    let aziz = client::connect_tcp_with(&addr1, login_msg("fc111111", "pw-aziz")).expect("aziz connects");
    let (aziz_id, aziz_state) = recv_welcome(&aziz);
    assert!(
        aziz_state.buildings.iter().all(|b| b.kind != BuildingKind::Tent),
        "Aziz's world should start fresh (just the starting Furnace, no Tent): {:?}",
        aziz_state.buildings
    );
    let spot = some_tent_spot(&aziz_state);
    aziz.send(ClientMsg::Cmd(PlayerCommand::Place {
        kind: BuildingKind::Tent,
        x: spot.0,
        y: spot.1,
    }));
    wait_state(&aziz, |s| {
        s.buildings
            .iter()
            .any(|b| b.kind == BuildingKind::Tent && b.owner == Some(aziz_id))
    });

    // A second account logging into the same listener must NOT land in
    // Aziz's world — this is the whole point of `WorldManager`.
    let vali = client::connect_tcp_with(&addr1, login_msg("fc222222", "pw-vali")).expect("vali connects");
    let (_vali_id, vali_state) = recv_welcome(&vali);
    assert!(
        vali_state.buildings.iter().all(|b| b.kind != BuildingKind::Tent),
        "Vali should see their own world (no Tent), not Aziz's tent: {:?}",
        vali_state.buildings
    );

    // A real shutdown, same path the dedicated server's SIGTERM handler
    // takes for both the (unused here) shared world and every account world.
    handle1.stop();
    handle1.join();
    wm1.stop_all();
    wm1.join_all();

    let aziz_save = worlds_dir.join("1.bin");
    let vali_save = worlds_dir.join("2.bin");
    assert!(aziz_save.exists(), "Aziz's world should have its own save file");
    assert!(vali_save.exists(), "Vali's world should have its own save file");

    // A fresh process (different seed, proving it's not reseeding) picks
    // both saves back up, still isolated from each other.
    let wm2 = WorldManager::new(4242, 12, false);
    let handle2 = start(4242, wm2.clone());
    let addr2 = addr_of(&handle2);

    let aziz2 = client::connect_tcp_with(&addr2, login_msg("fc111111", "pw-aziz")).expect("aziz reconnects");
    let (_aziz2_id, aziz2_state) = recv_welcome(&aziz2);
    assert!(
        aziz2_state
            .buildings
            .iter()
            .any(|b| b.kind == BuildingKind::Tent && b.owner == Some(aziz_id)),
        "Aziz's tent should survive the restart: {:?}",
        aziz2_state.buildings
    );

    let vali2 = client::connect_tcp_with(&addr2, login_msg("fc222222", "pw-vali")).expect("vali reconnects");
    let (_vali2_id, vali2_state) = recv_welcome(&vali2);
    assert!(
        vali2_state.buildings.iter().all(|b| b.kind != BuildingKind::Tent),
        "Vali's world should still have no Tent after restart, not Aziz's: {:?}",
        vali2_state.buildings
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
