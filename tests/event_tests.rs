//! Pure-simulation tests for V0.3 dynamic events: disease, blizzards and
//! refugee caravans.
//!
//! Style mirrors `sim_tests.rs` / `building_tests.rs`: most tests set
//! `GameState` fields directly (rather than waiting on `state.event_rng` to
//! roll the right way) so the assertions exercise the *effects* and the
//! `RespondEvent` command robustly, independent of RNG timing. A couple of
//! tests do drive the real trigger path (via `tick`) to make sure it's wired
//! up at all, and one is a strong determinism guard across the whole feature.

use frozen_city::game::sim;
use frozen_city::game::types::*;

const SEED: u64 = 12345;

#[test]
fn no_events_before_grace_day() {
    let mut state = sim::new_game(SEED, 12);

    // Day EVENT_GRACE_DAY (3) begins at absolute tick
    // (EVENT_GRACE_DAY - 1) * TICKS_PER_DAY; stop one tick short of that
    // rollover so the trigger checks never get a chance to run at all —
    // this makes the assertion below hold regardless of `event_rng`.
    let just_before_grace = (EVENT_GRACE_DAY as u64 - 1) * TICKS_PER_DAY - 1;
    while state.tick < just_before_grace {
        sim::tick(&mut state);
    }

    assert!(
        state.day() < EVENT_GRACE_DAY,
        "test setup should stay before the grace day, got day {}",
        state.day()
    );
    assert_eq!(state.disease_until, 0, "no disease before the grace day");
    assert_eq!(state.blizzard_until, 0, "no blizzard before the grace day");
    assert!(state.pending_event.is_none(), "no caravan before the grace day");
}

#[test]
fn disease_eventually_strikes_and_drains_hp() {
    // Part 1: confirm the real (event_rng-driven) trigger path can actually
    // fire within a reasonable window, across a spread of seeds. This is the
    // one guard that would catch the trigger logic being missing entirely.
    let mut winning_state: Option<GameState> = None;
    for seed in 0..200u64 {
        let mut state = sim::new_game(seed, 30);
        for _ in 0..(12 * TICKS_PER_DAY) {
            sim::tick(&mut state);
            if state.phase != GamePhase::Running {
                break; // this world ended early; try the next seed
            }
            if state.disease_until != 0 {
                winning_state = Some(state);
                break;
            }
        }
        if winning_state.is_some() {
            break;
        }
    }
    let found = winning_state
        .expect("expected disease to fire via the real trigger path within 200 seeds x 12 days");
    assert!(found.disease_active(), "disease_until was set but is not currently active");

    // Part 2: direct-set effect check, isolated from RNG entirely, so it's
    // robust regardless of trigger timing. Both worlds start warm (furnace
    // lit at level 1, new_game's default) and are kept well-fed; only the
    // experiment gets a disease.
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);
    control.stock.food = 1000.0;
    experiment.stock.food = 1000.0;
    experiment.disease_until = experiment.tick + 100;

    for _ in 0..(TICKS_PER_DAY / 4) {
        // a few in-game hours
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    let control_hp: f32 = control.survivors.iter().map(|s| s.hp).sum();
    let experiment_hp: f32 = experiment.survivors.iter().map(|s| s.hp).sum();
    assert!(
        experiment_hp < control_hp - 0.5,
        "a disease should drain HP faster than the healthy control: control_hp={control_hp}, experiment_hp={experiment_hp}"
    );
}

#[test]
fn blizzard_makes_it_colder() {
    let control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);
    experiment.blizzard_until = experiment.tick + 100;

    assert!(experiment.blizzard_active());
    assert!(!control.blizzard_active());

    assert!(
        experiment.temperature() + 0.001 < control.temperature(),
        "a blizzard should make it colder: control={}, experiment={}",
        control.temperature(),
        experiment.temperature()
    );

    let delta = control.temperature() - experiment.temperature();
    assert!(
        (delta - (-BLIZZARD_COLD)).abs() < 0.01,
        "the temperature delta should equal -BLIZZARD_COLD ({}), got {delta}",
        -BLIZZARD_COLD
    );
}

#[test]
fn caravan_accept_adds_survivors_and_costs_food() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    state.stock.food = 100.0;

    // 3 tents = 12 housing slots; with 8 starting survivors,
    // space = housing_capacity() + 2 - pop = 12 + 2 - 8 = 6, comfortably
    // above the offer's count of 3 so nothing here is capacity-limited.
    for _ in 0..3 {
        let (x, y) = find_spot(&state, BuildingKind::Tent);
        sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y });
    }

    let pop_before = state.survivors.len();
    let food_before = state.stock.food;
    state.pending_event = Some(CaravanOffer { count: 3, food_cost: 12, expires: state.tick + 100 });

    sim::apply_command(&mut state, 1, &PlayerCommand::RespondEvent { accept: true });

    assert!(state.pending_event.is_none(), "the offer should be resolved");
    let added = state.survivors.len() - pop_before;
    assert!((1..=3).contains(&added), "expected 1..=3 survivors added, got {added}");
    assert!(
        (food_before - state.stock.food - 12.0).abs() < 0.001,
        "food should have dropped by exactly the offer's food_cost: before={food_before}, after={}",
        state.stock.food
    );
}

