//! Pure-simulation tests for V0.19 roads: `PlayerCommand::BuildRoad`/
//! `RemoveRoad` wood accounting and validation, and `Tile::speed_factor`'s
//! road branch (the reason to build one at all). Mirrors the style of
//! `fatigue_tests.rs`/`illness_tests.rs` (direct field/tile mutation to set
//! up a scenario, control-vs-experiment isolation for behavioural checks).
//!
//! `snow_tests.rs` owns the weather side (`tick_snowfall`/`tick_snow_crews`/
//! `tick_clear_orders`); this file owns the road side (`BuildRoad`/
//! `RemoveRoad`, `Tile::speed_factor`/`move_cost`/`road_is_buried`).

use frozen_city::game::sim;
use frozen_city::game::types::*;

const SEED: u64 = 12345;

/// The first `n` tiles (in scan order) `can_lay_road` accepts — plenty for
/// any test below, and independent of exactly where this seed's forest/coal
/// blobs land.
fn find_road_spots(state: &GameState, n: usize) -> Vec<(u8, u8)> {
    let mut out = Vec::new();
    for y in 0..MAP_H as u8 {
        for x in 0..MAP_W as u8 {
            if state.can_lay_road(x, y).is_ok() {
                out.push((x, y));
                if out.len() == n {
                    return out;
                }
            }
        }
    }
    panic!("fewer than {n} valid road tiles on this map");
}

fn find_terrain(state: &GameState, terrain: Terrain) -> (u8, u8) {
    for y in 0..MAP_H as u8 {
        for x in 0..MAP_W as u8 {
            if state.tile(x, y).is_some_and(|t| t.terrain == terrain) {
                return (x, y);
            }
        }
    }
    panic!("no {terrain:?} tile on this map");
}

// --- A: BuildRoad wood accounting ---

#[test]
fn laying_a_road_costs_exactly_road_cost_wood_per_tile() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 1000.0;
    let spots = find_road_spots(&state, 6);

    let before = state.stock.wood;
    sim::apply_command(&mut state, 1, &PlayerCommand::BuildRoad { tiles: spots.clone() });

    assert!(
        (before - state.stock.wood - ROAD_COST_WOOD * spots.len() as f32).abs() < 1e-4,
        "6 tiles should cost exactly 6 * ROAD_COST_WOOD: spent {}",
        before - state.stock.wood
    );
    for (x, y) in &spots {
        assert!(state.tile(*x, *y).unwrap().road, "({x}, {y}) should have been laid");
        // Laying a road clears whatever had settled on it.
        assert_eq!(state.tile(*x, *y).unwrap().snow, 0);
    }
}

#[test]
fn a_road_drag_never_goes_into_debt_it_just_stops_where_the_wood_does() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    // Exactly enough for 5 of the 10 tiles below.
    state.stock.wood = ROAD_COST_WOOD * 5.0;
    let spots = find_road_spots(&state, 10);

    sim::apply_command(&mut state, 1, &PlayerCommand::BuildRoad { tiles: spots.clone() });

    assert!(state.stock.wood >= 0.0, "wood must never go negative: {}", state.stock.wood);
    assert!(
        state.stock.wood < 1e-4,
        "all affordable wood should have been spent (never fractionally hoarded): {}",
        state.stock.wood
    );
    let laid = spots.iter().filter(|(x, y)| state.tile(*x, *y).unwrap().road).count();
    assert_eq!(laid, 5, "exactly the 5 affordable tiles should have been laid, in drag order");
    // The FIRST 5 in the drag, specifically -- the drag stops, it doesn't
    // skip around picking cheap ones.
    for (x, y) in &spots[..5] {
        assert!(state.tile(*x, *y).unwrap().road, "({x}, {y}) is among the first 5 and should be laid");
    }
    for (x, y) in &spots[5..] {
        assert!(!state.tile(*x, *y).unwrap().road, "({x}, {y}) is past where the wood ran out");
    }
}

