//! The opening bootstrap: a fresh `new_game` starts with just its leader and
//! an unlit, under-construction Furnace — the player must assign the leader
//! to build it before `SetFurnaceLevel` does anything, and nobody else comes
//! looking for the city until it's lit. Building it is a real chop-and-carry
//! cycle (`Survivor::chop_target`/`carrying_wood`): walk to the nearest
//! forest tile, chop it, walk back, deliver — `FURNACE_LOGS_NEEDED` times.
//! `sim::new_game_bootstrapped` (used by most other test files) skips
//! straight past all of this; these tests are the ones that actually
//! exercise it.

use frozen_city::game::sim;
use frozen_city::game::types::*;

const SEED: u64 = 2026;

fn furnace(state: &GameState) -> &Building {
    state.buildings.iter().find(|b| b.kind == BuildingKind::Furnace).expect("furnace exists")
}

#[test]
fn new_game_starts_with_one_leader_and_an_unlit_furnace() {
    let state = sim::new_game(SEED, 12);
    assert_eq!(state.survivors.len(), 1, "just the leader");
    assert!(!state.furnace_lit);
    assert_eq!(state.furnace_level, 0);
    assert_eq!(state.heat_radius(), 0.0);
    assert!(furnace(&state).under_construction());
}

#[test]
fn the_leader_can_be_assigned_to_build_the_unfinished_furnace() {
    let mut state = sim::new_game(SEED, 12);
    let leader = state.survivors[0].id;
    let furnace_id = furnace(&state).id;

    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::AssignSurvivor { survivor: leader, building: Some(furnace_id) },
    );

    assert_eq!(
        state.survivors[0].assigned_building,
        Some(furnace_id),
        "the furnace still under construction accepts a named builder"
    );
    assert_eq!(furnace(&state).workers, 1);
}

#[test]
fn set_furnace_level_is_a_noop_while_still_under_construction() {
    let mut state = sim::new_game(SEED, 12);
    sim::apply_command(&mut state, 1, &PlayerCommand::SetFurnaceLevel { level: 2 });
    assert_eq!(state.furnace_level, 0, "can't dial in a burn level before the furnace exists");
    assert!(!state.furnace_lit);
}

#[test]
fn building_the_furnace_lights_it_but_keeps_the_level_command_locked_until_pech() {
    let mut state = sim::new_game(SEED, 12);
    let leader = state.survivors[0].id;
    let furnace_id = furnace(&state).id;
    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::AssignSurvivor { survivor: leader, building: Some(furnace_id) },
    );

    // Completion now takes as many chop-and-carry round trips as
    // `FURNACE_LOGS_NEEDED`, each a real walk whose length depends on how
    // far the nearest forest tile happens to be — no fixed tick formula
    // like the old worker-days model. `FURNACE_CHOP_RADIUS` bounds the
    // search, so a generous tick budget comfortably covers even a
    // maximally unlucky (furthest-tree) run.
    for _ in 0..(TICKS_PER_DAY * 10) {
        sim::tick(&mut state);
        if state.furnace_lit {
            break;
        }
    }

    assert!(state.furnace_lit, "the furnace should have finished building");
    assert_eq!(state.furnace_level, 1);
    assert!(!furnace(&state).under_construction());
    assert!(state.events.iter().any(|e| e.text.contains("lit")), "a lighting event was pushed");

    // The level command is still locked, though — a freshly-lit furnace is
    // only a rough "gulxan" (`Building.level` == 1), not yet an established
    // "Pech" (V0.9: `Building.level >= 7`, see `BuildingKind::upgradeable`'s
    // doc and `tests/construction_tests.rs`'s dedicated upgrade-chain test).
    sim::apply_command(&mut state, 1, &PlayerCommand::SetFurnaceLevel { level: 3 });
    assert_eq!(state.furnace_level, 1, "burn intensity stays locked until the furnace reaches level 7");

    // Nobody needs to keep staffing a lit furnace — the completion arm trims
    // the crew to `max_workers()` (0), freeing the leader for other work.
    assert_eq!(state.survivors[0].assigned_building, None);
}

