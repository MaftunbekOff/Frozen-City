//! End-to-end test for account sign-in (`ClientMsg::Login`): a real dedicated
//! server checking a real accounts DB over a real socket, same harness style
//! as `net_e2e.rs` but exercising the Login path instead of guest Hello.
//!
//! Deliberately a single `#[test]` (not several) even though it covers two
//! scenarios: it sets `FC_ACCOUNTS_DB`, a process-wide env var
//! `net::accounts::authenticate` reads, and cargo runs `#[test]` fns in one
//! binary on separate threads by default — a second test setting its own
//! path here would race this one.

use std::time::{Duration, Instant};

use frozen_city::net::client::{self, ClientConn};
use frozen_city::net::protocol::{ClientMsg, ServerMsg};
use frozen_city::net::server::{self, ServerConfig};

fn recv_first(conn: &ClientConn) -> ServerMsg {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match conn.recv_timeout(Duration::from_millis(500)) {
            Ok(msg) => return msg,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(e) => panic!("connection died before a message: {e:?}"),
        }
    }
    panic!("no message within 10s");
}

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
    let hash = bcrypt::hash("s3cret-pw", bcrypt::DEFAULT_COST).unwrap();
    conn.execute(
        "INSERT INTO accounts
            (telegram_id, telegram_username, display_username, first_name,
             last_name, birth_date, login, password_hash, created_at)
         VALUES (1, 'tguser', 'Aziz', 'Aziz', 'Karimov', '2000-01-01',
                 'fc999999', ?1, '2026-01-01T00:00:00')",
        [&hash],
    )
    .unwrap();
}

#[test]
fn account_login_flow() {
    let dir = std::env::temp_dir().join(format!("fc-account-login-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("accounts.db");
    // SAFETY: this test binary's only test function, so nothing else in the
    // process reads/writes the environment concurrently with this.
    unsafe {
        std::env::set_var("FC_ACCOUNTS_DB", &db_path);
    }
    seed_account(&db_path);

    let handle = server::start(ServerConfig {
        port: Some(0), // ephemeral port
        seed: 99,
        win_days: 12,
        persistent: false,
        verbose: false,
        save_path: None,
        idle_shutdown: None,
        central: false,
        owner_account: None,
        invites: None,
    })
    .expect("server starts");
    let port = handle.addr.expect("bound addr").port();
    let addr = format!("127.0.0.1:{port}");

    // Correct login joins as the account's display name, not whatever a
    // client might have sent as a free-text guest name.
    let good = client::connect_tcp_with(
        &addr,
        ClientMsg::Login {
            login: "fc999999".to_string(),
            password: "s3cret-pw".to_string(),
            token: None,
        },
    )
    .expect("connects");
    match recv_first(&good) {
        ServerMsg::Welcome { state, .. } => {
            assert!(state.players.iter().any(|p| p.name == "Aziz"));
        }
        other => panic!("expected Welcome, got {other:?}"),
    }

    // A wrong password is rejected with AuthFailed, not silently dropped or
    // let in as a guest.
    let bad = client::connect_tcp_with(
        &addr,
        ClientMsg::Login {
            login: "fc999999".to_string(),
            password: "wrong-password".to_string(),
            token: None,
        },
    )
    .expect("connects");
    match recv_first(&bad) {
        ServerMsg::AuthFailed { .. } => {}
        other => panic!("expected AuthFailed, got {other:?}"),
    }

    handle.stop();
    std::fs::remove_dir_all(&dir).ok();
}
