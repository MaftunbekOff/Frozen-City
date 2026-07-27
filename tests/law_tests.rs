//! Pure-simulation tests for the V0.18 book of laws: enact/repeal command
//! validation (cooldown, `MAX_ACTIVE_LAWS`, `LAW_MIN_DAY`, central-world
//! refusal) and each law's effect on the tick simulation. Mirrors the style
//! of `tech_tests.rs` (control-vs-experiment comparisons between two
//! otherwise-identical worlds ticked in lockstep, so a seed-driven RNG path
//! never confounds the isolated effect under test) and `illness_tests.rs`
//! (aggregate-across-seeds for the genuinely probabilistic infection roll).

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
/// any harvestable tile, making a production comparison invisible over a
/// short test run (mirrors `tech_tests.rs`/`xp_tests.rs`'s helper of the same
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

fn place_and_staff(state: &mut GameState, kind: BuildingKind, x: u8, y: u8, workers: i8) -> u32 {
    sim::apply_command(state, 1, &PlayerCommand::Place { kind, x, y, facing: 0 });
    let id = state.buildings.last().unwrap().id;
    sim::finish_all_construction(state);
    let cur = state.find_building(id).unwrap().workers as i8;
    sim::apply_command(state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: workers - cur });
    id
}

/// Lands `state.tick` at the START of the given `time_of_day` fraction on
/// whatever day it's currently mid-way through, rounding forward to the NEXT
/// occurrence. Copied verbatim from `fatigue_tests.rs` (each test file keeps
/// its own small helpers, matching this repo's existing convention).
fn seek_time_of_day(state: &mut GameState, fraction: f32) {
    let day_start = (state.tick / TICKS_PER_DAY) * TICKS_PER_DAY;
    let target = day_start + (fraction * TICKS_PER_DAY as f32) as u64;
    state.tick = if target > state.tick { target } else { target + TICKS_PER_DAY };
}

/// Tops every consumable up so starvation/thirst/cold never confound a
/// comparison that isn't about them (mirrors `illness_tests.rs`'s helper of
/// the same name).
fn fund(state: &mut GameState) {
    state.stock.food = 9999.0;
    state.stock.water = 9999.0;
    state.stock.coal = 9999.0;
}

// --- A: the empty book (pre-V0.18 balance untouched) ---

#[test]
fn empty_law_book_leaves_every_multiplier_at_exactly_1_and_morale_at_0() {
    let state = sim::new_game(SEED, 12);
    assert!(state.laws.is_empty(), "sanity: a fresh world starts with no laws enacted");

    assert_eq!(state.law_production_multiplier(), 1.0);
    assert_eq!(state.law_food_multiplier(), 1.0);
    assert_eq!(state.law_fatigue_multiplier(), 1.0);
    assert_eq!(state.law_rest_multiplier(), 1.0);
    assert_eq!(state.law_contagion_multiplier(), 1.0);
    assert_eq!(state.law_xp_multiplier(), 1.0);
    assert_eq!(state.law_death_morale_multiplier(), 1.0);
    assert_eq!(state.law_morale_per_day(), 0.0);
    assert_eq!(state.law_funeral_wood(), 0.0);
}

// --- B: enact/repeal command validation ---

#[test]
fn enact_then_repeal_round_trips() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.tick = TICKS_PER_DAY; // day 2, past LAW_MIN_DAY

    sim::apply_command(&mut state, 1, &PlayerCommand::EnactLaw { law: Law::LongShifts });
    assert!(state.has_law(Law::LongShifts));
    assert_eq!(state.laws.len(), 1);

    state.tick += LAW_COOLDOWN_TICKS; // let the council finish deliberating
    sim::apply_command(&mut state, 1, &PlayerCommand::RepealLaw { law: Law::LongShifts });
    assert!(!state.has_law(Law::LongShifts));
    assert!(state.laws.is_empty(), "a repealed law must leave the book empty again");
}

