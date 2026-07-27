//! Pure-simulation tests for V0.18 repeatable mission cycles
//! (`sim::missions`): a cleared cycle issuing a fresh, harder one, growth of
//! targets/rewards, the `SurviveDays` fresh-horizon guarantee, the Tunnel
//! unlock latch surviving replacement mission lists, the `MAX_MISSION_CYCLES`
//! ceiling, and the central world's total abstention. Mirrors the style of
//! `mission_tests.rs`/`tunnel_tests.rs` (direct field manipulation to set up
//! a scenario, then a single `sim::tick` to exercise the real hook).

use frozen_city::game::sim;
use frozen_city::game::types::*;

const SEED: u64 = 12345;

/// Tops every consumable up so starvation/thirst/cold never confound a test
/// that isn't about them (mirrors `illness_tests.rs`/`law_tests.rs`).
fn fund(state: &mut GameState) {
    state.stock.food = 9999.0;
    state.stock.water = 9999.0;
    state.stock.coal = 9999.0;
}

#[test]
fn a_cleared_first_cycle_issues_a_new_harder_one() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    fund(&mut state);
    state.tunnel.unlocked = true;
    for m in &mut state.missions {
        m.done = true;
    }
    let original = state.missions.clone();
    assert_eq!(state.mission_cycle, 0, "sanity: a fresh world hasn't issued any extra cycle yet");

    sim::tick(&mut state);

    assert_eq!(state.mission_cycle, 1, "clearing the opening cycle should issue cycle 1");
    assert_ne!(state.missions, original, "the new cycle's missions should differ from the cleared ones");
    assert!(
        state.missions.iter().all(|m| !m.done),
        "a freshly issued cycle should start with nothing done yet"
    );
}

#[test]
fn targets_and_rewards_grow_each_cycle() {
    let day_now = 1;
    let cycles: Vec<Vec<Mission>> = (0..3).map(|c| sim::missions::cycle_missions(c, day_now)).collect();
    assert_eq!(cycles[0].len(), cycles[1].len());
    assert_eq!(cycles[0].len(), cycles[2].len());

    for i in 0..cycles[0].len() {
        for c in 1..cycles.len() {
            let prev = &cycles[c - 1][i];
            let cur = &cycles[c][i];
            assert!(
                cur.kind.target() > prev.kind.target(),
                "cycle {c}'s mission {i} target should exceed cycle {}'s: prev={}, cur={}",
                c - 1,
                prev.kind.target(),
                cur.kind.target()
            );
            // Exactly one reward field is nonzero per mission kind (see
            // `cycle_missions`); summing sidesteps having to know which one.
            let prev_reward = prev.reward_wood + prev.reward_coal + prev.reward_food;
            let cur_reward = cur.reward_wood + cur.reward_coal + cur.reward_food;
            assert!(
                cur_reward > prev_reward,
                "cycle {c}'s mission {i} reward should exceed cycle {}'s: prev={prev_reward}, cur={cur_reward}",
                c - 1
            );
        }
    }
}

/// Regression: `scaled`'s compounding growth (1.6^cycle) alone would put
/// cycle 4's `Population` target at 66 (10 * 1.6^4, rounded) against
/// `MAX_POPULATION` (60) — a hard, unconditionally enforced ceiling (every
/// arrival/birth/migrant/expedition-return path gates on it in `tick.rs`/
/// `players.rs`/`command.rs`). A mission asking for more than that could
/// NEVER complete, which would also freeze every cycle after it (a cycle
/// only advances once the current one fully clears). `cycle_missions` must
/// clamp the target to the population ceiling instead.
#[test]
fn population_mission_target_never_exceeds_the_hard_population_cap() {
    for cycle in 0..=(MAX_MISSION_CYCLES + 2) {
        let missions = sim::missions::cycle_missions(cycle, 1);
        let population = missions
            .iter()
            .find_map(|m| match m.kind {
                MissionKind::Population(n) => Some(n),
                _ => None,
            })
            .expect("cycle_missions should always include a Population goal");
        assert!(
            population <= MAX_POPULATION as u32,
            "cycle {cycle}: Population target {population} exceeds MAX_POPULATION ({MAX_POPULATION}) \
             and could never be reached"
        );
    }
    // And the clamp must actually bind at the cycle the regression hit, not
    // just happen to stay under the ceiling by coincidence.
    let cycle4 = sim::missions::cycle_missions(4, 1);
    let cycle4_population = cycle4
        .iter()
        .find_map(|m| match m.kind {
            MissionKind::Population(n) => Some(n),
            _ => None,
        })
        .unwrap();
    assert_eq!(cycle4_population, MAX_POPULATION as u32, "cycle 4 should clamp exactly to the population ceiling");
}