#[test]
fn invalid_tiles_are_skipped_without_being_charged() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 1000.0;

    let forest = find_terrain(&state, Terrain::Forest);
    let coal = find_terrain(&state, Terrain::Coal);
    let occupied = (FURNACE_X, FURNACE_Y); // a building sits here from tick 0
    let out_of_bounds = (250u8, 250u8); // MAP is 64x64; well past the edge
    let already_road = find_road_spots(&state, 1)[0];
    // Pre-lay this one tile with a SEPARATE command so its "already a road"
    // rejection is genuine, not an artifact of appearing twice in one batch.
    sim::apply_command(&mut state, 1, &PlayerCommand::BuildRoad { tiles: vec![already_road] });
    let after_prelay = state.stock.wood;

    // `already_road` no longer satisfies `can_lay_road` now that it's laid,
    // so this returns a genuinely different, still-open tile.
    let valid = find_road_spots(&state, 1)[0];
    let tiles = vec![forest, coal, occupied, out_of_bounds, already_road, valid];
    sim::apply_command(&mut state, 1, &PlayerCommand::BuildRoad { tiles });

    assert!(
        (after_prelay - state.stock.wood - ROAD_COST_WOOD).abs() < 1e-4,
        "only the one genuinely valid tile should have been charged: spent {}",
        after_prelay - state.stock.wood
    );
    assert!(!state.tile(forest.0, forest.1).unwrap().road, "forest can't take a road");
    assert!(!state.tile(coal.0, coal.1).unwrap().road, "a coal deposit can't take a road");
    assert!(state.building_at(occupied.0, occupied.1).is_some(), "sanity: still occupied");
    assert!(!state.tile(occupied.0, occupied.1).unwrap().road, "an occupied tile can't take a road");
    assert!(state.tile(valid.0, valid.1).unwrap().road, "the one valid tile should have been laid");
}

#[test]
fn a_build_road_command_is_capped_at_max_road_tiles_per_command() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 1e6;
    // A handful more than the cap, all individually valid.
    let spots = find_road_spots(&state, MAX_ROAD_TILES_PER_COMMAND + 20);

    sim::apply_command(&mut state, 1, &PlayerCommand::BuildRoad { tiles: spots.clone() });

    let laid = spots.iter().filter(|(x, y)| state.tile(*x, *y).unwrap().road).count();
    assert_eq!(laid, MAX_ROAD_TILES_PER_COMMAND, "only the cap's worth should have been laid");
    for (x, y) in &spots[MAX_ROAD_TILES_PER_COMMAND..] {
        assert!(!state.tile(*x, *y).unwrap().road, "({x}, {y}) is past the cap and should be untouched");
    }
    assert!(
        (state.stock.wood - (1e6 - ROAD_COST_WOOD * MAX_ROAD_TILES_PER_COMMAND as f32)).abs() < 1e-2,
        "wood spent should stop exactly at the cap, not the full list: {}",
        state.stock.wood
    );
}

#[test]
fn tearing_up_a_road_refunds_road_refund_of_its_cost() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 1000.0;
    let wood_at_start = state.stock.wood;
    let spot = find_road_spots(&state, 1)[0];
    sim::apply_command(&mut state, 1, &PlayerCommand::BuildRoad { tiles: vec![spot] });
    assert!(state.tile(spot.0, spot.1).unwrap().road, "sanity: laid");

    let before = state.stock.wood;
    sim::apply_command(&mut state, 1, &PlayerCommand::RemoveRoad { tiles: vec![spot] });

    assert!(!state.tile(spot.0, spot.1).unwrap().road, "the road should be gone");
    assert!(
        (state.stock.wood - before - ROAD_COST_WOOD * ROAD_REFUND).abs() < 1e-4,
        "removal should refund exactly ROAD_COST_WOOD * ROAD_REFUND: got {}",
        state.stock.wood - before
    );
    // Never a net creation of wood: laying then tearing up must cost strictly
    // more than it gives back. Asserted on the OUTCOME rather than on
    // `ROAD_REFUND < 1.0` — the constant is known at compile time, so that
    // form is a tautology clippy rightly flags, and it wouldn't catch a
    // refund path that paid out more than the constant says.
    assert!(
        state.stock.wood < wood_at_start,
        "a build-then-remove round trip must never leave the colony with more wood \
         than it started with: start={wood_at_start}, end={}",
        state.stock.wood
    );
}

