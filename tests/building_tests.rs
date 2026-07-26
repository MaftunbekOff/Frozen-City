//! Pure-simulation tests for the new production/effect buildings: Greenhouse
//! (food producer), Hospital (heals survivors when staffed), Kitchen (reduces
//! food consumption when staffed), and Warehouse (discounts construction wood
//! cost when staffed).
//!
//! Style mirrors `sim_tests.rs`: build a scenario, tick a day (or half a day),
//! and assert on the resulting `GameState` — comparing a staffed-building run
//! against an otherwise-identical control run wherever a raw before/after
//! delta would be too sensitive to the rest of the simulation.

use frozen_city::game::sim;
use frozen_city::game::types::*;

/// Bincode encodes `BuildingKind` positionally (by declaration order) — every
/// already-persisted world's `Building.kind` bytes rely on these exact wire
/// indices never moving. This test exists because that invariant was broken
/// once already in this session's history (a new variant inserted BEFORE
/// `Tunnel` silently shifted it, misdecoding every persisted world's Tunnel
/// as the new kind) — pin down every current index so a future reorder fails
/// loudly here instead of corrupting production saves silently.
#[test]
fn building_kind_wire_indices_are_stable() {
    let cases: [(BuildingKind, u32); 15] = [
        (BuildingKind::Furnace, 0),
        (BuildingKind::Tent, 1),
        (BuildingKind::Sawmill, 2),
        (BuildingKind::CoalMine, 3),
        (BuildingKind::HunterHut, 4),
        (BuildingKind::Greenhouse, 5),
        (BuildingKind::Hospital, 6),
        (BuildingKind::Kitchen, 7),
        (BuildingKind::Warehouse, 8),
        (BuildingKind::Tunnel, 9),
        (BuildingKind::TailorShop, 10),
        (BuildingKind::Wall, 11),
        (BuildingKind::Gate, 12),
        (BuildingKind::Well, 13),
        (BuildingKind::Farmhouse, 14),
    ];
    for (kind, expected_index) in cases {
        let bytes = bincode::serialize(&kind).expect("BuildingKind always serializes");
        assert_eq!(
            bytes,
            expected_index.to_le_bytes(),
            "{kind:?}'s wire index changed — this silently corrupts every already-persisted \
             world's Building.kind on load. New variants MUST be appended at the very end \
             of the enum, never inserted before an existing one."
        );
    }
}

const SEED: u64 = 12345;

/// Places `kind` at `(x, y)`, instantly finishes its construction (these
/// tests probe a FINISHED building's effect — the construction phase has its
/// own suite, `construction_tests.rs`) and staffs it to exactly `workers`,
/// returning the new building's id.
fn place_and_staff(state: &mut GameState, kind: BuildingKind, x: u8, y: u8, workers: i8) -> u32 {
    sim::apply_command(state, 1, &PlayerCommand::Place { kind, x, y, facing: 0 });
    let id = state.buildings.last().unwrap().id;
    sim::finish_all_construction(state);
    let cur = state.find_building(id).unwrap().workers as i8;
    sim::apply_command(state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: workers - cur });
    id
}

#[test]
fn greenhouse_produces_food() {
    // Two otherwise-identical worlds; only `experiment` gets a staffed
    // Greenhouse. Comparing against the control isolates the greenhouse's
    // contribution from the (identical, seed-driven) baseline food economy.
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);

    experiment.stock.wood = 500.0;
    let (x, y) = find_spot(&experiment, BuildingKind::Greenhouse);
    let id = place_and_staff(&mut experiment, BuildingKind::Greenhouse, x, y, 2);
    assert_eq!(
        experiment.find_building(id).unwrap().workers,
        2,
        "both greenhouse slots should have been staffed from the idle pool"
    );

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    // 2 workers * 9.0 food/worker/day = 18 food/day of extra income; the
    // baseline hunger consumption is identical in both runs, so the whole
    // delta should show up in the stockpile. Use a generous tolerance well
    // below that to avoid brittleness.
    assert!(
        experiment.stock.food > control.stock.food + 5.0,
        "staffed greenhouse should leave meaningfully more food: control={}, experiment={}",
        control.stock.food,
        experiment.stock.food
    );
}

#[test]
fn staffed_hospital_speeds_healing() {
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);

    // Same low-HP starting point in both runs. The furnace stays lit at
    // level 1 (new_game_bootstrapped's default), so neither population is freezing.
    control.survivors[0].hp = 40.0;
    control.survivors[0].hunger = 10.0;
    experiment.survivors[0].hp = 40.0;
    experiment.survivors[0].hunger = 10.0;

    experiment.stock.wood = 500.0;
    let (x, y) = find_spot(&experiment, BuildingKind::Hospital);
    let id = place_and_staff(&mut experiment, BuildingKind::Hospital, x, y, 2);
    assert_eq!(
        experiment.find_building(id).unwrap().workers,
        2,
        "both hospital slots should have been staffed from the idle pool"
    );

    for _ in 0..(TICKS_PER_DAY / 2) {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    // Medical care is applied to every survivor each tick (not just the one
    // we lowered), so comparing the total HP across the population is the
    // most robust signal and avoids relying on survivor-list alignment.
    let control_hp: f32 = control.survivors.iter().map(|s| s.hp).sum();
    let experiment_hp: f32 = experiment.survivors.iter().map(|s| s.hp).sum();
    assert!(
        experiment_hp > control_hp + 1.0,
        "a staffed hospital should heal the population faster: control_hp={control_hp}, experiment_hp={experiment_hp}"
    );

    // The specific survivor we depressed should also individually be better
    // off (or at worst tied, if both runs happened to cap it at 100).
    assert!(
        experiment.survivors[0].hp >= control.survivors[0].hp,
        "the tracked survivor should not heal slower with a staffed hospital"
    );
}