/// Companion to the population-specific check above: every OTHER kind
/// `cycle_missions` issues must also stay reachable in principle at every
/// cycle. `BuildTents`/`Sawmills` (buildable space + wood) and
/// `StockpileCoal` (nothing in the sim ever caps `Stockpile.coal`) have no
/// hard ceiling to hit, so this only asserts they keep producing a sane,
/// nonzero, growing target rather than e.g. overflowing — the actual
/// reachability argument for those three is documented in `cycle_missions`.
#[test]
fn every_mission_kind_produces_a_sane_target_up_to_the_cap() {
    for cycle in 0..=MAX_MISSION_CYCLES {
        for m in sim::missions::cycle_missions(cycle, 1) {
            assert!(m.kind.target() > 0, "cycle {cycle}: {:?} has a zero target", m.kind);
        }
    }
}

#[test]
fn survive_days_target_is_always_a_fresh_horizon_ahead_of_the_current_day() {
    for cycle in 0..(MAX_MISSION_CYCLES + 2) {
        for day_now in [1u32, 5, 50, 200] {
            let missions = sim::missions::cycle_missions(cycle, day_now);
            let survive = missions
                .iter()
                .find(|m| matches!(m.kind, MissionKind::SurviveDays(_)))
                .expect("cycle_missions should always include a SurviveDays goal");
            let target = survive.kind.target();
            assert!(
                target > day_now,
                "cycle {cycle}, day_now {day_now}: SurviveDays target {target} must be strictly ahead of the current day, \
                 never already satisfied the moment it's issued"
            );
        }
    }
}

/// Companion to the isolated horizon check above: confirms a REAL cycle
/// issued mid-run by `tick` is never trivially already-complete overall
/// (not just the `SurviveDays` goal in isolation).
#[test]
fn a_freshly_issued_cycle_is_never_already_satisfied() {
    let mut state = sim::new_game_bootstrapped(SEED, 50);
    fund(&mut state);
    state.tunnel.unlocked = true;
    state.tick = 40 * TICKS_PER_DAY; // well into the run before this cycle is issued
    for m in &mut state.missions {
        m.done = true;
    }

    sim::tick(&mut state);

    assert_eq!(state.mission_cycle, 1);
    assert!(
        !state.all_missions_done(),
        "a freshly issued cycle must have at least the SurviveDays goal still pending"
    );
}

#[test]
fn the_tunnel_stays_unlocked_across_cycles() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    fund(&mut state);
    state.tunnel.unlocked = true;

    for _ in 0..5 {
        for m in &mut state.missions {
            m.done = true;
        }
        sim::tick(&mut state);
        assert!(state.tunnel.unlocked, "the Tunnel unlock latch must never clear once set");
    }
    assert_eq!(state.mission_cycle, 5, "sanity: 5 clears should have issued 5 extra cycles");
}

#[test]
fn cycles_stop_at_max_mission_cycles() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    fund(&mut state);
    state.tunnel.unlocked = true;
    state.mission_cycle = MAX_MISSION_CYCLES; // already at the cap
    for m in &mut state.missions {
        m.done = true;
    }
    let before = state.missions.clone();

    sim::tick(&mut state);

    assert_eq!(state.mission_cycle, MAX_MISSION_CYCLES, "must not issue a cycle past MAX_MISSION_CYCLES");
    assert_eq!(state.missions, before, "the mission list must be left untouched once the cap is reached");
}

#[test]
fn the_central_world_never_issues_mission_cycles() {
    let mut state = sim::new_game_central(SEED);
    assert!(state.missions.is_empty(), "sanity: the central world starts with no missions");
    assert!(!state.all_missions_done(), "sanity: all_missions_done is false on an empty list by design");

    for _ in 0..10 {
        sim::tick(&mut state);
    }

    assert_eq!(state.mission_cycle, 0, "the central world must never issue a mission cycle");
    assert!(state.missions.is_empty(), "the central world's mission list must stay empty");
}
