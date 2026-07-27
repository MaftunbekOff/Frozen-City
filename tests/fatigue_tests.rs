//! Pure-simulation tests for V0.17 fatigue: work/idle accrual by day,
//! Tent-bunk vs sleeping-rough recovery by night, the `fatigue_factor`
//! production penalty, and the new night walk to bed (which now overrides an
//! ASSIGNED survivor's walk-to-work goal, guarded off while carrying a log
//! home or before the furnace is ever lit). Mirrors the style of
//! `routine_tests.rs` (movement-by-comparison, `seek_time_of_day`) and
//! `profession_tests.rs` (control-vs-experiment production isolation).

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

/// Requires nearby forest — a plain Sawmill spot can otherwise land far from
/// any harvestable tile, making the production comparison below invisible
/// over a short test run (mirrors `profession_tests.rs`'s helper of the same
/// name).
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

fn place_and_finish(state: &mut GameState, kind: BuildingKind, x: u8, y: u8) -> u32 {
    sim::apply_command(state, 1, &PlayerCommand::Place { kind, x, y, facing: 0 });
    let id = state.buildings.last().unwrap().id;
    sim::finish_all_construction(state);
    id
}

/// Lands `state.tick` at the START of the given `time_of_day` fraction on
/// whatever day it's currently mid-way through, rounding forward to the NEXT
/// occurrence so the result is always in the future. Copied verbatim from
/// `routine_tests.rs` (each test file keeps its own small helpers, matching
/// this repo's existing convention).
fn seek_time_of_day(state: &mut GameState, fraction: f32) {
    let day_start = (state.tick / TICKS_PER_DAY) * TICKS_PER_DAY;
    let target = day_start + (fraction * TICKS_PER_DAY as f32) as u64;
    state.tick = if target > state.tick { target } else { target + TICKS_PER_DAY };
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

// --- A: accrual ---

#[test]
fn assigned_survivor_gains_fatigue_faster_than_an_idle_one() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    let sawmill = place_and_finish(&mut state, BuildingKind::Sawmill, x, y);
    let assigned = state.survivors[0].id;
    let idle = state.survivors[1].id;
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor: assigned, building: Some(sawmill) });

    // Land solidly inside the day window (`!is_night()` is 0.25..0.75) and
    // reset fatigue right there, so every tick below accrues at exactly one
    // of the two AWAKE rates (`FATIGUE_WORK_PER_DAY`/`FATIGUE_IDLE_PER_DAY`)
    // with no night-recovery mixed in.
    seek_time_of_day(&mut state, 0.26);
    for s in &mut state.survivors {
        s.fatigue = 0.0;
    }
    // Comfortably inside the day half of the cycle (0.26 .. 0.75).
    let ticks = TICKS_PER_DAY / 8;
    for _ in 0..ticks {
        assert!(!state.is_night(), "sanity: test must stay inside the day window throughout");
        sim::tick(&mut state);
    }

    let assigned_fatigue = state.survivors.iter().find(|s| s.id == assigned).unwrap().fatigue;
    let idle_fatigue = state.survivors.iter().find(|s| s.id == idle).unwrap().fatigue;
    let expected = |per_day: f32| per_day / TICKS_PER_DAY as f32 * ticks as f32;

    assert!(
        (assigned_fatigue - expected(FATIGUE_WORK_PER_DAY)).abs() < 0.1,
        "assigned survivor should accrue at FATIGUE_WORK_PER_DAY: expected {}, got {assigned_fatigue}",
        expected(FATIGUE_WORK_PER_DAY)
    );
    assert!(
        (idle_fatigue - expected(FATIGUE_IDLE_PER_DAY)).abs() < 0.1,
        "idle survivor should accrue at the slower FATIGUE_IDLE_PER_DAY: expected {}, got {idle_fatigue}",
        expected(FATIGUE_IDLE_PER_DAY)
    );
    assert!(
        assigned_fatigue > idle_fatigue,
        "an assigned worker should tire faster than an idle one: assigned={assigned_fatigue}, idle={idle_fatigue}"
    );
}

// --- B: night recovery ---

