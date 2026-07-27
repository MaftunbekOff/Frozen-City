//! Pure-simulation tests for V0.18 expeditions: launch mechanics (roster
//! removal, provisions, slot-freeing), `can_launch_expedition` refusals, the
//! return (full trip vs recall, the Ruined Town rescue and its population
//! cap), the "away is not dead" defeat-check interaction, and determinism
//! (including the `expedition_rng` stream isolation the module doc promises).
//! Mirrors the style of `fatigue_tests.rs`/`illness_tests.rs`: same imports,
//! same `fund`/`find_spot`/`place_and_finish` helpers, control-vs-experiment
//! comparisons, and "hunt across trials" for genuinely probabilistic paths.

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

fn fund(state: &mut GameState) {
    state.stock.food = 9999.0;
    state.stock.wood = 9999.0;
    state.stock.coal = 9999.0;
    state.stock.water = 9999.0;
}

/// Lands `state.tick` at the very start of in-game day `day` (`GameState::day`
/// is 1-based) — the same "assign `tick` directly" convention other sim tests
/// use (see `central_tests.rs`).
fn set_day(state: &mut GameState, day: u32) {
    state.tick = (day as u64 - 1) * TICKS_PER_DAY;
}

/// Minimal hand-built settler for tests that need one without going through
/// `new_game`'s RNG (the central-world tests can't reach `new_survivor`, a
/// crate-private helper) — mirrors the literal `central_tests.rs` uses for
/// the same reason.
fn minimal_survivor(id: u32, name: &str) -> Survivor {
    Survivor {
        id,
        name: name.to_string(),
        hp: 100.0,
        hunger: 0.0,
        assigned_building: None,
        owner: None,
        x: 0.0,
        y: 0.0,
        move_target: None,
        profession: Profession::from_id_hash(id),
        xp: 0.0,
        trained_kind: None,
        chop_target: None,
        carrying_wood: false,
        thirst: 0.0,
        bury_target: None,
        fatigue: 0.0,
        sick_left: 0.0,
        age_days: 30.0,
        partner: None,
    }
}

// --- A: launch mechanics ---

#[test]
fn launching_removes_the_party_frees_slots_charges_provisions_and_counts_as_away() {
    let mut state = sim::new_game_bootstrapped(SEED, 30);
    fund(&mut state);
    set_day(&mut state, EXPEDITION_MIN_DAY);

    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    let sawmill = place_and_finish(&mut state, BuildingKind::Sawmill, x, y);
    // `Place` auto-staffs a construction crew from idle workers (capped at
    // `max_workers()` once finished) -- clear it first so the named
    // assignment below starts from a known empty building, mirroring
    // `illness_tests.rs`'s `place_and_staff` helper.
    let crew = state.find_building(sawmill).unwrap().workers as i8;
    sim::apply_command(&mut state, 1, &PlayerCommand::AdjustWorkers { building: sawmill, delta: -crew });
    assert_eq!(state.find_building(sawmill).unwrap().workers, 0, "sanity: cleared the auto-assigned construction crew");

    let a = state.survivors[0].id;
    let b = state.survivors[1].id;
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor: a, building: Some(sawmill) });
    assert_eq!(state.find_building(sawmill).unwrap().workers, 1, "sanity: assigned before launch");

    let before_pop = state.survivors.len();
    let food_before = state.stock.food;
    let site = ExpeditionSite::FrozenWoods;
    let provisions = site.provisions_per_member_day() * site.days() * 2.0;

    sim::apply_command(&mut state, 1, &PlayerCommand::LaunchExpedition { site, members: vec![a, b] });

    assert_eq!(state.survivors.len(), before_pop - 2, "both party members should leave the roster");
    assert!(
        state.survivors.iter().all(|s| s.id != a && s.id != b),
        "party members must not still be present in survivors"
    );
    assert_eq!(state.find_building(sawmill).unwrap().workers, 0, "the freed slot must be given back");
    assert!(
        (state.stock.food - (food_before - provisions)).abs() < 0.01,
        "provisions should be charged up front for the whole planned trip: expected {}, food went {} -> {}",
        provisions,
        food_before,
        state.stock.food
    );
    assert_eq!(state.people_away(), 2);
    assert_eq!(state.expeditions.len(), 1);
    assert_eq!(state.expeditions[0].party.len(), 2);
}

