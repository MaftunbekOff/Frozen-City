//! Pure-simulation tests for V0.18 survivor life cycle: continuous aging,
//! the Child/Adult/Elder stage boundaries and their work/food/frailty
//! factors, pairing (symmetry, one-per-day, partner cleanup on death),
//! couples raising the birth rate, old-age death going through the normal
//! death path, and the separate `lifecycle_rng` stream. Mirrors the style of
//! `fatigue_tests.rs` and `illness_tests.rs` (same imports, helper patterns,
//! control-vs-experiment isolation for direct-effect checks, a "hunt across
//! seeds/rolls" for the genuinely probabilistic paths).

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

/// Requires nearby forest, same reasoning as `fatigue_tests.rs`/
/// `profession_tests.rs`'s helper of the same name — otherwise a production
/// comparison can be invisible over a short test run.
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

/// `place_and_finish`, then clears the auto-crewed anonymous construction
/// crew it leaves behind: `Place` fills up to `CONSTRUCTION_CREW_MAX` idle
/// survivors in as anonymous workers, and finishing only clamps that down to
/// the kind's `max_workers()` rather than zeroing it — so a freshly finished
/// 1-2-worker building (Sawmill, Kitchen, ...) can already be at full
/// capacity before a test ever calls `AssignSurvivor`, which would then
/// silently no-op against a building with no room left. Mirrors the same
/// defensive step `profession_tests.rs`'s
/// `leader_gets_the_profession_match_bonus_at_any_building` already takes.
fn place_finish_and_clear_crew(state: &mut GameState, kind: BuildingKind, x: u8, y: u8) -> u32 {
    let id = place_and_finish(state, kind, x, y);
    let cur = state.find_building(id).unwrap().workers as i8;
    sim::apply_command(state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: -cur });
    id
}

/// Marks every starting mission as already done, so a test that places a
/// Sawmill/Tent/etc. purely to exercise `AssignSurvivor` doesn't also
/// silently collect a mission reward (e.g. `MissionKind::Sawmills(1)`'s
/// `reward_wood: 20`, see `sim::missions`) into the very stockpile a
/// production comparison is trying to measure.
fn disable_missions(state: &mut GameState) {
    for m in state.missions.iter_mut() {
        m.done = true;
    }
}

fn fund(state: &mut GameState) {
    state.stock.food = 9999.0;
    state.stock.water = 9999.0;
    state.stock.coal = 9999.0;
}

/// Lands `state.tick` exactly on the once-daily pairing/old-age roll offset
/// (`BIRTH_TICK`) on whatever day it's currently mid-way through — used by
/// the pairing tests below to call `sim::lifecycle::tick_lifecycle` directly,
/// isolating it from production/hunger/arrivals (which a full `sim::tick`
/// would also run).
fn align_to_birth_tick(state: &mut GameState) {
    let day_start = (state.tick / TICKS_PER_DAY) * TICKS_PER_DAY;
    state.tick = day_start + BIRTH_TICK;
}

// --- A: aging + life stage boundaries ---

#[test]
fn age_advances_exactly_one_day_per_ticks_per_day() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    let sid = state.survivors[0].id;
    let start = state.survivors.iter().find(|s| s.id == sid).unwrap().age_days;
    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut state);
    }
    let end = state.survivors.iter().find(|s| s.id == sid).unwrap().age_days;
    // `+= 1.0/TICKS_PER_DAY` accumulated 750 times into an already-nonzero
    // f32 accrues real summation drift (each add's rounding error scales
    // with the CURRENT accumulator magnitude, not the tiny delta) -- a
    // couple thousandths of a day, not a logic bug. A generous 1% tolerance
    // still easily catches anything actually wrong (a skipped day reads as
    // ~0.0, a double-counted one as ~2.0).
    assert!(
        (end - start - 1.0).abs() < 0.01,
        "age should advance by exactly 1.0 per TICKS_PER_DAY ticks: start={start}, end={end}"
    );
}