#[test]
fn a_bunked_survivor_recovers_fatigue_faster_than_one_sleeping_rough() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 500.0;
    // One Tent = TENT_CAPACITY (4) bunks; `bunked_ids` hands them to the
    // first 4 survivors in roster order — the other 4 of the 8 bootstrapped
    // survivors sleep rough all night, every night.
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    place_and_finish(&mut state, BuildingKind::Tent, x, y);
    assert_eq!(state.housing_capacity(), TENT_CAPACITY, "sanity: exactly one Tent's worth of bunks");

    let bunked_id = state.survivors[0].id;
    let unbunked_id = state.survivors[4].id;
    assert!(state.bunked_ids().contains(&bunked_id), "sanity: survivor 0 should hold a bunk");
    assert!(!state.bunked_ids().contains(&unbunked_id), "sanity: survivor 4 should have no bunk");

    for s in &mut state.survivors {
        s.fatigue = 90.0;
    }
    seek_time_of_day(&mut state, 0.80); // solidly inside the night window
    assert!(state.is_night());

    let ticks = TICKS_PER_DAY / 8; // stays well inside the same night
    for _ in 0..ticks {
        sim::tick(&mut state);
    }

    let bunked_fatigue = state.survivors.iter().find(|s| s.id == bunked_id).unwrap().fatigue;
    let unbunked_fatigue = state.survivors.iter().find(|s| s.id == unbunked_id).unwrap().fatigue;
    let expected = |per_day: f32| (90.0 - per_day / TICKS_PER_DAY as f32 * ticks as f32).clamp(0.0, 100.0);

    assert!(
        (bunked_fatigue - expected(FATIGUE_TENT_REST_PER_DAY)).abs() < 0.1,
        "a bunked survivor should recover at FATIGUE_TENT_REST_PER_DAY: expected {}, got {bunked_fatigue}",
        expected(FATIGUE_TENT_REST_PER_DAY)
    );
    assert!(
        (unbunked_fatigue - expected(FATIGUE_ROUGH_REST_PER_DAY)).abs() < 0.1,
        "an unbunked survivor should recover at the slower FATIGUE_ROUGH_REST_PER_DAY: expected {}, got {unbunked_fatigue}",
        expected(FATIGUE_ROUGH_REST_PER_DAY)
    );
    assert!(
        bunked_fatigue < unbunked_fatigue,
        "a bunked survivor should recover fatigue faster than one sleeping rough: bunked={bunked_fatigue}, unbunked={unbunked_fatigue}"
    );
}

// --- C: fatigue_factor / production ---

#[test]
fn fatigue_factor_is_1_below_tired_and_falls_to_the_floor_at_100() {
    let mut state = sim::new_game(SEED, 12);
    let s = &mut state.survivors[0];

    s.fatigue = 0.0;
    assert_eq!(s.fatigue_factor(), 1.0, "no penalty while well-rested");
    s.fatigue = FATIGUE_TIRED;
    assert_eq!(s.fatigue_factor(), 1.0, "still no penalty exactly at the threshold");
    s.fatigue = 100.0;
    assert!(
        (s.fatigue_factor() - FATIGUE_MIN_FACTOR).abs() < 1e-6,
        "fully exhausted should floor at FATIGUE_MIN_FACTOR, got {}",
        s.fatigue_factor()
    );
    // Halfway between the threshold and 100 should be halfway between 1.0
    // and the floor (the falloff is linear).
    s.fatigue = (FATIGUE_TIRED + 100.0) / 2.0;
    let expected_mid = 1.0 - 0.5 * (1.0 - FATIGUE_MIN_FACTOR);
    assert!(
        (s.fatigue_factor() - expected_mid).abs() < 1e-4,
        "expected the halfway factor {expected_mid}, got {}",
        s.fatigue_factor()
    );
}