#[test]
fn the_cooldown_blocks_any_further_book_edit_until_it_elapses() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.tick = TICKS_PER_DAY;

    sim::apply_command(&mut state, 1, &PlayerCommand::EnactLaw { law: Law::LongShifts });
    assert_eq!(state.laws.len(), 1);

    // Still on cooldown: even a DIFFERENT law is refused, and the one just
    // enacted can't be repealed either.
    assert_eq!(state.can_enact_law(Law::Curfew), Err("The council is still deliberating"));
    assert_eq!(state.can_repeal_law(Law::LongShifts), Err("The council is still deliberating"));
    sim::apply_command(&mut state, 1, &PlayerCommand::EnactLaw { law: Law::Curfew });
    assert_eq!(state.laws.len(), 1, "cooldown should block enacting a second law immediately after the first");

    state.tick += LAW_COOLDOWN_TICKS;
    sim::apply_command(&mut state, 1, &PlayerCommand::EnactLaw { law: Law::Curfew });
    assert_eq!(state.laws.len(), 2, "once the cooldown has elapsed a second law should enact fine");
}

#[test]
fn max_active_laws_cap_refuses_a_fourth() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.tick = TICKS_PER_DAY;
    // Set up the cap directly (bypassing the command layer, same convention
    // `tech_tests.rs` uses for `experiment.techs.push(..)`) so this isolates
    // the CAP itself, not the cooldown between enactments.
    state.laws = vec![Law::LongShifts, Law::Curfew, Law::CommunalMeals];
    assert_eq!(state.laws.len(), MAX_ACTIVE_LAWS);

    assert_eq!(state.can_enact_law(Law::Quarantine), Err("Too many laws already stand"));
    sim::apply_command(&mut state, 1, &PlayerCommand::EnactLaw { law: Law::Quarantine });

    assert_eq!(state.laws.len(), MAX_ACTIVE_LAWS, "a 4th law must be refused once the cap is reached");
    assert!(!state.has_law(Law::Quarantine));
}

#[test]
fn laws_are_refused_before_law_min_day_and_allowed_from_it_on() {
    let mut state = sim::new_game_bootstrapped(SEED, 12); // starts on day 1
    assert!(state.day() < LAW_MIN_DAY, "sanity: a fresh world starts before LAW_MIN_DAY");
    assert_eq!(state.can_enact_law(Law::LongShifts), Err("Too early to pass laws"));

    sim::apply_command(&mut state, 1, &PlayerCommand::EnactLaw { law: Law::LongShifts });
    assert!(state.laws.is_empty(), "too early: the law must not be enacted");

    // Lands exactly on day LAW_MIN_DAY.
    state.tick = TICKS_PER_DAY * (LAW_MIN_DAY as u64 - 1);
    assert_eq!(state.day(), LAW_MIN_DAY);
    sim::apply_command(&mut state, 1, &PlayerCommand::EnactLaw { law: Law::LongShifts });
    assert!(state.has_law(Law::LongShifts), "once LAW_MIN_DAY is reached the law should enact");
}

#[test]
fn double_enact_is_refused_and_does_not_duplicate() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.tick = TICKS_PER_DAY;
    sim::apply_command(&mut state, 1, &PlayerCommand::EnactLaw { law: Law::LongShifts });
    assert_eq!(state.laws.len(), 1);
    state.law_cooldown_until = state.tick; // clear the cooldown so only "already in force" is exercised

    assert_eq!(state.can_enact_law(Law::LongShifts), Err("Already in force"));
    sim::apply_command(&mut state, 1, &PlayerCommand::EnactLaw { law: Law::LongShifts });
    assert_eq!(state.laws.len(), 1, "enacting an already-enacted law must not duplicate it");
}

#[test]
fn repealing_a_law_not_in_force_is_a_noop() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.tick = TICKS_PER_DAY;

    assert_eq!(state.can_repeal_law(Law::LongShifts), Err("Not in force"));
    sim::apply_command(&mut state, 1, &PlayerCommand::RepealLaw { law: Law::LongShifts });
    assert!(state.laws.is_empty());
    assert_eq!(state.law_cooldown_until, 0, "a refused repeal must not spend the cooldown either");
}

#[test]
fn the_central_world_has_no_lawbook() {
    let mut state = sim::new_game_central(SEED);
    state.tick = TICKS_PER_DAY * 5; // well past LAW_MIN_DAY — irrelevant here, central refuses outright

    assert_eq!(state.can_enact_law(Law::LongShifts), Err("The Global World has no lawbook"));
    sim::apply_command(&mut state, 1, &PlayerCommand::EnactLaw { law: Law::LongShifts });
    assert!(state.laws.is_empty(), "the central world must never accept a law");
}

// --- C: each law's effect is observable in the sim ---