#[test]
fn removing_road_from_a_tile_that_never_had_one_is_a_no_op() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    let spot = find_road_spots(&state, 1)[0];
    let before = state.stock.wood;

    sim::apply_command(&mut state, 1, &PlayerCommand::RemoveRoad { tiles: vec![spot] });

    assert_eq!(state.stock.wood, before, "no road existed there, so no refund should be paid");
}

#[test]
fn roads_are_refused_in_the_central_world() {
    let mut state = sim::new_game_central(SEED);
    let spot = find_road_spots(&state, 1)[0];
    assert!(
        !state.can_issue(1, &PlayerCommand::BuildRoad { tiles: vec![spot] }),
        "BuildRoad must not be authorized in the central world"
    );

    let before_wood = state.stock.wood;
    sim::apply_command(&mut state, 1, &PlayerCommand::BuildRoad { tiles: vec![spot] });
    assert!(!state.tile(spot.0, spot.1).unwrap().road, "no road should have been laid");
    assert_eq!(state.stock.wood, before_wood, "no wood should have moved");

    // Pre-lay a road by hand (bypassing the refused command) so RemoveRoad
    // has something to refuse tearing up too.
    let idx = tile_index(spot.0, spot.1);
    state.tiles[idx].road = true;
    assert!(
        !state.can_issue(1, &PlayerCommand::RemoveRoad { tiles: vec![spot] }),
        "RemoveRoad must not be authorized in the central world either"
    );
    sim::apply_command(&mut state, 1, &PlayerCommand::RemoveRoad { tiles: vec![spot] });
    assert!(state.tile(spot.0, spot.1).unwrap().road, "the hand-placed road must survive the refused command");
}

// --- B: Tile::speed_factor / move_cost / road_is_buried ---

#[test]
fn speed_factor_is_monotonic_never_blocks_and_never_exceeds_road_speed() {
    let mut prev_open = f32::INFINITY;
    let mut prev_road = f32::INFINITY;
    for snow in 0..=SNOW_MAX {
        let open = Tile { terrain: Terrain::Snow, deposit: 0, road: false, snow };
        let road = Tile { terrain: Terrain::Snow, deposit: 0, road: true, snow };
        assert!(open.speed_factor() > 0.0, "open ground must never fully block movement, snow={snow}");
        assert!(road.speed_factor() > 0.0, "a road must never fully block movement, snow={snow}");
        assert!(
            road.speed_factor() <= TILE_ROAD_SPEED + 1e-5,
            "a road must never exceed TILE_ROAD_SPEED, snow={snow} got {}",
            road.speed_factor()
        );
        assert!(
            open.speed_factor() <= prev_open + 1e-5,
            "open ground speed must be non-increasing as snow deepens, snow={snow}"
        );
        assert!(
            road.speed_factor() <= prev_road + 1e-5,
            "road speed must be non-increasing as snow deepens, snow={snow}"
        );
        prev_open = open.speed_factor();
        prev_road = road.speed_factor();
    }
    assert!(
        (prev_open - SNOW_SLOWEST_FACTOR).abs() < 1e-5,
        "open ground at SNOW_MAX should floor at exactly SNOW_SLOWEST_FACTOR, got {prev_open}"
    );
}