#[test]
fn a_fully_fatigued_worker_produces_measurably_less_than_a_rested_one() {
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);
    for state in [&mut control, &mut experiment] {
        state.stock.wood = 500.0;
    }

    let (x, y) = find_sawmill_spot_near_forest(&control);
    let control_id = place_and_finish(&mut control, BuildingKind::Sawmill, x, y);
    let control_survivor = control.survivors[0].id;
    sim::apply_command(
        &mut control,
        1,
        &PlayerCommand::AssignSurvivor { survivor: control_survivor, building: Some(control_id) },
    );
    control.survivors.iter_mut().find(|s| s.id == control_survivor).unwrap().fatigue = 0.0;

    let (x2, y2) = find_sawmill_spot_near_forest(&experiment); // same seed -> same spot
    let exp_id = place_and_finish(&mut experiment, BuildingKind::Sawmill, x2, y2);
    let exp_survivor = experiment.survivors[0].id;
    sim::apply_command(
        &mut experiment,
        1,
        &PlayerCommand::AssignSurvivor { survivor: exp_survivor, building: Some(exp_id) },
    );
    // Pinned at 100 the whole run: `fatigue`'s own dynamics would otherwise
    // shift it during the loop below, muddying the isolated comparison this
    // test is after — but 100 is already the clamp ceiling, so it can only
    // ever go DOWN from here (only at night, and only were this survivor
    // bunked, which they are not — no Tent exists in this test at all).
    experiment.survivors.iter_mut().find(|s| s.id == exp_survivor).unwrap().fatigue = 100.0;

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        control.stock.wood > experiment.stock.wood,
        "a rested worker should out-produce a fully fatigued one: control={}, experiment={}",
        control.stock.wood,
        experiment.stock.wood
    );
}

// --- D: the night walk ---

#[test]
fn an_assigned_bunked_survivor_walks_to_their_tent_at_night_instead_of_work() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 500.0;
    let (tx, ty) = find_spot(&state, BuildingKind::Tent);
    place_and_finish(&mut state, BuildingKind::Tent, tx, ty);
    let (sx, sy) = find_spot(&state, BuildingKind::Sawmill);
    let sawmill = place_and_finish(&mut state, BuildingKind::Sawmill, sx, sy);
    let tent_pos = (tx as f32 + 0.5, ty as f32 + 1.15);

    let survivor = state.survivors[0].id; // one of the first TENT_CAPACITY -> holds a bunk
    assert!(state.bunked_ids().contains(&survivor), "sanity: this survivor must hold a bunk");
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor, building: Some(sawmill) });

    seek_time_of_day(&mut state, 0.80);
    let s0 = state.survivors.iter().find(|s| s.id == survivor).unwrap();
    let start_dist = dist((s0.x, s0.y), tent_pos);

    for _ in 0..10 {
        sim::tick(&mut state);
    }

    let s = state.survivors.iter().find(|s| s.id == survivor).unwrap();
    let new_dist = dist((s.x, s.y), tent_pos);
    assert!(
        new_dist < start_dist,
        "an assigned, bunked survivor should walk to their Tent at night instead of staying at work: start={start_dist}, now={new_dist}"
    );
}

#[test]
fn the_night_walk_never_claims_a_survivor_carrying_wood_home() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 500.0;
    let (tx, ty) = find_spot(&state, BuildingKind::Tent);
    place_and_finish(&mut state, BuildingKind::Tent, tx, ty);
    let (sx, sy) = find_spot(&state, BuildingKind::Sawmill);
    let sawmill = place_and_finish(&mut state, BuildingKind::Sawmill, sx, sy);
    let sawmill_pos = (sx as f32 + 0.5, sy as f32 + 0.5);
    let tent_pos = (tx as f32 + 0.5, ty as f32 + 1.15);

    let survivor = state.survivors[0].id; // holds a bunk
    assert!(state.bunked_ids().contains(&survivor));
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor, building: Some(sawmill) });
    // This never happens in practice for a Sawmill worker (only a
    // Furnace-building errand ever sets `carrying_wood`) — set directly to
    // exercise the guard itself, mirroring how `illness_tests.rs` sets
    // `sick_left` directly rather than waiting on the real trigger.
    state.survivors.iter_mut().find(|s| s.id == survivor).unwrap().carrying_wood = true;

    seek_time_of_day(&mut state, 0.80);
    assert!(state.is_night());
    let s0 = state.survivors.iter().find(|s| s.id == survivor).unwrap();
    let start_to_sawmill = dist((s0.x, s0.y), sawmill_pos);
    let start_to_tent = dist((s0.x, s0.y), tent_pos);

    for _ in 0..10 {
        sim::tick(&mut state);
    }

    let s = state.survivors.iter().find(|s| s.id == survivor).unwrap();
    assert!(
        dist((s.x, s.y), sawmill_pos) < start_to_sawmill,
        "a survivor carrying a log home must keep heading to their assigned building, not bed"
    );
    assert!(
        dist((s.x, s.y), tent_pos) >= start_to_tent - 0.01,
        "a survivor carrying a log home must not have been routed toward the tent instead"
    );
}