#[test]
fn can_launch_expedition_refuses_undersized_or_oversized_parties() {
    let mut state = sim::new_game_bootstrapped(SEED, 30);
    set_day(&mut state, EXPEDITION_MIN_DAY);
    let ids: Vec<u32> = state.survivors.iter().map(|s| s.id).collect();

    assert!(
        state.can_launch_expedition(ExpeditionSite::FrozenWoods, &ids[..1]).is_err(),
        "one person alone is not a party"
    );
    assert!(
        state
            .can_launch_expedition(ExpeditionSite::FrozenWoods, &ids[..(EXPEDITION_MAX_PARTY + 1)])
            .is_err(),
        "over the party cap should be refused"
    );
    assert!(
        state.can_launch_expedition(ExpeditionSite::FrozenWoods, &ids[..2]).is_ok(),
        "sanity: an ordinary 2-person party from a fresh bootstrapped colony should be fine"
    );
}

#[test]
fn can_launch_expedition_refuses_children_and_the_sick() {
    let mut state = sim::new_game_bootstrapped(SEED, 30);
    set_day(&mut state, EXPEDITION_MIN_DAY);
    let child = state.survivors[0].id;
    state.survivors[0].age_days = 0.0;
    let sick = state.survivors[1].id;
    state.survivors[1].sick_left = SICKNESS_TICKS;
    let healthy_adult = state.survivors[2].id;

    assert!(
        state.can_launch_expedition(ExpeditionSite::FrozenWoods, &[child, healthy_adult]).is_err(),
        "a child must not travel"
    );
    assert!(
        state.can_launch_expedition(ExpeditionSite::FrozenWoods, &[sick, healthy_adult]).is_err(),
        "the sick must not travel"
    );
}

#[test]
fn can_launch_expedition_refuses_a_duplicate_id() {
    let mut state = sim::new_game_bootstrapped(SEED, 30);
    set_day(&mut state, EXPEDITION_MIN_DAY);
    let a = state.survivors[0].id;
    assert!(
        state.can_launch_expedition(ExpeditionSite::FrozenWoods, &[a, a]).is_err(),
        "the same survivor listed twice is not a two-person party"
    );
}

#[test]
fn can_launch_expedition_refuses_to_empty_the_colony() {
    let mut state = sim::new_game_bootstrapped(SEED, 30);
    set_day(&mut state, EXPEDITION_MIN_DAY);
    // One survivor short of what a legal party would need to leave behind.
    state.survivors.truncate(EXPEDITION_MIN_PARTY + EXPEDITION_MIN_STAY_HOME - 1);
    let ids: Vec<u32> = state.survivors.iter().take(EXPEDITION_MIN_PARTY).map(|s| s.id).collect();

    assert!(
        state.can_launch_expedition(ExpeditionSite::FrozenWoods, &ids).is_err(),
        "sending out a party that would leave fewer than EXPEDITION_MIN_STAY_HOME behind must be refused"
    );
}

#[test]
fn can_launch_expedition_refuses_before_the_minimum_day() {
    let state = sim::new_game_bootstrapped(SEED, 30);
    assert!(state.day() < EXPEDITION_MIN_DAY, "sanity: a fresh bootstrapped colony is still early");
    let ids: Vec<u32> = state.survivors.iter().take(2).map(|s| s.id).collect();

    assert!(
        state.can_launch_expedition(ExpeditionSite::FrozenWoods, &ids).is_err(),
        "the colony should not be able to send anyone out before EXPEDITION_MIN_DAY"
    );
}

#[test]
fn can_launch_expedition_refuses_a_second_active_party() {
    let mut state = sim::new_game_bootstrapped(SEED, 30);
    fund(&mut state);
    set_day(&mut state, EXPEDITION_MIN_DAY);
    let a = state.survivors[0].id;
    let b = state.survivors[1].id;
    sim::apply_command(&mut state, 1, &PlayerCommand::LaunchExpedition { site: ExpeditionSite::FrozenWoods, members: vec![a, b] });
    assert_eq!(state.expeditions.len(), 1, "sanity: the first party is away");

    let c = state.survivors[0].id;
    let d = state.survivors[1].id;
    assert!(
        state.can_launch_expedition(ExpeditionSite::AbandonedMine, &[c, d]).is_err(),
        "only one party may be on the road at a time"
    );
}