#[test]
fn a_clear_road_is_full_speed_and_a_buried_road_is_no_better_than_open_ground() {
    let clear_road = Tile { terrain: Terrain::Snow, deposit: 0, road: true, snow: 0 };
    assert_eq!(clear_road.speed_factor(), TILE_ROAD_SPEED, "a snow-free road should be full TILE_ROAD_SPEED");
    assert!(!clear_road.road_is_buried());

    // At and beyond ROAD_SNOW_PENALTY, a road should offer nothing an
    // identical patch of open ground doesn't already have.
    for snow in [ROAD_SNOW_PENALTY, ROAD_SNOW_PENALTY + 10, SNOW_MAX] {
        let buried_road = Tile { terrain: Terrain::Snow, deposit: 0, road: true, snow };
        let open = Tile { terrain: Terrain::Snow, deposit: 0, road: false, snow };
        assert!(buried_road.road_is_buried(), "snow={snow} should already read as buried");
        assert!(
            (buried_road.speed_factor() - open.speed_factor()).abs() < 1e-5,
            "a buried road (snow={snow}) should be no faster than open ground: road {} vs open {}",
            buried_road.speed_factor(),
            open.speed_factor()
        );
    }

    // Just short of the penalty threshold, the road should still be helping
    // (faster than open ground) and not yet read as buried.
    let almost_buried = Tile { terrain: Terrain::Snow, deposit: 0, road: true, snow: ROAD_SNOW_PENALTY - 1 };
    let open_same_depth = Tile { terrain: Terrain::Snow, deposit: 0, road: false, snow: ROAD_SNOW_PENALTY - 1 };
    assert!(!almost_buried.road_is_buried());
    assert!(
        almost_buried.speed_factor() > open_same_depth.speed_factor(),
        "a road one unit short of the penalty threshold should still beat open ground"
    );
}

#[test]
fn move_cost_is_the_finite_reciprocal_of_speed_factor() {
    for snow in [0u8, 1, ROAD_SNOW_PENALTY, SNOW_MAX] {
        for road in [false, true] {
            let t = Tile { terrain: Terrain::Snow, deposit: 0, road, snow };
            assert!(t.move_cost().is_finite(), "move_cost must never be infinite, road={road} snow={snow}");
            assert!(
                (t.move_cost() - 1.0 / t.speed_factor()).abs() < 1e-5,
                "move_cost should be exactly 1 / speed_factor, road={road} snow={snow}"
            );
        }
    }
}

// --- C: the behavioural payoff — actually crossing the ground faster ---

#[test]
fn a_survivor_crosses_a_cleared_road_measurably_faster_than_open_snow() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    // No Tent anywhere in this world, so `bunked_ids()` is empty and nobody
    // is ever pulled off their walk toward a night's sleep -- the movement
    // comparison below stays uncontaminated by the daily routine (mirrors
    // `an_assigned_bunked_survivor_walks_to_their_tent...`'s reasoning in
    // `fatigue_tests.rs`, just avoided instead of exercised).
    for b in &state.buildings {
        assert_ne!(b.kind, BuildingKind::Tent, "sanity: no Tent should exist in a fresh bootstrapped world");
    }
    // Comfortably between breakfast (ends 0.31) and lunch (starts 0.48), so
    // no meal-time routine goal can claim either survivor mid-test either.
    let day_start = (state.tick / TICKS_PER_DAY) * TICKS_PER_DAY;
    state.tick = day_start + (0.33 * TICKS_PER_DAY as f32) as u64;

    // Two parallel THREE-ROW bands, far enough apart (around y=20 and y=40)
    // that neither survivor's local `road_step` neighbour search ever sees
    // the other's tiles. Three rows, not one: `road_step` picks its
    // direction by comparing neighbour cost, and a single-tile-wide corridor
    // sitting inside otherwise-untouched (cheaper) ground would let it drift
    // off-row onto a diagonal neighbour instead of walking the intended
    // strip — widening it removes any cheaper alternative to step onto.
    // Every tile is set directly rather than built via `BuildRoad`/terrain —
    // `speed_factor` only reads `road`/`snow`, never `terrain`, so this is a
    // faithful, deterministic setup.
    for x in 10u8..=30 {
        for y in 19u8..=21 {
            state.tiles[tile_index(x, y)] = Tile { terrain: Terrain::Snow, deposit: 0, road: true, snow: 0 };
        }
        for y in 39u8..=41 {
            state.tiles[tile_index(x, y)] = Tile { terrain: Terrain::Snow, deposit: 0, road: false, snow: 25 };
        }
    }

    let road_id = state.survivors[0].id;
    let snow_id = state.survivors[1].id;
    {
        let s = state.survivors.iter_mut().find(|s| s.id == road_id).unwrap();
        s.x = 10.5;
        s.y = 20.5;
        s.assigned_building = None;
        s.move_target = Some((30, 20));
    }
    {
        let s = state.survivors.iter_mut().find(|s| s.id == snow_id).unwrap();
        s.x = 10.5;
        s.y = 40.5;
        s.assigned_building = None;
        s.move_target = Some((30, 40));
    }
    let goal_road = (30.5f32, 20.5f32);
    let goal_snow = (30.5f32, 40.5f32);
    let dist = |a: (f32, f32), b: (f32, f32)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();

    for _ in 0..20 {
        sim::tick(&mut state);
    }

    let road_survivor = state.survivors.iter().find(|s| s.id == road_id).unwrap();
    let snow_survivor = state.survivors.iter().find(|s| s.id == snow_id).unwrap();
    let road_remaining = dist((road_survivor.x, road_survivor.y), goal_road);
    let snow_remaining = dist((snow_survivor.x, snow_survivor.y), goal_snow);

    assert!(
        road_remaining < snow_remaining,
        "the road walker should have closed more distance in the same 20 ticks: road remaining {road_remaining}, \
         snow remaining {snow_remaining}"
    );
    // Quantitatively: TILE_ROAD_SPEED (1.6) vs open ground at snow=25 (~0.84)
    // is nearly a 2x difference, so this shouldn't be a photo finish.
    assert!(
        road_remaining < snow_remaining - 3.0,
        "the gap should be measurable, not marginal: road remaining {road_remaining}, snow remaining {snow_remaining}"
    );
}

