//! Pure-simulation tests for V0.7 morale: death/staffed-building adjustments
//! and the production multiplier bands. Mirrors the style of
//! `building_tests.rs`/`leader_tests.rs`.

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

fn place_and_staff(state: &mut GameState, kind: BuildingKind, x: u8, y: u8, workers: i8) -> u32 {
    sim::apply_command(state, 1, &PlayerCommand::Place { kind, x, y, facing: 0 });
    let id = state.buildings.last().unwrap().id;
    // V0.8: bu testlar bitgan bino effektini sinaydi — maydonchani darhol
    // bitirib, so'ralgan ishchi soniga keltiramiz.
    sim::finish_all_construction(state);
    let cur = state.find_building(id).unwrap().workers as i8;
    sim::apply_command(state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: workers - cur });
    id
}

#[test]
fn fresh_world_starts_at_morale_70_with_multiplier_1() {
    let state = sim::new_game(SEED, 12);
    assert_eq!(state.morale, MORALE_START);
    assert_eq!(state.morale, 70.0);
    assert_eq!(state.morale_multiplier(), 1.0);
}

#[test]
fn death_drops_morale() {
    let mut state = sim::new_game(SEED, 12);
    let before = state.morale;
    state.survivors[0].hp = 0.0;
    sim::tick(&mut state);
    assert!(
        state.morale < before - MORALE_DEATH_PENALTY + 0.5,
        "a death should drop morale by roughly MORALE_DEATH_PENALTY: before={before}, after={}",
        state.morale
    );
}

#[test]
fn staffed_kitchen_raises_morale_over_a_day() {
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);
    // Pin morale below the baseline so the kitchen's push and the passive
    // drift both point the same direction, then compare against a control
    // with no kitchen (drift alone).
    control.morale = 40.0;
    experiment.morale = 40.0;
    experiment.stock.wood = 500.0;
    let (x, y) = find_spot(&experiment, BuildingKind::Kitchen);
    place_and_staff(&mut experiment, BuildingKind::Kitchen, x, y, 1);

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        experiment.morale > control.morale,
        "a staffed kitchen should raise morale faster than drift alone: control={}, experiment={}",
        control.morale,
        experiment.morale
    );
}

#[test]
fn staffed_hospital_raises_morale_over_a_day() {
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);
    control.morale = 40.0;
    experiment.morale = 40.0;
    experiment.stock.wood = 500.0;
    let (x, y) = find_spot(&experiment, BuildingKind::Hospital);
    place_and_staff(&mut experiment, BuildingKind::Hospital, x, y, 1);

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        experiment.morale > control.morale,
        "a staffed hospital should raise morale faster than drift alone: control={}, experiment={}",
        control.morale,
        experiment.morale
    );
}

/// Isolates pure drift from every OTHER morale input (death, starvation,
/// blizzard, staffed Kitchen/Hospital, leader) by calling the drift math
/// directly rather than ticking a full simulated world for many in-game days
/// — over that long a run a real world will starve, freeze, or catch a
/// blizzard/disease well before drift alone would matter, which isn't what
/// this test is about.
#[test]
fn morale_drifts_toward_baseline_and_never_overshoots() {
    let drift_per_tick = |morale: f32| {
        let drift_cap = MORALE_DRIFT_PER_DAY / TICKS_PER_DAY as f32;
        (morale + (MORALE_BASELINE - morale).clamp(-drift_cap, drift_cap)).clamp(0.0, 100.0)
    };

    // Drift moves at a constant rate of MORALE_DRIFT_PER_DAY per day (it
    // clamps the per-tick step, not the remaining gap), so closing the
    // largest possible gap (0..100, 100 units) takes ~100 in-game days at
    // the current constant — run comfortably past that.
    let mut high = 100.0f32;
    let mut low = 0.0f32;
    for _ in 0..(TICKS_PER_DAY * 110) {
        high = drift_per_tick(high);
        low = drift_per_tick(low);
    }

    assert!((high - MORALE_BASELINE).abs() < 1.0, "morale from above should settle near the baseline: {high}");
    assert!((low - MORALE_BASELINE).abs() < 1.0, "morale from below should settle near the baseline: {low}");
}

/// Companion to the isolated drift test above: confirms a REAL world with no
/// deaths/starvation/blizzard actually reaches the baseline too, i.e. that
/// `tick`'s drift logic is wired up and not just correct in isolation. Kept
/// short (well under a day) so nothing else has a chance to fire.
#[test]
fn a_healthy_ticking_world_drifts_toward_baseline_too() {
    let mut state = sim::new_game(SEED, 12);
    // `new_game` auto-appoints the sole starting survivor as leader, whose
    // daily morale bonus would otherwise fight the downward drift this test
    // is isolating.
    state.leader = None;
    state.stock.food = 100_000.0;
    state.stock.coal = 100_000.0;
    state.morale = 100.0;

    for _ in 0..(TICKS_PER_DAY / 4) {
        sim::tick(&mut state);
    }

    assert!(
        state.morale < 100.0 && state.morale > MORALE_BASELINE,
        "morale should have drifted down from 100 toward (but not past) the baseline: {}",
        state.morale
    );
}

#[test]
fn morale_multiplier_bands() {
    let mut state = sim::new_game(SEED, 12);

    state.morale = 10.0;
    assert_eq!(state.morale_multiplier(), 0.8, "<25 -> 0.8");

    state.morale = 24.9;
    assert_eq!(state.morale_multiplier(), 0.8);

    state.morale = 25.0;
    assert_eq!(state.morale_multiplier(), 0.9, "25..50 -> 0.9");

    state.morale = 49.9;
    assert_eq!(state.morale_multiplier(), 0.9);

    state.morale = 50.0;
    assert_eq!(state.morale_multiplier(), 1.0, "50..=75 -> 1.0");

    state.morale = 75.0;
    assert_eq!(state.morale_multiplier(), 1.0);

    state.morale = 75.1;
    assert_eq!(state.morale_multiplier(), 1.1, ">75 -> 1.1");

    state.morale = 100.0;
    assert_eq!(state.morale_multiplier(), 1.1);
}

#[test]
fn morale_is_clamped_to_0_100() {
    let mut state = sim::new_game(SEED, 12);
    state.morale = 5.0;
    for s in &mut state.survivors {
        s.hp = 0.0;
    }
    sim::tick(&mut state);
    assert!(state.morale >= 0.0, "morale must never go negative, got {}", state.morale);
}
