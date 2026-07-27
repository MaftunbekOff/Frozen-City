//! Pure-simulation tests for V0.15: the daily meal/sleep routine
//! (`sim::tick`'s private `routine_goal`, tested here through its
//! observable effect on survivor movement — a cosmetic-only fallback
//! reached only when nothing else claims a survivor's `goal`), plus V0.16's
//! `sim::survivor_is_at_meal` — the render-only "has this survivor actually
//! arrived at the Kitchen to eat" query the client uses to pick a seated
//! pose. No new persisted state, no save migration.

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

/// Places `kind`, finishes it, and sends its auto-crew home again.
///
/// That last step matters here more than anywhere else: V0.20 crews a new
/// site with NAMED survivors, and `finish_all_construction` keeps as many of
/// them as the finished building employs. Every test in this file is about
/// what an IDLE survivor does with their day — meals, bed, standing still —
/// so a survivor still holding a job would be following their assignment
/// instead, and the routine under test would never run.
fn place_and_finish(state: &mut GameState, kind: BuildingKind, x: u8, y: u8) -> u32 {
    sim::apply_command(state, 1, &PlayerCommand::Place { kind, x, y, facing: 0 });
    let id = state.buildings.last().unwrap().id;
    sim::finish_all_construction(state);
    let cur = state.find_building(id).unwrap().workers as i8;
    if cur > 0 {
        sim::apply_command(state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: -cur });
    }
    id
}

/// Lands `state.tick` at the START of the given `time_of_day` fraction on
/// whatever day it's currently mid-way through, rounding forward to the
/// NEXT occurrence so the result is always in the future (never negative /
/// wrapping backward across a `u64`).
fn seek_time_of_day(state: &mut GameState, fraction: f32) {
    let day_start = (state.tick / TICKS_PER_DAY) * TICKS_PER_DAY;
    let target = day_start + (fraction * TICKS_PER_DAY as f32) as u64;
    state.tick = if target > state.tick { target } else { target + TICKS_PER_DAY };
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

#[test]
fn idle_survivor_walks_toward_the_kitchen_during_breakfast() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (kx, ky) = find_spot(&state, BuildingKind::Kitchen);
    place_and_finish(&mut state, BuildingKind::Kitchen, kx, ky);
    // Just past the tile's south edge, not dead-center — see `routine_goal`'s
    // doc comment (clipping + unclickability if parked exactly on center).
    let kitchen_pos = (kx as f32 + 0.5, ky as f32 + 1.15);

    seek_time_of_day(&mut state, BREAKFAST_WINDOW.0 + 0.01);
    let start_pos = (state.survivors[0].x, state.survivors[0].y);
    let start_dist = dist(start_pos, kitchen_pos);
    assert!(start_dist > 0.5, "sanity: survivor doesn't already start on the kitchen tile");

    // Stay well inside the ~30-tick-wide window the whole time.
    for _ in 0..10 {
        sim::tick(&mut state);
    }

    let new_dist = dist((state.survivors[0].x, state.survivors[0].y), kitchen_pos);
    assert!(
        new_dist < start_dist,
        "an idle survivor should walk toward the Kitchen during breakfast: start={start_dist}, now={new_dist}"
    );
}

#[test]
fn idle_survivor_walks_toward_the_nearest_tent_at_night() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (tx, ty) = find_spot(&state, BuildingKind::Tent);
    place_and_finish(&mut state, BuildingKind::Tent, tx, ty);
    let tent_pos = (tx as f32 + 0.5, ty as f32 + 1.15);
    // V0.17: the night walk to bed (which this test predates) is gated on
    // the furnace having been lit at least once — nobody sleeps while the
    // opening chop-and-carry crisis is still unresolved (see `sim::tick`'s
    // sleep leg and `sleep_goal`'s doc comment). `new_game`'s lone starting
    // survivor never builds it in this test, so light it directly; the
    // night-walk mechanic itself is what's under test here, not the furnace
    // bootstrap (covered separately by `furnace_bootstrap_tests.rs`).
    state.furnace_lit = true;
    state.furnace_level = 1;

    seek_time_of_day(&mut state, 0.90); // well inside `is_night`'s !(0.25..0.75)
    assert!(state.is_night(), "sanity: 0.90 should read as night");
    let start_pos = (state.survivors[0].x, state.survivors[0].y);
    let start_dist = dist(start_pos, tent_pos);
    assert!(start_dist > 0.5, "sanity: survivor doesn't already start on the tent tile");

    for _ in 0..10 {
        sim::tick(&mut state);
    }

    let new_dist = dist((state.survivors[0].x, state.survivors[0].y), tent_pos);
    assert!(
        new_dist < start_dist,
        "an idle survivor should walk toward their nearest Tent at night: start={start_dist}, now={new_dist}"
    );
}

