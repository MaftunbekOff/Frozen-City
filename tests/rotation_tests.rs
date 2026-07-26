//! Pure-simulation tests for V0.16: turning an already-built building in
//! place (`PlayerCommand::RotateBuilding`) and the `facing` byte a `Place`
//! carries. Orientation is purely visual — no gameplay rule may read it — but
//! rotation is modelled on relocation: free of wood, `level`/workers kept,
//! discounted re-construction while the crew re-squares the structure. Style
//! mirrors `relocation_tests.rs` (the closest sibling, same machinery).

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

fn place_and_finish(state: &mut GameState, kind: BuildingKind, x: u8, y: u8, facing: u8) -> u32 {
    sim::apply_command(state, 1, &PlayerCommand::Place { kind, x, y, facing });
    let id = state.buildings.last().unwrap().id;
    sim::finish_all_construction(state);
    id
}

#[test]
fn placing_stores_the_chosen_facing() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    let id = place_and_finish(&mut state, BuildingKind::Tent, x, y, 3);

    assert_eq!(state.find_building(id).unwrap().facing, 3);
}

#[test]
fn placing_clamps_an_out_of_range_facing() {
    // A hand-crafted client must not be able to smuggle in a value the
    // renderer would turn into an absurd angle.
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    let id = place_and_finish(&mut state, BuildingKind::Tent, x, y, 250);

    assert!(state.find_building(id).unwrap().facing < 4, "facing is always a quarter-turn index");
}

#[test]
fn rotating_advances_a_quarter_turn_and_charges_no_wood() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    let id = place_and_finish(&mut state, BuildingKind::Tent, x, y, 0);
    let wood_before = state.stock.wood;

    sim::apply_command(&mut state, 1, &PlayerCommand::RotateBuilding { building: id });

    let b = state.find_building(id).unwrap();
    assert_eq!(b.facing, 1);
    assert_eq!((b.x, b.y), (x, y), "rotating never moves the footprint");
    assert_eq!(state.stock.wood, wood_before, "rotating must never charge wood");
    assert!(b.under_construction(), "rotating re-enters the construction state, like relocating");
}

#[test]
fn rotating_wraps_around_after_four_turns() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    let id = place_and_finish(&mut state, BuildingKind::Tent, x, y, 0);

    for expected in [1, 2, 3, 0] {
        sim::apply_command(&mut state, 1, &PlayerCommand::RotateBuilding { building: id });
        assert_eq!(state.find_building(id).unwrap().facing, expected);
        // Each turn re-enters construction; finish it so the next one is legal.
        sim::finish_all_construction(&mut state);
    }
}

#[test]
fn rotating_preserves_level_and_assigned_workers() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::Greenhouse);
    let id = place_and_finish(&mut state, BuildingKind::Greenhouse, x, y, 0);
    if let Some(b) = state.buildings.iter_mut().find(|b| b.id == id) {
        b.level = 4;
    }
    let survivor = state.survivors[0].id;
    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::AssignSurvivor { survivor, building: Some(id) },
    );

    sim::apply_command(&mut state, 1, &PlayerCommand::RotateBuilding { building: id });

    assert_eq!(state.find_building(id).unwrap().level, 4, "level survives a rotation");
    assert_eq!(
        state.survivors.iter().find(|s| s.id == survivor).unwrap().assigned_building,
        Some(id),
        "named worker assignments survive a rotation"
    );
}

#[test]
fn rotating_finishes_after_the_discounted_workdays() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    let id = place_and_finish(&mut state, BuildingKind::Tent, x, y, 0);
    let full = BuildingKind::Tent.build_workdays();

    sim::apply_command(&mut state, 1, &PlayerCommand::RotateBuilding { building: id });

    let left = state.find_building(id).unwrap().build_left;
    assert!(left > 0.0 && left < full, "a rotation costs less than a fresh build: {left} vs {full}");
}

#[test]
fn rotating_an_under_construction_building_is_refused() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::Place { kind: BuildingKind::Tent, x, y, facing: 0 },
    );
    let id = state.buildings.last().unwrap().id;
    assert!(state.find_building(id).unwrap().under_construction(), "sanity: still a fresh site");

    sim::apply_command(&mut state, 1, &PlayerCommand::RotateBuilding { building: id });

    assert_eq!(state.find_building(id).unwrap().facing, 0, "an unfinished site cannot be turned");
    assert!(state.can_rotate(id).is_err());
}

#[test]
fn rotating_a_non_buildable_kind_is_refused() {
    let state = sim::new_game(SEED, 12);
    let furnace = state.buildings.iter().find(|b| b.kind == BuildingKind::Furnace).unwrap().id;
    let tunnel = state.buildings.iter().find(|b| b.kind == BuildingKind::Tunnel).unwrap().id;

    assert!(state.can_rotate(furnace).is_err(), "the Furnace is a fixture, not a turnable building");
    assert!(state.can_rotate(tunnel).is_err());
}

#[test]
fn rotating_a_missing_building_is_refused() {
    let state = sim::new_game(SEED, 12);
    assert!(state.can_rotate(9_999).is_err());
}

#[test]
fn only_the_owning_account_may_rotate_a_central_building() {
    let mut state = sim::new_game_central(5);
    sim::player_joined_as(&mut state, 10, "Aziz", Some(1));
    sim::player_joined_as(&mut state, 20, "Vali", Some(2));
    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    sim::apply_command(
        &mut state,
        10,
        &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y, facing: 0 },
    );
    let sawmill = state.buildings.iter().find(|b| b.kind == BuildingKind::Sawmill).unwrap().id;
    sim::finish_all_construction(&mut state);

    assert!(
        !state.can_issue(20, &PlayerCommand::RotateBuilding { building: sawmill }),
        "a stranger account must never rotate another account's central building"
    );
    assert!(
        state.can_issue(10, &PlayerCommand::RotateBuilding { building: sawmill }),
        "the owning account may rotate its own central building"
    );
}
