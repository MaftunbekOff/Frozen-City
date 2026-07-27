//! Pure-simulation tests for V0.7 leadership: `SetLeader`, the production
//! bonus while alive, the mourning penalty (+ event) on death, and gating the
//! refugee-caravan choice on a living leader. Mirrors the style of
//! `event_tests.rs` / `building_tests.rs`.

use frozen_city::game::sim;
use frozen_city::game::types::*;

const SEED: u64 = 12345;

/// Requires nearby forest — a plain Sawmill spot can
/// otherwise land far from any harvestable tile, making production (and any
/// multiplier applied to it) invisible over a short test run.
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

#[test]
fn set_leader_records_the_survivor_and_guests_may_issue_it_too() {
    let mut state = sim::new_game(SEED, 12);
    sim::player_joined(&mut state, 1, "Owner");
    sim::player_joined(&mut state, 2, "Guest");
    let candidate = state.survivors[0].id;

    // Guests have full command authority alongside the owner (see
    // `GameState::can_issue`), so both may issue SetLeader.
    assert!(state.can_issue(2, &PlayerCommand::SetLeader { survivor: candidate }));
    assert!(state.can_issue(1, &PlayerCommand::SetLeader { survivor: candidate }));

    sim::apply_command(&mut state, 1, &PlayerCommand::SetLeader { survivor: candidate });
    assert_eq!(state.leader, Some(candidate));
    assert!(state.leader_alive());
}

#[test]
fn set_leader_is_disallowed_entirely_in_the_central_world() {
    let mut state = sim::new_game_central(5);
    sim::player_joined_as(&mut state, 10, "Aziz", Some(1));
    assert!(!state.can_issue(10, &PlayerCommand::SetLeader { survivor: 1 }));
}

#[test]
fn leader_alive_boosts_production() {
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);
    for state in [&mut control, &mut experiment] {
        state.stock.wood = 500.0;
    }
    // `new_game` now auto-appoints the sole starting survivor as leader —
    // clear it on the control so the comparison isolates the leader bonus.
    control.leader = None;
    let (x, y) = find_sawmill_spot_near_forest(&control);
    sim::apply_command(&mut control, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y, facing: 0 });
    let control_id = control.buildings.last().unwrap().id;
    // V0.21: a mill only cuts once its saw bench is in. `finish_all_construction`
    // fits the first one (and skips the build phase this test doesn't care about).
    sim::finish_all_construction(&mut control);
    sim::apply_command(&mut control, 1, &PlayerCommand::AdjustWorkers { building: control_id, delta: 2 });

    let (x, y) = find_sawmill_spot_near_forest(&experiment);
    sim::apply_command(&mut experiment, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y, facing: 0 });
    let exp_id = experiment.buildings.last().unwrap().id;
    sim::finish_all_construction(&mut experiment);
    sim::apply_command(&mut experiment, 1, &PlayerCommand::AdjustWorkers { building: exp_id, delta: 2 });
    experiment.leader = Some(experiment.survivors[0].id);

    // A long enough window that the ~8% gap reliably lands on a different
    // whole-wood-unit count (`take_forest_unit` only credits the stockpile
    // in integer steps) instead of being lost to rounding over a short run.
    for _ in 0..(TICKS_PER_DAY * 5) {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        experiment.stock.wood > control.stock.wood,
        "a living leader's +8% should yield more wood: control={}, experiment={}",
        control.stock.wood,
        experiment.stock.wood
    );
}

#[test]
fn leader_death_starts_mourning_and_pushes_an_event() {
    let mut state = sim::new_game(SEED, 12);
    let leader_id = state.survivors[0].id;
    state.leader = Some(leader_id);

    // Kill the leader and tick once so the death path runs.
    state.survivors.iter_mut().find(|s| s.id == leader_id).unwrap().hp = 0.0;
    sim::tick(&mut state);

    assert_eq!(state.leader, None, "the leader seat is cleared on death");
    assert!(state.mourning_active(), "mourning should begin immediately");
    assert_eq!(state.mourning_until, state.tick + TICKS_PER_DAY);
    assert!(
        state.events.iter().any(|e| e.text.contains("mourns")),
        "a mourning event should be pushed: {:?}",
        state.events
    );
}