#[test]
fn long_shifts_raises_production() {
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);
    experiment.laws.push(Law::LongShifts);
    for state in [&mut control, &mut experiment] {
        state.stock.wood = 500.0;
    }

    let (cx, cy) = find_sawmill_spot_near_forest(&control);
    let control_id = place_and_staff(&mut control, BuildingKind::Sawmill, cx, cy, 2);
    let (ex, ey) = find_sawmill_spot_near_forest(&experiment); // same seed -> same spot
    let exp_id = place_and_staff(&mut experiment, BuildingKind::Sawmill, ex, ey, 2);
    assert_eq!(control.find_building(control_id).unwrap().workers, 2);
    assert_eq!(experiment.find_building(exp_id).unwrap().workers, 2);

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        experiment.stock.wood > control.stock.wood,
        "Long Shifts should raise production: control={}, experiment={}",
        control.stock.wood,
        experiment.stock.wood
    );
}

#[test]
fn long_shifts_raises_fatigue_accrual() {
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);
    experiment.laws.push(Law::LongShifts);
    for state in [&mut control, &mut experiment] {
        state.stock.wood = 500.0;
    }

    let (cx, cy) = find_spot(&control, BuildingKind::Sawmill);
    let sawmill_c = place_and_finish(&mut control, BuildingKind::Sawmill, cx, cy);
    let (ex, ey) = find_spot(&experiment, BuildingKind::Sawmill); // same seed -> same spot
    let sawmill_e = place_and_finish(&mut experiment, BuildingKind::Sawmill, ex, ey);
    // Clear the anonymous construction-crew headcount first — see
    // `apprenticeship_speeds_xp_accrual`'s comment on why `AssignSurvivor`
    // would otherwise silently refuse (capacity already full of anonymous
    // workers). Fatigue itself doesn't strictly need this (the law scales the
    // idle rate identically to the work rate), but this keeps the intent
    // below — an actual WORKING survivor's fatigue — true to what the
    // comment claims.
    for (state, id) in [(&mut control, sawmill_c), (&mut experiment, sawmill_e)] {
        let cur = state.find_building(id).unwrap().workers as i8;
        if cur > 0 {
            sim::apply_command(state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: -cur });
        }
    }
    let sid = control.survivors[0].id; // same seed -> same id in both worlds
    sim::apply_command(&mut control, 1, &PlayerCommand::AssignSurvivor { survivor: sid, building: Some(sawmill_c) });
    sim::apply_command(&mut experiment, 1, &PlayerCommand::AssignSurvivor { survivor: sid, building: Some(sawmill_e) });
    assert_eq!(control.survivors.iter().find(|s| s.id == sid).unwrap().assigned_building, Some(sawmill_c));
    assert_eq!(experiment.survivors.iter().find(|s| s.id == sid).unwrap().assigned_building, Some(sawmill_e));

    // Land inside the day window and reset fatigue right there, mirroring
    // `fatigue_tests.rs`'s accrual test — every tick below then accrues at
    // exactly the (law-scaled) AWAKE rate, with no night-recovery mixed in.
    seek_time_of_day(&mut control, 0.26);
    seek_time_of_day(&mut experiment, 0.26);
    for state in [&mut control, &mut experiment] {
        for s in &mut state.survivors {
            s.fatigue = 0.0;
        }
    }

    let ticks = TICKS_PER_DAY / 8;
    for _ in 0..ticks {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    let control_fatigue = control.survivors.iter().find(|s| s.id == sid).unwrap().fatigue;
    let experiment_fatigue = experiment.survivors.iter().find(|s| s.id == sid).unwrap().fatigue;
    assert!(
        experiment_fatigue > control_fatigue,
        "Long Shifts should raise fatigue accrual: control={control_fatigue}, experiment={experiment_fatigue}"
    );
}

#[test]
fn extra_rations_raises_food_consumption() {
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);
    experiment.laws.push(Law::ExtraRations);
    for state in [&mut control, &mut experiment] {
        state.stock.food = 1000.0;
        state.stock.water = 1000.0;
        state.stock.coal = 1000.0;
    }

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        experiment.stock.food < control.stock.food,
        "Extra Rations (1.22x food use) should leave less food than the control: control={}, experiment={}",
        control.stock.food,
        experiment.stock.food
    );
}

#[test]
fn extra_rations_raises_morale() {
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);
    experiment.laws.push(Law::ExtraRations);
    // Pin morale below the baseline so the law's push and passive drift both
    // point the same direction, mirroring `morale_tests.rs`'s
    // `staffed_kitchen_raises_morale_over_a_day`.
    control.morale = 40.0;
    experiment.morale = 40.0;
    for state in [&mut control, &mut experiment] {
        state.stock.food = 1000.0;
        state.stock.water = 1000.0;
        state.stock.coal = 1000.0;
    }

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        experiment.morale > control.morale,
        "Extra Rations should raise morale faster than drift alone: control={}, experiment={}",
        control.morale,
        experiment.morale
    );
}