#[test]
fn can_launch_expedition_refuses_in_the_central_world() {
    let mut state = sim::new_game_central(SEED);
    let migrants = vec![minimal_survivor(1, "Anna"), minimal_survivor(2, "Bek")];
    sim::inject_migrants(&mut state, 42, "Aziz", migrants);
    let ids: Vec<u32> = state.survivors.iter().map(|s| s.id).collect();
    assert_eq!(ids.len(), 2, "sanity: both migrants settled");

    assert!(
        state.can_launch_expedition(ExpeditionSite::FrozenWoods, &ids).is_err(),
        "no expeditions leave the Global World"
    );
}

// --- B: the return (full trip, recall, rescue) ---

#[test]
fn a_full_trip_returns_everyone_and_credits_a_haul() {
    let mut state = sim::new_game_bootstrapped(SEED, 30);
    fund(&mut state);
    set_day(&mut state, EXPEDITION_MIN_DAY);
    let before_ids: std::collections::BTreeSet<u32> = state.survivors.iter().map(|s| s.id).collect();
    let a = state.survivors[0].id;
    let b = state.survivors[1].id;
    let site = ExpeditionSite::AbandonedMine; // no rescue chance -- keeps roster membership predictable

    sim::apply_command(&mut state, 1, &PlayerCommand::LaunchExpedition { site, members: vec![a, b] });
    let id = state.expeditions[0].id;
    let wood_before = state.stock.wood;
    let coal_before = state.stock.coal;

    // Jump straight to the return tick and resolve directly, the same
    // "skip the wait" convenience `finish_all_construction` gives construction
    // tests, instead of spinning through hundreds of real ticks.
    state.tick = state.expeditions[0].return_tick;
    sim::expedition::resolve_return(&mut state, id);

    assert!(state.expeditions.is_empty());
    let after_ids: std::collections::BTreeSet<u32> = state.survivors.iter().map(|s| s.id).collect();
    assert_eq!(after_ids, before_ids, "the whole roster should be back, unchanged in membership");
    assert!(state.stock.wood > wood_before, "a full trip should credit wood");
    assert!(state.stock.coal > coal_before, "a full trip should credit coal");
    assert_eq!(state.people_away(), 0);
}

#[test]
fn a_recalled_party_earns_less_haul_than_completing_the_full_trip() {
    let mut control = sim::new_game_bootstrapped(SEED, 30);
    let mut experiment = sim::new_game_bootstrapped(SEED, 30);
    for state in [&mut control, &mut experiment] {
        fund(state);
        set_day(state, EXPEDITION_MIN_DAY);
    }
    let site = ExpeditionSite::AbandonedMine;
    let a = control.survivors[0].id; // same seed -> same ids in both worlds
    let b = control.survivors[1].id;
    for state in [&mut control, &mut experiment] {
        sim::apply_command(state, 1, &PlayerCommand::LaunchExpedition { site, members: vec![a, b] });
    }

    let control_id = control.expeditions[0].id;
    control.tick = control.expeditions[0].return_tick;
    let control_wood_before = control.stock.wood;
    sim::expedition::resolve_return(&mut control, control_id);
    let control_haul_wood = control.stock.wood - control_wood_before;

    let experiment_id = experiment.expeditions[0].id;
    let departed = experiment.expeditions[0].departed_tick;
    // Recalled a quarter of the way out; `recall`'s "home is as far as
    // they've come" then walks them back over the same distance.
    experiment.tick = departed + site.trip_ticks() / 4;
    sim::apply_command(&mut experiment, 1, &PlayerCommand::RecallExpedition { expedition: experiment_id });
    let return_tick = experiment.expeditions[0].return_tick;
    assert!(return_tick < departed + site.trip_ticks(), "sanity: a recall should shorten the trip");
    experiment.tick = return_tick;
    let experiment_wood_before = experiment.stock.wood;
    sim::expedition::resolve_return(&mut experiment, experiment_id);
    let experiment_haul_wood = experiment.stock.wood - experiment_wood_before;

    assert!(
        experiment_haul_wood < control_haul_wood,
        "a recalled party should earn less than one that finished the trip: recalled={experiment_haul_wood}, full={control_haul_wood}"
    );
    assert!(experiment_haul_wood > 0.0, "sanity: a recalled party still earns something for the days they did travel");
}

