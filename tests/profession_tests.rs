//! Pure-simulation tests for V0.7 professions: deterministic spawn
//! assignment and the matching-building production bonus. Mirrors the style
//! of `building_tests.rs` (control vs. experiment comparison to isolate the
//! bonus from the rest of the simulation).

use frozen_city::game::sim;
use frozen_city::game::types::*;

const SEED: u64 = 12345;

/// Requires nearby forest — a plain Sawmill spot can
/// otherwise land far from any harvestable tile, making production (and any
/// multiplier applied to it) invisible over a short test run.
fn find_sawmill_spot_near_forest(state: &GameState) -> (u8, u8) {
    for y in 0..MAP_H as u8 {
        for x in 0..MAP_W as u8 {
            if state.can_place(BuildingKind::Sawmill, x, y).is_ok() && state.forest_near(x, y, 4) > 100 {
                return (x, y);
            }
        }
    }
    panic!("no sawmill spot near forest");
}

#[test]
fn spawn_assigns_a_deterministic_profession() {
    let a = sim::new_game(SEED, 12);
    let b = sim::new_game(SEED, 12);
    let professions_a: Vec<Profession> = a.survivors.iter().map(|s| s.profession).collect();
    let professions_b: Vec<Profession> = b.survivors.iter().map(|s| s.profession).collect();
    assert_eq!(professions_a, professions_b, "same seed must yield the same professions");

    // Sanity: with 8 survivors and 6 professions drawn from the sim RNG,
    // it would be a suspiciously narrow roll for every one to match — this
    // is a weak smoke check that the RNG stream is actually being consulted
    // (not e.g. every survivor defaulting to the same profession).
    let distinct: std::collections::HashSet<Profession> = professions_a.iter().copied().collect();
    assert!(distinct.len() > 1, "expected some variety among 8 survivors, got {professions_a:?}");
}

#[test]
fn from_id_hash_is_deterministic_and_covers_all_professions() {
    let a: Vec<Profession> = (1..=200u32).map(Profession::from_id_hash).collect();
    let b: Vec<Profession> = (1..=200u32).map(Profession::from_id_hash).collect();
    assert_eq!(a, b, "the same id must always hash to the same profession");

    let distinct: std::collections::HashSet<Profession> = a.into_iter().collect();
    assert_eq!(
        distinct.len(),
        Profession::ALL.len(),
        "200 ids should exercise every profession at least once"
    );
}

#[test]
fn matching_profession_boosts_production() {
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);
    for state in [&mut control, &mut experiment] {
        state.stock.wood = 500.0;
    }

    let (x, y) = find_sawmill_spot_near_forest(&control);
    sim::apply_command(&mut control, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y });
    let control_id = control.buildings.last().unwrap().id;
    let control_survivor = control.survivors[0].id;
    // Force a Lumberjack in the control too, but leave them UNASSIGNED
    // (anonymous pool) so the profession bonus has no identity to attach to.
    control.survivors.iter_mut().find(|s| s.id == control_survivor).unwrap().profession =
        Profession::Lumberjack;
    sim::apply_command(&mut control, 1, &PlayerCommand::AdjustWorkers { building: control_id, delta: 1 });

    let (x, y) = find_sawmill_spot_near_forest(&experiment);
    sim::apply_command(&mut experiment, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y });
    let exp_id = experiment.buildings.last().unwrap().id;
    let exp_survivor = experiment.survivors[0].id;
    experiment.survivors.iter_mut().find(|s| s.id == exp_survivor).unwrap().profession =
        Profession::Lumberjack;
    sim::apply_command(
        &mut experiment,
        1,
        &PlayerCommand::AssignSurvivor { survivor: exp_survivor, building: Some(exp_id) },
    );

    // Long enough that the +25% gap reliably lands on a different whole-
    // wood-unit count instead of being lost to `take_forest_unit`'s integer
    // rounding over a short run.
    for _ in 0..(TICKS_PER_DAY * 3) {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        experiment.stock.wood > control.stock.wood,
        "a named Lumberjack in a Sawmill should out-produce an anonymous worker: \
         control={}, experiment={}",
        control.stock.wood,
        experiment.stock.wood
    );
}

#[test]
fn mismatched_profession_gets_no_bonus() {
    let mut baseline = sim::new_game(SEED, 12);
    let mut mismatched = sim::new_game(SEED, 12);
    for state in [&mut baseline, &mut mismatched] {
        state.stock.wood = 500.0;
    }

    let (x, y) = find_sawmill_spot_near_forest(&baseline);
    sim::apply_command(&mut baseline, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y });
    let baseline_id = baseline.buildings.last().unwrap().id;
    sim::apply_command(&mut baseline, 1, &PlayerCommand::AdjustWorkers { building: baseline_id, delta: 1 });

    let (x, y) = find_sawmill_spot_near_forest(&mismatched);
    sim::apply_command(&mut mismatched, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y });
    let mismatched_id = mismatched.buildings.last().unwrap().id;
    let survivor = mismatched.survivors[0].id;
    // Explicitly a non-Lumberjack profession working the Sawmill.
    mismatched.survivors.iter_mut().find(|s| s.id == survivor).unwrap().profession = Profession::Cook;
    sim::apply_command(
        &mut mismatched,
        1,
        &PlayerCommand::AssignSurvivor { survivor, building: Some(mismatched_id) },
    );

    for _ in 0..(TICKS_PER_DAY / 2) {
        sim::tick(&mut baseline);
        sim::tick(&mut mismatched);
    }

    assert!(
        (baseline.stock.wood - mismatched.stock.wood).abs() < 0.01,
        "a named worker with no matching profession should produce the same as an anonymous \
         one: baseline={}, mismatched={}",
        baseline.stock.wood,
        mismatched.stock.wood
    );
}