#[test]
fn curfew_improves_overnight_rest() {
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);
    experiment.laws.push(Law::Curfew);
    for state in [&mut control, &mut experiment] {
        state.stock.wood = 500.0;
    }

    let (x, y) = find_spot(&control, BuildingKind::Tent);
    place_and_finish(&mut control, BuildingKind::Tent, x, y);
    let (x2, y2) = find_spot(&experiment, BuildingKind::Tent); // same seed -> same spot
    place_and_finish(&mut experiment, BuildingKind::Tent, x2, y2);

    let sid = control.survivors[0].id; // one of the first TENT_CAPACITY -> holds a bunk in both
    assert!(control.bunked_ids().contains(&sid));
    assert!(experiment.bunked_ids().contains(&sid));

    for state in [&mut control, &mut experiment] {
        for s in &mut state.survivors {
            s.fatigue = 90.0;
        }
    }
    seek_time_of_day(&mut control, 0.80); // solidly inside the night window
    seek_time_of_day(&mut experiment, 0.80);

    let ticks = TICKS_PER_DAY / 8; // stays well inside the same night
    for _ in 0..ticks {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    let control_fatigue = control.survivors.iter().find(|s| s.id == sid).unwrap().fatigue;
    let experiment_fatigue = experiment.survivors.iter().find(|s| s.id == sid).unwrap().fatigue;
    assert!(
        experiment_fatigue < control_fatigue,
        "Curfew should let a bunked survivor recover fatigue faster overnight: control={control_fatigue}, experiment={experiment_fatigue}"
    );
}

#[test]
fn quarantine_measurably_slows_infection() {
    // Aggregate across many seeds rather than asserting anything about a
    // single seed's exact rolls — mirrors `illness_tests.rs`'s "hunt across
    // seeds" style for genuinely probabilistic paths. Quarantine's 0.35x
    // contagion factor is a large, blunt effect (an active outbreak alone is
    // OUTBREAK_INFECT_CHANCE_PER_DAY = 0.30 per healthy survivor per day), so
    // summing total infections across a spread of seeds smooths out per-seed
    // noise reliably.
    let mut control_total = 0usize;
    let mut experiment_total = 0usize;
    for seed in 0..20u64 {
        let mut control = sim::new_game_bootstrapped(seed, 30);
        let mut experiment = sim::new_game_bootstrapped(seed, 30);
        experiment.laws.push(Law::Quarantine);
        for state in [&mut control, &mut experiment] {
            fund(state);
            state.disease_until = state.tick + TICKS_PER_DAY;
        }

        for _ in 0..(4 * TICKS_PER_DAY) {
            fund(&mut control);
            fund(&mut experiment);
            // Keep the outbreak window open throughout (same trick
            // `illness_tests.rs` uses) so the comparison isolates the law's
            // contagion multiplier rather than when the window happens to
            // close.
            control.disease_until = control.tick + TICKS_PER_DAY;
            experiment.disease_until = experiment.tick + TICKS_PER_DAY;
            sim::tick(&mut control);
            sim::tick(&mut experiment);
        }
        control_total += control.sick_count();
        experiment_total += experiment.sick_count();
    }

    assert!(
        experiment_total < control_total,
        "Quarantine (0.35x contagion) should leave measurably fewer sick, summed across seeds: control={control_total}, experiment={experiment_total}"
    );
}