#[test]
fn a_survivor_on_open_ground_walks_at_exactly_the_snow_depths_speed_factor() {
    // A tighter, quantitative companion to the qualitative race above:
    // pins down that the per-tick step size on UNIFORM ground (where
    // `road_step`'s neighbour search reduces to the straight line, per its
    // own doc comment) matches `speed_factor` * `SURVIVOR_SPEED_PER_TICK`
    // exactly, not just "faster/slower".
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    for b in &state.buildings {
        assert_ne!(b.kind, BuildingKind::Tent);
    }
    let day_start = (state.tick / TICKS_PER_DAY) * TICKS_PER_DAY;
    state.tick = day_start + (0.33 * TICKS_PER_DAY as f32) as u64;

    let snow_depth = 40u8;
    // Three rows wide for the same reason as the race test above -- see its
    // comment. Without this, `road_step` could greedily drift onto
    // untouched, cheaper neighbouring ground instead of walking the
    // intended uniform strip, which would break the exact-step-size
    // assertion below.
    for x in 10u8..=30 {
        for y in 19u8..=21 {
            state.tiles[tile_index(x, y)] = Tile { terrain: Terrain::Snow, deposit: 0, road: false, snow: snow_depth };
        }
    }
    let sid = state.survivors[0].id;
    {
        let s = state.survivors.iter_mut().find(|s| s.id == sid).unwrap();
        s.x = 10.5;
        s.y = 20.5;
        s.assigned_building = None;
        s.move_target = Some((30, 20));
    }

    let expected_step = Tile { terrain: Terrain::Snow, deposit: 0, road: false, snow: snow_depth }.speed_factor()
        * SURVIVOR_SPEED_PER_TICK;
    let ticks = 5u64;
    for _ in 0..ticks {
        sim::tick(&mut state);
    }
    let s = state.survivors.iter().find(|s| s.id == sid).unwrap();
    let travelled = s.x - 10.5;
    assert!(
        (travelled - expected_step * ticks as f32).abs() < 0.05,
        "over {ticks} ticks on uniform snow={snow_depth} ground, distance travelled should match \
         speed_factor * SURVIVOR_SPEED_PER_TICK exactly: expected {}, got {travelled}",
        expected_step * ticks as f32
    );
}