#[test]
fn mourning_penalizes_production() {
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);
    for state in [&mut control, &mut experiment] {
        state.stock.wood = 500.0;
        // Both worlds start with a living (auto-appointed) leader, whose
        // production bonus would otherwise take priority over — and so
        // mask — the mourning penalty (`leader_multiplier` treats the two
        // as mutually exclusive). Clear it so mourning is actually reached.
        state.leader = None;
    }
    let (x, y) = find_sawmill_spot_near_forest(&control);
    sim::apply_command(&mut control, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y, facing: 0 });
    let control_id = control.buildings.last().unwrap().id;
    // V0.21: a mill only cuts once its saw bench is in. `finish_all_construction`
    // fits the first one (and skips the build phase this test doesn't care about).
    sim::finish_all_construction(&mut control);
    sim::apply_command(&mut control, 1, &PlayerCommand::AdjustWorkers { building: control_id, delta: 2 });

    let (x, y) = find_sawmill_spot_near_forest(&experiment);
    sim::apply_command(&mut experiment, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y, facing: 0 });
    let exp_id = experiment.buildings.last().unwrap().id;
    sim::finish_all_construction(&mut experiment);
    sim::apply_command(&mut experiment, 1, &PlayerCommand::AdjustWorkers { building: exp_id, delta: 2 });
    // Long enough (and mourning lasts a full in-game day) that the -15% gap
    // reliably lands on a different whole-wood-unit count instead of being
    // lost to `take_forest_unit`'s integer rounding over a short run.
    experiment.mourning_until = experiment.tick + TICKS_PER_DAY;

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        experiment.stock.wood < control.stock.wood,
        "mourning's -15% should yield less wood: control={}, experiment={}",
        control.stock.wood,
        experiment.stock.wood
    );
}

#[test]
fn respond_event_is_a_noop_without_a_living_leader() {
    let mut state = sim::new_game(SEED, 12);
    // `new_game` auto-appoints the sole starting survivor as leader; this
    // test wants the genuinely-leaderless case.
    state.leader = None;
    state.stock.food = 100.0;
    let pop_before = state.survivors.len();
    state.pending_event = Some(CaravanOffer { count: 3, food_cost: 12, expires: state.tick + 100 });

    sim::apply_command(&mut state, 1, &PlayerCommand::RespondEvent { accept: true });

    assert!(
        state.pending_event.is_some(),
        "without a leader the offer must be left standing for the deadline to resolve it"
    );
    assert_eq!(state.survivors.len(), pop_before);
    assert_eq!(state.stock.food, 100.0);
}

#[test]
fn respond_event_works_again_once_a_leader_is_set() {
    let mut state = sim::new_game(SEED, 12);
    state.leader = Some(state.survivors[0].id);
    state.stock.food = 100.0;
    state.pending_event = Some(CaravanOffer { count: 3, food_cost: 12, expires: state.tick + 100 });

    sim::apply_command(&mut state, 1, &PlayerCommand::RespondEvent { accept: false });

    assert!(state.pending_event.is_none(), "a living leader can resolve the caravan choice");
}

#[test]
fn leaderless_caravan_auto_resolves_to_reject_at_the_deadline() {
    let mut state = sim::new_game(SEED, 12);
    // `new_game` auto-appoints the sole starting survivor as leader; this
    // test wants the genuinely-leaderless case.
    state.leader = None;
    let pop_before = state.survivors.len();
    state.pending_event = Some(CaravanOffer { count: 3, food_cost: 12, expires: state.tick + 3 });

    // Repeatedly "voting" accept must not do anything without a leader...
    for _ in 0..3 {
        sim::apply_command(&mut state, 1, &PlayerCommand::RespondEvent { accept: true });
    }
    assert!(state.pending_event.is_some());

    // ...and the offer still lapses on its own at the existing deadline.
    for _ in 0..5 {
        sim::tick(&mut state);
    }
    assert!(state.pending_event.is_none(), "an unresolved caravan should still auto-expire");
    assert_eq!(
        state.survivors.len(),
        pop_before,
        "auto-resolution is a reject: no survivors should have been added"
    );
}
