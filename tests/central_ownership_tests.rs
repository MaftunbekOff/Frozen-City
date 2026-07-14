//! Pure-simulation tests for V0.5 "leftover" central-world work: account-based
//! building ownership (only the owning ACCOUNT may demolish a central
//! building it placed, across sessions) and the per-account contribution
//! ledger (economy v1). Also covers the V2 -> V3 save-format migration this
//! work required (`Building::owner_account`, `GameState::central_ledger`).
//! Mirrors the style of `central_tests.rs` / `roles_tests.rs`.

use frozen_city::game::sim;
use frozen_city::game::types::*;
use frozen_city::net::legacy::{BuildingV2, GameStateV2, SurvivorV3};
use frozen_city::net::persist;

fn find_spot(state: &GameState, kind: BuildingKind) -> (u8, u8) {
    for y in 0..MAP_H as u8 {
        for x in 0..MAP_W as u8 {
            if state.can_place(kind, x, y).is_ok() {
                return (x, y);
            }
        }
    }
    panic!("no valid spot for {kind:?}");
}

fn settler(id: u32) -> Survivor {
    let (x, y) = GameState::spawn_position(id);
    Survivor {
        id,
        name: format!("S{id}"),
        hp: 90.0,
        hunger: 10.0,
        assigned_building: None,
        owner: None,
        x,
        y,
        move_target: None,
        profession: Profession::from_id_hash(id),
        xp: 0.0,
        trained_kind: None,
        chop_target: None,
        carrying_wood: false,
    }
}

// --- Account-based building ownership ---

#[test]
fn central_building_is_owned_by_the_placing_account() {
    let mut state = sim::new_game_central(5);
    sim::player_joined_as(&mut state, 10, "Aziz", Some(1));
    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    sim::apply_command(&mut state, 10, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y });

    let b = state.buildings.iter().find(|b| b.kind == BuildingKind::Sawmill).unwrap();
    assert_eq!(b.owner, Some(10), "session attribution unchanged");
    assert_eq!(b.owner_account, Some(1), "central buildings are additionally account-owned");
}

#[test]
fn only_the_owning_account_may_demolish_across_sessions() {
    let mut state = sim::new_game_central(5);
    // Aziz places a sawmill from player-session 10.
    sim::player_joined_as(&mut state, 10, "Aziz", Some(1));
    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    sim::apply_command(&mut state, 10, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y });
    let sawmill = state.buildings.iter().find(|b| b.kind == BuildingKind::Sawmill).unwrap().id;

    // Aziz disconnects and reconnects as a NEW session id (11) — same
    // account. Ownership must follow the account, not the stale session id.
    state.players.retain(|p| p.id != 10);
    sim::player_joined_as(&mut state, 11, "Aziz", Some(1));
    assert!(
        state.can_issue(11, &PlayerCommand::Demolish { building: sawmill }),
        "the same account, even from a fresh session, may demolish what it placed"
    );
    sim::apply_command(&mut state, 11, &PlayerCommand::Demolish { building: sawmill });
    assert!(state.find_building(sawmill).is_none(), "demolish must actually go through");
}

#[test]
fn a_different_account_may_not_demolish_someone_elses_central_building() {
    let mut state = sim::new_game_central(5);
    sim::player_joined_as(&mut state, 10, "Aziz", Some(1));
    sim::player_joined_as(&mut state, 20, "Vali", Some(2));
    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    sim::apply_command(&mut state, 10, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y });
    let sawmill = state.buildings.iter().find(|b| b.kind == BuildingKind::Sawmill).unwrap().id;

    assert!(
        !state.can_issue(20, &PlayerCommand::Demolish { building: sawmill }),
        "a stranger account must never demolish another account's central building"
    );
    sim::apply_command(&mut state, 20, &PlayerCommand::Demolish { building: sawmill });
    assert!(state.find_building(sawmill).is_some(), "must remain standing: demolish was a no-op");

    // Even the ORIGINAL session id alone (10) isn't enough once ownership is
    // account-based — but here 10 is still logged in as Aziz, so it's still
    // allowed. Prove the negative differently: an anonymous/guest session (no
    // account) must not be able to piggyback on a stale player id either.
    sim::player_joined(&mut state, 30, "Guest"); // no account
    assert!(
        !state.can_issue(30, &PlayerCommand::Demolish { building: sawmill }),
        "an unauthenticated session must never demolish an account-owned central building"
    );
}