#[test]
fn resolve_return_rescues_a_survivor_from_the_ruined_town_when_there_is_room() {
    // `rescue_chance` is genuinely probabilistic -- hunt across a spread of
    // `expedition_rng` seed values for it actually firing, mirroring the
    // "hunt across seeds" pattern in `illness_tests.rs`/`event_tests.rs`.
    let mut found = false;
    for trial in 0..200u64 {
        let mut state = sim::new_game_bootstrapped(SEED, 30);
        fund(&mut state);
        set_day(&mut state, EXPEDITION_MIN_DAY);
        let a = state.survivors[0].id;
        let b = state.survivors[1].id;
        sim::apply_command(&mut state, 1, &PlayerCommand::LaunchExpedition { site: ExpeditionSite::RuinedTown, members: vec![a, b] });
        let id = state.expeditions[0].id;
        state.tick = state.expeditions[0].return_tick;
        state.expedition_rng = trial;
        let pop_before = state.survivors.len();
        sim::expedition::resolve_return(&mut state, id);
        if state.survivors.len() > pop_before + 2 {
            found = true;
            break;
        }
    }
    assert!(found, "a Ruined Town return should eventually rescue someone within 200 trial seeds");
}

#[test]
fn resolve_return_does_not_rescue_beyond_the_population_cap() {
    let site = ExpeditionSite::RuinedTown;
    // Find a seed that actually triggers a rescue when there's room (same
    // hunt as the test above), so the cap check below is exercised for real
    // rather than trivially passing because no rescue was ever rolled.
    let mut trial_seed = None;
    for trial in 0..200u64 {
        let mut probe = sim::new_game_bootstrapped(SEED, 30);
        fund(&mut probe);
        set_day(&mut probe, EXPEDITION_MIN_DAY);
        let a = probe.survivors[0].id;
        let b = probe.survivors[1].id;
        sim::apply_command(&mut probe, 1, &PlayerCommand::LaunchExpedition { site, members: vec![a, b] });
        let id = probe.expeditions[0].id;
        probe.tick = probe.expeditions[0].return_tick;
        probe.expedition_rng = trial;
        let pop_before = probe.survivors.len();
        sim::expedition::resolve_return(&mut probe, id);
        if probe.survivors.len() > pop_before + 2 {
            trial_seed = Some(trial);
            break;
        }
    }
    let trial = trial_seed.expect("a rescue-triggering seed should exist within 200 trials");

    // Identical setup and the identical `expedition_rng` seed (so the SAME
    // hazard/rescue rolls fire), except the colony is already full once the
    // party walks back in -- no room for a rescued extra.
    let mut state = sim::new_game_bootstrapped(SEED, 30);
    fund(&mut state);
    set_day(&mut state, EXPEDITION_MIN_DAY);
    let a = state.survivors[0].id;
    let b = state.survivors[1].id;
    sim::apply_command(&mut state, 1, &PlayerCommand::LaunchExpedition { site, members: vec![a, b] });
    let id = state.expeditions[0].id;
    let template = state.survivors[0].clone();
    while state.survivors.len() < (MAX_POPULATION as usize - 2) {
        let mut s = template.clone();
        s.id = state.next_id;
        state.next_id += 1;
        state.survivors.push(s);
    }
    assert_eq!(state.survivors.len() + 2, MAX_POPULATION as usize, "sanity: exactly full once the party returns");
    state.tick = state.expeditions[0].return_tick;
    state.expedition_rng = trial;
    let pop_before = state.survivors.len();

    sim::expedition::resolve_return(&mut state, id);

    assert_eq!(
        state.survivors.len(),
        pop_before + 2,
        "no rescued extra should fit once the colony is already at MAX_POPULATION"
    );
}

