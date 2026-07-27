//! Pure-simulation tests for V0.19 snow: `sim::snow::tick_snowfall` (fall
//! rate, blizzard rate, the `SNOW_MAX` ceiling, trampling), `tick_snow_crews`
//! (a staffed Snow Crew's radius), and `tick_clear_orders`
//! (`PlayerCommand::ClearSnow` hand-shovelling). Mirrors the style of
//! `fatigue_tests.rs`/`illness_tests.rs` (direct field mutation to set up a
//! scenario, control-vs-experiment isolation, seeking to a safe time-of-day
//! window before a movement-sensitive check).
//!
//! `road_tests.rs` owns the road side (`BuildRoad`/`RemoveRoad`,
//! `Tile::speed_factor`); this file owns the weather side.
//!
//! Several tests call `sim::snow::tick_snowfall`/`tick_snow_crews` directly
//! rather than the full `sim::tick` — `snow.rs` is its own `pub mod` (see
//! `sim.rs`), called from one marked hook point in `tick.rs`, exactly like
//! `lifecycle`/`expedition`/`missions`. Calling the hook directly isolates
//! the cadence math from hunger/movement/day-rollover noise the same way
//! `illness_tests.rs` sets `sick_left` directly instead of waiting on the
//! real infection roll.

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
    sim::apply_command(state, 1, &PlayerCommand::Place { kind, x, y, facing: 0 });
    let id = state.buildings.last().unwrap().id;
    sim::finish_all_construction(state);
    id
}

/// `place_and_finish`, then clears the auto-crewed anonymous construction
/// crew it leaves behind -- same reasoning/shape as `fatigue_tests.rs`'s
/// helper of the same name.
fn place_finish_and_clear_crew(state: &mut GameState, kind: BuildingKind, x: u8, y: u8) -> u32 {
    let id = place_and_finish(state, kind, x, y);
    let cur = state.find_building(id).unwrap().workers as i8;
    sim::apply_command(state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: -cur });
    id
}

/// A tile at Chebyshev distance at least `min_dist` from `(bx, by)`,
/// in-bounds. Used to pick a point deliberately outside a Snow Crew's
/// `SNOW_CREW_RADIUS`, wherever `find_spot` happened to land the building.
fn far_point(bx: u8, by: u8, min_dist: i32) -> (u8, u8) {
    for &dx in &[min_dist, -min_dist] {
        let nx = bx as i32 + dx;
        if in_bounds(nx, by as i32) {
            return (nx as u8, by);
        }
    }
    for &dy in &[min_dist, -min_dist] {
        let ny = by as i32 + dy;
        if in_bounds(bx as i32, ny) {
            return (bx, ny as u8);
        }
    }
    panic!("no in-bounds point at Chebyshev distance {min_dist} from ({bx}, {by})");
}

// --- A: snowfall rate ---

#[test]
fn snowfall_accumulates_at_exactly_snow_fall_per_day_in_fair_weather() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    // No survivors at all -- isolates the fall term from trampling, which
    // has its own dedicated tests below.
    state.survivors.clear();
    state.tick = 0;
    assert!(!state.blizzard_active(), "sanity: fair weather");

    let days = 2u64;
    for _ in 0..(days * TICKS_PER_DAY) {
        state.tick += 1;
        sim::snow::tick_snowfall(&mut state);
    }

    let expected = (days as f32 * SNOW_FALL_PER_DAY) as u8;
    assert!(
        state.tiles.iter().all(|t| t.snow == expected),
        "every tile should have accumulated exactly {expected} after {days} fair-weather days"
    );
}

