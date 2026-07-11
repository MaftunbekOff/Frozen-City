//! Pure-simulation tests for the central world (the Global World through the
//! Tunnel): settler migration in/out, per-account command authority, and the
//! "permanent meeting place" tick rules (no hunger, no death, no win/lose).
//! Mirrors the style of `roles_tests.rs` / `mission_tests.rs`.

use frozen_city::game::sim;
use frozen_city::game::types::*;

#[test]
fn central_world_starts_empty_and_flagged() {
    let state = sim::new_game_central(5);

    assert!(state.central);
    assert!(state.survivors.is_empty(), "settlers only arrive through the Tunnel");
    assert!(state.missions.is_empty(), "no personal-world progression in the hub");
    assert!(!state.tunnel.unlocked);
    assert_eq!(state.phase, GamePhase::Running);
}

#[test]
fn empty_central_world_never_loses_or_wins_by_days() {
    let mut state = sim::new_game_central(5);

    // One tick with zero survivors would flag an ordinary world Lost.
    sim::tick(&mut state);
    assert_eq!(state.phase, GamePhase::Running, "an empty hub must not be a defeat");

    // Jump to just before the day-count victory would fire and cross it.
    state.tick = state.win_days as u64 * TICKS_PER_DAY - 1;
    sim::tick(&mut state);
    sim::tick(&mut state);
    assert_eq!(state.phase, GamePhase::Running, "no day-count victory in the hub");
}

#[test]
fn central_settlers_never_hunger_or_die() {
    let mut state = sim::new_game_central(5);
    let migrants = vec![Survivor {
        id: 1,
        name: "Anna".into(),
        hp: 1.0, // would freeze/starve to death in one bad day anywhere else
        hunger: 119.0,
        assigned_building: None,
        owner: None,
    }];
    sim::inject_migrants(&mut state, 42, "Aziz", migrants);
    state.stock.food = 0.0;
    state.furnace_level = 0; // no heat at all

    for _ in 0..(2 * TICKS_PER_DAY) {
        sim::tick(&mut state);
    }

    assert_eq!(state.survivors.len(), 1, "settlers are a presence, not mouths to feed");
    let s = &state.survivors[0];
    assert_eq!(s.hp, 1.0, "hp must not drift in the hub");
    assert_eq!(s.hunger, 119.0, "hunger must not drift in the hub");
}

#[test]
fn inject_reids_sets_owner_and_caps_per_account() {
    let mut state = sim::new_game_central(5);
    let make = |id: u32| Survivor {
        id,
        name: format!("S{id}"),
        hp: 90.0,
        hunger: 10.0,
        assigned_building: Some(7), // stale reference from the source world
        owner: None,
    };

    // Two batches from "different personal worlds" with clashing ids.
    let settled = sim::inject_migrants(&mut state, 1, "Aziz", vec![make(1), make(2), make(3)]);
    assert_eq!(settled, 3);
    let settled = sim::inject_migrants(&mut state, 2, "Vali", vec![make(1), make(2)]);
    assert_eq!(settled, 2);

    let ids: Vec<u32> = state.survivors.iter().map(|s| s.id).collect();
    let mut deduped = ids.clone();
    deduped.sort_unstable();
    deduped.dedup();
    assert_eq!(ids.len(), deduped.len(), "settler ids must be re-issued uniquely: {ids:?}");
    assert!(
        state.survivors.iter().all(|s| s.assigned_building.is_none()),
        "stale building assignments from the source world must be cleared"
    );
    assert_eq!(state.owned_settlers(1), 3);
    assert_eq!(state.owned_settlers(2), 2);

    // Overfilling one account stops at the cap.
    let over: Vec<Survivor> = (10..20).map(make).collect();
    let settled = sim::inject_migrants(&mut state, 1, "Aziz", over);
    assert_eq!(
        state.owned_settlers(1),
        CENTRAL_MIGRANTS_PER_ACCOUNT,
        "an account may never exceed the settler cap"
    );
    assert_eq!(settled, CENTRAL_MIGRANTS_PER_ACCOUNT - 3);
}