#[test]
fn the_night_walk_never_fires_before_the_furnace_has_ever_been_lit() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 500.0;
    let (tx, ty) = find_spot(&state, BuildingKind::Tent);
    place_and_finish(&mut state, BuildingKind::Tent, tx, ty);
    let (sx, sy) = find_spot(&state, BuildingKind::Sawmill);
    let sawmill = place_and_finish(&mut state, BuildingKind::Sawmill, sx, sy);
    let sawmill_pos = (sx as f32 + 0.5, sy as f32 + 0.5);
    let tent_pos = (tx as f32 + 0.5, ty as f32 + 1.15);

    let survivor = state.survivors[0].id; // holds a bunk
    assert!(state.bunked_ids().contains(&survivor));
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor, building: Some(sawmill) });
    // `furnace_level == 0` forces `tick`'s fuel block to set `furnace_lit =
    // false` unconditionally every tick (see the `else` arm there), unlike
    // just poking `furnace_lit` directly, which the very next tick would
    // recompute back to `true` from the still-stocked coal.
    state.furnace_level = 0;

    seek_time_of_day(&mut state, 0.80);
    assert!(state.is_night());
    let s0 = state.survivors.iter().find(|s| s.id == survivor).unwrap();
    let start_to_sawmill = dist((s0.x, s0.y), sawmill_pos);
    let start_to_tent = dist((s0.x, s0.y), tent_pos);

    for _ in 0..10 {
        sim::tick(&mut state);
    }

    let s = state.survivors.iter().find(|s| s.id == survivor).unwrap();
    assert!(!state.furnace_lit, "sanity: the furnace must still read as unlit");
    assert!(
        dist((s.x, s.y), sawmill_pos) < start_to_sawmill,
        "with the furnace never lit, an assigned survivor must keep heading to work, not bed"
    );
    assert!(
        dist((s.x, s.y), tent_pos) >= start_to_tent - 0.01,
        "with the furnace never lit, a survivor must not have been routed toward the tent"
    );
}

// --- survivor_is_resting (V0.17 client-only render query) ---

#[test]
fn survivor_is_resting_is_true_only_once_a_bunked_survivor_has_arrived_at_their_tent() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 500.0;
    let (tx, ty) = find_spot(&state, BuildingKind::Tent);
    place_and_finish(&mut state, BuildingKind::Tent, tx, ty);
    let tent_pos = (tx as f32 + 0.5, ty as f32 + 1.15);

    let survivor = state.survivors[0].id;
    assert!(state.bunked_ids().contains(&survivor), "sanity: holds a bunk");

    seek_time_of_day(&mut state, 0.80);
    assert!(
        !sim::survivor_is_resting(&state, state.survivors.iter().find(|s| s.id == survivor).unwrap()),
        "just started walking toward the tent: not arrived yet"
    );

    // Teleport close instead of simulating the whole cross-map walk (already
    // covered by `an_assigned_bunked_survivor_walks_to_their_tent_at_night_instead_of_work`
    // above) — mirrors `routine_tests.rs`'s `survivor_is_at_meal` test.
    let s_idx = state.survivors.iter().position(|s| s.id == survivor).unwrap();
    state.survivors[s_idx].x = tent_pos.0 + 0.2;
    state.survivors[s_idx].y = tent_pos.1;
    for _ in 0..5 {
        sim::tick(&mut state);
    }
    let s = state.survivors.iter().find(|s| s.id == survivor).unwrap();
    assert!(
        sim::survivor_is_resting(&state, s),
        "should read as resting once arrived at the Tent at night"
    );
}

#[test]
fn survivor_is_resting_is_false_for_an_unbunked_survivor_even_at_night() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 500.0;
    let (tx, ty) = find_spot(&state, BuildingKind::Tent);
    place_and_finish(&mut state, BuildingKind::Tent, tx, ty);

    let unbunked = state.survivors[4].id; // beyond the one Tent's TENT_CAPACITY bunks
    assert!(!state.bunked_ids().contains(&unbunked), "sanity: holds no bunk");

    seek_time_of_day(&mut state, 0.80);
    for _ in 0..10 {
        sim::tick(&mut state);
    }
    let s = state.survivors.iter().find(|s| s.id == unbunked).unwrap();
    assert!(
        !sim::survivor_is_resting(&state, s),
        "an unbunked survivor should never read as resting, however long the night runs"
    );
}
