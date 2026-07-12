//! Heavy scale tests for the V0.4/V0.5 capacity acceptance criteria — both
//! `#[ignore]`d (production machine; run explicitly, bounded runtime). Same
//! harness style as the other account/central e2e tests, but each test here
//! measures and prints wall-clock numbers instead of just asserting pass/fail.
//!
//! Run with:
//!   nice -n 10 cargo test --release --test scale_e2e -- --ignored --nocapture --test-threads=1
//!
//! Each test is self-contained (own env vars, own accounts DB, own worlds
//! dir) and cleans up after itself; `--test-threads=1` also keeps the two
//! ignored tests from racing each other's `FC_ACCOUNTS_DB`/
//! `FC_ACCOUNT_WORLDS_DIR` env vars (same reasoning as every other e2e file
//! in this suite for why env-var-setting tests don't run concurrently in one
//! binary).

use std::time::{Duration, Instant};

use frozen_city::game::sim;
use frozen_city::net::client::{self, ClientConn};
use frozen_city::net::persist;
use frozen_city::net::protocol::{ClientMsg, ServerMsg};
use frozen_city::net::server::{self, ServerConfig};
use frozen_city::net::world_manager::WorldManager;

fn seed_n_accounts(db_path: &std::path::Path, n: i64) {
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
    // One shared bcrypt hash for every seeded account: hashing N times would
    // dominate setup time for no test value (bcrypt's cost factor is
    // deliberately slow) — same password for all of them is fine, this test
    // isn't exercising auth diversity.
    let hash = bcrypt::hash("pw-scale", bcrypt::DEFAULT_COST).unwrap();
    for i in 1..=n {
        let name = format!("Scaler{i}");
        let login = format!("fcSC{i:06}");
        conn.execute(
            "INSERT INTO accounts
                (telegram_id, telegram_username, display_username, first_name,
                 last_name, birth_date, login, password_hash, created_at)
             VALUES (?1, ?2, ?3, ?3, 'Karimov', '2000-01-01', ?4, ?5, '2026-01-01T00:00:00')",
            rusqlite::params![i, name, name, login, hash],
        )
        .unwrap();
    }
}

fn recv_welcome(conn: &ClientConn, timeout: Duration) -> Option<u64> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match conn.recv_timeout(Duration::from_millis(500)) {
            Ok(ServerMsg::Welcome { player_id, .. }) => return Some(player_id),
            Ok(_) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => return None,
        }
    }
    None
}

fn count_states(conn: &ClientConn, window: Duration) -> usize {
    let deadline = Instant::now() + window;
    let mut n = 0usize;
    while Instant::now() < deadline {
        match conn.recv_timeout(Duration::from_millis(300)) {
            Ok(ServerMsg::State { .. }) => n += 1,
            Ok(_) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => break,
        }
    }
    n
}

/// Current process RSS in KB, read straight from `/proc/self/status` (Linux
/// only, which is what this production box runs) — no extra crate needed for
/// a one-shot diagnostic number.
fn rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest.trim().trim_end_matches(" kB").trim().parse().ok();
        }
    }
    None
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

fn addr_of(handle: &server::ServerHandle) -> String {
    format!("127.0.0.1:{}", handle.addr.expect("bound addr").port())
}

