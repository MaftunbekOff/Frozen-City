//! Pure-simulation tests for V0.12: the cow/sheep livestock resource system
//! (Farmhouse raising -> food) and `Profession::Farmer` matching BOTH
//! `Greenhouse` and `Farmhouse` (the one profession with two trades instead
//! of one). Style mirrors `wildlife_tests.rs` closely — livestock/Farmhouse
//! are the domesticated counterpart to wildlife/HunterHut, same mechanics,
//! different flavor.

use frozen_city::game::sim;
use frozen_city::game::types::*;

const SEED: u64 = 12345;

/// Places `kind` at `(x, y)`, instantly finishes its construction and staffs
/// it to exactly `workers`, returning the new building's id. Mirrors
/// `wildlife_tests.rs`'s helper of the same name.
fn place_and_staff(state: &mut GameState, kind: BuildingKind, x: u8, y: u8, workers: i8) -> u32 {
    sim::apply_command(state, 1, &PlayerCommand::Place { kind, x, y });
    let id = state.buildings.last().unwrap().id;
    sim::finish_all_construction(state);
    let cur = state.find_building(id).unwrap().workers as i8;
    sim::apply_command(state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: workers - cur });
    id
}

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

#[test]
fn livestock_starts_nonzero_and_regenerates_toward_cap() {
    let state = sim::new_game(SEED, 12);
    assert_eq!(state.livestock.cow, COW_START, "a fresh world starts with a real cow herd");
    assert_eq!(state.livestock.sheep, SHEEP_START, "a fresh world starts with a real sheep flock");

    let mut state = sim::new_game(SEED, 12);
    state.livestock.cow = 5.0; // well below COW_CAP
    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut state);
    }
    assert!(state.livestock.cow > 5.0, "cows should regrow over a day: got {}", state.livestock.cow);
    assert!(state.livestock.cow < COW_CAP, "one day shouldn't reach the cap from 5.0");
}

#[test]
fn livestock_never_exceeds_cap() {
    let mut state = sim::new_game(SEED, 12);
    state.livestock.cow = COW_CAP;
    state.livestock.sheep = SHEEP_CAP;
    for _ in 0..(TICKS_PER_DAY * 5) {
        sim::tick(&mut state);
        assert!(state.livestock.cow <= COW_CAP, "cow herd exceeded its cap: {}", state.livestock.cow);
        assert!(state.livestock.sheep <= SHEEP_CAP, "sheep flock exceeded its cap: {}", state.livestock.sheep);
    }
}

#[test]
fn staffed_farmhouse_produces_food_and_depletes_livestock() {
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);
    control.stock.wood = 500.0;
    experiment.stock.wood = 500.0;

    let (x, y) = find_spot(&experiment, BuildingKind::Farmhouse);
    place_and_staff(&mut experiment, BuildingKind::Farmhouse, x, y, 2);

    let control_livestock_before = control.livestock.cow + control.livestock.sheep;
    let experiment_livestock_before = experiment.livestock.cow + experiment.livestock.sheep;

    // A few hours (not a full day) isolates farming's depletion from
    // regen's noise, which would otherwise partially mask the delta over a
    // longer run — same reasoning as the wildlife/HunterHut test.
    let ticks_per_hour = TICKS_PER_DAY / 24;
    for _ in 0..(ticks_per_hour * 6) {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        experiment.stock.food > control.stock.food,
        "a staffed Farmhouse should produce more food than the control: control={}, experiment={}",
        control.stock.food,
        experiment.stock.food
    );
    let control_livestock_after = control.livestock.cow + control.livestock.sheep;
    let experiment_livestock_after = experiment.livestock.cow + experiment.livestock.sheep;
    assert!(
        experiment_livestock_after - experiment_livestock_before
            < control_livestock_after - control_livestock_before,
        "farming should net-deplete livestock relative to the unfarmed control"
    );
}

#[test]
fn farmhouse_gracefully_idles_when_livestock_is_exhausted() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    state.livestock.cow = 0.0;
    state.livestock.sheep = 0.0;

    let (x, y) = find_spot(&state, BuildingKind::Farmhouse);
    place_and_staff(&mut state, BuildingKind::Farmhouse, x, y, 2);

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut state);
        assert!(state.livestock.cow >= 0.0, "cow herd must never go negative");
        assert!(state.livestock.sheep >= 0.0, "sheep flock must never go negative");
    }

    // Livestock starts and stays at 0.0 (logistic regen from 0 never leaves
    // 0), so farming has nothing to draw from the whole run — no panic, no
    // negative stock, just idle production, same as an empty coal deposit.
    assert_eq!(state.livestock.cow, 0.0);
    assert_eq!(state.livestock.sheep, 0.0);

    let mut control = sim::new_game(SEED, 12);
    control.stock.wood = 500.0;
    control.livestock.cow = 0.0;
    control.livestock.sheep = 0.0;
    // No Farmhouse placed at all — isolates ordinary hunger consumption from
    // any farming income.
    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
    }
    assert!(
        (state.stock.food - control.stock.food).abs() < 0.01,
        "an exhausted Farmhouse should add no food income beyond ordinary consumption: \
         no-farm={}, exhausted-farm={}",
        control.stock.food,
        state.stock.food
    );
}