#[test]
fn life_stage_boundaries_and_per_stage_factors_match_the_documented_constants() {
    let mut state = sim::new_game(SEED, 12);
    let s = &mut state.survivors[0];

    s.age_days = ADULT_AGE_DAYS - 0.01;
    assert_eq!(s.stage(), LifeStage::Child, "just below ADULT_AGE_DAYS is still a child");
    assert_eq!(s.age_work_factor(), CHILD_WORK_FACTOR);
    assert_eq!(s.food_factor(), CHILD_FOOD_FACTOR);
    assert!(s.is_frail());

    s.age_days = ADULT_AGE_DAYS; // boundary-inclusive on the adult side
    assert_eq!(s.stage(), LifeStage::Adult, "exactly ADULT_AGE_DAYS is already an adult");
    assert_eq!(s.age_work_factor(), 1.0);
    assert_eq!(s.food_factor(), 1.0);
    assert!(!s.is_frail());

    s.age_days = ELDER_AGE_DAYS - 0.01;
    assert_eq!(s.stage(), LifeStage::Adult, "just below ELDER_AGE_DAYS is still an adult");

    s.age_days = ELDER_AGE_DAYS; // boundary-inclusive on the elder side
    assert_eq!(s.stage(), LifeStage::Elder, "exactly ELDER_AGE_DAYS is already an elder");
    assert_eq!(s.age_work_factor(), ELDER_WORK_FACTOR);
    assert_eq!(s.food_factor(), 1.0, "food scaling only ever discounts children, never elders");
    assert!(s.is_frail());
}

// --- B: production integration (named-assignment path) ---

#[test]
fn an_all_child_named_crew_produces_nothing() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 500.0;
    disable_missions(&mut state);
    let (x, y) = find_sawmill_spot_near_forest(&state);
    let sawmill = place_finish_and_clear_crew(&mut state, BuildingKind::Sawmill, x, y);
    // Fill the Sawmill's full 2-slot crew with named children.
    let a = state.survivors[0].id;
    let b = state.survivors[1].id;
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor: a, building: Some(sawmill) });
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor: b, building: Some(sawmill) });
    for s in state.survivors.iter_mut() {
        if s.id == a || s.id == b {
            s.age_days = 0.0;
        }
    }
    assert_eq!(state.find_building(sawmill).unwrap().workers, 2, "sanity: both children hold a named slot");

    let wood_before = state.stock.wood;
    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut state);
    }
    assert_eq!(
        state.stock.wood, wood_before,
        "CHILD_WORK_FACTOR is 0.0 -- a fully named, all-child crew should produce exactly zero wood"
    );
}

#[test]
fn a_survivor_produces_nothing_as_a_child_and_starts_producing_the_instant_they_are_an_adult() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 500.0;
    disable_missions(&mut state);
    let (x, y) = find_sawmill_spot_near_forest(&state);
    let sawmill = place_finish_and_clear_crew(&mut state, BuildingKind::Sawmill, x, y);
    let sid = state.survivors[0].id;
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor: sid, building: Some(sawmill) });
    assert_eq!(state.find_building(sawmill).unwrap().workers, 1, "sanity: the survivor holds a named slot");

    // Comfortably below the threshold (relative, not a fixed offset, so this
    // stays valid whatever ADULT_AGE_DAYS is tuned to) -- one day's worth of
    // continuous aging (`tick_lifecycle`, +1.0) can't accidentally cross it
    // mid-run and muddy the "still a child" half of this comparison.
    state.survivors.iter_mut().find(|s| s.id == sid).unwrap().age_days = ADULT_AGE_DAYS * 0.5;
    assert_eq!(state.survivors.iter().find(|s| s.id == sid).unwrap().stage(), LifeStage::Child);
    let wood_as_child_before = state.stock.wood;
    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut state);
    }
    assert_eq!(
        state.stock.wood, wood_as_child_before,
        "still a child for this whole phase -- should have produced exactly zero"
    );

    // Exactly at the threshold: the single place `ADULT_AGE_DAYS` is
    // interpreted (`Survivor::stage`) is boundary-inclusive on the adult side.
    state.survivors.iter_mut().find(|s| s.id == sid).unwrap().age_days = ADULT_AGE_DAYS;
    assert_eq!(state.survivors.iter().find(|s| s.id == sid).unwrap().stage(), LifeStage::Adult);
    let wood_as_adult_before = state.stock.wood;
    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut state);
    }
    assert!(
        state.stock.wood > wood_as_adult_before,
        "the same survivor, now an adult, should produce measurably more than zero this phase"
    );
}