#[test]
fn legacy_central_building_with_no_owner_account_stays_demolishable_by_placing_session() {
    // Migration default: `owner_account: None`, as any pre-V3 central
    // building becomes. Behavior must match the OLD rule exactly (session-id
    // check), not become permanently stuck to nobody.
    let mut state = sim::new_game_central(5);
    sim::player_joined_as(&mut state, 10, "Aziz", Some(1));
    state.buildings.push(Building {
        id: 999,
        kind: BuildingKind::Sawmill,
        x: 5,
        y: 5,
        workers: 0,
        progress: 0.0,
        owner: Some(10),
        owner_account: None, // migration default
        level: 1,
        build_left: 0.0,
    });
    assert!(
        state.can_issue(10, &PlayerCommand::Demolish { building: 999 }),
        "legacy central building: the placing SESSION may still demolish it"
    );
    sim::player_joined_as(&mut state, 20, "Vali", Some(2));
    assert!(
        !state.can_issue(20, &PlayerCommand::Demolish { building: 999 }),
        "but a different session/account may not"
    );
}

#[test]
fn personal_and_guest_world_owner_role_logic_is_untouched() {
    // Sanity: non-central worlds keep using Role::Owner/Guest + session-id
    // building `owner`, never touching `owner_account`.
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    sim::player_joined(&mut state, 2, "Guest");
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    sim::apply_command(&mut state, 2, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y });
    let tent = state.buildings.iter().find(|b| b.kind == BuildingKind::Tent).unwrap();
    assert_eq!(tent.owner, Some(2));
    assert_eq!(tent.owner_account, None, "owner_account is a central-world-only concept");
    assert!(state.is_owner(1));
    assert_eq!(state.player(1).unwrap().role, Role::Owner);
    assert_eq!(state.player(2).unwrap().role, Role::Guest);
}

// --- Contribution ledger (economy v1) ---

#[test]
fn ledger_is_empty_outside_the_central_world() {
    let state = sim::new_game(5, 12);
    assert!(state.central_ledger.is_empty());
    let state = sim::new_game_central(5);
    assert!(state.central_ledger.is_empty(), "empty until an account actually contributes");
}

#[test]
fn placing_a_central_building_debits_the_account_ledger_for_wood_spent() {
    let mut state = sim::new_game_central(5);
    sim::player_joined_as(&mut state, 10, "Aziz", Some(1));
    let before_wood = state.stock.wood;
    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    sim::apply_command(&mut state, 10, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y });

    let spent = before_wood - state.stock.wood;
    assert!(spent > 0.0, "sanity: a sawmill costs wood");
    let totals = state.ledger_for(1);
    assert_eq!(totals.wood_spent, spent, "ledger must record exactly what was spent");
    assert_eq!(totals.wood, 0.0, "spend is tracked separately from production credit");
}

