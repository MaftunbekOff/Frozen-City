//! Pure-simulation tests for V0.17 individual illness: the once-daily
//! infection roll (outbreak + contagion, drawn from the separate
//! `illness_rng` stream), the sick HP/production penalties, staffed-Hospital
//! recovery, and the recovery event. Mirrors the style of `event_tests.rs`
//! (control-vs-experiment isolation for direct-effect checks, a real-trigger
//! "hunt across seeds" for the genuinely probabilistic paths, and a strong
//! determinism guard) and `profession_tests.rs`/`morale_tests.rs` (building
//! helpers).

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

/// Requires nearby forest, same reasoning as `profession_tests.rs`'s helper
/// of the same name — otherwise the production comparison below can be
/// invisible over a short test run.
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

fn place_and_staff(state: &mut GameState, kind: BuildingKind, x: u8, y: u8, workers: i8) -> u32 {
    sim::apply_command(state, 1, &PlayerCommand::Place { kind, x, y, facing: 0 });
    let id = state.buildings.last().unwrap().id;
    sim::finish_all_construction(state);
    let cur = state.find_building(id).unwrap().workers as i8;
    sim::apply_command(state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: workers - cur });
    id
}

fn place_and_finish(state: &mut GameState, kind: BuildingKind, x: u8, y: u8) -> u32 {
    sim::apply_command(state, 1, &PlayerCommand::Place { kind, x, y, facing: 0 });
    let id = state.buildings.last().unwrap().id;
    sim::finish_all_construction(state);
    id
}

fn fund(state: &mut GameState) {
    state.stock.food = 9999.0;
    state.stock.water = 9999.0;
    state.stock.coal = 9999.0;
}

// --- A: the once-daily infection roll (outbreak) ---

#[test]
fn an_active_outbreak_eventually_infects_someone_via_the_daily_roll() {
    // The roll is genuinely probabilistic (`OUTBREAK_INFECT_CHANCE_PER_DAY`
    // per healthy survivor per day) — mirrors `event_tests.rs`'s
    // `disease_eventually_strikes_and_drains_hp` Part 1: hunt across a
    // spread of seeds for the real, tick-driven trigger path actually firing
    // at least once, rather than asserting anything about a single seed's
    // exact roll.
    let mut found = false;
    'seeds: for seed in 0..50u64 {
        let mut state = sim::new_game_bootstrapped(seed, 30);
        fund(&mut state);
        state.disease_until = state.tick + 10 * TICKS_PER_DAY;
        for _ in 0..(6 * TICKS_PER_DAY) {
            fund(&mut state);
            sim::tick(&mut state);
            if state.phase != GamePhase::Running {
                continue 'seeds; // this world ended early; try the next seed
            }
            if state.sick_count() > 0 {
                found = true;
                break 'seeds;
            }
        }
    }
    assert!(
        found,
        "an active outbreak should eventually infect someone via the real ILLNESS_TICK roll within 50 seeds x 6 days"
    );
}

/// The dynamic-events feature draws from a separate `event_rng` stream
/// (`event_tests.rs`'s `events_do_not_break_determinism`); illness draws from
/// yet another one of its own (`illness_rng`) for exactly the same reason —
/// two identical worlds, identically ticked, must land on the identical set
/// of infections.
#[test]
fn same_seed_and_setup_yields_identical_infections() {
    let mut a = sim::new_game_bootstrapped(SEED, 30);
    let mut b = sim::new_game_bootstrapped(SEED, 30);
    for state in [&mut a, &mut b] {
        fund(state);
        state.disease_until = state.tick + 10 * TICKS_PER_DAY;
    }

    for _ in 0..(8 * TICKS_PER_DAY) {
        fund(&mut a);
        fund(&mut b);
        sim::tick(&mut a);
        sim::tick(&mut b);
    }

    assert_eq!(a, b, "identical seeds and setup must yield byte-identical infection outcomes");
}