#[test]
fn caravan_decline_changes_nothing() {
    let mut state = sim::new_game(SEED, 12);
    let pop_before = state.survivors.len();
    let food_before = state.stock.food;
    state.pending_event = Some(CaravanOffer { count: 3, food_cost: 12, expires: state.tick + 100 });

    sim::apply_command(&mut state, 1, &PlayerCommand::RespondEvent { accept: false });

    assert_eq!(state.survivors.len(), pop_before, "declining must not change the population");
    assert_eq!(state.stock.food, food_before, "declining must not spend food");
    assert!(state.pending_event.is_none(), "declining should still clear the pending offer");
}

#[test]
fn caravan_expires_when_ignored() {
    let mut state = sim::new_game(SEED, 12);
    state.pending_event = Some(CaravanOffer { count: 3, food_cost: 12, expires: state.tick + 3 });

    for _ in 0..5 {
        sim::tick(&mut state);
    }

    assert!(state.pending_event.is_none(), "an ignored caravan offer should auto-expire");
}

#[test]
fn caravan_accept_respects_capacity_and_food() {
    // Case A: no housing at all -> no room for anyone, but the offer is
    // still resolved (pending cleared) and no food is spent.
    let mut state = sim::new_game(SEED, 12);
    state.stock.food = 1000.0;
    let pop_before = state.survivors.len();
    state.pending_event = Some(CaravanOffer { count: 5, food_cost: 20, expires: state.tick + 100 });

    sim::apply_command(&mut state, 1, &PlayerCommand::RespondEvent { accept: true });

    assert!(state.pending_event.is_none());
    assert_eq!(
        state.survivors.len(),
        pop_before,
        "with zero tents there is no room for the caravan"
    );
    assert_eq!(state.stock.food, 1000.0, "no food should be spent when nobody could be housed");

    // Case B: plenty of housing but not enough food -> still no survivors
    // added, even though there would have been room.
    let mut state2 = sim::new_game(SEED, 12);
    state2.stock.wood = 500.0;
    for _ in 0..3 {
        let (x, y) = find_spot(&state2, BuildingKind::Tent);
        sim::apply_command(&mut state2, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y });
    }
    state2.stock.food = 5.0; // less than the offer's food_cost
    let pop_before2 = state2.survivors.len();
    state2.pending_event = Some(CaravanOffer { count: 5, food_cost: 20, expires: state2.tick + 100 });

    sim::apply_command(&mut state2, 1, &PlayerCommand::RespondEvent { accept: true });

    assert!(state2.pending_event.is_none());
    assert_eq!(
        state2.survivors.len(),
        pop_before2,
        "insufficient food should block the caravan even with room to house them"
    );
    assert_eq!(state2.stock.food, 5.0, "food should be untouched when the caravan couldn't be accepted");
}

/// The dynamic-events feature draws from a separate `event_rng` stream, so
/// two identical worlds ticked identically must stay byte-identical exactly
/// as before events existed — a strong guard against events accidentally
/// touching the main `rng`.
#[test]
fn events_do_not_break_determinism() {
    let mut a = sim::new_game(SEED, 30);
    let mut b = sim::new_game(SEED, 30);
    for _ in 0..3000 {
        sim::tick(&mut a);
        sim::tick(&mut b);
    }
    assert_eq!(a, b);
}

#[test]
fn can_issue_respond_event_under_build_but_not_view_only() {
    let mut state = sim::new_game(SEED, 12);
    sim::player_joined(&mut state, 1, "Owner");
    sim::player_joined(&mut state, 2, "Guest");

    state.guest_perm = GuestPermission::Build;
    assert!(state.can_issue(2, &PlayerCommand::RespondEvent { accept: true }));

    state.guest_perm = GuestPermission::ViewOnly;
    assert!(!state.can_issue(2, &PlayerCommand::RespondEvent { accept: true }));
}

#[test]
fn caravan_accept_charges_only_for_admitted_refugees() {
    let mut state = sim::new_game(5, 12);
    // Only room for 1 more (no tents -> capacity 0, so space = 0 + 2 - pop).
    state.survivors.truncate(1);
    state.stock.food = 100.0;
    // Offer 3 refugees; only 1 can be housed, so the city must pay for 1, not 3.
    state.pending_event = Some(CaravanOffer {
        count: 3,
        food_cost: 3 * CARAVAN_FOOD_PER_PERSON,
        expires: state.tick + 100,
    });

    let food_before = state.stock.food;
    sim::apply_command(&mut state, 1, &PlayerCommand::RespondEvent { accept: true });

    assert_eq!(state.survivors.len(), 2, "exactly one refugee could be housed");
    let charged = food_before - state.stock.food;
    assert!(
        (charged - CARAVAN_FOOD_PER_PERSON as f32).abs() < 0.001,
        "charged for 1 admitted refugee, not the full offer of 3 (charged {charged})"
    );
    assert!(state.pending_event.is_none());
}

// --- helpers ---

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