#[test]
fn resolve_return_never_draws_from_the_main_rng_even_when_it_rescues_someone() {
    // Regression guard: the rescue used to spend a `state.rng` draw to build
    // the new survivor instead of the local (expedition_rng-backed) `rng`,
    // quietly breaking the module's "own stream" guarantee on every trip
    // that actually rescued someone. Hunt for a trial that fires the rescue
    // (same technique as the tests above), then check `rng` didn't move.
    for trial in 0..200u64 {
        let mut state = sim::new_game_bootstrapped(SEED, 30);
        fund(&mut state);
        set_day(&mut state, EXPEDITION_MIN_DAY);
        let a = state.survivors[0].id;
        let b = state.survivors[1].id;
        sim::apply_command(&mut state, 1, &PlayerCommand::LaunchExpedition { site: ExpeditionSite::RuinedTown, members: vec![a, b] });
        let id = state.expeditions[0].id;
        state.tick = state.expeditions[0].return_tick;
        state.expedition_rng = trial;
        let rng_before = state.rng;
        let pop_before = state.survivors.len();

        sim::expedition::resolve_return(&mut state, id);

        if state.survivors.len() > pop_before + 2 {
            assert_eq!(
                state.rng, rng_before,
                "a rescue must draw the new survivor from expedition_rng, never the main rng stream"
            );
            return;
        }
    }
    panic!("no rescue-triggering seed found within 200 trials to exercise this guard");
}

#[test]
fn launching_the_leader_clears_the_leader_seat_without_restoring_it_on_return() {
    let mut state = sim::new_game_bootstrapped(SEED, 30);
    fund(&mut state);
    set_day(&mut state, EXPEDITION_MIN_DAY);
    let leader = state.survivors[0].id;
    let other = state.survivors[1].id;
    sim::apply_command(&mut state, 1, &PlayerCommand::SetLeader { survivor: leader });
    assert_eq!(state.leader, Some(leader), "sanity: leadership was set");

    sim::apply_command(&mut state, 1, &PlayerCommand::LaunchExpedition { site: ExpeditionSite::FrozenWoods, members: vec![leader, other] });
    assert_eq!(state.leader, None, "the leader's seat empties when they leave with the party");

    let id = state.expeditions[0].id;
    state.tick = state.expeditions[0].return_tick;
    sim::expedition::resolve_return(&mut state, id);

    assert!(state.survivors.iter().any(|s| s.id == leader), "the former leader should be back among the living");
    assert_eq!(state.leader, None, "coming home does not automatically restore the old leader's seat");
}

#[test]
fn recalling_on_the_same_tick_as_launch_still_resolves_cleanly() {
    let mut state = sim::new_game_bootstrapped(SEED, 30);
    fund(&mut state);
    set_day(&mut state, EXPEDITION_MIN_DAY);
    let a = state.survivors[0].id;
    let b = state.survivors[1].id;
    let before_pop = state.survivors.len();

    sim::apply_command(&mut state, 1, &PlayerCommand::LaunchExpedition { site: ExpeditionSite::AbandonedMine, members: vec![a, b] });
    let id = state.expeditions[0].id;
    // No tick passed between launch and recall -- home is as far away as
    // they've come, i.e. no distance at all.
    sim::apply_command(&mut state, 1, &PlayerCommand::RecallExpedition { expedition: id });
    assert_eq!(
        state.expeditions[0].return_tick, state.expeditions[0].departed_tick,
        "zero distance out should mean an immediate return"
    );

    sim::tick(&mut state);

    assert!(state.expeditions.is_empty(), "an expedition recalled on the tick it left should resolve on the very next tick");
    assert_eq!(state.survivors.len(), before_pop, "everyone should be back, no one lost");
    assert!(state.survivors.iter().any(|s| s.id == a) && state.survivors.iter().any(|s| s.id == b));
}

// --- C: defeat-check interaction ---