#[test]
fn staffed_kitchen_reduces_food_consumption() {
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);

    // Plenty of food, and neither run builds a food producer, so the only
    // thing moving the stockpile is survivor consumption.
    control.stock.food = 1000.0;
    experiment.stock.food = 1000.0;
    experiment.stock.wood = 500.0;

    let (x, y) = find_spot(&experiment, BuildingKind::Kitchen);
    let id = place_and_staff(&mut experiment, BuildingKind::Kitchen, x, y, 1);
    assert_eq!(
        experiment.find_building(id).unwrap().workers,
        1,
        "the kitchen's single slot should have been staffed"
    );

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        experiment.stock.food > control.stock.food,
        "a staffed kitchen (0.75x consumption) should leave more food than the control: control={}, experiment={}",
        control.stock.food,
        experiment.stock.food
    );
}

#[test]
fn staffed_warehouse_discounts_construction() {
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);

    control.stock.wood = 500.0;
    experiment.stock.wood = 500.0;

    let (wx, wy) = find_spot(&experiment, BuildingKind::Warehouse);
    let id = place_and_staff(&mut experiment, BuildingKind::Warehouse, wx, wy, 1);
    assert_eq!(
        experiment.find_building(id).unwrap().workers,
        1,
        "the warehouse's single slot should have been staffed"
    );

    let control_wood_before = control.stock.wood;
    let experiment_wood_before = experiment.stock.wood;

    let (tx, ty) = find_spot(&control, BuildingKind::Tent);
    sim::apply_command(
        &mut control,
        1,
        &PlayerCommand::Place { kind: BuildingKind::Tent, x: tx, y: ty, facing: 0 },
    );
    let (tx2, ty2) = find_spot(&experiment, BuildingKind::Tent);
    sim::apply_command(
        &mut experiment,
        1,
        &PlayerCommand::Place { kind: BuildingKind::Tent, x: tx2, y: ty2, facing: 0 },
    );

    let control_cost = control_wood_before - control.stock.wood;
    let experiment_cost = experiment_wood_before - experiment.stock.wood;
    assert!(
        (experiment_cost - control_cost * WAREHOUSE_BUILD_DISCOUNT).abs() < 0.01,
        "a staffed warehouse should discount the next build by exactly \
         WAREHOUSE_BUILD_DISCOUNT: control_cost={control_cost}, experiment_cost={experiment_cost}"
    );
}

#[test]
fn tailor_shop_place_and_staff_like_other_producers() {
    // A regression guard that the new TailorShop variant is wired correctly
    // into Place/AdjustWorkers/wood-cost/construction like every other
    // buildable kind — not a wildlife-specific test (see wildlife_tests.rs
    // for the fur->cloth conversion itself).
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::TailorShop);
    let id = place_and_staff(&mut state, BuildingKind::TailorShop, x, y, 2);
    assert_eq!(
        state.find_building(id).unwrap().workers,
        2,
        "both tailor shop slots should have been staffed from the idle pool"
    );
}

#[test]
fn new_buildings_cost_wood_and_need_clear_ground() {
    let cases = [
        (BuildingKind::Greenhouse, 35u32),
        (BuildingKind::Hospital, 35u32),
        (BuildingKind::Kitchen, 25u32),
        (BuildingKind::Warehouse, 30u32),
        (BuildingKind::TailorShop, 45u32),
        (BuildingKind::Wall, 4u32),
        (BuildingKind::Gate, 5u32),
    ];

    for (kind, wood_cost) in cases {
        assert_eq!(kind.cost_wood(), wood_cost, "{kind:?} wood cost mismatch");

        let mut state = sim::new_game(SEED, 12);
        let (x, y) = find_spot(&state, kind);

        // Not enough wood: rejected even on perfectly clear ground.
        state.stock.wood = (wood_cost as f32) - 1.0;
        assert!(
            state.can_place(kind, x, y).is_err(),
            "{kind:?} should be rejected with insufficient wood"
        );

        // Enough wood, clear snow: accepted.
        state.stock.wood = 500.0;
        assert!(
            state.can_place(kind, x, y).is_ok(),
            "{kind:?} should be placeable on clear snow with enough wood"
        );

        // A coal deposit is not "clear ground" for these buildings (only
        // CoalMine may sit on one).
        let (cx, cy) = find_spot(&state, BuildingKind::CoalMine);
        assert!(
            state.can_place(kind, cx, cy).is_err(),
            "{kind:?} should not be placeable on a coal deposit"
        );
    }
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