#[test]
fn extract_prefers_idle_and_frees_staffed_slots() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    let start_pop = state.survivors.len();
    assert!(start_pop >= 4, "sanity: default start population");

    // Stand up a sawmill and pin one named survivor to it.
    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y });
    let sawmill = state.buildings.iter().find(|b| b.kind == BuildingKind::Sawmill).unwrap().id;
    let pinned = state.survivors[0].id;
    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::AssignSurvivor { survivor: pinned, building: Some(sawmill) },
    );
    assert_eq!(state.find_building(sawmill).unwrap().workers, 1);

    // Taking fewer than the idle pool must not touch the pinned worker.
    let taken = sim::extract_migrants(&mut state, 2);
    assert_eq!(taken.len(), 2);
    assert!(taken.iter().all(|s| s.id != pinned), "idle survivors go first");
    assert_eq!(state.find_building(sawmill).unwrap().workers, 1);

    // Draining everyone must free the sawmill slot on the way out.
    let rest = sim::extract_migrants(&mut state, 100);
    assert_eq!(rest.len(), start_pop - 2);
    assert!(state.survivors.is_empty());
    assert_eq!(
        state.find_building(sawmill).unwrap().workers,
        0,
        "a migrating worker must free their staffed slot"
    );
    assert!(rest.iter().all(|s| s.assigned_building.is_none()));
}

#[test]
fn central_authority_follows_settler_ownership() {
    let mut state = sim::new_game_central(5);
    // Aziz (account 1) and Vali (account 2), each with one settler.
    sim::player_joined_as(&mut state, 10, "Aziz", Some(1));
    sim::player_joined_as(&mut state, 20, "Vali", Some(2));
    let mk = |id: u32| Survivor {
        id,
        name: format!("S{id}"),
        hp: 90.0,
        hunger: 10.0,
        assigned_building: None,
        owner: None,
    };
    sim::inject_migrants(&mut state, 1, "Aziz", vec![mk(1)]);
    sim::inject_migrants(&mut state, 2, "Vali", vec![mk(2)]);
    let azizs = state.survivors.iter().find(|s| s.owner == Some(1)).unwrap().id;
    let valis = state.survivors.iter().find(|s| s.owner == Some(2)).unwrap().id;

    // Nobody is Owner in the hub — not even the first joiner.
    assert!(state.owner_id.is_none());
    assert!(state.players.iter().all(|p| p.role == Role::Guest));

    let assign_own = PlayerCommand::AssignSurvivor { survivor: azizs, building: None };
    let assign_foreign = PlayerCommand::AssignSurvivor { survivor: valis, building: None };
    assert!(state.can_issue(10, &assign_own), "your settler: allowed");
    assert!(!state.can_issue(10, &assign_foreign), "someone else's settler: denied");
    assert!(state.can_issue(20, &assign_foreign), "...but their owner may");

    assert!(
        !state.can_issue(10, &PlayerCommand::AdjustWorkers { building: 0, delta: 1 }),
        "the anonymous worker pool has no meaning where every settler is owned"
    );
    assert!(!state.can_issue(10, &PlayerCommand::SetFurnaceLevel { level: 3 }));
    assert!(!state.can_issue(10, &PlayerCommand::InvestTunnel));
    assert!(!state.can_issue(10, &PlayerCommand::Research { tech: Tech::Tools }));
    assert!(!state.can_issue(10, &PlayerCommand::RespondEvent { accept: true }));
    assert!(state.can_issue(10, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x: 1, y: 1 }));

    // And the full apply path respects it: Vali's settler stays put when Aziz
    // tries to move him onto a building.
    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    sim::apply_command(&mut state, 10, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y });
    let sawmill = state.buildings.iter().find(|b| b.kind == BuildingKind::Sawmill).unwrap().id;
    sim::apply_command(
        &mut state,
        10,
        &PlayerCommand::AssignSurvivor { survivor: valis, building: Some(sawmill) },
    );
    assert_eq!(
        state.survivors.iter().find(|s| s.id == valis).unwrap().assigned_building,
        None,
        "a foreign assignment attempt must be a no-op"
    );
    sim::apply_command(
        &mut state,
        10,
        &PlayerCommand::AssignSurvivor { survivor: azizs, building: Some(sawmill) },
    );
    assert_eq!(
        state.survivors.iter().find(|s| s.id == azizs).unwrap().assigned_building,
        Some(sawmill),
        "your own settler assignment must work"
    );
}

// --- helpers ---

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
