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
    sim::apply_command(state, 1, &PlayerCommand::Place { kind, x, y, facing: 0 });
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
fn adjust_workers_sends_a_named_worker_home() {
    // Until V0.20 the `-` button refused to touch a named assignment: it only
    // drained "anonymous slack", the headcount a building carried with nobody
    // actually bound to it. V0.20 binds a real survivor to every slot, which
    // leaves no slack for `-` to drain — so the old rule would have made the
    // button permanently dead. `-` now means what it looks like: send someone
    // home. `AssignSurvivor { building: None }` is still the way to release
    // one SPECIFIC person; this releases the most recently added.
    let mut state = sim::new_game(SEED, 12);
    let kitchen = place(&mut state, BuildingKind::Kitchen); // max_workers() == 1
    let survivor = state.survivors[0].id;

    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor, building: Some(kitchen) });
    assert_eq!(state.find_building(kitchen).unwrap().workers, 1, "sanity: assigned");

    sim::apply_command(&mut state, 1, &PlayerCommand::AdjustWorkers { building: kitchen, delta: -1 });

    assert_eq!(
        state.find_building(kitchen).unwrap().workers, 0,
        "`-` must actually free the slot"
    );
    assert_eq!(
        state.survivors.iter().find(|s| s.id == survivor).unwrap().assigned_building,
        None,
        "and the survivor it freed must really be unassigned, not just uncounted"
    );
    assert_eq!(state.idle_workers(), state.survivors.len() as u32, "everyone is idle again");
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

// --- Headcount integrity: a named assignment must never double-count ---
//
// `Building.workers` is a headcount of named survivors plus anonymous slack.
// Reported from play: placing a building auto-crews it from the idle pool
// ("1 building, 0 idle"), and then assigning that same survivor to the site by
// name made it read "2 building" — one person counted twice.

/// The colony can never have more people at work than it has people.
fn assert_headcount_sane(state: &GameState, when: &str) {
    let total: u32 = state.buildings.iter().map(|b| b.workers as u32).sum();
    assert!(
        total <= state.survivors.len() as u32,
        "{when}: {total} at work but only {} survivors exist",
        state.survivors.len()
    );
    for b in &state.buildings {
        let named = state.survivors.iter().filter(|s| s.assigned_building == Some(b.id)).count();
        assert!(
            named <= b.workers as usize,
            "{when}: building {} has {named} named workers but a headcount of {}",
            b.id,
            b.workers
        );
    }
}

#[test]
fn a_placed_site_crews_itself_with_real_named_survivors() {
    // Reported from play: a freshly placed Tent said "1 building" while the
    // idle count read 0 and the only survivor stood around with an empty
    // workplace, walking nowhere. `Place` used to raise `workers` anonymously
    // — a headcount bound to nobody — so the number claimed a worker the
    // world never actually sent. V0.20 crews by NAME.
    let mut state = sim::new_game_bootstrapped(7, 12);
    state.stock.wood = 500.0;
    let idle_before = state.idle_workers();
    assert!(idle_before > 0, "sanity: somebody is free to be crewed");

    let (x, y) = find_spot(&state, BuildingKind::Tent);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y, facing: 0 });
    let site = state.buildings.last().unwrap().id;

    let crewed = state.find_building(site).unwrap().workers as usize;
    let named = state.survivors.iter().filter(|s| s.assigned_building == Some(site)).count();
    assert!(crewed > 0, "a new site should pull a crew off the idle pool");
    assert_eq!(
        named, crewed,
        "every worker the site counts must be a real survivor standing in it —          that mismatch is the whole bug"
    );
    assert_eq!(
        state.idle_workers() as usize,
        idle_before as usize - crewed,
        "the idle pool should drop by exactly the people who took the job"
    );
    assert_headcount_sane(&state, "after placing");
}

#[test]
fn naming_a_survivor_to_the_site_that_already_crewed_them_is_a_noop() {
    // The other half of the same story: once the site has crewed someone by
    // name, the player clicking that same person onto that same site must not
    // count them twice (`AssignSurvivor`'s `prev == Some(new_id)` guard).
    let mut state = sim::new_game_bootstrapped(7, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y, facing: 0 });
    let site = state.buildings.last().unwrap().id;

    let before = state.find_building(site).unwrap().workers;
    let who = state
        .survivors
        .iter()
        .find(|s| s.assigned_building == Some(site))
        .map(|s| s.id)
        .expect("the site crewed somebody");
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor: who, building: Some(site) });

    assert_eq!(
        state.find_building(site).unwrap().workers, before,
        "re-naming someone already working here must not add a second slot"
    );
    assert_headcount_sane(&state, "after re-naming");
}

#[test]
fn naming_a_survivor_with_real_slack_still_grows_the_headcount() {
    // The other half of the rule: when the colony genuinely has idle hands,
    // a named assignment is a NEW worker and the headcount must rise.
    let mut state = sim::new_game_bootstrapped(7, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y, facing: 0 });
    let mill = state.buildings.last().unwrap().id;
    sim::finish_all_construction(&mut state);
    // Clear whatever crew the placement left behind, so the building starts
    // empty with the colony still full of idle people.
    let crew = state.find_building(mill).unwrap().workers as i8;
    if crew > 0 {
        sim::apply_command(&mut state, 1, &PlayerCommand::AdjustWorkers { building: mill, delta: -crew });
    }
    assert_eq!(state.find_building(mill).unwrap().workers, 0);
    assert!(state.idle_workers() > 0, "sanity: the colony has slack");

    let who = state.survivors[0].id;
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor: who, building: Some(mill) });

    assert_eq!(state.find_building(mill).unwrap().workers, 1);
    assert_headcount_sane(&state, "after naming with slack");
}