/// V0.4 criterion: "50+ shaxsiy olam bitta serverda parallel (yuk testi)" —
/// 50 distinct account worlds live in parallel on one server, each ticking
/// independently, server stays responsive to all of them at once.
#[test]
#[ignore]
fn fifty_account_worlds_tick_independently() {
    const N: i64 = 50;
    let db_dir = std::env::temp_dir().join(format!("fc-scale50-db-{}", std::process::id()));
    let worlds_dir = std::env::temp_dir().join(format!("fc-scale50-worlds-{}", std::process::id()));
    std::fs::create_dir_all(&db_dir).unwrap();
    std::fs::remove_dir_all(&worlds_dir).ok();
    std::fs::create_dir_all(&worlds_dir).unwrap();
    let db_path = db_dir.join("accounts.db");
    // SAFETY: run with --test-threads=1 (see module doc); no concurrent
    // access to these env vars within the process.
    unsafe {
        std::env::set_var("FC_ACCOUNTS_DB", &db_path);
        std::env::set_var("FC_ACCOUNT_WORLDS_DIR", &worlds_dir);
    }
    seed_n_accounts(&db_path, N);

    let wm = WorldManager::new(1001, 12, false);
    let handle = server::start_with_accounts(
        ServerConfig {
            port: Some(0),
            seed: 1001,
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
    let addr = addr_of(&handle);

    let overall_start = Instant::now();
    let mut conns = Vec::with_capacity(N as usize);
    let mut join_latencies = Vec::with_capacity(N as usize);
    for i in 1..=N {
        let login = format!("fcSC{i:06}");
        let t0 = Instant::now();
        let conn = client::connect_tcp_with(&addr, login_msg(&login, "pw-scale"))
            .unwrap_or_else(|e| panic!("account {i} connects: {e}"));
        let welcomed = recv_welcome(&conn, Duration::from_secs(10));
        let elapsed = t0.elapsed();
        assert!(welcomed.is_some(), "account {i} must receive a Welcome");
        join_latencies.push(elapsed);
        conns.push(conn);
    }
    let all_joined_elapsed = overall_start.elapsed();

    // Every world must be independently alive: each connection should see at
    // least one State snapshot within a few ticks (TICK_MS=200ms).
    let mut ticking = 0usize;
    for c in &conns {
        if count_states(c, Duration::from_secs(3)) > 0 {
            ticking += 1;
        }
    }

    join_latencies.sort();
    let p50 = join_latencies[join_latencies.len() / 2];
    let p99 = join_latencies[(join_latencies.len() * 99 / 100).min(join_latencies.len() - 1)];
    let max = *join_latencies.last().unwrap();
    println!(
        "[scale_e2e] 50 account worlds: all_joined_in={:?} join_latency(p50={:?}, p99={:?}, max={:?}) worlds_ticking={}/{} rss_kb={:?}",
        all_joined_elapsed, p50, p99, max, ticking, N, rss_kb()
    );

    assert_eq!(ticking, N as usize, "every one of the 50 account worlds must tick independently");

    drop(conns);
    handle.stop();
    handle.join();
    wm.stop_all();
    wm.join_all();

    std::fs::remove_dir_all(&db_dir).ok();
    std::fs::remove_dir_all(&worlds_dir).ok();
    std::env::remove_var("FC_ACCOUNTS_DB");
    std::env::remove_var("FC_ACCOUNT_WORLDS_DIR");
}

/// V0.5 criterion: "100+ concurrent avatar bitta hub'da (sun'iy yuk testi)" —
/// 100 graduated accounts all `EnterCentral` into the ONE shared central
/// world, all receive snapshots for >=10 ticks with no disconnects.
#[test]
#[ignore]
fn hundred_concurrent_clients_in_one_central_world() {
    const N: i64 = 100;
    let db_dir = std::env::temp_dir().join(format!("fc-scale100-db-{}", std::process::id()));
    let worlds_dir = std::env::temp_dir().join(format!("fc-scale100-worlds-{}", std::process::id()));
    std::fs::create_dir_all(&db_dir).unwrap();
    std::fs::remove_dir_all(&worlds_dir).ok();
    std::fs::create_dir_all(&worlds_dir).unwrap();
    let db_path = db_dir.join("accounts.db");
    // SAFETY: run with --test-threads=1 (see module doc).
    unsafe {
        std::env::set_var("FC_ACCOUNTS_DB", &db_path);
        std::env::set_var("FC_ACCOUNT_WORLDS_DIR", &worlds_dir);
    }
    seed_n_accounts(&db_path, N);

    // Craft each account's personal world already graduated, so EnterCentral
    // doesn't have to grind a playthrough per account — same trick as
    // `central_world_e2e.rs`, applied N times. Small survivor pool per
    // account keeps the migration step (and the resulting central-world
    // snapshot size at N=100 owners) cheap, matching "keep load bounded".
    for i in 1..=N {
        let mut w = sim::new_game(2000 + i as u64, 12);
        w.graduated = true;
        persist::save_at(&w, worlds_dir.join(format!("{i}.bin")).to_str().unwrap()).unwrap();
    }

    let wm = WorldManager::new(2001, 12, false);
    let handle = server::start_with_accounts(
        ServerConfig {
            port: Some(0),
            seed: 2001,
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
    let addr = addr_of(&handle);

    let overall_start = Instant::now();
    let mut conns = Vec::with_capacity(N as usize);
    let mut join_latencies = Vec::with_capacity(N as usize);
    let mut disconnected_during_join = 0usize;
    for i in 1..=N {
        let login = format!("fcSC{i:06}");
        let t0 = Instant::now();
        match client::connect_tcp_with(&addr, enter_central_msg(&login, "pw-scale")) {
            Ok(conn) => {
                let welcomed = recv_welcome(&conn, Duration::from_secs(10));
                let elapsed = t0.elapsed();
                if welcomed.is_some() {
                    join_latencies.push(elapsed);
                    conns.push(Some(conn));
                } else {
                    disconnected_during_join += 1;
                    conns.push(None);
                }
            }
            Err(_) => {
                disconnected_during_join += 1;
                conns.push(None);
            }
        }
    }
    let all_joined_elapsed = overall_start.elapsed();

    // Steady state: every still-live connection must keep receiving State
    // snapshots for >=10 ticks (TICK_MS=200ms -> >=2s of observation), with
    // no disconnects along the way.
    let observe_window = Duration::from_secs(4); // >= 10 ticks at 200ms
    let mut disconnected_during_observe = 0usize;
    let mut min_states = usize::MAX;
    let mut total_states = 0usize;
    for c in conns.iter().flatten() {
        let n = count_states(c, observe_window);
        if n == 0 {
            disconnected_during_observe += 1;
        } else {
            min_states = min_states.min(n);
        }
        total_states += n;
    }
    let live = conns.iter().filter(|c| c.is_some()).count();
    let avg_states = if live > 0 { total_states as f64 / live as f64 } else { 0.0 };
    // Snapshot cadence per connection: TICK_MS=200ms is the server's send
    // interval, so wall-clock-per-snapshot-received approximates that,
    // inflated by however much this box's scheduler/network adds under load.
    let approx_interval_ms = if avg_states > 0.0 {
        observe_window.as_millis() as f64 / avg_states
    } else {
        f64::NAN
    };

    join_latencies.sort();
    let p50 = if !join_latencies.is_empty() { join_latencies[join_latencies.len() / 2] } else { Duration::ZERO };
    let p99 = if !join_latencies.is_empty() {
        join_latencies[(join_latencies.len() * 99 / 100).min(join_latencies.len() - 1)]
    } else {
        Duration::ZERO
    };
    let max = join_latencies.last().copied().unwrap_or(Duration::ZERO);
    println!(
        "[scale_e2e] 100 central clients: all_joined_in={:?} join_latency(p50={:?}, p99={:?}, max={:?}) \
         welcomed={}/{} disconnected_during_join={} disconnected_during_observe={} \
         avg_states_per_conn_in_{:?}={:.1} approx_snapshot_interval_ms={:.0} rss_kb={:?}",
        all_joined_elapsed, p50, p99, max, live, N, disconnected_during_join,
        disconnected_during_observe, observe_window, avg_states, approx_interval_ms, rss_kb()
    );

    assert_eq!(disconnected_during_join, 0, "all 100 graduated accounts must be admitted to central");
    assert_eq!(disconnected_during_observe, 0, "no client should be dropped during steady state");
    assert!(min_states >= 10, "every client must see >= 10 State snapshots in the observation window, saw min={min_states}");

    drop(conns);
    handle.stop();
    handle.join();
    wm.stop_all();
    wm.join_all();

    std::fs::remove_dir_all(&db_dir).ok();
    std::fs::remove_dir_all(&worlds_dir).ok();
    std::env::remove_var("FC_ACCOUNTS_DB");
    std::env::remove_var("FC_ACCOUNT_WORLDS_DIR");
}