#[test]
fn the_colony_is_not_lost_while_the_last_survivors_are_away_and_returning() {
    let mut state = sim::new_game_bootstrapped(SEED, 30);
    fund(&mut state);
    set_day(&mut state, EXPEDITION_MIN_DAY);
    let a = state.survivors[0].id;
    let b = state.survivors[1].id;
    sim::apply_command(&mut state, 1, &PlayerCommand::LaunchExpedition { site: ExpeditionSite::FrozenWoods, members: vec![a, b] });
    assert_eq!(state.expeditions.len(), 1);

    // Everyone who stayed behind is gone -- the only survivors left alive are
    // the two on the road.
    state.survivors.clear();
    sim::tick(&mut state);
    assert_eq!(
        state.phase,
        GamePhase::Running,
        "a party still out there and coming home must not read as defeat"
    );

    let return_tick = state.expeditions[0].return_tick;
    while state.tick < return_tick {
        fund(&mut state);
        sim::tick(&mut state);
    }
    fund(&mut state);
    sim::tick(&mut state); // resolves the now-due expedition

    assert!(state.expeditions.is_empty(), "the party should be home");
    assert!(state.survivors.len() >= 2, "the returning party should be back among the living");
    assert_eq!(state.phase, GamePhase::Running, "the colony is alive again, not lost");
}

// --- D: determinism ---

#[test]
fn same_seed_and_commands_yield_identical_outcomes() {
    let mut a = sim::new_game_bootstrapped(SEED, 30);
    let mut b = sim::new_game_bootstrapped(SEED, 30);
    for state in [&mut a, &mut b] {
        fund(state);
        set_day(state, EXPEDITION_MIN_DAY);
    }
    let site = ExpeditionSite::AbandonedMine;
    let m1 = a.survivors[0].id;
    let m2 = a.survivors[1].id;
    for state in [&mut a, &mut b] {
        sim::apply_command(state, 1, &PlayerCommand::LaunchExpedition { site, members: vec![m1, m2] });
    }

    let return_tick = a.expeditions[0].return_tick;
    while a.tick < return_tick {
        fund(&mut a);
        fund(&mut b);
        sim::tick(&mut a);
        sim::tick(&mut b);
    }
    for _ in 0..5 {
        fund(&mut a);
        fund(&mut b);
        sim::tick(&mut a);
        sim::tick(&mut b);
    }

    assert_eq!(a, b, "identical seeds, setup and commands must yield byte-identical outcomes");
}

/// The core "own stream" guarantee the module doc makes: launching AND
/// completing an expedition must never re-sequence `rng`/`event_rng` —
/// compared against a control world that never sends anyone out (mirrors
/// `illness_tests.rs`'s `illness_rolls_draw_from_their_own_stream...`).
/// Uses a site with zero rescue chance so the comparison stays focused on
/// stream isolation itself rather than the rescue path (covered separately
/// above).
#[test]
fn launching_and_completing_an_expedition_never_perturbs_rng_or_event_rng() {
    let mut control = sim::new_game_bootstrapped(SEED, 30);
    let mut experiment = sim::new_game_bootstrapped(SEED, 30);
    for state in [&mut control, &mut experiment] {
        fund(state);
        set_day(state, EXPEDITION_MIN_DAY);
    }
    let site = ExpeditionSite::AbandonedMine;
    let a = control.survivors[0].id;
    let b = control.survivors[1].id;

    sim::apply_command(&mut experiment, 1, &PlayerCommand::LaunchExpedition { site, members: vec![a, b] });
    assert_eq!(control.rng, experiment.rng, "sanity: launching itself must not draw from rng");
    assert_eq!(control.event_rng, experiment.event_rng, "sanity: launching itself must not draw from event_rng");

    let return_tick = experiment.expeditions[0].return_tick;
    while experiment.tick < return_tick {
        fund(&mut control);
        fund(&mut experiment);
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }
    fund(&mut control);
    fund(&mut experiment);
    sim::tick(&mut control);
    sim::tick(&mut experiment); // resolves the now-due expedition

    assert!(experiment.expeditions.is_empty(), "sanity: the party should be home by now");
    assert_eq!(
        control.rng, experiment.rng,
        "a party being away and coming home must never perturb the main rng stream"
    );
    assert_eq!(
        control.event_rng, experiment.event_rng,
        "a party being away and coming home must never perturb the event_rng stream"
    );
    assert_ne!(
        control.expedition_rng, experiment.expedition_rng,
        "the expedition's own stream should have advanced from its hazard/haul rolls, unlike the control's"
    );
}