/// The core "separate stream" guarantee: an outbreak's daily infection rolls
/// consume draws from `illness_rng` alone, never `rng` or `event_rng` — kept
/// entirely BEFORE `EVENT_GRACE_DAY` (3) so the two worlds below can't
/// diverge in `event_rng` for an unrelated reason (once
/// `state.day() >= EVENT_GRACE_DAY`, the day-rollover disease/blizzard/
/// caravan checks short-circuit around their own `erng.chance(..)` call
/// whenever `disease_active()` already differs between the two worlds,
/// which would confound this comparison; the once-daily illness roll itself
/// has no such grace-day gate, so this window is enough to exercise it).
#[test]
fn illness_rolls_draw_from_their_own_stream_leaving_rng_and_event_rng_untouched() {
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);
    fund(&mut control);
    fund(&mut experiment);
    // Active outbreak in the experiment only, set directly (not via the
    // grace-gated trigger) so no `event_rng` draw is spent either way.
    experiment.disease_until = experiment.tick + 5 * TICKS_PER_DAY;

    let just_before_grace = (EVENT_GRACE_DAY as u64 - 1) * TICKS_PER_DAY - 1;
    while control.tick < just_before_grace {
        fund(&mut control);
        fund(&mut experiment);
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert_eq!(control.rng, experiment.rng, "the illness roll must never perturb the main sim rng stream");
    assert_eq!(
        control.event_rng, experiment.event_rng,
        "the illness roll must never perturb the event rng stream"
    );
    assert_ne!(
        control.illness_rng, experiment.illness_rng,
        "the outbreak world's illness_rng should have advanced from its daily infection rolls, unlike the control's"
    );
}

// --- B: sick HP drain and production penalty ---

#[test]
fn a_sick_survivor_loses_extra_hp_at_sick_hp_per_day() {
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);
    fund(&mut control);
    fund(&mut experiment);

    let sid = control.survivors[0].id; // same seed -> same id in both worlds
    for state in [&mut control, &mut experiment] {
        state.survivors.iter_mut().find(|s| s.id == sid).unwrap().hp = 90.0;
    }
    experiment.survivors.iter_mut().find(|s| s.id == sid).unwrap().sick_left = SICKNESS_TICKS;

    let ticks = 20u64;
    for _ in 0..ticks {
        fund(&mut control);
        fund(&mut experiment);
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    let control_hp = control.survivors.iter().find(|s| s.id == sid).unwrap().hp;
    let experiment_hp = experiment.survivors.iter().find(|s| s.id == sid).unwrap().hp;
    // Both worlds share the identical warmth/hunger environment (same seed,
    // same funded stock), so any HP difference between them is exactly the
    // sick-only drain term, added once per tick on top of everything else.
    // Note the units: `SICK_HP_PER_DAY` is per DAY (divided by
    // `TICKS_PER_DAY`), unlike the hunger/thirst/cold drains right next to it
    // in `tick.rs`, which are written as per-HOUR rates against `tph`.
    let expected_diff = ticks as f32 * SICK_HP_PER_DAY / TICKS_PER_DAY as f32;
    let actual_diff = control_hp - experiment_hp;
    assert!(
        (actual_diff - expected_diff).abs() < 0.01,
        "a sick survivor should lose exactly SICK_HP_PER_DAY/day more than a healthy control: expected diff {expected_diff}, got {actual_diff}"
    );
}

#[test]
fn a_sick_survivor_produces_at_sick_work_factor() {
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

    let (x2, y2) = find_sawmill_spot_near_forest(&experiment); // same seed -> same spot
    let exp_id = place_and_finish(&mut experiment, BuildingKind::Sawmill, x2, y2);
    let exp_survivor = experiment.survivors[0].id;
    sim::apply_command(
        &mut experiment,
        1,
        &PlayerCommand::AssignSurvivor { survivor: exp_survivor, building: Some(exp_id) },
    );
    // SICKNESS_TICKS is a full 2 in-game days; the run below is only 1, so
    // this survivor stays sick (`sick_left` decays at most 1 tick's worth
    // per tick with no Hospital) for the entire comparison.
    experiment.survivors.iter_mut().find(|s| s.id == exp_survivor).unwrap().sick_left = SICKNESS_TICKS;

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        control.stock.wood > experiment.stock.wood,
        "a sick worker (SICK_WORK_FACTOR) should produce less than a healthy one: control={}, experiment={}",
        control.stock.wood,
        experiment.stock.wood
    );
}

// --- C: staffed Hospital recovery + the recovery event ---