#[test]
fn snowfall_accumulates_at_exactly_the_blizzard_rate_during_a_blizzard() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.survivors.clear();
    state.tick = 0;
    // Comfortably past this test's whole window, so `blizzard_active()`
    // stays true throughout.
    state.blizzard_until = 3 * TICKS_PER_DAY;
    assert!(state.blizzard_active());

    // One day only: `SNOW_FALL_BLIZZARD_PER_DAY` is calibrated to bury a road
    // (`ROAD_SNOW_PENALTY`) in a single storm, which puts two days' worth past
    // `SNOW_MAX` and would make this measure the ceiling instead of the rate.
    // The cadence is still what's under test -- `TICKS_PER_DAY` doesn't divide
    // evenly by the blizzard rate, which is exactly where a fixed
    // "every N ticks" interval drifts. The ceiling has its own test below.
    let days = 1u64;
    for _ in 0..(days * TICKS_PER_DAY) {
        state.tick += 1;
        sim::snow::tick_snowfall(&mut state);
    }

    let expected = (days as f32 * SNOW_FALL_BLIZZARD_PER_DAY) as u8;
    assert!(
        state.tiles.iter().all(|t| t.snow == expected),
        "every tile should have accumulated exactly {expected} after {days} blizzard day(s) -- \
         this is the case a fixed \"every N ticks\" cadence gets wrong, since TICKS_PER_DAY \
         doesn't divide evenly by SNOW_FALL_BLIZZARD_PER_DAY"
    );
}

#[test]
fn snow_never_exceeds_snow_max_even_under_a_long_blizzard() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.survivors.clear();
    state.tick = 0;
    state.blizzard_until = u64::MAX;

    // 10 days at 34/day is 340 -- more than triple SNOW_MAX (100).
    for _ in 0..(10 * TICKS_PER_DAY) {
        state.tick += 1;
        sim::snow::tick_snowfall(&mut state);
    }

    assert!(
        state.tiles.iter().all(|t| t.snow == SNOW_MAX),
        "snow must clamp at SNOW_MAX, never wrap or exceed it"
    );
}

// --- B: trampling ---

#[test]
fn a_survivors_own_tile_ends_up_clearer_than_an_identical_untouched_one() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.tick = 0;
    state.survivors.truncate(1); // exactly one survivor, parked in a known spot
    let trampled = (10u8, 10u8);
    let untouched = (20u8, 20u8);
    state.tiles[tile_index(trampled.0, trampled.1)].snow = 80;
    state.tiles[tile_index(untouched.0, untouched.1)].snow = 80;
    state.survivors[0].x = trampled.0 as f32 + 0.5;
    state.survivors[0].y = trampled.1 as f32 + 0.5;

    for _ in 0..TICKS_PER_DAY {
        state.tick += 1;
        sim::snow::tick_snowfall(&mut state);
    }

    let trampled_snow = state.tiles[tile_index(trampled.0, trampled.1)].snow;
    let untouched_snow = state.tiles[tile_index(untouched.0, untouched.1)].snow;
    assert!(
        trampled_snow < untouched_snow,
        "a tile stood on all day should end up clearer than an identical one nobody visited: \
         trampled {trampled_snow}, untouched {untouched_snow}"
    );
    // Quantitative: both gain SNOW_FALL_PER_DAY from the fall term (exact,
    // per the fair-weather test above); the trampled one additionally loses
    // SNOW_TRAMPLE_PER_DAY, also exact over a clean day boundary.
    let expected_untouched = 80 + SNOW_FALL_PER_DAY as i32;
    let expected_trampled = 80 + SNOW_FALL_PER_DAY as i32 - SNOW_TRAMPLE_PER_DAY as i32;
    assert_eq!(untouched_snow as i32, expected_untouched);
    assert_eq!(trampled_snow as i32, expected_trampled);
}

#[test]
fn trampling_does_not_stack_when_several_survivors_share_one_tile() {
    // Before the fix this guards, the trample pass collected one tile index
    // PER SURVIVOR standing there and subtracted 1 for each -- so a
    // clustered doorway or Tent packed down several times faster than a
    // single trodden path. Two survivors sharing a tile should trample it
    // at the exact same rate as one.
    let shared = (15u8, 15u8);

    let mut crowded = sim::new_game_bootstrapped(SEED, 12);
    crowded.tick = 0;
    crowded.survivors.truncate(3);
    crowded.tiles[tile_index(shared.0, shared.1)].snow = 90;
    for s in &mut crowded.survivors {
        s.x = shared.0 as f32 + 0.5;
        s.y = shared.1 as f32 + 0.5;
    }

    let mut lone = sim::new_game_bootstrapped(SEED, 12);
    lone.tick = 0;
    lone.survivors.truncate(1);
    lone.tiles[tile_index(shared.0, shared.1)].snow = 90;
    lone.survivors[0].x = shared.0 as f32 + 0.5;
    lone.survivors[0].y = shared.1 as f32 + 0.5;

    for _ in 0..TICKS_PER_DAY {
        crowded.tick += 1;
        sim::snow::tick_snowfall(&mut crowded);
        lone.tick += 1;
        sim::snow::tick_snowfall(&mut lone);
    }

    let crowded_snow = crowded.tiles[tile_index(shared.0, shared.1)].snow;
    let lone_snow = lone.tiles[tile_index(shared.0, shared.1)].snow;
    assert_eq!(
        crowded_snow, lone_snow,
        "3 survivors sharing a tile should trample it at the same rate as 1, not 3x as fast: \
         crowded {crowded_snow}, lone {lone_snow}"
    );
}

