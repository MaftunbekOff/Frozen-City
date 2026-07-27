//! V0.19 balance probe (dev tool, not shipped logic): how fast weather
//! closes a road, how much of a staffed Snow Crew's radius it can actually
//! hold open, and how much faster a cleared road is to walk than open snow.
//! Prints numbers, makes no assertions -- the tests own correctness, this
//! owns "is the number sensible", and this is the first time anyone has
//! seen these particular ones.
//!
//! Mirrors `v018_balance.rs`'s shape (a few seeded setups, printed curves),
//! including its `comfortable()` helper -- but most of the probes below
//! deliberately DON'T use it, calling `sim::snow::tick_snowfall`/
//! `tick_snow_crews` directly instead of the full `sim::tick`. These are
//! pure weather/road mechanics with no dependence on hunger, warmth or
//! production, so there is no frostbite for them to accidentally measure --
//! the exact failure mode `comfortable()` exists to guard against in
//! `v018_balance.rs`'s probes, which DO run the whole colony for a week.
//! The one probe here that genuinely needs a living, working colony over
//! many in-game days -- whether a road actually stays open under REAL,
//! randomly-timed blizzards, not a hypothetical sustained one -- uses
//! `comfortable()` for exactly that reason.

use fc_game::{sim, types::*};

/// Copied from `v018_balance.rs` rather than shared (these dev tools don't
/// link to each other) -- a colony with food, fuel, a lit furnace and
/// HOUSING, so a long multi-day probe measures the thing it's probing
/// instead of measuring frostbite. See that file's copy for the full
/// reasoning on why the Tents aren't optional.
fn comfortable(seed: u64) -> GameState {
    let mut st = sim::new_game_bootstrapped(seed, 60);
    st.stock.food = 9e5;
    st.stock.water = 9e5;
    st.stock.coal = 9e5;
    st.stock.wood = 9e5;
    st.furnace_level = 3;
    for b in st.buildings.iter_mut() {
        b.level = BUILDING_MAX_LEVEL;
    }
    for (x, y) in [(29u8, 29u8), (33, 29), (29, 33), (33, 33)] {
        sim::apply_command(&mut st, 0, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y, facing: 0 });
    }
    sim::finish_all_construction(&mut st);
    st
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

fn find_road_spot_near(state: &GameState, cx: u8, cy: u8, r: i32) -> (u8, u8) {
    for dy in -r..=r {
        for dx in -r..=r {
            let (x, y) = (cx as i32 + dx, cy as i32 + dy);
            if in_bounds(x, y) && state.can_lay_road(x as u8, y as u8).is_ok() {
                return (x as u8, y as u8);
            }
        }
    }
    panic!("no layable road tile within {r} of ({cx}, {cy})");
}

fn place_and_staff_snow_crew(state: &mut GameState, x: u8, y: u8) -> u32 {
    sim::apply_command(state, 0, &PlayerCommand::Place { kind: BuildingKind::SnowCrew, x, y, facing: 0 });
    let id = state.buildings.last().unwrap().id;
    sim::finish_all_construction(state);
    let cur = state.find_building(id).unwrap().workers as i8;
    sim::apply_command(state, 0, &PlayerCommand::AdjustWorkers { building: id, delta: 2 - cur });
    id
}

// --- 1: how fast weather closes a road ---

/// In-game days for a freshly cleared road tile to become "buried"
/// (`Tile::road_is_buried`, i.e. `snow >= ROAD_SNOW_PENALTY`) under
/// sustained weather. `blizzard` sustains `SNOW_FALL_BLIZZARD_PER_DAY`
/// indefinitely rather than the real `BLIZZARD_TICKS` (one day) -- a
/// hypothetical "what if it never let up" upper bound, not what a single
/// real storm does (see the realistic probe at the bottom for that).
/// Isolated: survivors cleared out (no trampling), `tick_snowfall` called
/// directly, so nothing else can move this number.
fn road_closure_days(blizzard: bool) -> f64 {
    let mut st = sim::new_game_bootstrapped(1, 12);
    st.survivors.clear();
    st.tick = 0;
    if blizzard {
        st.blizzard_until = u64::MAX;
    }
    let watch = (10u8, 10u8);
    st.tiles[tile_index(watch.0, watch.1)] = Tile { terrain: Terrain::Snow, deposit: 0, road: true, snow: 0 };

    let cap_days = 60u64;
    for tick in 1..=(cap_days * TICKS_PER_DAY) {
        st.tick = tick;
        sim::snow::tick_snowfall(&mut st);
        if st.tiles[tile_index(watch.0, watch.1)].road_is_buried() {
            return tick as f64 / TICKS_PER_DAY as f64;
        }
    }
    f64::INFINITY
}

