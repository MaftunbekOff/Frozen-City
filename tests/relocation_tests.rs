//! Pure-simulation tests for V0.14: relocating an already-built building
//! (`PlayerCommand::RelocateBuilding`) — free (no wood), preserves `level`
//! and worker assignments, takes a discounted rebuild time, and needs no
//! save migration (reuses `Building::x`/`y`/`build_left` verbatim). Style
//! mirrors `construction_tests.rs` (the closest sibling: same
//! `build_left`/`under_construction` machinery, just reused for a move
//! instead of a level-up).

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

#[test]
fn relocating_moves_the_building_and_charges_no_wood() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    let id = place_and_finish(&mut state, BuildingKind::Tent, x, y);
    let (tx, ty) = find_spot(&state, BuildingKind::Tent);
    assert_ne!((tx, ty), (x, y), "sanity: find_spot should offer a different tile the second time");
    let wood_before = state.stock.wood;

    sim::apply_command(&mut state, 1, &PlayerCommand::RelocateBuilding { building: id, x: tx, y: ty });

    let b = state.find_building(id).unwrap();
    assert_eq!((b.x, b.y), (tx, ty), "the building should have moved to the target tile");
    assert_eq!(state.stock.wood, wood_before, "relocating must never charge wood");
    assert!(b.under_construction(), "relocating re-enters the construction state");
}

#[test]
fn relocating_preserves_level() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    let id = place_and_finish(&mut state, BuildingKind::Tent, x, y);
    if let Some(b) = state.buildings.iter_mut().find(|b| b.id == id) {
        b.level = 5;
    }

    let (tx, ty) = find_spot(&state, BuildingKind::Tent);
    sim::apply_command(&mut state, 1, &PlayerCommand::RelocateBuilding { building: id, x: tx, y: ty });

    assert_eq!(state.find_building(id).unwrap().level, 5, "relocating must not reset the building's level");
}

#[test]
fn relocation_completes_after_the_discounted_workdays_and_resumes_production() {
    // Greenhouse (flat, uncapped `state.stock.food += amount`, no map-tile
    // dependency) rather than Sawmill — isolates relocation's own timing
    // from an unrelated "is the new spot near forest" confound.
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::Greenhouse);
    let id = place_and_finish(&mut state, BuildingKind::Greenhouse, x, y);
    let cur = state.find_building(id).unwrap().workers as i8;
    sim::apply_command(&mut state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: -cur });
    sim::apply_command(&mut state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: 2 });
    assert_eq!(state.find_building(id).unwrap().workers, 2, "sanity: both slots staffed before relocating");

    let (tx, ty) = find_spot(&state, BuildingKind::Greenhouse);
    sim::apply_command(&mut state, 1, &PlayerCommand::RelocateBuilding { building: id, x: tx, y: ty });
    assert!(state.find_building(id).unwrap().under_construction());

    // A generous tick budget comfortably covers even a discounted-but-real
    // rebuild time; the crew is already staffed so it finishes on its own.
    let mut finished = false;
    for _ in 0..(TICKS_PER_DAY * 5) {
        sim::tick(&mut state);
        if !state.find_building(id).unwrap().under_construction() {
            finished = true;
            break;
        }
    }
    assert!(finished, "relocation should finish well within 5 days");

    let food_before = state.stock.food;
    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut state);
    }
    assert!(
        state.stock.food > food_before + 5.0,
        "a relocated, re-finished Greenhouse should resume producing: before={food_before}, after={}",
        state.stock.food
    );
}

#[test]
fn relocating_an_under_construction_building_is_refused() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y, facing: 0 });
    let id = state.buildings.last().unwrap().id;
    assert!(state.find_building(id).unwrap().under_construction(), "sanity: still a fresh site");

    let (tx, ty) = find_spot(&state, BuildingKind::Tent);
    sim::apply_command(&mut state, 1, &PlayerCommand::RelocateBuilding { building: id, x: tx, y: ty });

    assert_eq!((state.find_building(id).unwrap().x, state.find_building(id).unwrap().y), (x, y));
}

#[test]
fn relocating_onto_an_occupied_tile_is_refused() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (x1, y1) = find_spot(&state, BuildingKind::Tent);
    let id = place_and_finish(&mut state, BuildingKind::Tent, x1, y1);
    let (x2, y2) = find_spot(&state, BuildingKind::Tent);
    place_and_finish(&mut state, BuildingKind::Tent, x2, y2);

    sim::apply_command(&mut state, 1, &PlayerCommand::RelocateBuilding { building: id, x: x2, y: y2 });

    assert_eq!((state.find_building(id).unwrap().x, state.find_building(id).unwrap().y), (x1, y1));
}

#[test]
fn relocating_onto_its_own_current_tile_is_a_harmless_noop_shape() {
    // Not forbidden — `can_relocate` excludes the building's own footprint
    // from the occupancy check — but re-confirms it doesn't spuriously
    // reject or duplicate anything.
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    let id = place_and_finish(&mut state, BuildingKind::Tent, x, y);

    assert!(state.can_relocate(id, x, y).is_ok(), "relocating onto your own current tile must be allowed");
    sim::apply_command(&mut state, 1, &PlayerCommand::RelocateBuilding { building: id, x, y });
    assert_eq!(state.buildings.iter().filter(|b| b.id == id).count(), 1, "no duplicate building");
}

#[test]
fn relocating_a_non_buildable_kind_is_refused() {
    let state = sim::new_game(SEED, 12);
    let furnace = state.buildings.iter().find(|b| b.kind == BuildingKind::Furnace).unwrap().id;
    let tunnel = state.buildings.iter().find(|b| b.kind == BuildingKind::Tunnel).unwrap().id;

    assert!(state.can_relocate(furnace, 10, 10).is_err());
    assert!(state.can_relocate(tunnel, 10, 10).is_err());
}

#[test]
fn relocating_a_coal_mine_requires_a_coal_deposit_at_the_target() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::CoalMine);
    let id = place_and_finish(&mut state, BuildingKind::CoalMine, x, y);

    let (snow_x, snow_y) = find_spot(&state, BuildingKind::Tent);
    assert!(
        state.can_relocate(id, snow_x, snow_y).is_err(),
        "a CoalMine must not be relocatable onto plain snow"
    );

    // The tile it's already standing on obviously still qualifies.
    assert!(state.can_relocate(id, x, y).is_ok());
}

#[test]
fn only_the_owning_account_may_relocate_a_central_building() {
    let mut state = sim::new_game_central(5);
    sim::player_joined_as(&mut state, 10, "Aziz", Some(1));
    sim::player_joined_as(&mut state, 20, "Vali", Some(2));
    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    sim::apply_command(&mut state, 10, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y, facing: 0 });
    let sawmill = state.buildings.iter().find(|b| b.kind == BuildingKind::Sawmill).unwrap().id;
    sim::finish_all_construction(&mut state);
    let (tx, ty) = find_spot(&state, BuildingKind::Sawmill);

    assert!(
        !state.can_issue(20, &PlayerCommand::RelocateBuilding { building: sawmill, x: tx, y: ty }),
        "a stranger account must never relocate another account's central building"
    );
    assert!(
        state.can_issue(10, &PlayerCommand::RelocateBuilding { building: sawmill, x: tx, y: ty }),
        "the owning account may relocate its own central building"
    );
}

