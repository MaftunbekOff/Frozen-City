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
fn spawn_assigns_a_deterministic_profession() {
    let a = sim::new_game_bootstrapped(SEED, 12);
    let b = sim::new_game_bootstrapped(SEED, 12);
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
    sim::apply_command(&mut control, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y, facing: 0 });
    let control_id = control.buildings.last().unwrap().id;
    // V0.8: maydonchani bitirib, avto-brigadani bo'shatamiz — test aynan
    // 1 ishchilik farqni o'lchaydi.
    sim::finish_all_construction(&mut control);
    let cur = control.find_building(control_id).unwrap().workers as i8;
    sim::apply_command(&mut control, 1, &PlayerCommand::AdjustWorkers { building: control_id, delta: -cur });
    let control_survivor = control.survivors[0].id;
    // Force a Lumberjack in the control too, but leave them UNASSIGNED
    // (anonymous pool) so the profession bonus has no identity to attach to.
    control.survivors.iter_mut().find(|s| s.id == control_survivor).unwrap().profession =
        Profession::Lumberjack;
    sim::apply_command(&mut control, 1, &PlayerCommand::AdjustWorkers { building: control_id, delta: 1 });

    let (x, y) = find_sawmill_spot_near_forest(&experiment);
    sim::apply_command(&mut experiment, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y, facing: 0 });
    let exp_id = experiment.buildings.last().unwrap().id;
    sim::finish_all_construction(&mut experiment);
    let cur = experiment.find_building(exp_id).unwrap().workers as i8;
    sim::apply_command(&mut experiment, 1, &PlayerCommand::AdjustWorkers { building: exp_id, delta: -cur });
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
fn tailor_matches_tailor_shop_for_profession_bonus() {
    // Mirrors `matching_profession_boosts_production` but for the 7th
    // profession/building pair: a named Tailor assigned to a TailorShop
    // should out-produce an anonymous worker there. Fur is seeded generously
    // in both worlds so cloth output is never fur-capped (isolating the
    // profession bonus from the fur supply mechanic tested separately in
    // wildlife_tests.rs).
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);
    for state in [&mut control, &mut experiment] {
        state.stock.wood = 500.0;
        state.stock.fur = 1000.0;
    }

    let (x, y) = find_spot(&control, BuildingKind::TailorShop);
    sim::apply_command(&mut control, 1, &PlayerCommand::Place { kind: BuildingKind::TailorShop, x, y, facing: 0 });
    let control_id = control.buildings.last().unwrap().id;
    sim::finish_all_construction(&mut control);
    let cur = control.find_building(control_id).unwrap().workers as i8;
    sim::apply_command(&mut control, 1, &PlayerCommand::AdjustWorkers { building: control_id, delta: -cur });
    let control_survivor = control.survivors[0].id;
    // Force a Tailor in the control too, but leave them UNASSIGNED
    // (anonymous pool) so the profession bonus has no identity to attach to.
    control.survivors.iter_mut().find(|s| s.id == control_survivor).unwrap().profession =
        Profession::Tailor;
    sim::apply_command(&mut control, 1, &PlayerCommand::AdjustWorkers { building: control_id, delta: 1 });

    let (x, y) = find_spot(&experiment, BuildingKind::TailorShop);
    sim::apply_command(&mut experiment, 1, &PlayerCommand::Place { kind: BuildingKind::TailorShop, x, y, facing: 0 });
    let exp_id = experiment.buildings.last().unwrap().id;
    sim::finish_all_construction(&mut experiment);
    let cur = experiment.find_building(exp_id).unwrap().workers as i8;
    sim::apply_command(&mut experiment, 1, &PlayerCommand::AdjustWorkers { building: exp_id, delta: -cur });
    let exp_survivor = experiment.survivors[0].id;
    experiment.survivors.iter_mut().find(|s| s.id == exp_survivor).unwrap().profession =
        Profession::Tailor;
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
        experiment.stock.cloth > control.stock.cloth,
        "a named Tailor in a Tailor Shop should out-produce an anonymous worker: \
         control={}, experiment={}",
        control.stock.cloth,
        experiment.stock.cloth
    );
}

#[test]
fn mismatched_profession_gets_no_bonus() {
    let mut baseline = sim::new_game(SEED, 12);
    let mut mismatched = sim::new_game(SEED, 12);
    for state in [&mut baseline, &mut mismatched] {
        state.stock.wood = 500.0;
        // Isolate the profession-match bonus from the (V0.7) leader's own
        // universal-profession bypass — both worlds start with their sole
        // survivor auto-appointed leader (see `mapgen::new_game`), which
        // would otherwise mask a mismatched profession's missing bonus.
        state.leader = None;
    }

    let (x, y) = find_sawmill_spot_near_forest(&baseline);
    sim::apply_command(&mut baseline, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y, facing: 0 });
    let baseline_id = baseline.buildings.last().unwrap().id;
    // V0.8: bitirish + brigada tozalash (yuqoridagi test bilan bir sabab).
    sim::finish_all_construction(&mut baseline);
    let cur = baseline.find_building(baseline_id).unwrap().workers as i8;
    sim::apply_command(&mut baseline, 1, &PlayerCommand::AdjustWorkers { building: baseline_id, delta: -cur });
    sim::apply_command(&mut baseline, 1, &PlayerCommand::AdjustWorkers { building: baseline_id, delta: 1 });

    let (x, y) = find_sawmill_spot_near_forest(&mismatched);
    sim::apply_command(&mut mismatched, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y, facing: 0 });
    let mismatched_id = mismatched.buildings.last().unwrap().id;
    sim::finish_all_construction(&mut mismatched);
    let cur = mismatched.find_building(mismatched_id).unwrap().workers as i8;
    sim::apply_command(&mut mismatched, 1, &PlayerCommand::AdjustWorkers { building: mismatched_id, delta: -cur });
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

