//! Pure-simulation tests for named survivor -> building assignment
//! (`AssignSurvivor`), including its interaction with the existing anonymous
//! `AdjustWorkers` worker pool and with survivor death / building demolition.
//!
//! Style mirrors `building_tests.rs`/`roles_tests.rs`: build a scenario with
//! `sim::new_game` + `sim::apply_command`, then assert on the resulting
//! `GameState`. `new_game` starts with a single survivor (the leader, id 1,
//! furnace id 0 still under construction) — `sim::new_game_bootstrapped`
//! instead starts with 8 (ids 1..=8) and a already-lit furnace, for tests
//! that need more than one idle survivor or don't care about the opening
//! bootstrap. Either way `state.survivors[0].id` etc. are stable to
//! reference directly.

use frozen_city::game::sim;
use frozen_city::game::types::*;

const SEED: u64 = 12345;

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

/// Places `kind` at a valid spot, instantly finishes its construction (the
/// V0.8 build phase has its own suite — `construction_tests.rs`) and drains
/// the auto-crew, returning its building id with workers at exactly 0 —
/// tests staff it explicitly, named or anonymous, as needed.
fn place(state: &mut GameState, kind: BuildingKind) -> u32 {
    state.stock.wood = 500.0;
    let (x, y) = find_spot(state, kind);
    sim::apply_command(state, 1, &PlayerCommand::Place { kind, x, y });
    let id = state.buildings.last().unwrap().id;
    sim::finish_all_construction(state);
    let cur = state.find_building(id).unwrap().workers as i8;
    if cur > 0 {
        sim::apply_command(state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: -cur });
    }
    id
}

#[test]
fn assign_survivor_sets_identity_and_increments_workers() {
    let mut state = sim::new_game(SEED, 12);
    let sawmill = place(&mut state, BuildingKind::Sawmill);
    let survivor = state.survivors[0].id;

    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::AssignSurvivor { survivor, building: Some(sawmill) },
    );

    assert_eq!(state.find_building(sawmill).unwrap().workers, 1);
    assert_eq!(
        state.survivors.iter().find(|s| s.id == survivor).unwrap().assigned_building,
        Some(sawmill)
    );
}

#[test]
fn assign_survivor_reassigns_between_buildings_moves_the_slot_not_duplicates_it() {
    let mut state = sim::new_game(SEED, 12);
    let a = place(&mut state, BuildingKind::Sawmill);
    let b = place(&mut state, BuildingKind::CoalMine);
    let survivor = state.survivors[0].id;

    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor, building: Some(a) });
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor, building: Some(b) });

    assert_eq!(state.find_building(a).unwrap().workers, 0, "old building's slot must be freed");
    assert_eq!(state.find_building(b).unwrap().workers, 1, "new building gets exactly one slot");
    assert_eq!(
        state.survivors.iter().find(|s| s.id == survivor).unwrap().assigned_building,
        Some(b)
    );
}

#[test]
fn assign_survivor_none_clears_and_frees_slot() {
    let mut state = sim::new_game(SEED, 12);
    let sawmill = place(&mut state, BuildingKind::Sawmill);
    let survivor = state.survivors[0].id;

    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor, building: Some(sawmill) });
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor, building: None });

    assert_eq!(state.find_building(sawmill).unwrap().workers, 0);
    assert_eq!(
        state.survivors.iter().find(|s| s.id == survivor).unwrap().assigned_building,
        None
    );
}

#[test]
fn assign_survivor_rejects_full_building() {
    // Bootstrapped: needs a second survivor, and a fresh `new_game` starts
    // with only the leader.
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    let kitchen = place(&mut state, BuildingKind::Kitchen); // max_workers() == 1
    let first = state.survivors[0].id;
    let second = state.survivors[1].id;

    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor: first, building: Some(kitchen) });
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor: second, building: Some(kitchen) });

    assert_eq!(state.find_building(kitchen).unwrap().workers, 1, "the full building must reject the second survivor");
    assert_eq!(
        state.survivors.iter().find(|s| s.id == second).unwrap().assigned_building,
        None,
        "the rejected survivor must not be recorded as assigned"
    );
}

#[test]
fn assign_survivor_is_noop_for_dead_or_unknown_survivor_id() {
    let mut state = sim::new_game(SEED, 12);
    let sawmill = place(&mut state, BuildingKind::Sawmill);

    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor: 99999, building: Some(sawmill) });

    assert_eq!(state.find_building(sawmill).unwrap().workers, 0);
}