#[test]
fn the_assigned_leader_picks_a_tree_and_walks_toward_it() {
    let mut state = sim::new_game(SEED, 12);
    let leader = state.survivors[0].id;
    let furnace_id = furnace(&state).id;
    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::AssignSurvivor { survivor: leader, building: Some(furnace_id) },
    );

    // One tick is enough for the idle-with-no-target branch to pick a tree
    // (see `tick.rs`'s furnace chop/carry block) — it doesn't need to have
    // arrived anywhere yet.
    sim::tick(&mut state);
    let s = &state.survivors[0];
    assert!(s.chop_target.is_some(), "an idle assigned leader should pick a tree to chop");
    assert!(!s.carrying_wood);

    let (tx, ty) = s.chop_target.unwrap();
    let before = (s.x, s.y);
    for _ in 0..50 {
        sim::tick(&mut state);
    }
    let after = &state.survivors[0];
    let d_before = ((before.0 - tx as f32 - 0.5).powi(2) + (before.1 - ty as f32 - 0.5).powi(2)).sqrt();
    let d_after = ((after.x - tx as f32 - 0.5).powi(2) + (after.y - ty as f32 - 0.5).powi(2)).sqrt();
    assert!(
        d_after < d_before || after.carrying_wood,
        "the leader should be walking toward their chop target (or have already arrived and \
         picked it up): before={d_before}, after={d_after}, carrying={}",
        after.carrying_wood
    );
}

#[test]
fn arriving_at_the_tree_chops_it_and_heads_home_carrying_wood() {
    let mut state = sim::new_game(SEED, 12);
    let leader = state.survivors[0].id;
    let furnace_id = furnace(&state).id;
    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::AssignSurvivor { survivor: leader, building: Some(furnace_id) },
    );

    let logs_before = furnace(&state).build_left;
    let mut ever_carried = false;
    for _ in 0..2000 {
        sim::tick(&mut state);
        if state.survivors[0].carrying_wood {
            ever_carried = true;
            break;
        }
    }
    assert!(ever_carried, "the leader should carry wood after reaching a tree");
    assert!(state.survivors[0].chop_target.is_none(), "chop_target clears once they're carrying");
    // Delivery only happens on arrival back home, not the instant they pick
    // up the log — so build_left hasn't moved yet at this exact moment.
    assert_eq!(furnace(&state).build_left, logs_before);
}

#[test]
fn moving_the_leader_away_cancels_their_chop_errand() {
    let mut state = sim::new_game(SEED, 12);
    let leader = state.survivors[0].id;
    let furnace_id = furnace(&state).id;
    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::AssignSurvivor { survivor: leader, building: Some(furnace_id) },
    );
    sim::tick(&mut state);
    assert!(state.survivors[0].chop_target.is_some(), "sanity: a chop errand is in progress");

    sim::apply_command(&mut state, 1, &PlayerCommand::MoveSurvivor { survivor: leader, x: 5, y: 5 });

    let s = &state.survivors[0];
    assert_eq!(s.assigned_building, None, "a manual walk unassigns them, same as any other job");
    assert_eq!(s.chop_target, None, "no tree should keep pulling them once un-assigned");
    assert!(!s.carrying_wood);
    assert_eq!(s.move_target, Some((5, 5)));
}

#[test]
fn nobody_arrives_while_the_furnace_stays_unbuilt() {
    let mut state = sim::new_game(SEED, 12);
    // Nobody is ever assigned to build it — run well past the old day>=2
    // arrival gate and confirm the population never grows.
    for _ in 0..(TICKS_PER_DAY * 5) {
        sim::tick(&mut state);
        if state.phase != GamePhase::Running {
            break;
        }
    }
    assert!(
        state.survivors.len() <= 1,
        "no newcomers seek a furnace that was never lit (got {})",
        state.survivors.len()
    );
}

#[test]
fn arrivals_resume_once_the_leader_lights_the_furnace() {
    // win_days is generous (not 12) so a day-12 victory can't cut the
    // 25-day arrival window short before a newcomer has a chance to show.
    let mut state = sim::new_game(SEED, 30);
    let leader = state.survivors[0].id;
    let furnace_id = furnace(&state).id;
    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::AssignSurvivor { survivor: leader, building: Some(furnace_id) },
    );

    // Each eligible morning independently rolls a 55% chance, and with zero
    // tents built, `housing_capacity() + 2 - pop` caps growth at a single
    // newcomer (pop 1 -> 2 fills that space) — so this only ever needs to
    // roll a hit once. 25 days gives ~23 independent tries after the day>=2
    // gate, leaving the odds of a false failure astronomically small
    // (0.45^23) without hardcoding a lucky seed.
    //
    // V0.11: fund food/water every tick — this test is about arrivals
    // resuming once the furnace lights, not survival economy, and over a
    // 25-day window an unfunded stockpile would let thirst (or plain
    // starvation) kill the lone survivor before any arrival ever gets a
    // chance to roll, wiping the population to 0 instead of demonstrating
    // the arrival mechanic.
    for _ in 0..(TICKS_PER_DAY * 25) {
        state.stock.food = 9999.0;
        state.stock.water = 9999.0;
        sim::tick(&mut state);
        if state.phase != GamePhase::Running || state.survivors.len() > 1 {
            break;
        }
    }

    assert!(state.furnace_lit);
    assert!(
        state.survivors.len() > 1,
        "newcomers should have arrived once the furnace was lit, got {}",
        state.survivors.len()
    );
}