// --- C: staffed Snow Crew ---

#[test]
fn a_staffed_snow_crew_clears_its_radius_and_leaves_the_rest_alone() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::SnowCrew);
    let id = place_and_finish(&mut state, BuildingKind::SnowCrew, x, y);
    // Force exactly 2 anonymous workers (flat 1.0 contribution each,
    // avoiding `survivor_contribution`'s profession/XP variance) --
    // `SnowCrew::max_workers()` is 2.
    let cur = state.find_building(id).unwrap().workers as i8;
    sim::apply_command(&mut state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: 2 - cur });
    assert_eq!(state.find_building(id).unwrap().workers, 2, "sanity: fully staffed");

    // Bury the whole map at max depth, so any change is unambiguously the
    // crew's doing.
    for t in state.tiles.iter_mut() {
        t.snow = SNOW_MAX;
    }

    // With every candidate tile tied at the exact same depth, the priority
    // sort's tie-break (ascending tile index) makes each call spend its
    // whole budget on a SINGLE fresh tile -- 2 workers at level 1 gives
    // budget 2.4/call, and `take` is budget-limited on the first (lowest-
    // index) target, so exactly one tile drops from 100 to 98 per call
    // (see `tick_snow_crews`: `take as u8` floors 2.4 to 2, and `budget -=
    // take` zeroes the float budget in the same step, so nothing carries to
    // a second target). The crew's own tile sits in the MIDDLE of the scan
    // order (dy = 0 of -7..=7), so it isn't reached until roughly half the
    // square's tiles have each had their first turn -- 300 calls safely
    // covers a full first pass over the largest possible radius square
    // (15x15 = 225, before any map-edge clamping shrinks it further).
    for _ in 0..300 {
        sim::snow::tick_snow_crews(&mut state);
    }

    let under_crew = state.tiles[tile_index(x, y)].snow;
    assert!(under_crew < SNOW_MAX, "the tile the crew stands on should have been cleared, got {under_crew}");

    let (fx, fy) = far_point(x, y, SNOW_CREW_RADIUS + 5);
    let outside = state.tiles[tile_index(fx, fy)].snow;
    assert_eq!(
        outside, SNOW_MAX,
        "a tile well outside SNOW_CREW_RADIUS should be untouched by the crew: got {outside}"
    );
}

#[test]
fn an_unstaffed_snow_crew_clears_nothing() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::SnowCrew);
    let id = place_finish_and_clear_crew(&mut state, BuildingKind::SnowCrew, x, y);
    assert_eq!(state.find_building(id).unwrap().workers, 0, "sanity: unstaffed");

    for t in state.tiles.iter_mut() {
        t.snow = SNOW_MAX;
    }
    for _ in 0..80 {
        sim::snow::tick_snow_crews(&mut state);
    }

    assert!(
        state.tiles.iter().all(|t| t.snow == SNOW_MAX),
        "an unstaffed Snow Crew (workers == 0) should clear nothing at all"
    );
}

