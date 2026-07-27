//! End-to-end test for V0.18's "way back": `ClientMsg::ReturnHome` brings an
//! account's settlers out of the central world and settles them back in that
//! account's own personal world — the mirror of `central_world_e2e.rs`'s
//! outbound `EnterCentral` trip, now closing the loop. Same harness style
//! (crafted on-disk saves rather than a live playthrough, real wire
//! protocol) as `central_world_e2e.rs`/`full_cycle_e2e.rs`.
//!
//! Covers:
//!   (a) full round trip: `EnterCentral` (leader + group migrate out) ->
//!       `ReturnHome` -> the same people are back home with profession/XP
//!       intact, and gone from the central world;
//!   (b) buildings placed in the central world stay there — going home is
//!       not a withdrawal;
//!   (c) a returning group that would exceed `MAX_POPULATION` leaves the
//!       surplus in the central world instead of losing them;
//!   (d) `ReturnHome` for an account with nobody in the central world (and
//!       the central world not even spawned yet) is a plain, successful
//!       sign-in to its own world — no graduation gate;
//!   (e) after returning, `EnterCentral` again re-migrates a group (the
//!       round trip is repeatable) and the earlier building is still there;
//!   (f) a bad login/password is refused exactly like `Login`/`EnterCentral`;
//!   (g) regression for a bug found while reviewing this feature: a personal
//!       world sitting in `GamePhase::Won`/`Lost` (mid the `WORLD_RESET_AFTER`
//!       countdown to an auto-reset that replaces `survivors` wholesale) must
//!       refuse returnees rather than inject them right before they'd be
//!       silently erased — they stay in the central world instead
//!       (`sim::inject_returnees`'s `state.phase != GamePhase::Running` gate).
//!
//! Deliberately a single `#[test]`: it sets `FC_ACCOUNTS_DB` and
//! `FC_ACCOUNT_WORLDS_DIR`, process-wide env vars, and cargo runs `#[test]`
//! fns in one binary on separate threads by default.

use std::time::{Duration, Instant};

