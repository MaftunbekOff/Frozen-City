//! Pure-simulation tests for the V0.3 technology tree: researching techs via
//! `PlayerCommand::Research`, and each tech's effect on the tick simulation.
//!
//! Style mirrors `building_tests.rs` / `sim_tests.rs`: control-vs-experiment
//! comparisons between two otherwise-identical `new_game` worlds, ticked in
//! lockstep so the (seed-driven) RNG paths for arrivals/cold-snaps stay
//! identical between the two runs and the only difference is the tech under
//! test — a raw before/after delta would be too sensitive to the rest of the
//! simulation.

use frozen_city::game::sim;
use frozen_city::game::types::*;

const SEED: u64 = 12345;

/// Places `kind` at `(x, y)` and assigns `workers` to it, returning the new
/// building's id. Callers check `find_building(id).unwrap().workers` to make
/// sure the requested worker count was actually reached before ticking.
fn place_and_staff(state: &mut GameState, kind: BuildingKind, x: u8, y: u8, workers: i8) -> u32 {
    sim::apply_command(state, 1, &PlayerCommand::Place { kind, x, y });
    let id = state.buildings.last().unwrap().id;
    // V0.8: bu testlar bitgan bino effektini sinaydi — maydonchani darhol
    // bitirib, so'ralgan ishchi soniga keltiramiz.
    sim::finish_all_construction(state);
    let cur = state.find_building(id).unwrap().workers as i8;
    sim::apply_command(state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: workers - cur });
    id
}

#[test]
fn research_unlocks_and_deducts_cost() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    state.stock.coal = 500.0;

    let wood_before = state.stock.wood;
    let coal_before = state.stock.coal;

    sim::apply_command(&mut state, 1, &PlayerCommand::Research { tech: Tech::Tools });

    assert!(state.has_tech(Tech::Tools), "Tools should be unlocked");
    assert!(
        (state.stock.wood - (wood_before - 50.0)).abs() < 0.001,
        "wood should drop by the tech's cost (50): {}",
        state.stock.wood
    );
    assert!(
        (state.stock.coal - (coal_before - 10.0)).abs() < 0.001,
        "coal should drop by the tech's cost (10): {}",
        state.stock.coal
    );

    // A second identical Research is a no-op: already owned, so no further
    // deduction and the tech isn't duplicated in the list.
    let wood_after = state.stock.wood;
    let coal_after = state.stock.coal;
    sim::apply_command(&mut state, 1, &PlayerCommand::Research { tech: Tech::Tools });
    assert_eq!(
        state.techs.iter().filter(|&&t| t == Tech::Tools).count(),
        1,
        "researching an already-owned tech must not duplicate it"
    );
    assert_eq!(state.stock.wood, wood_after);
    assert_eq!(state.stock.coal, coal_after);

    // Insufficient resources: does not unlock, no partial deduction.
    state.stock.wood = 0.0;
    sim::apply_command(&mut state, 1, &PlayerCommand::Research { tech: Tech::Insulation });
    assert!(!state.has_tech(Tech::Insulation), "insufficient wood must block research");
}

#[test]
fn research_is_a_noop_without_resources() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 0.0;
    state.stock.coal = 0.0;

    sim::apply_command(&mut state, 1, &PlayerCommand::Research { tech: Tech::Tools });

    assert!(!state.has_tech(Tech::Tools), "no resources -> no research");
    assert!(state.techs.is_empty());
    assert_eq!(state.stock.wood, 0.0);
    assert_eq!(state.stock.coal, 0.0);
}

#[test]
fn efficient_furnace_burns_less_fuel() {
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);
    experiment.techs.push(Tech::EfficientFurnace);

    control.stock.coal = 1000.0;
    experiment.stock.coal = 1000.0;

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        experiment.stock.coal > control.stock.coal,
        "Efficient Furnace should leave more coal: control={}, experiment={}",
        control.stock.coal,
        experiment.stock.coal
    );
}