#[test]
fn assign_survivor_is_noop_for_unknown_or_zero_capacity_building() {
    // Bootstrapped: a *finished* Furnace has zero worker capacity (nobody
    // needs to staff it once lit — see `BuildingKind::max_workers`). A
    // freshly-`new_game`d Furnace is still under construction and DOES
    // accept a builder — that's covered in `tests/furnace_bootstrap_tests.rs`.
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    let survivor = state.survivors[0].id;

    // Furnace (id 0) accepts no workers once built.
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor, building: Some(0) });
    assert_eq!(
        state.survivors.iter().find(|s| s.id == survivor).unwrap().assigned_building,
        None,
        "the Furnace has zero worker capacity and must reject assignment"
    );

    // A building id that doesn't exist at all.
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor, building: Some(424242) });
    assert_eq!(
        state.survivors.iter().find(|s| s.id == survivor).unwrap().assigned_building,
        None
    );
}

#[test]
fn adjust_workers_cannot_evict_a_named_assignment() {
    let mut state = sim::new_game(SEED, 12);
    let kitchen = place(&mut state, BuildingKind::Kitchen); // max_workers() == 1
    let survivor = state.survivors[0].id;

    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor, building: Some(kitchen) });
    sim::apply_command(&mut state, 1, &PlayerCommand::AdjustWorkers { building: kitchen, delta: -1 });

    assert_eq!(
        state.find_building(kitchen).unwrap().workers, 1,
        "AdjustWorkers must not drain a named assignment's slot"
    );
    assert_eq!(
        state.survivors.iter().find(|s| s.id == survivor).unwrap().assigned_building,
        Some(kitchen),
        "the named survivor must remain assigned"
    );
}

#[test]
fn death_of_a_named_worker_frees_exactly_their_own_buildings_slot() {
    // Bootstrapped: needs a second idle survivor for the anonymous
    // `AdjustWorkers` call below (anonymous workers are capped by
    // `idle_workers()`, and the lone leader is already named elsewhere).
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    let named_building = place(&mut state, BuildingKind::Sawmill);
    let anon_building = place(&mut state, BuildingKind::CoalMine);

    let named_survivor = state.survivors[0].id;
    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::AssignSurvivor { survivor: named_survivor, building: Some(named_building) },
    );
    // An unrelated anonymous worker, staffed the normal way.
    sim::apply_command(&mut state, 1, &PlayerCommand::AdjustWorkers { building: anon_building, delta: 1 });
    assert_eq!(state.find_building(anon_building).unwrap().workers, 1);

    // Kill only the named survivor and tick once so the death path runs.
    state.survivors.iter_mut().find(|s| s.id == named_survivor).unwrap().hp = 0.0;
    sim::tick(&mut state);

    assert!(
        state.survivors.iter().all(|s| s.id != named_survivor),
        "the dead survivor should have been removed"
    );
    assert_eq!(
        state.find_building(named_building).unwrap().workers, 0,
        "the named worker's own building should lose exactly its own slot"
    );
    assert_eq!(
        state.find_building(anon_building).unwrap().workers, 1,
        "an unrelated building's anonymous worker must be untouched by someone else's death"
    );
}

#[test]
fn demolishing_a_building_clears_assigned_survivors_pointer() {
    let mut state = sim::new_game(SEED, 12);
    let sawmill = place(&mut state, BuildingKind::Sawmill);
    let survivor = state.survivors[0].id;

    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor, building: Some(sawmill) });
    sim::apply_command(&mut state, 1, &PlayerCommand::Demolish { building: sawmill });

    assert_eq!(
        state.survivors.iter().find(|s| s.id == survivor).unwrap().assigned_building,
        None,
        "assigned_building must not dangle after its building is demolished"
    );
}

#[test]
fn total_workers_and_idle_workers_stay_consistent_with_named_assignment() {
    // Bootstrapped: needs idle survivors for the anonymous `AdjustWorkers`
    // call below (see the same note in the previous test).
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    let sawmill = place(&mut state, BuildingKind::Sawmill);
    let coal_mine = place(&mut state, BuildingKind::CoalMine);
    let pop = state.survivors.len() as u32;

    let named_survivor = state.survivors[0].id;
    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::AssignSurvivor { survivor: named_survivor, building: Some(sawmill) },
    );
    sim::apply_command(&mut state, 1, &PlayerCommand::AdjustWorkers { building: coal_mine, delta: 2 });

    assert_eq!(state.total_workers(), 3, "1 named + 2 anonymous");
    assert_eq!(state.idle_workers(), pop - 3);
}
