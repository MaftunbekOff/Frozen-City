//! Regression test for the "life keeps going while you're offline" behavior:
//! a personal (per-account) world must keep ticking in the background after
//! its last client disconnects, not freeze and only resume on the next
//! login. Before this fix `WorldManager`'s `IDLE_SHUTDOWN` was 5 minutes, so
//! this exact scenario (disconnect, wait, reconnect) would have shown the
//! same tick count either way; now it's 30 days, so any short real-world
//! gap between logins keeps advancing the sim exactly like the always-on
//! shared world already did.
//!
//! Deliberately a single `#[test]`: it sets `FC_ACCOUNTS_DB`/
//! `FC_ACCOUNT_WORLDS_DIR`, process-wide env vars `net::accounts`/
//! `net::world_manager` read — same reasoning as `account_world_e2e.rs`.

use std::time::{Duration, Instant};

use frozen_city::net::client::{self, ClientConn};
use frozen_city::net::protocol::{ClientMsg, ServerMsg};
use frozen_city::net::server::{self, ServerConfig};
use frozen_city::net::world_manager::WorldManager;

fn seed_one_account(db_path: &std::path::Path) {
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
    let hash = bcrypt::hash("pw-nodira", bcrypt::DEFAULT_COST).unwrap();
    conn.execute(
        "INSERT INTO accounts
            (telegram_id, telegram_username, display_username, first_name,
             last_name, birth_date, login, password_hash, created_at)
         VALUES (1, 'Nodira', 'Nodira', 'Nodira', 'Karimova', '2000-01-01', 'fc333333', ?1, '2026-01-01T00:00:00')",
        rusqlite::params![hash],
    )
    .unwrap();
}

fn recv_welcome(conn: &ClientConn) -> u64 {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match conn.recv_timeout(Duration::from_millis(500)) {
            Ok(ServerMsg::Welcome { state, .. }) => return state.tick,
            Ok(other) => panic!("expected Welcome, got {other:?}"),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(e) => panic!("connection died before Welcome: {e:?}"),
        }
    }
    panic!("no Welcome within 10s");
}

fn login_msg() -> ClientMsg {
    ClientMsg::Login { login: "fc333333".to_string(), password: "pw-nodira".to_string(), token: None }
}

#[test]
fn a_personal_world_keeps_ticking_after_its_last_client_disconnects() {
    let db_dir = std::env::temp_dir().join(format!("fc-idle-tick-e2e-db-{}", std::process::id()));
    let worlds_dir = std::env::temp_dir().join(format!("fc-idle-tick-e2e-worlds-{}", std::process::id()));
    std::fs::create_dir_all(&db_dir).unwrap();
    std::fs::remove_dir_all(&worlds_dir).ok();
    let db_path = db_dir.join("accounts.db");
    // SAFETY: this test binary's only test function, so nothing else in the
    // process reads/writes these environment variables concurrently.
    unsafe {
        std::env::set_var("FC_ACCOUNTS_DB", &db_path);
        std::env::set_var("FC_ACCOUNT_WORLDS_DIR", &worlds_dir);
    }
    seed_one_account(&db_path);

    let wm = WorldManager::new(7777, 12, false);
    let handle = server::start_with_accounts(
        ServerConfig {
            port: Some(0),
            seed: 7777,
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
        wm.clone(),
    )
    .expect("server starts");
    let addr = format!("127.0.0.1:{}", handle.addr.expect("bound addr").port());

    let conn1 = client::connect_tcp_with(&addr, login_msg()).expect("first login connects");
    let tick_before = recv_welcome(&conn1);
    drop(conn1); // the world's only client disconnects

    // Real-world gap with nobody connected — long enough that a frozen
    // world (the pre-fix 5-minute-eviction behavior didn't freeze this
    // quickly either, but a *paused* world would show zero tick movement
    // over any gap) would show no progress, while a live-ticking one
    // (TICK_MS = 200ms) advances several ticks.
    std::thread::sleep(Duration::from_millis(1500));

    let conn2 = client::connect_tcp_with(&addr, login_msg()).expect("second login reconnects");
    let tick_after = recv_welcome(&conn2);

    assert!(
        tick_after > tick_before + 3,
        "the world should keep ticking while nobody's connected, not freeze: \
         tick_before={tick_before}, tick_after={tick_after}"
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
