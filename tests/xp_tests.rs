//! Pure-simulation tests for V0.7 XP/levels: accrual while staffed, the
//! level production bonus, and the reset when reassigned to a different
//! building kind. Mirrors the style of `building_tests.rs`/`profession_tests.rs`.

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

/// Unlike `find_spot`, also requires nearby forest — a plain Sawmill spot can
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

fn place(state: &mut GameState, kind: BuildingKind) -> u32 {
    state.stock.wood = 500.0;
    let (x, y) = find_spot(state, kind);
    sim::apply_command(state, 1, &PlayerCommand::Place { kind, x, y, facing: 0 });
    let id = state.buildings.last().unwrap().id;
    // V0.8: maydonchani darhol bitirib, avto-brigadani bo'shatamiz — testlar
    // bo'sh binoni o'zi xohlagancha (nomlangan/anonim) to'ldiradi.
    sim::finish_all_construction(state);
    let cur = state.find_building(id).unwrap().workers as i8;
    if cur > 0 {
        sim::apply_command(state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: -cur });
    }
    id
}

#[test]
fn assignment_sets_trained_kind_and_xp_accrues_while_staffed() {
    let mut state = sim::new_game(SEED, 12);
    let sawmill = place(&mut state, BuildingKind::Sawmill);
    let survivor = state.survivors[0].id;

    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor, building: Some(sawmill) });
    let s = state.survivors.iter().find(|s| s.id == survivor).unwrap();
    assert_eq!(s.trained_kind, Some(BuildingKind::Sawmill));
    assert_eq!(s.xp, 0.0);

    for _ in 0..(TICKS_PER_DAY / 2) {
        sim::tick(&mut state);
    }
    let s = state.survivors.iter().find(|s| s.id == survivor).unwrap();
    assert!((s.xp - 0.5).abs() < 0.01, "half a day staffed should be ~0.5 accrued days, got {}", s.xp);
}

#[test]
fn xp_does_not_accrue_while_unassigned() {
    let mut state = sim::new_game(SEED, 12);
    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut state);
    }
    assert!(state.survivors.iter().all(|s| s.xp == 0.0), "an idle survivor should not gain xp");
}

#[test]
fn reassigning_to_a_different_kind_resets_xp_and_kind_reassigning_same_kind_keeps_it() {
    let mut state = sim::new_game(SEED, 12);
    let sawmill_a = place(&mut state, BuildingKind::Sawmill);
    let sawmill_b = place(&mut state, BuildingKind::Sawmill);
    let coal_mine = place(&mut state, BuildingKind::CoalMine);
    let survivor = state.survivors[0].id;

    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor, building: Some(sawmill_a) });
    for _ in 0..(TICKS_PER_DAY as usize) {
        sim::tick(&mut state);
    }
    let xp_after_one_day = state.survivors.iter().find(|s| s.id == survivor).unwrap().xp;
    assert!(xp_after_one_day > 0.9, "expected ~1.0 accrued day, got {xp_after_one_day}");

    // Same kind, different building: xp is preserved.
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor, building: Some(sawmill_b) });
    let s = state.survivors.iter().find(|s| s.id == survivor).unwrap();
    assert_eq!(s.trained_kind, Some(BuildingKind::Sawmill));
    assert!(s.xp > 0.9, "same-kind reassignment must not reset xp, got {}", s.xp);

    // Different kind: xp resets to 0 and trained_kind switches.
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor, building: Some(coal_mine) });
    let s = state.survivors.iter().find(|s| s.id == survivor).unwrap();
    assert_eq!(s.trained_kind, Some(BuildingKind::CoalMine));
    assert_eq!(s.xp, 0.0, "a different building KIND must reset xp");
}

#[test]
fn unassigning_does_not_reset_xp() {
    let mut state = sim::new_game(SEED, 12);
    let sawmill = place(&mut state, BuildingKind::Sawmill);
    let survivor = state.survivors[0].id;

    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor, building: Some(sawmill) });
    for _ in 0..(TICKS_PER_DAY as usize) {
        sim::tick(&mut state);
    }
    let xp_before = state.survivors.iter().find(|s| s.id == survivor).unwrap().xp;
    assert!(xp_before > 0.0);

    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor, building: None });
    let s = state.survivors.iter().find(|s| s.id == survivor).unwrap();
    assert_eq!(s.xp, xp_before, "plain unassignment must not touch xp");
    assert_eq!(s.trained_kind, Some(BuildingKind::Sawmill), "trained_kind survives an unassign too");
}

#[test]
fn higher_level_survivor_outproduces_a_fresh_one() {
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);
    for state in [&mut control, &mut experiment] {
        state.stock.wood = 500.0;
    }

    let (x, y) = find_sawmill_spot_near_forest(&control);
    sim::apply_command(&mut control, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y, facing: 0 });
    let control_id = control.buildings.last().unwrap().id;
    // V0.8: bitirish + brigada tozalash — test 1 nomlangan ishchini o'lchaydi.
    sim::finish_all_construction(&mut control);
    let cur = control.find_building(control_id).unwrap().workers as i8;
    sim::apply_command(&mut control, 1, &PlayerCommand::AdjustWorkers { building: control_id, delta: -cur });
    let control_survivor = control.survivors[0].id;
    sim::apply_command(
        &mut control,
        1,
        &PlayerCommand::AssignSurvivor { survivor: control_survivor, building: Some(control_id) },
    );

    let (x, y) = find_sawmill_spot_near_forest(&experiment);
    sim::apply_command(&mut experiment, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y, facing: 0 });
    let exp_id = experiment.buildings.last().unwrap().id;
    sim::finish_all_construction(&mut experiment);
    let cur = experiment.find_building(exp_id).unwrap().workers as i8;
    sim::apply_command(&mut experiment, 1, &PlayerCommand::AdjustWorkers { building: exp_id, delta: -cur });
    let exp_survivor = experiment.survivors[0].id;
    sim::apply_command(
        &mut experiment,
        1,
        &PlayerCommand::AssignSurvivor { survivor: exp_survivor, building: Some(exp_id) },
    );
    // Fast-forward the experiment survivor straight to level 3 xp without
    // ticking (isolates the level bonus from the passage of time itself,
    // which would also affect the control run).
    experiment.survivors.iter_mut().find(|s| s.id == exp_survivor).unwrap().xp = XP_DAYS_LEVEL_3;

    // Long enough that the +15% gap reliably lands on a different whole-
    // wood-unit count instead of being lost to `take_forest_unit`'s integer
    // rounding over a short run, but short of `XP_DAYS_LEVEL_2` (3 days) so
    // the control survivor can't climb to level 2 and narrow the gap itself.
    for _ in 0..(TICKS_PER_DAY * 2) {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        experiment.stock.wood > control.stock.wood,
        "a level-3 survivor (+15%) should out-produce a fresh one: control={}, experiment={}",
        control.stock.wood,
        experiment.stock.wood
    );
}