// --- 2: how much of its radius a Snow Crew can actually hold open ---

/// After `days` of sustained weather starting from bare ground, how many of
/// the tiles inside one staffed (2 anonymous workers, level 1) Snow Crew's
/// `SNOW_CREW_RADIUS` end up held below `ROAD_SNOW_PENALTY` ("open"),
/// against how many exist in that square -- plus the average depth across
/// all of them. Isolated the same way as `road_closure_days`.
fn snow_crew_holding_power(blizzard: bool, days: u64) -> (u32, u32, f64) {
    let mut st = sim::new_game_bootstrapped(2, 12);
    st.survivors.clear();
    st.tick = 0;
    if blizzard {
        st.blizzard_until = u64::MAX;
    }
    let (bx, by) = find_spot(&st, BuildingKind::SnowCrew);
    place_and_staff_snow_crew(&mut st, bx, by);

    for _ in 0..(days * TICKS_PER_DAY) {
        st.tick += 1;
        sim::snow::tick_snowfall(&mut st);
        sim::snow::tick_snow_crews(&mut st);
    }

    let mut held_open = 0u32;
    let mut total = 0u32;
    let mut depth_sum = 0u64;
    for dy in -SNOW_CREW_RADIUS..=SNOW_CREW_RADIUS {
        for dx in -SNOW_CREW_RADIUS..=SNOW_CREW_RADIUS {
            let (tx, ty) = (bx as i32 + dx, by as i32 + dy);
            if !in_bounds(tx, ty) {
                continue;
            }
            let snow = st.tiles[tile_index(tx as u8, ty as u8)].snow;
            total += 1;
            depth_sum += snow as u64;
            if snow < ROAD_SNOW_PENALTY {
                held_open += 1;
            }
        }
    }
    (held_open, total, depth_sum as f64 / total as f64)
}

// --- 3: walk time A -> B, with and without a road ---

/// Ticks for a lone survivor to cross `dist` tiles of uniform ground with
/// the given `road`/`snow` properties. Three rows wide, like
/// `road_tests.rs`'s movement tests, so `road_step`'s neighbour search can't
/// drift onto cheaper untouched ground next door and quietly change what's
/// being measured.
fn walk_ticks(road: bool, snow: u8, dist: u8) -> u64 {
    let mut st = sim::new_game_bootstrapped(4, 12);
    for x in 0..=dist {
        for y in 19u8..=21 {
            st.tiles[tile_index(x, y)] = Tile { terrain: Terrain::Snow, deposit: 0, road, snow };
        }
    }
    let sid = st.survivors[0].id;
    {
        let s = st.survivors.iter_mut().find(|s| s.id == sid).unwrap();
        s.x = 0.5;
        s.y = 20.5;
        s.assigned_building = None;
        s.move_target = Some((dist, 20));
    }
    let goal = (dist as f32 + 0.5, 20.5f32);
    let mut ticks = 0u64;
    loop {
        sim::tick(&mut st);
        ticks += 1;
        let s = st.survivors.iter().find(|s| s.id == sid).unwrap();
        let d = ((s.x - goal.0).powi(2) + (s.y - goal.1).powi(2)).sqrt();
        if d < 0.1 || ticks > 5000 {
            break;
        }
    }
    ticks
}

fn print_walk(label: &str, road: bool, snow: u8, dist: u8) {
    let ticks = walk_ticks(road, snow, dist);
    println!(
        "  {label:<20} {dist:>3} tiles -> {ticks:>5} ticks ({:>6.1}s real-time)",
        ticks as f32 * TICK_MS as f32 / 1000.0
    );
}

// --- 4: a REAL colony's road under REAL, randomly-timed blizzards ---