#[test]
fn farmer_matches_farmhouse_for_profession_bonus() {
    // Mirrors `wildlife_tests.rs`'s sibling test but for Farmhouse — a named
    // Farmer should out-produce an anonymous worker there, same as at their
    // other trade (Greenhouse, checked separately below).
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);
    for state in [&mut control, &mut experiment] {
        state.stock.wood = 500.0;
    }

    let (x, y) = find_spot(&control, BuildingKind::Farmhouse);
    sim::apply_command(&mut control, 1, &PlayerCommand::Place { kind: BuildingKind::Farmhouse, x, y });
    let control_id = control.buildings.last().unwrap().id;
    sim::finish_all_construction(&mut control);
    let cur = control.find_building(control_id).unwrap().workers as i8;
    sim::apply_command(&mut control, 1, &PlayerCommand::AdjustWorkers { building: control_id, delta: -cur });
    let control_survivor = control.survivors[0].id;
    // Force a Farmer in the control too, but leave them UNASSIGNED
    // (anonymous pool) so the profession bonus has no identity to attach to.
    control.survivors.iter_mut().find(|s| s.id == control_survivor).unwrap().profession =
        Profession::Farmer;
    sim::apply_command(&mut control, 1, &PlayerCommand::AdjustWorkers { building: control_id, delta: 1 });

    let (x, y) = find_spot(&experiment, BuildingKind::Farmhouse);
    sim::apply_command(&mut experiment, 1, &PlayerCommand::Place { kind: BuildingKind::Farmhouse, x, y });
    let exp_id = experiment.buildings.last().unwrap().id;
    sim::finish_all_construction(&mut experiment);
    let cur = experiment.find_building(exp_id).unwrap().workers as i8;
    sim::apply_command(&mut experiment, 1, &PlayerCommand::AdjustWorkers { building: exp_id, delta: -cur });
    let exp_survivor = experiment.survivors[0].id;
    experiment.survivors.iter_mut().find(|s| s.id == exp_survivor).unwrap().profession =
        Profession::Farmer;
    sim::apply_command(
        &mut experiment,
        1,
        &PlayerCommand::AssignSurvivor { survivor: exp_survivor, building: Some(exp_id) },
    );

    for _ in 0..(TICKS_PER_DAY * 3) {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        experiment.stock.food > control.stock.food,
        "a named Farmer in a Farmhouse should out-produce an anonymous worker: \
         control={}, experiment={}",
        control.stock.food,
        experiment.stock.food
    );
}

#[test]
fn farmer_still_matches_greenhouse_for_profession_bonus() {
    // Regression guard for `Profession::matches_building`'s refactor (Farmer
    // now matches two building kinds instead of one via a special case) —
    // this must keep working exactly as it did before Farmhouse existed.
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);
    for state in [&mut control, &mut experiment] {
        state.stock.wood = 500.0;
    }

    let (x, y) = find_spot(&control, BuildingKind::Greenhouse);
    let control_id = place_and_staff(&mut control, BuildingKind::Greenhouse, x, y, 0);
    let leader_id = control.leader;
    let control_survivor = control
        .survivors
        .iter()
        .find(|s| Some(s.id) != leader_id)
        .expect("bootstrapped world has more than one survivor")
        .id;
    control.survivors.iter_mut().find(|s| s.id == control_survivor).unwrap().profession =
        Profession::Farmer;
    sim::apply_command(&mut control, 1, &PlayerCommand::AdjustWorkers { building: control_id, delta: 1 });

    let (x, y) = find_spot(&experiment, BuildingKind::Greenhouse);
    let exp_id = place_and_staff(&mut experiment, BuildingKind::Greenhouse, x, y, 0);
    let leader_id = experiment.leader;
    let exp_survivor = experiment
        .survivors
        .iter()
        .find(|s| Some(s.id) != leader_id)
        .expect("bootstrapped world has more than one survivor")
        .id;
    experiment.survivors.iter_mut().find(|s| s.id == exp_survivor).unwrap().profession =
        Profession::Farmer;
    sim::apply_command(
        &mut experiment,
        1,
        &PlayerCommand::AssignSurvivor { survivor: exp_survivor, building: Some(exp_id) },
    );

    for _ in 0..(TICKS_PER_DAY * 3) {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        experiment.stock.food > control.stock.food,
        "a named Farmer in a Greenhouse should still out-produce an anonymous worker: \
         control={}, experiment={}",
        control.stock.food,
        experiment.stock.food
    );
}

#[test]
fn farmer_does_not_match_unrelated_buildings() {
    // Sanity: the Farmhouse special-case in `matches_building` must not have
    // accidentally widened to any OTHER building — a Farmer at a Sawmill is
    // still an ordinary 1.0x mismatch, not a bonus.
    assert!(!Profession::Farmer.matches_building(BuildingKind::Sawmill));
    assert!(!Profession::Farmer.matches_building(BuildingKind::Hospital));
    assert!(Profession::Farmer.matches_building(BuildingKind::Greenhouse));
    assert!(Profession::Farmer.matches_building(BuildingKind::Farmhouse));
    // Every other profession's `matches_building` must still be exactly
    // equivalent to the old single-kind `matching_building` check.
    for p in Profession::ALL {
        if p == Profession::Farmer {
            continue;
        }
        assert!(p.matches_building(p.matching_building()));
        assert!(!p.matches_building(BuildingKind::Farmhouse));
    }
}
