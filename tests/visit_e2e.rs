//! End-to-end test for V0.6's invite / guest co-op acceptance criteria
//! (ROADMAP.md V0.6 "Natija mezonlari": kick and permission limits are
//! confirmed by test). Drives the real flow over the wire: two graduated
//! accounts meet in the central world, the host `Invite`s the guest, the
//! guest leaves the central world and `VisitFriend`s into the host's
//! personal world, and from there the usual `GuestPermission`/`Kick`
//! machinery (already covered in isolation by `roles_e2e.rs`) is exercised
//! specifically through the visit path. Same harness style as
//! `central_world_e2e.rs` / `account_world_e2e.rs`.
//!
//! Per the concurrency note for this work: Agent B is in flight on
//! central-world building ownership and the visit-policy default, so this
//! file does NOT assert central-world demolish/ownership rules, and the
//! owner-offline case only asserts the DEFAULT (deny) behavior.
//!
//! Deliberately a single `#[test]`: it sets `FC_ACCOUNTS_DB` and
//! `FC_ACCOUNT_WORLDS_DIR`, process-wide env vars, and cargo runs `#[test]`
//! fns in one binary on separate threads by default — a second test setting
//! its own paths here would race this one.

use std::time::{Duration, Instant};

use frozen_city::game::sim;
use frozen_city::game::types::{BuildingKind, GameState, GuestPermission, PlayerCommand, Role};
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
    // Id 1 = Host (graduated, owns the personal world visited below).
    // Id 2 = Guestly (graduated too, so she can meet the host in central).
    // Id 3 = Newbie (never graduated — proves (g): a non-graduated visitor
    // can still use the guest path, no Tunnel requirement on that side).
    for (telegram_id, name, login, pw) in [
        (1i64, "Host", "fc300001", "pw-host"),
        (2i64, "Guestly", "fc300002", "pw-guestly"),
        (3i64, "Newbie", "fc300003", "pw-newbie"),
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

/// True if the connection died (server closed the socket) within the
/// deadline without ever handing back a `Welcome` — the shape a `VisitFriend`
/// refusal can also take if `AuthFailed` races the close.
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

/// A second valid tent spot, far enough from `first` that placing a tent at
/// `first` can't invalidate it. Both spots must be picked from a FULL state
/// (the `Welcome` snapshot): delta `State` messages omit `tiles` on most
/// ticks, and `can_place` -> `tile()` panics (index out of bounds) on an
/// empty tile grid — see the quirk note in this test's final report.
fn second_tent_spot(state: &GameState, first: (u8, u8)) -> (u8, u8) {
    (0..64u8)
        .flat_map(|y| (0..64u8).map(move |x| (x, y)))
        .find(|&(x, y)| {
            let far = (x as i16 - first.0 as i16).abs().max((y as i16 - first.1 as i16).abs()) >= 4;
            far && state.can_place(BuildingKind::Tent, x, y).is_ok()
        })
        .expect("a second, far-away valid tent spot")
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
fn invite_and_visit_flow() {
    let db_dir = std::env::temp_dir().join(format!("fc-visit-e2e-db-{}", std::process::id()));
    let worlds_dir = std::env::temp_dir().join(format!("fc-visit-e2e-worlds-{}", std::process::id()));
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

    // Host (account 1) and Guestly (account 2) both have graduated personal
    // worlds on disk, reusing the graduated-save crafting trick from
    // `central_world_e2e.rs`.
    let mut host_world = sim::new_game(111, 12);
    host_world.graduated = true;
    persist::save_at(&host_world, worlds_dir.join("1.bin").to_str().unwrap()).unwrap();
    let mut guestly_world = sim::new_game(222, 12);
    guestly_world.graduated = true;
    persist::save_at(&guestly_world, worlds_dir.join("2.bin").to_str().unwrap()).unwrap();
    // Newbie (account 3) never graduated — no save crafted, a fresh
    // (ungraduated) personal world would be generated on first Login.

    let wm = WorldManager::new(500, 12, false);
    let handle = start(500, wm.clone());
    let addr = addr_of(&handle);

    // --- (e) VisitFriend WITHOUT a standing invite is refused. ---
    let too_early = client::connect_tcp_with(&addr, visit_msg("fc300002", "pw-guestly", 1))
        .expect("guestly dials without an invite");
    assert!(
        dies_without_welcome(&too_early),
        "VisitFriend with no standing invite must be refused, not welcomed"
    );
    drop(too_early);

    // --- (f) VisitFriend while the host is OFFLINE is refused (default
    // policy) — even setting aside the missing-invite case, exercised
    // separately right below once an invite actually exists. ---

    // --- Meet in the central world: both accounts enter, host invites
    // guestly. ---
    let host_central = client::connect_tcp_with(&addr, enter_central_msg("fc300001", "pw-host"))
        .expect("host enters central");
    let (_host_central_pid, host_central_state) = recv_welcome(&host_central);
    assert!(host_central_state.central);

    let guestly_central =
        client::connect_tcp_with(&addr, enter_central_msg("fc300002", "pw-guestly"))
            .expect("guestly enters central");
    let (_guestly_central_pid, _) = recv_welcome(&guestly_central);

    // Wait until the host's view of central shows guestly present (so the
    // Invite target lookup — which scans currently-connected central
    // players — actually finds her).
    wait_state(&host_central, |s| {
        s.players.iter().any(|p| p.account == Some(2))
    });

    host_central.send(ClientMsg::Invite { account: 2 });
    let (invited_host, invited_host_name) = wait_invited(&guestly_central);
    assert_eq!(invited_host, 1);
    assert_eq!(invited_host_name, "Host");

    // --- (f) The invite now stands, but the host isn't in their PERSONAL
    // world (still in central) — the default owner-present-only policy must
    // still refuse. ---
    let while_host_in_central =
        client::connect_tcp_with(&addr, visit_msg("fc300002", "pw-guestly", 1))
            .expect("guestly dials while host is only in central, not home");
    assert!(
        dies_without_welcome(&while_host_in_central),
        "VisitFriend must be refused while the host isn't online in their OWN personal world (default policy)"
    );
    drop(while_host_in_central);

    // Guestly leaves central; host leaves central and goes home (Login to
    // their own personal world) so the standing invite can actually be used.
    drop(guestly_central);
    drop(host_central);
    let host_home = client::connect_tcp_with(&addr, login_msg("fc300001", "pw-host"))
        .expect("host goes home");
    let (host_home_pid, host_home_state) = recv_welcome(&host_home);
    assert!(!host_home_state.central);
    assert_eq!(
        host_home_state.guest_perm,
        GuestPermission::Build,
        "personal worlds default to Build for guests"
    );

    // --- (a) The visitor joins as Guest, not Owner; owner role stays
    // pinned to the host account no matter who is/was first in the roster. ---
    let guestly_visit = client::connect_tcp_with(&addr, visit_msg("fc300002", "pw-guestly", 1))
        .expect("guestly visits now that the host is home");
    let (guestly_pid, guestly_visit_state) = recv_welcome(&guestly_visit);
    assert!(guestly_visit_state.is_owner(host_home_pid), "the host stays Owner");
    assert!(
        !guestly_visit_state.is_owner(guestly_pid),
        "the visitor must never become Owner, even though invites/visits are dynamic"
    );
    let guestly_role = guestly_visit_state
        .player(guestly_pid)
        .map(|p| p.role)
        .expect("guestly is in the roster");
    assert_eq!(guestly_role, Role::Guest);

    // --- (c) Under the default Build permission the visitor CAN build, and
    // the event feed attributes it to the visitor's own display name. ---
    let spot = some_tent_spot(&guestly_visit_state);
    guestly_visit.send(ClientMsg::Cmd(PlayerCommand::Place {
        kind: BuildingKind::Tent,
        x: spot.0,
        y: spot.1,
    }));
    let built_state = wait_state(&guestly_visit, |s| {
        s.buildings
            .iter()
            .any(|b| b.kind == BuildingKind::Tent && b.owner == Some(guestly_pid))
    });
    assert!(
        built_state
            .events
            .iter()
            .any(|e| e.text.contains("Guestly") && e.text.contains("built")),
        "the visitor's build must be attributed to their own name in the event feed: {:?}",
        built_state.events
    );

    // --- (b) Tighten to ViewOnly: the visitor's next Place is a no-op. ---
    host_home.send(ClientMsg::SetGuestPermission { perm: GuestPermission::ViewOnly });
    let after_policy = wait_state(&guestly_visit, |s| s.guest_perm == GuestPermission::ViewOnly);
    let building_count_before = after_policy.buildings.len();
    // Picked from the full Welcome state, NOT `after_policy` (a delta
    // snapshot whose `tiles` are omitted — `can_place` panics on those).
    let spot2 = second_tent_spot(&guestly_visit_state, spot);
    guestly_visit.send(ClientMsg::Cmd(PlayerCommand::Place {
        kind: BuildingKind::Tent,
        x: spot2.0,
        y: spot2.1,
    }));
    // host_home's queue may still hold snapshots from before the guest's
    // tent existed (it was last drained well before step (c)) — sync it to
    // the present first, or the first recv below reads stale history and
    // fails spuriously (buildings 1 vs 2).
    wait_state(&host_home, |s| s.buildings.len() == building_count_before);
    let mut checked = 0;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && checked < 10 {
        if let Ok(ServerMsg::State { state, .. }) = host_home.recv_timeout(Duration::from_millis(500)) {
            assert_eq!(
                state.buildings.len(),
                building_count_before,
                "guest Place must be a no-op under ViewOnly, even for an invited visitor"
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "expected to observe at least one snapshot under ViewOnly");

    // --- (d) Kick disconnects the visitor. ---
    // Restore Build first so we can be sure the disconnect below is really
    // from Kick, not a lingering ViewOnly side effect.
    host_home.send(ClientMsg::SetGuestPermission { perm: GuestPermission::Build });
    wait_state(&guestly_visit, |s| s.guest_perm == GuestPermission::Build);
    host_home.send(ClientMsg::Kick { target: guestly_pid });
    wait_state(&host_home, |s| s.players.len() == 1);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut died = false;
    while Instant::now() < deadline {
        match guestly_visit.recv_timeout(Duration::from_millis(500)) {
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                died = true;
                break;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Ok(_) => continue,
        }
    }
    assert!(died, "a kicked visitor's connection must be closed by the server");
    drop(guestly_visit);

    // --- (g) "A visitor account that never graduated can still visit — no
    // Tunnel requirement on the guest path" (ROADMAP.md V0.6, "Onboarding'siz
    // mehmon"). `WorldManager::visit_friend` itself indeed has NO graduation
    // check on the visitor (confirmed by reading `world_manager.rs`: its only
    // gates are `InviteBook::valid` and host-presence) — but there is
    // currently NO WAY for an ungraduated account to ever receive a standing
    // invite to test that with: `Invite` (`server.rs`, the
    // `ClientMsg::Invite` arm) only fires while the SENDER is in the central
    // world and only targets accounts CURRENTLY CONNECTED to that same
    // central world — and entering the central world at all requires
    // graduation (`EnterCentral` → `CentralError::NotGraduated`). So an
    // ungraduated account can never be the target of a real `Invite`,
    // which means the "onboarding-less guest" feature this criterion
    // describes is not actually reachable end-to-end yet, even though the
    // guest-path code (`visit_friend`) was clearly written to allow it.
    // Documented as a product gap in the final report rather than faked here
    // with a non-existent test backdoor into `InviteBook` (it's private and
    // only ever written from inside the real `Invite` command handler).
    let host_central2 = client::connect_tcp_with(&addr, enter_central_msg("fc300001", "pw-host"))
        .expect("host re-enters central to attempt inviting newbie");
    recv_welcome(&host_central2);
    let newbie_central = client::connect_tcp_with(&addr, enter_central_msg("fc300003", "pw-newbie"))
        .expect("newbie should be refused: never graduated");
    let reason = recv_auth_failed(&newbie_central);
    assert!(
        reason.is_empty() || reason.contains("Tunnel"),
        "an ungraduated account can't enter CENTRAL itself, so it can never be an Invite target: {reason:?}"
    );
    drop(newbie_central);
    drop(host_central2);

    // What IS confirmed: an ungraduated account is otherwise a fully normal
    // account (can log into its own personal world) — it's specifically the
    // "receive an invite" leg of the guest path that's unreachable for it.
    let newbie_home = client::connect_tcp_with(&addr, login_msg("fc300003", "pw-newbie"))
        .expect("newbie can still log into their own (ungraduated) personal world");
    let (_newbie_pid, newbie_state) = recv_welcome(&newbie_home);
    assert!(!newbie_state.graduated, "sanity: newbie truly never graduated");
    drop(newbie_home);

    handle.stop();
    handle.join();
    wm.stop_all();
    wm.join_all();

    std::fs::remove_dir_all(&db_dir).ok();
    std::fs::remove_dir_all(&worlds_dir).ok();
    std::env::remove_var("FC_ACCOUNTS_DB");
    std::env::remove_var("FC_ACCOUNT_WORLDS_DIR");
}
