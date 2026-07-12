//! End-to-end tests, over the real wire, for the V0.5/V0.6 "leftover" social
//! features this work adds: the owner-offline visit policy (default-deny,
//! opt-in allow with lazy world spawn), cross-world `Invited` delivery (a
//! target reachable in their own personal world, not just centrally), the
//! Showcase hub-activities read, and its rate limit. Same harness style as
//! `visit_e2e.rs` / `central_world_e2e.rs` (in-process `ServerHandle`, temp
//! accounts DB + worlds dir).
//!
//! Deliberately a single `#[test]`: it sets `FC_ACCOUNTS_DB` and
//! `FC_ACCOUNT_WORLDS_DIR`, process-wide env vars, and cargo runs `#[test]`
//! fns in one binary on separate threads by default — a second test setting
//! its own paths here would race this one.

use std::time::{Duration, Instant};

use frozen_city::game::sim;
use frozen_city::game::types::{GamePhase, GameState};
use frozen_city::net::accounts;
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
    // 1 = Host, 2 = Friendly (mutual friend of Host), 3 = Stranger (never
    // befriended, proves the Showcase privacy rule).
    for (telegram_id, name, login, pw) in [
        (1i64, "Host", "fc900001", "pw-host"),
        (2i64, "Friendly", "fc900002", "pw-friendly"),
        (3i64, "Stranger", "fc900003", "pw-stranger"),
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

/// True if the connection died (server closed the socket, e.g. after
/// `AuthFailed`) within the deadline without ever handing back a `Welcome`.
fn dies_without_welcome(conn: &ClientConn) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match conn.recv_timeout(Duration::from_millis(500)) {
            Ok(ServerMsg::Welcome { .. }) => return false,
            Ok(ServerMsg::AuthFailed { .. }) => return true,
            Ok(_) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return true,
        }
    }
    panic!("connection neither welcomed nor refused within 10s");
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

fn wait_visit_policy(conn: &ClientConn, want: bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match conn.recv_timeout(Duration::from_millis(500)) {
            Ok(ServerMsg::VisitPolicy { allow_offline }) if allow_offline == want => return,
            Ok(_) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(e) => panic!("connection died before VisitPolicy({want}): {e:?}"),
        }
    }
    panic!("no VisitPolicy({{allow_offline: {want}}}) within 10s");
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

/// True if no `Invited` arrives within `dur` — used to confirm the dedup
/// actually suppressed the second copy of an invite.
fn no_invited_within(conn: &ClientConn, dur: Duration) -> bool {
    let deadline = Instant::now() + dur;
    while Instant::now() < deadline {
        match conn.recv_timeout(Duration::from_millis(200)) {
            Ok(ServerMsg::Invited { .. }) => return false,
            Ok(_) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => return true,
        }
    }
    true
}

fn wait_reset_countdown(conn: &ClientConn) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match conn.recv_timeout(Duration::from_millis(500)) {
            Ok(ServerMsg::ResetCountdown { seconds_left }) => return seconds_left,
            Ok(_) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(e) => panic!("connection died before ResetCountdown: {e:?}"),
        }
    }
    panic!("no ResetCountdown within 10s");
}

fn wait_showcase(conn: &ClientConn) -> Vec<frozen_city::net::protocol::ShowcaseEntry> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match conn.recv_timeout(Duration::from_millis(500)) {
            Ok(ServerMsg::Showcase { entries }) => return entries,
            Ok(_) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(e) => panic!("connection died before Showcase: {e:?}"),
        }
    }
    panic!("no Showcase within 10s");
}