#[test]
fn staffed_central_production_credits_the_owning_account() {
    let mut state = sim::new_game_central(5);
    sim::player_joined_as(&mut state, 10, "Aziz", Some(1));
    let (x, y) = find_spot(&state, BuildingKind::HunterHut);
    sim::apply_command(&mut state, 10, &PlayerCommand::Place { kind: BuildingKind::HunterHut, x, y });
    let hut = state.buildings.iter().find(|b| b.kind == BuildingKind::HunterHut).unwrap().id;
    // V0.8: bu test bitgan binoning ledger-kreditini sinaydi — markaziy
    // olamda hali aholi yo'q (brigada 0), maydonchani darhol bitiramiz.
    sim::finish_all_construction(&mut state);

    sim::inject_migrants(&mut state, 1, "Aziz", vec![settler(1)]);
    let s_id = state.survivors.iter().find(|s| s.owner == Some(1)).unwrap().id;
    sim::apply_command(
        &mut state,
        10,
        &PlayerCommand::AssignSurvivor { survivor: s_id, building: Some(hut) },
    );
    assert_eq!(state.find_building(hut).unwrap().workers, 1);

    let food_before = state.stock.food;
    for _ in 0..(TICKS_PER_DAY / 2) {
        sim::tick(&mut state);
    }
    let food_gained = state.stock.food - food_before;
    assert!(food_gained > 0.0, "sanity: a staffed hunter hut produces food");

    let totals = state.ledger_for(1);
    assert!(
        (totals.food - food_gained).abs() < 1e-3,
        "credited food ({}) must match what actually landed in the stockpile ({food_gained})",
        totals.food
    );
    assert_eq!(totals.wood, 0.0);
    assert_eq!(totals.coal, 0.0);
}

#[test]
fn ledger_never_updates_outside_the_central_world() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y });
    let sawmill = state.buildings.iter().find(|b| b.kind == BuildingKind::Sawmill).unwrap().id;
    let first_survivor = state.survivors[0].id;
    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::AssignSurvivor { survivor: first_survivor, building: Some(sawmill) },
    );
    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut state);
    }
    assert!(state.central_ledger.is_empty(), "personal-world production never touches the ledger");
}

#[test]
fn ledger_is_per_account_and_deterministic_across_replays() {
    let run = || {
        let mut state = sim::new_game_central(5);
        sim::player_joined_as(&mut state, 10, "Aziz", Some(1));
        sim::player_joined_as(&mut state, 20, "Vali", Some(2));
        let (x1, y1) = find_spot(&state, BuildingKind::Sawmill);
        sim::apply_command(&mut state, 10, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x: x1, y: y1 });
        let sawmill = state.buildings.iter().find(|b| b.kind == BuildingKind::Sawmill).unwrap().id;
        sim::inject_migrants(&mut state, 1, "Aziz", vec![settler(1)]);
        let aziz_settler = state.survivors.iter().find(|s| s.owner == Some(1)).unwrap().id;
        sim::apply_command(
            &mut state,
            10,
            &PlayerCommand::AssignSurvivor { survivor: aziz_settler, building: Some(sawmill) },
        );
        for _ in 0..(TICKS_PER_DAY / 3) {
            sim::tick(&mut state);
        }
        state
    };
    let a = run();
    let b = run();
    assert_eq!(a.central_ledger, b.central_ledger, "same inputs must produce the same ledger");
    assert_eq!(a.ledger_for(2), ContributionTotals::default(), "Vali contributed nothing");
    assert!(a.ledger_for(1).wood > 0.0 || a.ledger_for(1).wood_spent > 0.0);
}

// --- Save-format migration: V2 -> V3 roundtrip ---