#[test]
fn a_snow_crew_still_under_construction_clears_nothing_even_with_its_auto_crew() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::SnowCrew);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::SnowCrew, x, y, facing: 0 });
    let id = state.buildings.last().unwrap().id;
    // `Place` auto-crews idle survivors as a construction team -- workers >
    // 0, but the site is still under construction, which must gate the
    // effect (`!b.under_construction()` in `tick_snow_crews`).
    assert!(state.find_building(id).unwrap().under_construction(), "sanity: still a construction site");
    assert!(state.find_building(id).unwrap().workers > 0, "sanity: auto-crewed with idle hands");

    for t in state.tiles.iter_mut() {
        t.snow = SNOW_MAX;
    }
    for _ in 0..80 {
        sim::snow::tick_snow_crews(&mut state);
    }

    assert!(
        state.tiles.iter().all(|t| t.snow == SNOW_MAX),
        "a Snow Crew still under construction must clear nothing, no matter how many hands are on site"
    );
}

// --- D: hand-clearing (PlayerCommand::ClearSnow) ---

#[test]
fn clearing_snow_by_hand_walks_there_works_and_ends_at_zero() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    // No Tent anywhere in a fresh bootstrapped world, and a daytime window
    // well clear of both meal windows (0.27-0.31, 0.48-0.52) for the whole
    // walk-plus-work duration -- see `road_tests.rs`'s movement test for why
    // this matters: nothing should be able to pull the survivor off the job.
    for b in &state.buildings {
        assert_ne!(b.kind, BuildingKind::Tent);
    }
    let day_start = (state.tick / TICKS_PER_DAY) * TICKS_PER_DAY;
    state.tick = day_start + (0.32 * TICKS_PER_DAY as f32) as u64;

    // A handful of tiles from the furnace -- guaranteed clear terrain (the
    // furnace's forest/coal keep-clear radius), not that terrain matters to
    // movement or to this command's validation, just to keep the walk
    // uneventful.
    let target = (34u8, 31u8);
    state.tiles[tile_index(target.0, target.1)].snow = 60;
    let sid = state.survivors[0].id;
    {
        let s = state.survivors.iter_mut().find(|s| s.id == sid).unwrap();
        s.x = 26.5;
        s.y = 31.5;
    }

    sim::apply_command(&mut state, 1, &PlayerCommand::ClearSnow { survivor: sid, x: target.0, y: target.1 });
    assert_eq!(state.clear_orders.len(), 1, "sanity: the order was actually issued");

    // ~8 tiles of walk (~16 ticks) + CLEAR_SNOW_WORKDAYS worth of standing
    // work (45 ticks) + a tick or two of arrival lag -- 100 is comfortable
    // slack while still landing before the 0.48 lunch window.
    for _ in 0..100 {
        sim::tick(&mut state);
    }

    assert_eq!(
        state.tiles[tile_index(target.0, target.1)].snow, 0,
        "the tile should have been fully cleared"
    );
    assert!(state.clear_orders.is_empty(), "the completed order should have been removed");
    assert!(
        state.events.iter().any(|e| e.text.contains("was cleared")),
        "completion should be logged as an event: {:?}",
        state.events.iter().map(|e| &e.text).collect::<Vec<_>>()
    );
}

#[test]
fn a_clear_order_targeting_an_already_clear_tile_completes_harmlessly() {
    // The Snow Crew (or a previous hand-clear) may have already cleared a
    // tile by the time a survivor arrives -- `tick_clear_orders` doesn't
    // check current depth before counting down, so this should just
    // complete normally rather than underflow or get stuck.
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    for b in &state.buildings {
        assert_ne!(b.kind, BuildingKind::Tent);
    }
    let day_start = (state.tick / TICKS_PER_DAY) * TICKS_PER_DAY;
    state.tick = day_start + (0.32 * TICKS_PER_DAY as f32) as u64;

    let target = (34u8, 31u8);
    assert_eq!(state.tiles[tile_index(target.0, target.1)].snow, 0, "sanity: already clear");
    let sid = state.survivors[0].id;
    {
        let s = state.survivors.iter_mut().find(|s| s.id == sid).unwrap();
        s.x = 26.5;
        s.y = 31.5;
    }
    sim::apply_command(&mut state, 1, &PlayerCommand::ClearSnow { survivor: sid, x: target.0, y: target.1 });

    for _ in 0..100 {
        sim::tick(&mut state);
    }

    assert_eq!(state.tiles[tile_index(target.0, target.1)].snow, 0);
    assert!(state.clear_orders.is_empty(), "the order should still complete and be removed, even with nothing to clear");
}