#[test]
fn a_staffed_hospital_burns_illness_down_faster_than_the_natural_course() {
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);
    for state in [&mut control, &mut experiment] {
        state.stock.wood = 500.0;
    }

    let (x, y) = find_spot(&experiment, BuildingKind::Hospital);
    // One anonymous worker, level 1 (`level_factor() == 1.0`) -> exactly
    // 1.0 hospital "unit", so the expected recovery-rate formula below has
    // no other multiplier to account for.
    place_and_staff(&mut experiment, BuildingKind::Hospital, x, y, 1);

    let sid = control.survivors[0].id;
    control.survivors.iter_mut().find(|s| s.id == sid).unwrap().sick_left = SICKNESS_TICKS;
    experiment.survivors.iter_mut().find(|s| s.id == sid).unwrap().sick_left = SICKNESS_TICKS;

    let ticks = 100u64;
    for _ in 0..ticks {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    let control_left = control.survivors.iter().find(|s| s.id == sid).unwrap().sick_left;
    let experiment_left = experiment.survivors.iter().find(|s| s.id == sid).unwrap().sick_left;

    let hospital_units = 1.0f32;
    let experiment_rate = 1.0 + hospital_units * HOSPITAL_RECOVERY_PER_UNIT_DAY / TICKS_PER_DAY as f32;
    let expected_control = (SICKNESS_TICKS - ticks as f32 * 1.0).max(0.0);
    let expected_experiment = (SICKNESS_TICKS - ticks as f32 * experiment_rate).max(0.0);

    assert!(
        (control_left - expected_control).abs() < 0.5,
        "no-hospital control should burn down at exactly 1 tick/tick: expected {expected_control}, got {control_left}"
    );
    assert!(
        (experiment_left - expected_experiment).abs() < 0.5,
        "staffed-hospital experiment should burn down faster: expected {expected_experiment}, got {experiment_left}"
    );
    assert!(
        experiment_left < control_left,
        "a staffed Hospital should burn illness down faster than the natural course: control={control_left}, experiment={experiment_left}"
    );
}

#[test]
fn recovery_clears_the_sick_flag_and_logs_an_event() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    fund(&mut state);
    let sid = state.survivors[0].id;
    let name = state.survivors[0].name.clone();
    // Small enough to reach 0 within a handful of ticks at the natural
    // (no-Hospital) 1-tick-per-tick recovery rate.
    state.survivors.iter_mut().find(|s| s.id == sid).unwrap().sick_left = 3.0;
    assert!(state.survivors.iter().find(|s| s.id == sid).unwrap().is_sick());

    for _ in 0..5 {
        fund(&mut state);
        sim::tick(&mut state);
    }

    let s = state.survivors.iter().find(|s| s.id == sid).unwrap();
    assert!(!s.is_sick(), "sick_left should have reached 0 and cleared the flag");
    assert!(
        state.events.iter().any(|e| e.text == format!("{name} has recovered.")),
        "recovery should be logged as an event: {:?}",
        state.events.iter().map(|e| &e.text).collect::<Vec<_>>()
    );
}

// --- D: contagion after the outbreak window closes ---

#[test]
fn contagion_can_spread_from_an_already_sick_survivor_after_the_outbreak_ends() {
    // `CONTAGION_CHANCE_PER_SICK_PER_DAY` is small (0.04) so — mirroring the
    // "hunt across seeds" pattern above and in `event_tests.rs` — this
    // searches for the real path actually firing. `disease_until` is forced
    // to 0 every tick so the outbreak component of the roll never
    // contributes: any new infection here can only be patient zero's own
    // contagion, exercised well past `DISEASE_TICKS` would have expired one
    // naturally anyway.
    let mut found = false;
    'seeds: for seed in 0..80u64 {
        let mut state = sim::new_game_bootstrapped(seed, 30);
        fund(&mut state);
        state.disease_until = 0;
        let patient_zero = state.survivors[0].id;
        // Stays sick for the whole run, so it can keep spreading every day
        // (a staffed Hospital would clear it — there is none here).
        state.survivors.iter_mut().find(|s| s.id == patient_zero).unwrap().sick_left = SICKNESS_TICKS * 20.0;
        for _ in 0..(15 * TICKS_PER_DAY) {
            fund(&mut state);
            state.disease_until = 0; // the outbreak window stays closed throughout
            sim::tick(&mut state);
            if state.phase != GamePhase::Running {
                continue 'seeds;
            }
            if state.sick_count() > 1 {
                found = true;
                break 'seeds;
            }
        }
    }
    assert!(
        found,
        "contagion from an already-sick survivor should eventually infect someone else within 80 seeds x 15 days, \
         even with the outbreak window permanently closed"
    );
}