/// A funded, housed colony (`comfortable()`) with a staffed Snow Crew and
/// one actually-laid road tile inside its radius, run for `days` through
/// the genuine day-rollover blizzard roll (`tick.rs`'s
/// `erng.chance(BLIZZARD_CHANCE)`) -- not the sustained hypothetical the
/// isolated probes above use. Reports what fraction of days the road
/// actually ended the day buried, despite the crew.
/// `with_crew` is the whole question: an UNATTENDED road is where the
/// mechanic either bites or doesn't (a storm should shut it), and an attended
/// one is where the answer to that pressure — a staffed Snow Crew — either
/// holds or doesn't. Measuring only the attended case (which this probe
/// originally did) reports 0% burial and reads as "blizzards never close
/// roads", when it actually means "the crew was doing its job".
fn realistic_road_upkeep(seed: u64, days: u32, with_crew: bool) {
    let mut st = comfortable(seed);
    let (bx, by) = find_spot(&st, BuildingKind::SnowCrew);
    if with_crew {
        place_and_staff_snow_crew(&mut st, bx, by);
    }
    let (rx, ry) = find_road_spot_near(&st, bx, by, SNOW_CREW_RADIUS - 1);
    st.stock.wood = 9e5;
    sim::apply_command(&mut st, 0, &PlayerCommand::BuildRoad { tiles: vec![(rx, ry)] });
    assert!(st.tile(rx, ry).unwrap().road, "probe setup: the road must actually have been laid");

    let mut buried_days = 0u32;
    let mut blizzard_days = 0u32;
    for _ in 0..days {
        let was_blizzard_today = st.blizzard_active();
        for _ in 0..TICKS_PER_DAY {
            sim::tick(&mut st);
        }
        if was_blizzard_today || st.blizzard_active() {
            blizzard_days += 1;
        }
        if st.tile(rx, ry).unwrap().road_is_buried() {
            buried_days += 1;
        }
    }
    println!(
        "  seed {seed:<3} {:<12} over {days} days: {blizzard_days} touched by a blizzard, \
         road buried on {buried_days} days ({:>4.1}%)",
        if with_crew { "with crew" } else { "unattended" },
        100.0 * buried_days as f64 / days as f64
    );
}

fn main() {
    println!("=== how fast weather closes a road (sustained, hypothetical) ===");
    println!(
        "  fair weather ({} snow/day): {:.1} days to bury (ROAD_SNOW_PENALTY = {})",
        SNOW_FALL_PER_DAY,
        road_closure_days(false),
        ROAD_SNOW_PENALTY
    );
    println!(
        "  blizzard ({} snow/day):    {:.2} days to bury",
        SNOW_FALL_BLIZZARD_PER_DAY,
        road_closure_days(true)
    );
    // A real blizzard only lasts BLIZZARD_TICKS (one day) -- print what
    // fraction of the burial threshold a single storm covers on its own,
    // starting from bare ground.
    println!(
        "  one real blizzard day alone adds ~{:.0} snow ({:.0}% of ROAD_SNOW_PENALTY, from bare ground)",
        SNOW_FALL_BLIZZARD_PER_DAY,
        100.0 * SNOW_FALL_BLIZZARD_PER_DAY / ROAD_SNOW_PENALTY as f32
    );

    println!("\n=== how much of its radius one staffed Snow Crew can hold open ===");
    for (label, blizzard, days) in [("fair weather", false, 10u64), ("sustained blizzard", true, 10u64)] {
        let (open, total, avg_depth) = snow_crew_holding_power(blizzard, days);
        println!(
            "  {label:<20} after {days:>2} days: {open:>3}/{total} tiles held open (< {ROAD_SNOW_PENALTY}), \
             avg depth {avg_depth:>5.1}"
        );
    }

    println!("\n=== walk time across 20 tiles, with and without a road ===");
    print_walk("bare ground", false, 0, 20);
    print_walk("light snow (25)", false, 25, 20);
    print_walk("deep snow (80)", false, 80, 20);
    print_walk("cleared road", true, 0, 20);
    print_walk("buried road (60)", true, 60, 20);

    println!("\n=== a real colony's road, under real random blizzards ===");
    for seed in [10u64, 20, 30, 40, 50] {
        realistic_road_upkeep(seed, 40, false);
    }
    println!();
    for seed in [10u64, 20, 30, 40, 50] {
        realistic_road_upkeep(seed, 40, true);
    }
}