#[test]
fn an_adult_out_produces_an_elder_doing_the_same_job() {
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);
    for state in [&mut control, &mut experiment] {
        state.stock.wood = 500.0;
    }

    let (x, y) = find_sawmill_spot_near_forest(&control);
    let control_id = place_finish_and_clear_crew(&mut control, BuildingKind::Sawmill, x, y);
    let control_survivor = control.survivors[0].id;
    sim::apply_command(
        &mut control,
        1,
        &PlayerCommand::AssignSurvivor { survivor: control_survivor, building: Some(control_id) },
    );

    let (x2, y2) = find_sawmill_spot_near_forest(&experiment); // same seed -> same spot
    let exp_id = place_finish_and_clear_crew(&mut experiment, BuildingKind::Sawmill, x2, y2);
    let exp_survivor = experiment.survivors[0].id;
    sim::apply_command(
        &mut experiment,
        1,
        &PlayerCommand::AssignSurvivor { survivor: exp_survivor, building: Some(exp_id) },
    );
    experiment.survivors.iter_mut().find(|s| s.id == exp_survivor).unwrap().age_days = ELDER_AGE_DAYS;
    assert_eq!(control.find_building(control_id).unwrap().workers, 1, "sanity: control survivor holds a named slot");
    assert_eq!(experiment.find_building(exp_id).unwrap().workers, 1, "sanity: experiment survivor holds a named slot");

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        control.stock.wood > experiment.stock.wood,
        "an adult (age_work_factor 1.0) should out-produce an elder (ELDER_WORK_FACTOR) doing the same job: \
         control={}, experiment={}",
        control.stock.wood,
        experiment.stock.wood
    );
}

// --- C: a child still eats (a reduced share) and still needs a bunk ---

#[test]
fn a_child_eats_a_reduced_food_share() {
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);
    for state in [&mut control, &mut experiment] {
        state.stock.food = 9999.0;
        state.stock.water = 9999.0;
    }
    let sid = control.survivors[0].id; // same seed -> same id in both worlds
    for state in [&mut control, &mut experiment] {
        // High enough that 20 ticks of net decline never drops it near the
        // 25 eat-threshold, so this survivor eats every single tick in both
        // worlds -- the comparison below isolates food_factor alone.
        state.survivors.iter_mut().find(|s| s.id == sid).unwrap().hunger = 90.0;
    }
    experiment.survivors.iter_mut().find(|s| s.id == sid).unwrap().age_days = 0.0; // a child

    let food_before_control = control.stock.food;
    let food_before_experiment = experiment.stock.food;
    let ticks = 20u64;
    for _ in 0..ticks {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }
    let control_eaten = food_before_control - control.stock.food;
    let experiment_eaten = food_before_experiment - experiment.stock.food;

    // Every other survivor is identical (same seed, nothing about being a
    // child touches any shared rng stream), so the two colonies' total food
    // consumption differs by EXACTLY this one survivor's own reduced share.
    let portion_per_tick = FOOD_PER_SURVIVOR_DAY / TICKS_PER_DAY as f32;
    let expected_diff = ticks as f32 * portion_per_tick * (1.0 - CHILD_FOOD_FACTOR);
    let actual_diff = control_eaten - experiment_eaten;
    assert!(
        (actual_diff - expected_diff).abs() < 0.05,
        "a child should eat exactly CHILD_FOOD_FACTOR of an adult's portion: expected diff {expected_diff}, got {actual_diff}"
    );
    assert!(experiment_eaten > 0.0, "sanity: the child should still be eating something, just less");
}