#[test]
fn a_working_survivor_ignores_the_routine_and_keeps_heading_to_work() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (kx, ky) = find_spot(&state, BuildingKind::Kitchen);
    place_and_finish(&mut state, BuildingKind::Kitchen, kx, ky);
    let (sx, sy) = find_spot(&state, BuildingKind::Sawmill);
    let sawmill = place_and_finish(&mut state, BuildingKind::Sawmill, sx, sy);
    let sawmill_pos = (sx as f32 + 0.5, sy as f32 + 0.5);
    let survivor = state.survivors[0].id;
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor, building: Some(sawmill) });

    seek_time_of_day(&mut state, BREAKFAST_WINDOW.0 + 0.01);
    for _ in 0..10 {
        sim::tick(&mut state);
    }

    let s = state.survivors.iter().find(|s| s.id == survivor).unwrap();
    assert!(
        dist((s.x, s.y), sawmill_pos) < dist((s.x, s.y), (kx as f32 + 0.5, ky as f32 + 0.5)),
        "an assigned survivor must keep heading to work, not the Kitchen, even during breakfast"
    );
}

#[test]
fn outside_meal_and_sleep_windows_an_idle_survivor_stays_put() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (kx, ky) = find_spot(&state, BuildingKind::Kitchen);
    place_and_finish(&mut state, BuildingKind::Kitchen, kx, ky);
    let (tx, ty) = find_spot(&state, BuildingKind::Tent);
    place_and_finish(&mut state, BuildingKind::Tent, tx, ty);

    // Mid-afternoon: daytime (not night), well clear of both meal windows.
    seek_time_of_day(&mut state, 0.60);
    assert!(!state.is_night());
    let start_pos = (state.survivors[0].x, state.survivors[0].y);

    for _ in 0..10 {
        sim::tick(&mut state);
    }

    let new_pos = (state.survivors[0].x, state.survivors[0].y);
    assert!(
        dist(start_pos, new_pos) < 0.01,
        "an idle survivor outside every routine window should stay put: {start_pos:?} -> {new_pos:?}"
    );
}

#[test]
fn survivor_is_at_meal_only_once_arrived_during_a_meal_window() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (kx, ky) = find_spot(&state, BuildingKind::Kitchen);
    place_and_finish(&mut state, BuildingKind::Kitchen, kx, ky);
    let kitchen_pos = (kx as f32 + 0.5, ky as f32 + 1.15);

    seek_time_of_day(&mut state, BREAKFAST_WINDOW.0 + 0.01);
    assert!(
        !sim::survivor_is_at_meal(&state, &state.survivors[0]),
        "just-started walking toward the Kitchen: not arrived yet"
    );

    // Teleport the survivor to just outside arrival range instead of
    // simulating however many real ticks a cross-map walk would take: a
    // multi-day simulation would risk starvation/cold/blizzard deaths that
    // have nothing to do with what this test is checking (the "arrived"
    // transition), and the walk itself is already covered by
    // `idle_survivor_walks_toward_the_kitchen_during_breakfast` above. A
    // handful of ticks, well inside the current ~30-tick window, is enough
    // to close this short a gap.
    state.survivors[0].x = kitchen_pos.0 + 0.2;
    state.survivors[0].y = kitchen_pos.1;
    for _ in 0..5 {
        sim::tick(&mut state);
    }
    assert!(
        sim::survivor_is_at_meal(&state, &state.survivors[0]),
        "should read as at-meal once arrived at the Kitchen within the window"
    );
}

#[test]
fn survivor_is_at_meal_is_false_outside_meal_windows_and_for_working_survivors() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (kx, ky) = find_spot(&state, BuildingKind::Kitchen);
    place_and_finish(&mut state, BuildingKind::Kitchen, kx, ky);

    // Mid-afternoon: never at-meal, regardless of how long we simulate.
    seek_time_of_day(&mut state, 0.60);
    for _ in 0..10 {
        sim::tick(&mut state);
    }
    assert!(!sim::survivor_is_at_meal(&state, &state.survivors[0]));

    // An assigned survivor heads to their own workplace, not the Kitchen,
    // even during breakfast — the routine never claims them.
    let (sx, sy) = find_spot(&state, BuildingKind::Sawmill);
    let sawmill = place_and_finish(&mut state, BuildingKind::Sawmill, sx, sy);
    let survivor = state.survivors[0].id;
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor, building: Some(sawmill) });
    seek_time_of_day(&mut state, BREAKFAST_WINDOW.0 + 0.01);
    for _ in 0..10 {
        sim::tick(&mut state);
    }
    let s = state.survivors.iter().find(|s| s.id == survivor).unwrap();
    assert!(!sim::survivor_is_at_meal(&state, s));
}

#[test]
fn with_no_kitchen_or_tent_the_routine_never_panics_or_moves_anyone() {
    let mut state = sim::new_game(SEED, 12);
    // Deliberately no Kitchen, no Tent — only the starting Furnace/Tunnel.
    let start_pos = (state.survivors[0].x, state.survivors[0].y);

    seek_time_of_day(&mut state, BREAKFAST_WINDOW.0 + 0.01);
    for _ in 0..10 {
        sim::tick(&mut state);
    }
    seek_time_of_day(&mut state, 0.90);
    for _ in 0..10 {
        sim::tick(&mut state);
    }

    let new_pos = (state.survivors[0].x, state.survivors[0].y);
    assert!(
        dist(start_pos, new_pos) < 0.01,
        "with nowhere to route to, an idle survivor must simply stay put: {start_pos:?} -> {new_pos:?}"
    );
}