#[test]
fn better_tools_boosts_production() {
    // A HunterHut's food output has no terrain dependency (unlike a Sawmill's
    // nearby forest), so staffing it identically in both worlds isolates the
    // Tools multiplier cleanly.
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);
    experiment.techs.push(Tech::Tools);

    control.stock.wood = 500.0;
    experiment.stock.wood = 500.0;

    let (cx, cy) = find_spot(&control, BuildingKind::HunterHut);
    let cid = place_and_staff(&mut control, BuildingKind::HunterHut, cx, cy, 2);
    assert_eq!(
        control.find_building(cid).unwrap().workers,
        2,
        "control HunterHut should be fully staffed"
    );

    let (ex, ey) = find_spot(&experiment, BuildingKind::HunterHut);
    let eid = place_and_staff(&mut experiment, BuildingKind::HunterHut, ex, ey, 2);
    assert_eq!(
        experiment.find_building(eid).unwrap().workers,
        2,
        "experiment HunterHut should be fully staffed"
    );

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        experiment.stock.food > control.stock.food,
        "Better Tools should boost HunterHut food production: control={}, experiment={}",
        control.stock.food,
        experiment.stock.food
    );
}

#[test]
fn rationing_reduces_food_consumption() {
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);
    experiment.techs.push(Tech::Rationing);

    control.stock.food = 1000.0;
    experiment.stock.food = 1000.0;

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        experiment.stock.food > control.stock.food,
        "Rationing (0.85x consumption) should leave more food than the control: control={}, experiment={}",
        control.stock.food,
        experiment.stock.food
    );
}

#[test]
fn medicine_boosts_hospital_healing() {
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);
    experiment.techs.push(Tech::Medicine);

    control.survivors[0].hp = 40.0;
    experiment.survivors[0].hp = 40.0;

    control.stock.wood = 500.0;
    experiment.stock.wood = 500.0;

    let (cx, cy) = find_spot(&control, BuildingKind::Hospital);
    let cid = place_and_staff(&mut control, BuildingKind::Hospital, cx, cy, 2);
    assert_eq!(
        control.find_building(cid).unwrap().workers,
        2,
        "control Hospital should be fully staffed"
    );

    let (ex, ey) = find_spot(&experiment, BuildingKind::Hospital);
    let eid = place_and_staff(&mut experiment, BuildingKind::Hospital, ex, ey, 2);
    assert_eq!(
        experiment.find_building(eid).unwrap().workers,
        2,
        "experiment Hospital should be fully staffed"
    );

    for _ in 0..(TICKS_PER_DAY / 2) {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    // Compare total population HP rather than a single survivor: hospital
    // care applies to everyone each tick, so this is the most robust signal.
    let control_hp: f32 = control.survivors.iter().map(|s| s.hp).sum();
    let experiment_hp: f32 = experiment.survivors.iter().map(|s| s.hp).sum();
    assert!(
        experiment_hp > control_hp + 1.0,
        "Medicine should heal the population faster: control_hp={control_hp}, experiment_hp={experiment_hp}"
    );
}

#[test]
fn insulation_reduces_cold_damage() {
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);
    experiment.techs.push(Tech::Insulation);

    // No furnace heat and no tents in either world, so every survivor is
    // exposed to the raw outside temperature and the only remaining
    // difference is Insulation's flat warmth bonus.
    control.furnace_level = 0;
    control.furnace_lit = false;
    experiment.furnace_level = 0;
    experiment.furnace_lit = false;

    for s in &mut control.survivors {
        s.hp = 50.0;
    }
    for s in &mut experiment.survivors {
        s.hp = 50.0;
    }

    // A few in-game hours is enough to show a measurable HP gap without
    // risking anyone actually dying (which would change population sizes and
    // make the raw HP-sum comparison unfair).
    let ticks_per_hour = TICKS_PER_DAY / 24;
    for _ in 0..(ticks_per_hour * 6) {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert_eq!(
        control.survivors.len(),
        experiment.survivors.len(),
        "nobody should have died in either run"
    );

    let control_hp: f32 = control.survivors.iter().map(|s| s.hp).sum();
    let experiment_hp: f32 = experiment.survivors.iter().map(|s| s.hp).sum();
    assert!(
        experiment_hp >= control_hp,
        "Insulation should reduce (or reverse) cold damage: control_hp={control_hp}, experiment_hp={experiment_hp}"
    );
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