use frozen_city::game::sim;
use frozen_city::game::types::{
    BuildingKind, GamePhase, GameState, PlayerCommand, Profession, Survivor,
    CENTRAL_MIGRANTS_PER_ACCOUNT, MAX_POPULATION,
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
    // Id 1 = Aziz (main round trip). Id 2 = Vali (graduated central-world
    // observer, watches account 1/4/5's settlers from the outside). Id 3 =
    // Iroda (never graduated, never touches central — the "plain sign-in"
    // case). Id 4 = Karim (MAX_POPULATION overflow). Id 5 = Nodira
    // (Won-phase regression).
    for (telegram_id, name, login, pw) in [
        (1i64, "Aziz", "fc111111", "pw-aziz"),
        (2i64, "Vali", "fc222222", "pw-vali"),
        (3i64, "Iroda", "fc333333", "pw-iroda"),
        (4i64, "Karim", "fc444444", "pw-karim"),
        (5i64, "Nodira", "fc555555", "pw-nodira"),
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

/// Poll `conn`'s view of the (shared) central world until `account` owns
/// exactly `expected` settlers there — used to observe another account's
/// extraction/injection from an independent connection, exactly the way a
/// second player actually would.
fn wait_owned(conn: &ClientConn, account: i64, expected: usize) -> GameState {
    wait_state(conn, |s| s.owned_settlers(account) == expected)
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

fn return_home_msg(login: &str, password: &str) -> ClientMsg {
    ClientMsg::ReturnHome {
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

/// A minimal, valid settler for directly seeding a central-world save (the
/// crafting trick every V0.5/V0.18 e2e test uses to skip a full playthrough).
/// `sim::new_survivor` (the real constructor `inject_migrants` etc. use) is
/// `pub(crate)` and unreachable from here, so this mirrors its field
/// defaults closely enough for these tests' purposes — nobody here cares
/// about hp/hunger/thirst specifics, only id/owner/profession/xp.
fn make_settler(id: u32, owner: Option<i64>) -> Survivor {
    let (x, y) = GameState::spawn_position(id);
    Survivor {
        id,
        name: format!("Settler{id}"),
        hp: 90.0,
        hunger: 20.0,
        assigned_building: None,
        owner,
        x,
        y,
        move_target: None,
        profession: Profession::ALL[(id as usize) % Profession::ALL.len()],
        xp: 0.0,
        trained_kind: None,
        chop_target: None,
        carrying_wood: false,
        thirst: 15.0,
        bury_target: None,
        fatigue: 0.0,
        sick_left: 0.0,
        age_days: 30.0, // safely adult (ADULT_AGE_DAYS=14, ELDER_AGE_DAYS=90)
        partner: None,
    }
}

/// Push `count` settlers owned by `account` onto `state`, id'd from its own
/// counter so they never collide with ids the central world hands out later
/// (`inject_migrants` draws from the same `next_id`).
fn seed_owned_settlers(state: &mut GameState, account: i64, count: u32) {
    for _ in 0..count {
        let id = state.next_id;
        state.next_id += 1;
        state.survivors.push(make_settler(id, Some(account)));
    }
}

#[test]
fn return_home_round_trip_and_edge_cases() {
    let db_dir = std::env::temp_dir().join(format!("fc-return-home-e2e-db-{}", std::process::id()));
    let worlds_dir =
        std::env::temp_dir().join(format!("fc-return-home-e2e-worlds-{}", std::process::id()));
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

    // --- Aziz (account 1): the main round-trip subject. Bootstrapped
    // population (8), graduated, phase stays Running — the same
    // save-crafting convention `central_world_e2e.rs` uses so this test
    // exercises the migration mechanics, not the graduation/reset dance
    // `full_cycle_e2e.rs` already covers. One survivor (the leader — V0.16's
    // migration always brings the leader first) is marked with a
    // distinctive profession/XP so the round trip's identity-preservation
    // can be checked precisely. ---
    let mut aziz_world = sim::new_game_bootstrapped(111, 12);
    aziz_world.graduated = true;
    let aziz_start_pop = aziz_world.survivors.len();
    assert!(aziz_start_pop > CENTRAL_MIGRANTS_PER_ACCOUNT);
    let leader_id = aziz_world.leader.expect("new_game seeds a leader");
    {
        let leader = aziz_world.survivors.iter_mut().find(|s| s.id == leader_id).unwrap();
        leader.profession = Profession::Medic;
        leader.xp = 42.0;
    }
    persist::save_at(&aziz_world, worlds_dir.join("1.bin").to_str().unwrap()).unwrap();

    // --- Vali (account 2): graduated, minimal population, used purely as an
    // independent observer sitting in the central world throughout — the
    // same way a second real player would see settlers come and go. ---
    let mut vali_world = sim::new_game(222, 12);
    vali_world.graduated = true;
    persist::save_at(&vali_world, worlds_dir.join("2.bin").to_str().unwrap()).unwrap();

    // --- Iroda (account 3): never graduated, never has settlers anywhere —
    // the "nobody in the central world" case. ---
    let iroda_world = sim::new_game(333, 12);
    let iroda_start_pop = iroda_world.survivors.len();
    persist::save_at(&iroda_world, worlds_dir.join("3.bin").to_str().unwrap()).unwrap();

    // --- Karim (account 4): personal world already near the population
    // ceiling (MAX_POPULATION - 2), graduated, Running — set up so a full
    // 5-settler return trip can only fit 2. ---
    let mut karim_world = sim::new_game_bootstrapped(444, 12);
    karim_world.graduated = true;
    while karim_world.survivors.len() < (MAX_POPULATION as usize - 2) {
        let id = karim_world.next_id;
        karim_world.next_id += 1;
        karim_world.survivors.push(make_settler(id, None));
    }
    let karim_start_pop = karim_world.survivors.len();
    assert_eq!(karim_start_pop, MAX_POPULATION as usize - 2);
    persist::save_at(&karim_world, worlds_dir.join("4.bin").to_str().unwrap()).unwrap();

    // --- Nodira (account 5): graduated, but the personal world is sitting
    // in GamePhase::Won — the regression case for the reset-race bug found
    // while reviewing this feature (see module doc, point g). ---
    let mut nodira_world = sim::new_game_bootstrapped(555, 12);
    nodira_world.graduated = true;
    nodira_world.phase = GamePhase::Won;
    let nodira_start_pop = nodira_world.survivors.len();
    persist::save_at(&nodira_world, worlds_dir.join("5.bin").to_str().unwrap()).unwrap();

    // --- The central world's save, pre-seeded with Karim's and Nodira's
    // settlers already living there (as if migrated in an earlier session) —
    // written directly to `central.bin` so it's what the server lazily loads
    // on first central access, same trick `central_world_e2e.rs` uses for
    // the personal-world saves. ---
    let mut central_seed = sim::new_game_central(999);
    seed_owned_settlers(&mut central_seed, 4, 5); // Karim: 5 waiting to come home
    seed_owned_settlers(&mut central_seed, 5, 3); // Nodira: 3 waiting to come home
    persist::save_at(&central_seed, worlds_dir.join("central.bin").to_str().unwrap()).unwrap();

    let wm = WorldManager::new(9001, 12, false);
    let handle = start(9001, wm.clone());
    let addr = addr_of(&handle);

    // === (d) Iroda: ReturnHome with no settlers anywhere, central not even
    // spawned yet, and never graduated — must be a plain, successful
    // sign-in into her own world, not a refusal. ===
    let iroda = client::connect_tcp_with(&addr, return_home_msg("fc333333", "pw-iroda"))
        .expect("iroda dials ReturnHome");
    let (_iroda_pid, iroda_state) = recv_welcome(&iroda);
    assert!(!iroda_state.central, "ReturnHome must land in the personal world");
    assert!(!iroda_state.graduated, "ReturnHome must not require graduation");
    assert_eq!(
        iroda_state.survivors.len(),
        iroda_start_pop,
        "an account with nobody in the central world keeps its own population unchanged"
    );
    drop(iroda);

    // === Vali enters central and stays as the observer for the rest of the
    // test — this is also the first thing to actually spawn the central
    // world, loading the pre-seeded save above. ===
    let vali = client::connect_tcp_with(&addr, enter_central_msg("fc222222", "pw-vali"))
        .expect("vali dials");
    let (_vali_pid, vali_central) = recv_welcome(&vali);
    assert!(vali_central.central);
    assert_eq!(vali_central.owned_settlers(4), 5, "karim's pre-seeded settlers must be there");
    assert_eq!(vali_central.owned_settlers(5), 3, "nodira's pre-seeded settlers must be there");

    // === (g) Nodira: her personal world is Won — ReturnHome must refuse to
    // inject her settlers there (they'd be erased by the next auto-reset)
    // and leave all 3 in the central world instead. ===
    let nodira = client::connect_tcp_with(&addr, return_home_msg("fc555555", "pw-nodira"))
        .expect("nodira dials ReturnHome");
    let (_nodira_pid, nodira_home) = recv_welcome(&nodira);
    assert!(!nodira_home.central);
    assert_eq!(
        nodira_home.phase,
        GamePhase::Won,
        "the crafted Won-phase world stays Won across this short test"
    );
    assert_eq!(
        nodira_home.survivors.len(),
        nodira_start_pop,
        "a Won/Lost personal world must refuse returnees rather than lose them to the next reset"
    );
    drop(nodira);
    wait_owned(&vali, 5, 3); // still all 3 in the central world, nobody lost
    let central_after_nodira = wait_owned(&vali, 5, 3);
    assert_eq!(
        central_after_nodira.owned_settlers(5),
        3,
        "settlers refused by a Won/Lost home must bounce back to the central world, not vanish"
    );

    // === (c) Karim: a full 5-settler return trip only has room for 2
    // (MAX_POPULATION - 2 already at home); the other 3 must stay in the
    // central world rather than being dropped. ===
    let karim = client::connect_tcp_with(&addr, return_home_msg("fc444444", "pw-karim"))
        .expect("karim dials ReturnHome");
    let (_karim_pid, karim_home) = recv_welcome(&karim);
    assert!(!karim_home.central);
    assert_eq!(
        karim_home.survivors.len(),
        MAX_POPULATION as usize,
        "exactly enough returnees must fit to reach the population ceiling, no more"
    );
    drop(karim);
    let central_after_karim = wait_owned(&vali, 4, 3);
    assert_eq!(
        central_after_karim.owned_settlers(4),
        3,
        "the 3 settlers who didn't fit must remain owned in the central world"
    );

    // === (a)+(b) Aziz: full round trip. EnterCentral migrates a group
    // (leader-first) out; place a building while there; ReturnHome brings
    // the SAME people back (profession/XP intact via the marked leader) and
    // clears them from the central world; the building stays put. ===
    let aziz = client::connect_tcp_with(&addr, enter_central_msg("fc111111", "pw-aziz"))
        .expect("aziz dials EnterCentral");
    let (_aziz_central_pid, aziz_central) = recv_welcome(&aziz);
    assert!(aziz_central.central);
    assert_eq!(
        aziz_central.owned_settlers(1),
        CENTRAL_MIGRANTS_PER_ACCOUNT,
        "first entry brings a full settler group"
    );
    let marked_in_central = aziz_central
        .survivors
        .iter()
        .find(|s| s.owner == Some(1) && s.xp == 42.0 && s.profession == Profession::Medic);
    assert!(
        marked_in_central.is_some(),
        "the marked leader must be among the migrated group: {:?}",
        aziz_central.survivors.iter().filter(|s| s.owner == Some(1)).collect::<Vec<_>>()
    );

    // (b) Place a building in the central world as Aziz.
    let spot = (0..64u8)
        .flat_map(|y| (0..64u8).map(move |x| (x, y)))
        .find(|&(x, y)| aziz_central.can_place(BuildingKind::Sawmill, x, y).is_ok())
        .expect("a sawmill spot in the central world");
    aziz.send(ClientMsg::Cmd(PlayerCommand::Place {
        kind: BuildingKind::Sawmill,
        x: spot.0,
        y: spot.1,
        facing: 0,
    }));
    wait_state(&aziz, |s| s.buildings.iter().any(|b| b.kind == BuildingKind::Sawmill));
    drop(aziz);

    // (a) ReturnHome: the group (including the marked leader) comes home.
    let aziz_home = client::connect_tcp_with(&addr, return_home_msg("fc111111", "pw-aziz"))
        .expect("aziz dials ReturnHome");
    let (_aziz_home_pid, home_state) = recv_welcome(&aziz_home);
    assert!(!home_state.central);
    assert_eq!(
        home_state.survivors.len(),
        aziz_start_pop,
        "everyone who left must come back — home population is whole again"
    );
    let marked_at_home = home_state
        .survivors
        .iter()
        .find(|s| s.xp == 42.0 && s.profession == Profession::Medic);
    assert!(
        marked_at_home.is_some(),
        "the marked leader's profession/XP must survive the round trip intact"
    );
    assert_eq!(
        marked_at_home.unwrap().owner,
        None,
        "home again: nobody is a central-world-owned settler in their own city"
    );
    assert!(
        home_state.survivors.iter().all(|s| s.owner.is_none()),
        "no survivor at home should carry a central `owner` tag"
    );

    // Gone from the central world, observed independently through Vali.
    let central_after_return = wait_owned(&vali, 1, 0);
    assert_eq!(
        central_after_return.owned_settlers(1),
        0,
        "aziz's settlers must be gone from the central world after coming home"
    );

    // === (e) Repeatable: EnterCentral again re-migrates a fresh group, and
    // the Sawmill Aziz built earlier is still standing (owned_account intact
    // — going home was never a withdrawal). ===
    drop(aziz_home);
    let aziz2 = client::connect_tcp_with(&addr, enter_central_msg("fc111111", "pw-aziz"))
        .expect("aziz re-enters central");
    let (_aziz2_pid, central2) = recv_welcome(&aziz2);
    assert_eq!(
        central2.owned_settlers(1),
        CENTRAL_MIGRANTS_PER_ACCOUNT,
        "a later EnterCentral must top the group back up from the (now-whole) personal world"
    );
    let sawmill = central2.buildings.iter().find(|b| b.kind == BuildingKind::Sawmill);
    assert!(sawmill.is_some(), "the sawmill built before returning home must still be there");
    assert_eq!(
        sawmill.unwrap().owner_account,
        Some(1),
        "the sawmill must still belong to aziz's account"
    );
    drop(aziz2);
    drop(vali);

    // === (f) Bad login/password refused exactly like Login/EnterCentral —
    // same generic reason, no enumeration of which field was wrong. ===
    let bad_login = client::connect_tcp_with(&addr, login_msg("fc111111", "not-the-password"))
        .expect("bad login dials");
    let login_reason = recv_auth_failed(&bad_login);
    let bad_return = client::connect_tcp_with(&addr, return_home_msg("fc111111", "not-the-password"))
        .expect("bad ReturnHome dials");
    let return_reason = recv_auth_failed(&bad_return);
    assert_eq!(
        return_reason, login_reason,
        "ReturnHome must refuse a bad password with the exact same reason Login does"
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