#[test]
fn v2_mirror_migrates_to_v3_with_none_owner_account_and_empty_ledger() {
    let mut modern = sim::new_game_central(7);
    modern.buildings.push(Building {
        id: 42,
        kind: BuildingKind::Sawmill,
        x: 3,
        y: 3,
        workers: 0,
        progress: 0.0,
        owner: Some(10),
        owner_account: None,
        level: 1,
        build_left: 0.0,
    });
    let v2 = GameStateV2 {
        tick: modern.tick,
        win_days: modern.win_days,
        tiles: modern.tiles.clone(),
        buildings: modern
            .buildings
            .iter()
            .map(|b| BuildingV2 {
                id: b.id,
                kind: b.kind,
                x: b.x,
                y: b.y,
                workers: b.workers,
                progress: b.progress,
                owner: b.owner,
            })
            .collect(),
        survivors: modern.survivors.iter().map(SurvivorV3::from).collect(),
        stock: modern.stock,
        furnace_level: modern.furnace_level,
        furnace_lit: modern.furnace_lit,
        cold_snap: modern.cold_snap,
        players: modern.players.clone(),
        phase: modern.phase,
        events: modern.events.clone(),
        total_events: modern.total_events,
        chat: modern.chat.clone(),
        total_chat: modern.total_chat,
        pings: modern.pings.clone(),
        missions: modern.missions.clone(),
        tunnel: modern.tunnel,
        graduated: modern.graduated,
        central: modern.central,
        techs: modern.techs.clone(),
        disease_until: modern.disease_until,
        blizzard_until: modern.blizzard_until,
        pending_event: modern.pending_event,
        event_rng: modern.event_rng,
        guest_perm: modern.guest_perm,
        owner_id: modern.owner_id,
        next_id: modern.next_id,
        rng: modern.rng,
    };

    let migrated: GameState = v2.into();
    // The starting Furnace (id 0, owner None) and Tunnel fixture (from
    // `new_game`, unmodified by `new_game_central`) plus the sawmill pushed
    // above.
    assert_eq!(migrated.buildings.len(), 3);
    let sawmill = migrated.buildings.iter().find(|b| b.id == 42).expect("sawmill survives the hop");
    assert_eq!(sawmill.owner, Some(10));
    assert!(
        migrated.buildings.iter().all(|b| b.owner_account.is_none()),
        "V2 predates account ownership"
    );
    assert!(migrated.central_ledger.is_empty(), "V2 predates the contribution ledger");
    assert!(migrated.central, "central flag must survive the hop");
}

#[test]
fn v2_file_on_disk_round_trips_through_persist_load_at() {
    let path = std::env::temp_dir()
        .join(format!("fc-central-ownership-test-v2-{}.bin", std::process::id()))
        .to_str()
        .unwrap()
        .to_string();

    let mut modern = sim::new_game_central(9);
    sim::player_joined_as(&mut modern, 10, "Aziz", Some(1));
    let v2 = GameStateV2 {
        tick: modern.tick,
        win_days: modern.win_days,
        tiles: modern.tiles.clone(),
        buildings: modern
            .buildings
            .iter()
            .map(|b| BuildingV2 {
                id: b.id,
                kind: b.kind,
                x: b.x,
                y: b.y,
                workers: b.workers,
                progress: b.progress,
                owner: b.owner,
            })
            .collect(),
        survivors: modern.survivors.iter().map(SurvivorV3::from).collect(),
        stock: modern.stock,
        furnace_level: modern.furnace_level,
        furnace_lit: modern.furnace_lit,
        cold_snap: modern.cold_snap,
        players: modern.players.clone(),
        phase: modern.phase,
        events: modern.events.clone(),
        total_events: modern.total_events,
        chat: modern.chat.clone(),
        total_chat: modern.total_chat,
        pings: modern.pings.clone(),
        missions: modern.missions.clone(),
        tunnel: modern.tunnel,
        graduated: modern.graduated,
        central: modern.central,
        techs: modern.techs.clone(),
        disease_until: modern.disease_until,
        blizzard_until: modern.blizzard_until,
        pending_event: modern.pending_event,
        event_rng: modern.event_rng,
        guest_perm: modern.guest_perm,
        owner_id: modern.owner_id,
        next_id: modern.next_id,
        rng: modern.rng,
    };
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"FCWORLD2");
    bytes.extend_from_slice(&bincode::serialize(&v2).unwrap());
    std::fs::write(&path, bytes).unwrap();

    let loaded = persist::load_at(&path).expect("a V2 production save must load, never wipe");
    assert_eq!(loaded.players.len(), 1);
    assert!(loaded.central_ledger.is_empty());
    assert!(loaded.buildings.iter().all(|b| b.owner_account.is_none()));

    // And re-saving now writes V3, which round-trips as-is.
    persist::save_at(&loaded, &path).unwrap();
    assert_eq!(persist::load_at(&path).expect("V3 reload"), loaded);
    std::fs::remove_file(&path).ok();
}