#[test]
fn a_child_still_needs_a_bunk_like_anyone_else() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    place_and_finish(&mut state, BuildingKind::Tent, x, y);
    assert_eq!(state.housing_capacity(), TENT_CAPACITY, "sanity: exactly one Tent's worth of bunks");

    let child_id = state.survivors[0].id;
    state.survivors.iter_mut().find(|s| s.id == child_id).unwrap().age_days = 0.0;
    assert_eq!(state.survivors.iter().find(|s| s.id == child_id).unwrap().stage(), LifeStage::Child);

    // Roster order is unaffected by aging alone, so a child among the first
    // TENT_CAPACITY survivors still holds a bunk exactly like an adult would
    // -- no age-based exemption exists anywhere in the housing accounting.
    assert!(
        state.bunked_ids().contains(&child_id),
        "a child should still occupy a housing slot like anyone else"
    );
}

// --- D: pairing (symmetry, at most one couple per roll, own rng stream) ---

#[test]
fn pairing_is_symmetric_and_forms_at_most_one_couple_per_roll() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    align_to_birth_tick(&mut state);

    let mut found = false;
    // PARTNER_CHANCE_PER_DAY = 0.22 with 8 eligible adults -> the odds of
    // never once succeeding in 500 independent daily rolls are astronomically
    // small ((1 - 0.22)^500), so this is a deterministic-in-practice hunt,
    // not a flaky one.
    for _ in 0..500 {
        let before: std::collections::HashSet<u32> =
            state.survivors.iter().filter(|s| s.partner.is_some()).map(|s| s.id).collect();
        sim::lifecycle::tick_lifecycle(&mut state);
        let newly: Vec<u32> = state
            .survivors
            .iter()
            .filter(|s| s.partner.is_some() && !before.contains(&s.id))
            .map(|s| s.id)
            .collect();
        if newly.is_empty() {
            continue;
        }
        assert_eq!(newly.len(), 2, "exactly one couple (two people) should form from a single roll");
        let a = state.survivors.iter().find(|s| s.id == newly[0]).unwrap().clone();
        let b = state.survivors.iter().find(|s| s.id == newly[1]).unwrap().clone();
        assert_eq!(a.partner, Some(b.id), "pairing must be symmetric");
        assert_eq!(b.partner, Some(a.id), "pairing must be symmetric");
        found = true;
        break;
    }
    assert!(found, "a pairing should occur within 500 daily rolls at PARTNER_CHANCE_PER_DAY");
}

#[test]
fn both_sides_of_a_partnership_clear_when_one_dies() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    let a_id = state.survivors[0].id;
    let b_id = state.survivors[1].id;
    for s in state.survivors.iter_mut() {
        if s.id == a_id {
            s.partner = Some(b_id);
        } else if s.id == b_id {
            s.partner = Some(a_id);
        }
    }

    // Force A's death directly and deterministically (comfortably below
    // zero so it can't be nudged back positive by this same tick's warmth
    // regen) -- mirrors how illness_tests.rs sets `sick_left` directly
    // rather than waiting for a real trigger.
    state.survivors.iter_mut().find(|s| s.id == a_id).unwrap().hp = -1.0;
    sim::tick(&mut state);

    assert!(state.survivors.iter().all(|s| s.id != a_id), "sanity: A should be dead and removed");
    let b = state.survivors.iter().find(|s| s.id == b_id).unwrap();
    assert_eq!(b.partner, None, "the surviving partner's link should clear when the other side dies");
}

// --- E: a couple raises the birth rate (statistical, deterministic seeds) ---

fn add_tents(state: &mut GameState, n: usize) {
    state.stock.wood = 9999.0;
    for _ in 0..n {
        let (x, y) = find_spot(state, BuildingKind::Tent);
        place_and_finish(state, BuildingKind::Tent, x, y);
    }
}