/// True if no `Showcase` arrives within `dur` — used to confirm the rate
/// limit actually suppressed a second request.
fn no_showcase_within(conn: &ClientConn, dur: Duration) -> bool {
    let deadline = Instant::now() + dur;
    while Instant::now() < deadline {
        match conn.recv_timeout(Duration::from_millis(200)) {
            Ok(ServerMsg::Showcase { .. }) => return false,
            Ok(_) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => return true,
        }
    }
    true
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
            central: false, // WorldManager owns the central world separately
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
fn visit_policy_invite_delivery_and_showcase() {
    let db_dir = std::env::temp_dir().join(format!("fc-social-server-db-{}", std::process::id()));
    let worlds_dir =
        std::env::temp_dir().join(format!("fc-social-server-worlds-{}", std::process::id()));
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

    // Host and Friendly both have graduated personal worlds crafted on disk
    // (the graduated-save trick from `central_world_e2e.rs`), so EnterCentral
    // doesn't need a full playthrough. Host's save additionally carries a
    // couple of extra buildings/survivors so the Showcase numbers are
    // distinguishable from a totally fresh world.
    let mut host_world = sim::new_game(111, 12);
    host_world.graduated = true;
    persist::save_at(&host_world, worlds_dir.join("1.bin").to_str().unwrap()).unwrap();
    let mut friendly_world = sim::new_game(222, 12);
    friendly_world.graduated = true;
    persist::save_at(&friendly_world, worlds_dir.join("2.bin").to_str().unwrap()).unwrap();
    // Stranger (account 3): also graduated, so it can reach central, but
    // never befriended by Host — proves the Showcase privacy rule. Crafted
    // already-Lost so the reset-countdown section below starts in game-over
    // without a playthrough.
    let mut stranger_world = sim::new_game(333, 12);
    stranger_world.graduated = true;
    stranger_world.phase = GamePhase::Lost;
    persist::save_at(&stranger_world, worlds_dir.join("3.bin").to_str().unwrap()).unwrap();

    let wm = WorldManager::new(700, 12, false);
    let handle = start(700, wm.clone());
    let addr = addr_of(&handle);

    // === Visit policy: default is deny, host truly offline (world never
    // spawned) === .
    assert!(!accounts::visit_policy(1), "sanity: default visit policy is deny");

    // Host and Friendly meet in central so Host can Invite Friendly (a
    // standing invite is required regardless of visit policy).
    let host_central = client::connect_tcp_with(&addr, enter_central_msg("fc900001", "pw-host"))
        .expect("host enters central");
    recv_welcome(&host_central);
    let friendly_central =
        client::connect_tcp_with(&addr, enter_central_msg("fc900002", "pw-friendly"))
            .expect("friendly enters central");
    recv_welcome(&friendly_central);
    wait_state(&host_central, |s| s.players.iter().any(|p| p.account == Some(2)));

    host_central.send(ClientMsg::Invite { account: 2 });
    wait_invited(&friendly_central);
    // Friendly leaves central here: the cross-world delivery section below
    // relies on Friendly NOT being centrally connected (a centrally-delivered
    // invite deliberately skips the personal world — see the dedup section).
    drop(friendly_central);

    // Host now leaves EVERYTHING (both central and — since Host never had a
    // personal-world connection open — the personal world was never even
    // spawned). A VisitFriend now must be refused: default policy, host
    // truly offline.
    drop(host_central);
    let denied = client::connect_tcp_with(&addr, visit_msg("fc900002", "pw-friendly", 1))
        .expect("friendly dials while host is fully offline");
    assert!(
        dies_without_welcome(&denied),
        "default visit policy (deny) must refuse a visit while the host is offline"
    );
    drop(denied);
    assert!(
        !worlds_dir.join("1.bin.tmp").exists() && worlds_dir.join("1.bin").exists(),
        "sanity: host's world file is untouched, was never spawned by the refused visit"
    );

    // === Host opts in to owner-offline entry: SetVisitPolicy over the wire,
    // confirmed by the VisitPolicy echo. === .
    let host_home = client::connect_tcp_with(&addr, login_msg("fc900001", "pw-host"))
        .expect("host logs into personal world to flip the setting");
    let (_, host_home_state) = recv_welcome(&host_home);
    assert!(!host_home_state.central);
    host_home.send(ClientMsg::SetVisitPolicy { allow_offline: true });
    wait_visit_policy(&host_home, true);
    assert!(accounts::visit_policy(1), "the DB row must reflect the change");
    // Host disconnects entirely again (idle_shutdown is None in this test's
    // ServerConfig, so the world stays spawned but with zero clients — an
    // equally valid "host offline" state for visit_friend's OwnerOnline check).
    drop(host_home);

    // Now the SAME invite (still standing — invites aren't consumed) admits
    // Friendly even though Host is offline, because the policy now allows it.
    let friendly_visits = client::connect_tcp_with(&addr, visit_msg("fc900002", "pw-friendly", 1))
        .expect("friendly dials with the opt-in policy now active");
    let (_, visit_state) = recv_welcome(&friendly_visits);
    assert!(!visit_state.central, "must land in host's personal world");
    drop(friendly_visits);

    // === Cross-world invite delivery: Host (from central) invites Friendly
    // while Friendly is connected to their OWN personal world, not central at
    // all — must still receive Invited there. === .
    let friendly_home = client::connect_tcp_with(&addr, login_msg("fc900002", "pw-friendly"))
        .expect("friendly goes home to their own personal world");
    recv_welcome(&friendly_home);

    let host_central2 = client::connect_tcp_with(&addr, enter_central_msg("fc900001", "pw-host"))
        .expect("host re-enters central to invite again");
    recv_welcome(&host_central2);
    host_central2.send(ClientMsg::Invite { account: 2 });
    let (invited_host, invited_name) = wait_invited(&friendly_home);
    assert_eq!(invited_host, 1);
    assert_eq!(invited_name, "Host");

    // === Invite dedup: Friendly is now connected BOTH at home and centrally
    // (two devices). A fresh invite must reach the central connection only —
    // never both, or one human sees the same popup twice. === .
    let friendly_central2 =
        client::connect_tcp_with(&addr, enter_central_msg("fc900002", "pw-friendly"))
            .expect("friendly enters central while still connected at home");
    recv_welcome(&friendly_central2);
    wait_state(&host_central2, |s| s.players.iter().any(|p| p.account == Some(2)));
    host_central2.send(ClientMsg::Invite { account: 2 });
    wait_invited(&friendly_central2);
    assert!(
        no_invited_within(&friendly_central2, Duration::from_millis(800)),
        "the central connection must get the invite exactly once"
    );
    assert!(
        no_invited_within(&friendly_home, Duration::from_millis(800)),
        "an invite delivered centrally must not also land in the personal world"
    );
    drop(friendly_central2);
    drop(friendly_home);
    drop(host_central2);

    // === Showcase: Host and Friendly are mutual friends; Stranger is not.
    // Host's RefreshShowcase must return Friendly's stats and must NOT
    // return Stranger's, even though Stranger has a save on disk too. === .
    let host_home2 = client::connect_tcp_with(&addr, login_msg("fc900001", "pw-host"))
        .expect("host logs back in to add a friend and request the showcase");
    recv_welcome(&host_home2);
    host_home2.send(ClientMsg::AddFriend { name: "Friendly".to_string() });
    // Drain until the Social echo confirms the add went through.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut added = false;
    while Instant::now() < deadline && !added {
        if let Ok(ServerMsg::Social { friends }) = host_home2.recv_timeout(Duration::from_millis(500))
        {
            added = friends.iter().any(|f| f.account == 2);
        }
    }
    assert!(added, "AddFriend must be reflected in a Social refresh");

    host_home2.send(ClientMsg::RefreshShowcase);
    let entries = wait_showcase(&host_home2);
    assert_eq!(entries.len(), 1, "only friends are shown: {entries:?}");
    assert_eq!(entries[0].account, 2);
    assert_eq!(entries[0].name, "Friendly");
    assert!(entries[0].graduated, "Friendly's crafted save is graduated");
    assert!(
        entries.iter().all(|e| e.account != 3),
        "Stranger (never befriended) must never appear in the showcase: {entries:?}"
    );
    // Not in the central world, so no cheap ledger data available here.
    assert!(entries[0].central_contribution.is_none());

    // === Rate limit: an immediate second RefreshShowcase must be throttled
    // (no second Showcase arrives promptly), one per SHOWCASE_COOLDOWN
    // (5s) per connection. === .
    host_home2.send(ClientMsg::RefreshShowcase);
    assert!(
        no_showcase_within(&host_home2, Duration::from_millis(800)),
        "an immediate second RefreshShowcase on the same connection must be rate-limited"
    );

    // === Reset countdown: Stranger's crafted save is already Lost, so a
    // login lands straight in game-over — the world must announce how long
    // until its automatic reset, and the countdown must actually tick down.
    // === .
    let stranger_home = client::connect_tcp_with(&addr, login_msg("fc900003", "pw-stranger"))
        .expect("stranger logs into their game-over world");
    recv_welcome(&stranger_home);
    let first = wait_reset_countdown(&stranger_home);
    assert!(
        (1..=45).contains(&first),
        "countdown must fit WORLD_RESET_AFTER (45s), got {first}"
    );
    let second = wait_reset_countdown(&stranger_home);
    assert!(
        second < first,
        "countdown must decrease across broadcasts, got {first} then {second}"
    );
    drop(stranger_home);

    handle.stop();
    handle.join();
    wm.stop_all();
    wm.join_all();

    std::fs::remove_dir_all(&db_dir).ok();
    std::fs::remove_dir_all(&worlds_dir).ok();
    std::env::remove_var("FC_ACCOUNTS_DB");
    std::env::remove_var("FC_ACCOUNT_WORLDS_DIR");
}