#[test]
fn apprenticeship_speeds_xp_accrual() {
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);
    experiment.laws.push(Law::Apprenticeship);
    for state in [&mut control, &mut experiment] {
        state.stock.wood = 500.0;
    }

    let (x, y) = find_spot(&control, BuildingKind::Sawmill);
    let sawmill_c = place_and_finish(&mut control, BuildingKind::Sawmill, x, y);
    let (x2, y2) = find_spot(&experiment, BuildingKind::Sawmill); // same seed -> same spot
    let sawmill_e = place_and_finish(&mut experiment, BuildingKind::Sawmill, x2, y2);
    // `Place` auto-fills a construction crew from idle survivors purely as a
    // building-side HEADCOUNT (`Building.workers`) — it never sets any
    // specific survivor's `assigned_building`, so after `finish_all_construction`
    // this Sawmill can already read as fully staffed (workers == max_workers)
    // by entirely ANONYMOUS workers. `AssignSurvivor` refuses once
    // `workers >= capacity`, so without clearing that anonymous headcount
    // first, our NAMED survivor below could never actually get in — and only
    // `AssignSurvivor`'s `Some(new_id)` arm ever sets `trained_kind`, which
    // XP accrual requires. Same pattern as `xp_tests.rs`'s own `place()`
    // helper (and unlike `place_and_staff`, which just requests a total
    // headcount and doesn't care whether it's named).
    for (state, id) in [(&mut control, sawmill_c), (&mut experiment, sawmill_e)] {
        let cur = state.find_building(id).unwrap().workers as i8;
        if cur > 0 {
            sim::apply_command(state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: -cur });
        }
    }

    let sid = control.survivors[0].id; // same seed -> same id in both worlds
    sim::apply_command(&mut control, 1, &PlayerCommand::AssignSurvivor { survivor: sid, building: Some(sawmill_c) });
    sim::apply_command(&mut experiment, 1, &PlayerCommand::AssignSurvivor { survivor: sid, building: Some(sawmill_e) });
    assert_eq!(control.survivors.iter().find(|s| s.id == sid).unwrap().trained_kind, Some(BuildingKind::Sawmill));
    assert_eq!(experiment.survivors.iter().find(|s| s.id == sid).unwrap().trained_kind, Some(BuildingKind::Sawmill));

    for _ in 0..(TICKS_PER_DAY / 2) {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    let control_xp = control.survivors.iter().find(|s| s.id == sid).unwrap().xp;
    let experiment_xp = experiment.survivors.iter().find(|s| s.id == sid).unwrap().xp;
    assert!(
        experiment_xp > control_xp,
        "Apprenticeship should speed XP accrual: control={control_xp}, experiment={experiment_xp}"
    );
}

#[test]
fn funeral_rites_halves_the_death_morale_hit_and_charges_wood() {
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);
    experiment.laws.push(Law::FuneralRites);
    for state in [&mut control, &mut experiment] {
        state.stock.wood = 100.0;
    }
    let control_before_morale = control.morale;
    let experiment_before_morale = experiment.morale;
    let experiment_before_wood = experiment.stock.wood;

    control.survivors[0].hp = 0.0;
    experiment.survivors[0].hp = 0.0;
    sim::tick(&mut control);
    sim::tick(&mut experiment);

    let control_drop = control_before_morale - control.morale;
    let experiment_drop = experiment_before_morale - experiment.morale;
    assert!(
        experiment_drop < control_drop,
        "Funeral Rites should soften the death-morale hit: control_drop={control_drop}, experiment_drop={experiment_drop}"
    );
    assert!(
        (experiment_drop - control_drop / 2.0).abs() < 0.5,
        "Funeral Rites halves the hit exactly (0.5x death_morale_factor): control_drop={control_drop}, experiment_drop={experiment_drop}"
    );

    assert_eq!(control.stock.wood, 100.0, "no Funeral Rites law -> no wood spent on a death");
    let experiment_wood_spent = experiment_before_wood - experiment.stock.wood;
    let expected_wood = Law::FuneralRites.funeral_wood();
    assert!(
        (experiment_wood_spent - expected_wood).abs() < 0.01,
        "Funeral Rites should charge exactly funeral_wood() per death: expected {expected_wood}, got {experiment_wood_spent}"
    );
}

// --- D: composition ---

#[test]
fn two_laws_compose_multiplicatively_not_additively() {
    let mut state = sim::new_game(SEED, 12);
    state.laws = vec![Law::LongShifts, Law::Curfew];

    let multiplicative = Law::LongShifts.production_factor() * Law::Curfew.production_factor();
    assert!(
        (state.law_production_multiplier() - multiplicative).abs() < 1e-6,
        "two laws should compose by MULTIPLYING their factors: expected {multiplicative}, got {}",
        state.law_production_multiplier()
    );

    // Guard against a naive additive composition ((a-1)+(b-1)+1) landing on
    // the same number by coincidence.
    let additive = Law::LongShifts.production_factor() + Law::Curfew.production_factor() - 1.0;
    assert!(
        (state.law_production_multiplier() - additive).abs() > 1e-4,
        "sanity: multiplicative and additive composition must differ for these two factors (else this test can't tell them apart)"
    );
}