/// The leader is a generalist for as long as they hold the seat — a
/// mismatched profession still gets `PROFESSION_MATCH_BONUS` while leading,
/// and loses it the moment someone else is appointed (their own
/// `Profession` field never changes; only the bonus check does).
#[test]
fn leader_gets_the_profession_match_bonus_at_any_building() {
    let mut baseline = sim::new_game(SEED, 12);
    let mut led = sim::new_game(SEED, 12);
    for state in [&mut baseline, &mut led] {
        state.stock.wood = 500.0;
    }

    let (x, y) = find_sawmill_spot_near_forest(&baseline);
    sim::apply_command(&mut baseline, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y, facing: 0 });
    let baseline_id = baseline.buildings.last().unwrap().id;
    sim::finish_all_construction(&mut baseline);
    let cur = baseline.find_building(baseline_id).unwrap().workers as i8;
    sim::apply_command(&mut baseline, 1, &PlayerCommand::AdjustWorkers { building: baseline_id, delta: -cur });
    sim::apply_command(&mut baseline, 1, &PlayerCommand::AdjustWorkers { building: baseline_id, delta: 1 });

    let (x, y) = find_sawmill_spot_near_forest(&led);
    sim::apply_command(&mut led, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y, facing: 0 });
    let led_id = led.buildings.last().unwrap().id;
    sim::finish_all_construction(&mut led);
    let cur = led.find_building(led_id).unwrap().workers as i8;
    sim::apply_command(&mut led, 1, &PlayerCommand::AdjustWorkers { building: led_id, delta: -cur });
    let survivor = led.survivors[0].id;
    // A Cook (mismatched) working a Sawmill — but appointed leader first.
    led.survivors.iter_mut().find(|s| s.id == survivor).unwrap().profession = Profession::Cook;
    sim::apply_command(&mut led, 1, &PlayerCommand::SetLeader { survivor });
    sim::apply_command(&mut led, 1, &PlayerCommand::AssignSurvivor { survivor, building: Some(led_id) });

    // V0.11: this test never builds/staffs the furnace's warmth for anyone
    // beyond the bare "lit" huddling bonus (no tents), and neither world
    // funds food/water — over several days of hard winter cold that's
    // enough to actually kill the lone tracked survivor. A death here isn't
    // just noise: baseline's ANONYMOUS worker slot is fungible (any living
    // survivor can silently backfill it, so a newcomer arrival masks the
    // death), but led's NAMED slot is tied to this specific survivor's id
    // and permanently empties out when they die, even if a newcomer is
    // still alive — an asymmetry that has nothing to do with what this test
    // is actually checking (leader-bonus reverting). Keep everyone alive
    // and fed so the comparison stays about profession/leader bonuses only.
    for _ in 0..(TICKS_PER_DAY * 3) {
        for state in [&mut baseline, &mut led] {
            state.stock.food = 999.0;
            state.stock.water = 999.0;
            for s in state.survivors.iter_mut() {
                s.hp = s.hp.max(80.0);
            }
        }
        sim::tick(&mut baseline);
        sim::tick(&mut led);
    }

    assert!(
        led.stock.wood > baseline.stock.wood,
        "a mismatched-profession leader should still out-produce an anonymous worker: \
         baseline={}, led={}",
        baseline.stock.wood,
        led.stock.wood
    );

    // Strip the leader seat away (appoint nobody by clearing it directly —
    // there's no `SetLeader(None)` command, only appointing a different
    // survivor) and confirm the very next tick's production reverts to
    // ordinary mismatched behavior: no more bonus than the anonymous baseline.
    led.leader = None;
    let wood_before = led.stock.wood;
    let control_before = baseline.stock.wood;
    // A longer window than the old half-day (V0.11's other tick-order
    // changes can nudge exactly which tick a chunky `take_forest_unit`
    // harvest lands on) — over just a few harvests, a single whole-unit
    // boundary shift is noise; over two full days there are enough harvests
    // that a single unit's slop stays well below what an actual lingering
    // bonus would produce. Same alive-and-fed guard as phase 1, above.
    for _ in 0..(TICKS_PER_DAY * 2) {
        for state in [&mut baseline, &mut led] {
            state.stock.food = 999.0;
            state.stock.water = 999.0;
            for s in state.survivors.iter_mut() {
                s.hp = s.hp.max(80.0);
            }
        }
        sim::tick(&mut baseline);
        sim::tick(&mut led);
    }
    let led_gain = led.stock.wood - wood_before;
    let control_gain = baseline.stock.wood - control_before;
    assert!(
        (led_gain - control_gain).abs() < 1.5,
        "once the leader seat is empty, the Cook's own (mismatched) profession applies again: \
         control_gain={control_gain}, led_gain={led_gain}"
    );
}