#[test]
fn a_clear_order_is_abandoned_without_clearing_anything_if_the_survivor_dies_en_route() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    let day_start = (state.tick / TICKS_PER_DAY) * TICKS_PER_DAY;
    state.tick = day_start + (0.32 * TICKS_PER_DAY as f32) as u64;

    let target = (34u8, 31u8);
    state.tiles[tile_index(target.0, target.1)].snow = 60;
    let sid = state.survivors[0].id;
    {
        let s = state.survivors.iter_mut().find(|s| s.id == sid).unwrap();
        // Far enough away that a few ticks definitely leaves them en route,
        // not already arrived.
        s.x = 5.5;
        s.y = 31.5;
    }
    sim::apply_command(&mut state, 1, &PlayerCommand::ClearSnow { survivor: sid, x: target.0, y: target.1 });
    assert_eq!(state.clear_orders.len(), 1);

    for _ in 0..5 {
        sim::tick(&mut state);
    }
    let en_route = state.survivors.iter().find(|s| s.id == sid).unwrap();
    assert!(
        (en_route.x - 34.5).abs() > 1.0,
        "sanity: still walking, not already at the tile -- this test is about dying BEFORE finishing"
    );

    // Simulate death directly, the same way `illness_tests.rs` sets
    // `sick_left` directly rather than triggering the real cause -- the
    // point here is `tick_clear_orders`' abandonment path, not the death
    // pathway itself.
    state.survivors.retain(|s| s.id != sid);

    sim::tick(&mut state);

    assert!(state.clear_orders.is_empty(), "the order should be dropped once its survivor is gone");
    assert_eq!(
        state.tiles[tile_index(target.0, target.1)].snow, 60,
        "nobody ever finished the job -- the tile must remain exactly as buried as it started"
    );
}

#[test]
fn two_orders_finishing_the_same_tile_the_same_tick_still_log_exactly_one_event() {
    // Nothing outside `sim::snow` actually lets two orders land on the same
    // tile (`apply_command`'s `ClearSnow` handler re-points an existing
    // order rather than duplicating it), but `tick_clear_orders` shouldn't
    // rely on that caller-side invariant -- constructed directly, the same
    // way this file sets other internal state (`clear_orders`, `sick_left`
    // in `illness_tests.rs`) without going through a command.
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    let target = (30u8, 31u8);
    state.tiles[tile_index(target.0, target.1)].snow = 40;
    let s1 = state.survivors[0].id;
    let s2 = state.survivors[1].id;
    for sid in [s1, s2] {
        let s = state.survivors.iter_mut().find(|s| s.id == sid).unwrap();
        s.x = target.0 as f32 + 0.5;
        s.y = target.1 as f32 + 0.5;
        s.move_target = None;
        s.assigned_building = None;
    }
    state.clear_orders.push(ClearOrder { x: target.0, y: target.1, survivor: s1, work_left: 1.0 });
    state.clear_orders.push(ClearOrder { x: target.0, y: target.1, survivor: s2, work_left: 1.0 });

    sim::snow::tick_clear_orders(&mut state);

    assert_eq!(state.tiles[tile_index(target.0, target.1)].snow, 0);
    assert!(state.clear_orders.is_empty(), "both orders on the finished tile should be gone");
    let cleared_events = state
        .events
        .iter()
        .filter(|e| e.text == format!("The snow at ({}, {}) was cleared.", target.0, target.1))
        .count();
    assert_eq!(cleared_events, 1, "one tile finishing should log exactly one event, however many orders named it");
}

// --- E: the central world ---

#[test]
fn the_central_world_never_accumulates_snow() {
    let mut state = sim::new_game_central(SEED);
    assert!(state.central);

    for _ in 0..(5 * TICKS_PER_DAY) {
        sim::tick(&mut state);
    }

    assert!(
        state.tiles.iter().all(|t| t.snow == 0),
        "the central world has no weather (`tick` skips the blizzard block for it) -- snow must never fall there"
    );
}
