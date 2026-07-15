//! Pure-simulation tests for V0.15: the daily meal/sleep routine
//! (`sim::tick`'s private `routine_goal`, tested here through its
//! observable effect on survivor movement — a cosmetic-only fallback
//! reached only when nothing else claims a survivor's `goal`). No new
//! persisted state, no save migration.

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

fn place_and_finish(state: &mut GameState, kind: BuildingKind, x: u8, y: u8) -> u32 {
    sim::apply_command(state, 1, &PlayerCommand::Place { kind, x, y });
    let id = state.buildings.last().unwrap().id;
    sim::finish_all_construction(state);
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
