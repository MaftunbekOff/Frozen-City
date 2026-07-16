//! Regression test for a real production bug (found 2026-07-15): restarting
//! the server while a player was connected used to leave their roster entry
//! behind forever — `state.players` is part of the saved `GameState`, but
//! nothing ever re-claimed or cleaned up an entry left over from a process
//! that no longer exists, so every restart-while-connected permanently added
//! a ghost "<name> (owner)" duplicate to the roster. `sim_loop` now clears
//! `state.players` on every boot (see its doc comment) since "who's
//! connected" is inherently a live, in-memory concept that must start empty
//! regardless of what got saved.
//!
//! Deliberately a single `#[test]`: it sets `FC_ACCOUNTS_DB`/
//! `FC_ACCOUNT_WORLDS_DIR` — same reasoning as `account_world_e2e.rs`.

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
    let hash = bcrypt::hash("pw-elyor", bcrypt::DEFAULT_COST).unwrap();
    conn.execute(
        "INSERT INTO accounts
            (telegram_id, telegram_username, display_username, first_name,
             last_name, birth_date, login, password_hash, created_at)
         VALUES (1, 'Elyor', 'Elyor', 'Elyor', 'Nazarov', '2000-01-01', 'fc555555', ?1, '2026-01-01T00:00:00')",
        rusqlite::params![hash],
    )
    .unwrap();
}

fn recv_welcome(conn: &ClientConn) -> u64 {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match conn.recv_timeout(Duration::from_millis(500)) {
            Ok(ServerMsg::Welcome { state, .. }) => return state.players.len() as u64,
            Ok(other) => panic!("expected Welcome, got {other:?}"),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(e) => panic!("connection died before Welcome: {e:?}"),
        }
    }
    panic!("no Welcome within 10s");
}

fn login_msg() -> ClientMsg {
    ClientMsg::Login { login: "fc555555".to_string(), password: "pw-elyor".to_string(), token: None }
}

fn start(seed: u64, wm: std::sync::Arc<WorldManager>) -> server::ServerHandle {
    server::start_with_accounts(
        ServerConfig {
            port: Some(0),
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

#[test]
fn a_restart_while_connected_does_not_leave_a_ghost_roster_entry() {
    let db_dir = std::env::temp_dir().join(format!("fc-stale-roster-e2e-db-{}", std::process::id()));
    let worlds_dir = std::env::temp_dir().join(format!("fc-stale-roster-e2e-worlds-{}", std::process::id()));
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

    let wm1 = WorldManager::new(3131, 12, false);
    let handle1 = start(3131, wm1.clone());
    let addr1 = format!("127.0.0.1:{}", handle1.addr.expect("bound addr").port());

    let conn1 = client::connect_tcp_with(&addr1, login_msg()).expect("first login connects");
    let roster_len_first_join = recv_welcome(&conn1);
    assert_eq!(roster_len_first_join, 1, "the lone connecting player should be the only roster entry");

    // Simulate exactly the bug scenario: the server restarts (a deploy, a
    // systemd restart) WITHOUT the client ever cleanly disconnecting first
    // — `conn1` is just dropped, same as a socket a deploy yanks the rug
    // out from under. `handle1.stop()`/`join()` still saves the world (the
    // graceful-shutdown path), but with the connected player's roster entry
    // still present at the moment of saving, exactly like the real bug.
    drop(conn1);
    handle1.stop();
    handle1.join();
    wm1.stop_all();
    wm1.join_all();

    let wm2 = WorldManager::new(4242, 12, false);
    let handle2 = start(4242, wm2.clone());
    let addr2 = format!("127.0.0.1:{}", handle2.addr.expect("bound addr").port());

    let conn2 = client::connect_tcp_with(&addr2, login_msg()).expect("second login connects");
    let roster_len_after_restart = recv_welcome(&conn2);
    assert_eq!(
        roster_len_after_restart, 1,
        "after a restart, reconnecting should show exactly one roster entry, not a ghost duplicate \
         left over from the connection that existed when the server went down"
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