/// Runs `days` of real ticks (fully funded, so starvation/thirst never gate
/// a birth) and counts births specifically -- a newly appeared survivor id
/// whose `age_days` is exactly 0.0 the tick it appears. Arrivals (the other
/// way new ids show up) always draw a nonzero starting age
/// (`ARRIVAL_AGE_MIN_DAYS..ARRIVAL_AGE_MAX_DAYS`), so this can't double-count
/// them, and `total_events`/the capped `events` log never enters into it.
///
/// `suppress_pairing`, when set, clears every survivor's `partner` right
/// after each tick -- PARTNER_CHANCE_PER_DAY (0.22) is high enough that an
/// unforced colony pairs up on its own well within a 30-day window (P(at
/// least one pairing) = 1 - 0.78^30 =~ 99.9%), which would otherwise let the
/// "no couple" side of the comparison below quietly acquire one too and
/// converge with the "has a couple" side instead of staying a clean control.
fn count_births(state: &mut GameState, days: u64, suppress_pairing: bool) -> u32 {
    let mut count = 0u32;
    let mut known_ids: std::collections::HashSet<u32> = state.survivors.iter().map(|s| s.id).collect();
    for _ in 0..(days * TICKS_PER_DAY) {
        fund(state);
        sim::tick(state);
        if suppress_pairing {
            for s in state.survivors.iter_mut() {
                s.partner = None;
            }
        }
        if state.phase != GamePhase::Running {
            break;
        }
        for s in &state.survivors {
            if known_ids.insert(s.id) && s.age_days == 0.0 {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn a_colony_with_a_couple_has_a_higher_birth_rate_than_one_without() {
    fn make_colony(seed: u64, with_couple: bool) -> GameState {
        let mut state = sim::new_game_bootstrapped(seed, 200);
        add_tents(&mut state, 6); // headroom for population growth either way
        if with_couple {
            let a = state.survivors[0].id;
            let b = state.survivors[1].id;
            for s in state.survivors.iter_mut() {
                if s.id == a {
                    s.partner = Some(b);
                } else if s.id == b {
                    s.partner = Some(a);
                }
            }
        }
        state
    }

    let seeds = 0..20u64;
    let days = 30u64;
    let with_total: u32 =
        seeds.clone().map(|seed| count_births(&mut make_colony(seed, true), days, false)).sum();
    let without_total: u32 =
        seeds.map(|seed| count_births(&mut make_colony(seed, false), days, true)).sum();

    assert!(
        with_total > without_total,
        "a colony with a couple (BIRTH_COUPLE_MULTIPLIER) should produce more births in aggregate over \
         {days} days x 20 seeds: with={with_total}, without={without_total}"
    );
}

// --- F: old age eventually kills, through the normal death path ---

#[test]
fn old_age_death_eventually_fires_through_the_normal_death_path() {
    let mut found = false;
    // Deliberately funded/warm/fed throughout -- this is exactly the colony
    // condition ("warm and fed") under which a same-tick heal used to be
    // able to silently swallow the old-age kill before the fix in
    // `lifecycle::daily_old_age` (an exactly-0.0 hp getting nudged back
    // positive by this tick's own warmth regen, in `sim::tick`'s survivor
    // loop, before the shared death check ever ran).
    'seeds: for seed in 0..20u64 {
        let mut state = sim::new_game_bootstrapped(seed, 400);
        fund(&mut state);
        let target = state.survivors[0].id;
        let target_name = state.survivors[0].name.clone();
        // 3x past ELDER_AGE_DAYS -> a ~10%/day chance, so this is a matter
        // of days, not a multi-week hunt.
        state.survivors.iter_mut().find(|s| s.id == target).unwrap().age_days = ELDER_AGE_DAYS * 3.0;
        for _ in 0..(25 * TICKS_PER_DAY) {
            fund(&mut state);
            sim::tick(&mut state);
            if state.phase != GamePhase::Running {
                continue 'seeds; // this world ended early; try the next seed
            }
            if !state.survivors.iter().any(|s| s.id == target) {
                assert_eq!(
                    state.corpses.iter().filter(|c| c.id == target).count(),
                    1,
                    "old-age death should leave exactly one corpse, same as every other death"
                );
                assert!(
                    state.events.iter().any(|e| e.text == format!("{target_name} has died of old age.")),
                    "old-age death should be logged as an event: {:?}",
                    state.events.iter().map(|e| &e.text).collect::<Vec<_>>()
                );
                found = true;
                break 'seeds;
            }
        }
    }
    assert!(
        found,
        "an old-age death should eventually fire within 20 seeds x 25 days for a survivor already 3x past ELDER_AGE_DAYS"
    );
}

#[test]
fn an_old_age_kill_lands_well_below_zero_not_exactly_at_it() {
    // Regression for the same-tick-heal bug above, exercised directly at the
    // unit level: `tick_lifecycle` alone (no production/hunger/regen at all)
    // should already leave a comfortably negative hp the moment the roll
    // succeeds, not a bare 0.0 a single small same-tick top-up could undo.
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    let target = state.survivors[0].id;
    state.survivors.iter_mut().find(|s| s.id == target).unwrap().age_days = ELDER_AGE_DAYS * 3.0;
    align_to_birth_tick(&mut state);

    let mut killed = false;
    for _ in 0..500 {
        sim::lifecycle::tick_lifecycle(&mut state);
        let hp = state.survivors.iter().find(|s| s.id == target).unwrap().hp;
        if hp <= 0.0 {
            assert!(hp < -1.0, "the old-age kill should land well below zero, not exactly at it: hp={hp}");
            killed = true;
            break;
        }
    }
    assert!(killed, "an elder 3x past ELDER_AGE_DAYS should fail the odds within 500 daily rolls");
}

// --- G: children/elders are frailer against illness (FRAIL_SICK_MULTIPLIER) ---

#[test]
fn children_and_elders_catch_illness_more_often_than_adults() {
    fn infection_hits(seeds: std::ops::Range<u64>, age_days: f32) -> u32 {
        let mut count = 0u32;
        for seed in seeds {
            let mut state = sim::new_game_bootstrapped(seed, 30);
            fund(&mut state);
            state.disease_until = state.tick + TICKS_PER_DAY;
            let target = state.survivors[0].id;
            state.survivors.iter_mut().find(|s| s.id == target).unwrap().age_days = age_days;
            for _ in 0..TICKS_PER_DAY {
                fund(&mut state);
                sim::tick(&mut state);
            }
            if state.survivors.iter().find(|s| s.id == target).is_some_and(|s| s.is_sick()) {
                count += 1;
            }
        }
        count
    }

    let seeds = 0..80u64;
    let adult_age = (ARRIVAL_AGE_MIN_DAYS + ARRIVAL_AGE_MAX_DAYS) / 2.0;
    let child_hits = infection_hits(seeds.clone(), 0.0);
    let adult_hits = infection_hits(seeds, adult_age);

    assert!(
        child_hits > adult_hits,
        "a frail (child) survivor should be infected in more of the 80 hunted seeds than an adult \
         (FRAIL_SICK_MULTIPLIER): child={child_hits}/80, adult={adult_hits}/80"
    );
}

// --- H: determinism -- lifecycle rolls never shift the other rng streams ---

#[test]
fn tick_lifecycle_never_touches_the_other_rng_streams() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    align_to_birth_tick(&mut state);
    let (rng_before, erng_before, irng_before) = (state.rng, state.event_rng, state.illness_rng);
    let lifecycle_rng_before = state.lifecycle_rng;

    sim::lifecycle::tick_lifecycle(&mut state);

    assert_eq!(state.rng, rng_before, "tick_lifecycle must never touch the main sim rng stream");
    assert_eq!(state.event_rng, erng_before, "tick_lifecycle must never touch the event rng stream");
    assert_eq!(state.illness_rng, irng_before, "tick_lifecycle must never touch the illness rng stream");
    assert_ne!(
        state.lifecycle_rng, lifecycle_rng_before,
        "sanity: the daily pairing roll always draws at least its `chance` roll from lifecycle_rng"
    );
}

// --- I: the depopulation invariant -- old age alone must not empty a
// well-supplied colony ---

#[test]
fn old_age_does_not_collapse_a_well_supplied_colony_over_a_month() {
    // Regression for the V0.18 balance bug: a colony that never wants for
    // food/water/coal and stays warm can still be wiped out entirely once
    // its founding cohort ages into Elder in a tight band and starts failing
    // `OLD_AGE_DEATH_PER_DAY` rolls together. `ADULT_AGE_DAYS`/`ELDER_AGE_DAYS`/
    // the arrival band were retuned (see `types.rs`) specifically so a
    // founding cohort spreads its old-age risk over ~26 days instead of one
    // cliff -- this pins that outcome rather than the exact numbers.
    //
    // The window (30 days) is deliberately short of where `temperature()`
    // turns lethal on its own (the mid-40s, by design -- confirmed via
    // `examples/v018_balance.rs`, a pre-existing V0.3 mechanic, not V0.18's
    // to fix) -- funded stock and a warm furnace rule out hunger/thirst/cold
    // as the cause of any depopulation seen here, isolating old age as the
    // one remaining explanation this test can pin down.
    let mut state = sim::new_game_bootstrapped(SEED, 90);
    // `GameState.furnace_level` alone (burn intensity) barely moves
    // `heat_radius()` -- that formula keys off the FURNACE BUILDING's own
    // `level` (structure tier 1-10), only unlocking its big "whole map"
    // branch (`struct_level >= 7`) once that's cranked too. Mirrors
    // `examples/v018_balance.rs`'s `comfortable()` helper exactly (which is
    // how the coordinator's own probe stayed warm past day 40): both fields,
    // both maxed, on every building, not just the furnace.
    state.furnace_level = 8;
    for b in state.buildings.iter_mut() {
        b.level = 8;
    }
    for _ in 0..2 {
        let (x, y) = find_spot(&state, BuildingKind::Tent);
        sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y, facing: 0 });
    }
    sim::finish_all_construction(&mut state);

    let days = 30u64;
    let mut death_causes: std::collections::HashMap<&str, u32> = Default::default();
    let mut events_seen = state.total_events;
    for _ in 0..(days * TICKS_PER_DAY) {
        fund(&mut state);
        sim::tick(&mut state);
        // `state.events` is a capped log (12 entries) -- scanning only the
        // freshly appended tail (sized off the monotonic `total_events`
        // counter) instead of the whole vec at the end is what makes this
        // safe over a run long enough to evict most of it.
        let new = (state.total_events - events_seen) as usize;
        events_seen = state.total_events;
        if new > 0 {
            let n = new.min(state.events.len());
            for e in &state.events[state.events.len() - n..] {
                for cause in
                    ["died of old age", "succumbed to illness", "froze to death", "starved", "died of thirst"]
                {
                    if e.text.contains(cause) {
                        *death_causes.entry(cause).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    assert!(
        !state.survivors.is_empty(),
        "a well-supplied, warm colony should not fully depopulate within {days} days: death causes {death_causes:?}"
    );
    // Without this, "zero old-age deaths" could just mean nobody ever aged
    // into Elder at all in this window, which would make the assertion
    // above vacuous rather than a real test of the fix.
    assert!(
        state.survivors.iter().any(|s| s.stage() == LifeStage::Elder) || death_causes.contains_key("died of old age"),
        "sanity: at ELDER_AGE_DAYS={ELDER_AGE_DAYS} and a {days}-day run, elders should actually exist by now -- \
         otherwise this test isn't exercising anything"
    );
}
