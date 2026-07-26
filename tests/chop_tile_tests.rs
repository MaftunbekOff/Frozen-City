//! `PlayerCommand::ChopTile` — a player manually sending a survivor to chop
//! a specific forest tile, independent of the Furnace-building errand that
//! auto-picks trees (`furnace_bootstrap_tests.rs`). A manual chop credits
//! the stockpile the instant it's chopped (there's no assigned building to
//! carry the log home to) instead of carrying it anywhere.

use frozen_city::game::sim;
use frozen_city::game::types::*;

const SEED: u64 = 909;

fn find_a_forest_tile(state: &GameState) -> (u8, u8) {
    for y in 0..MAP_H as u8 {
        for x in 0..MAP_W as u8 {
            if state.tile(x, y).is_some_and(|t| t.terrain == Terrain::Forest && t.deposit > 0) {
                return (x, y);
            }
        }
    }
    panic!("no forest tile found");
}

#[test]
fn chop_tile_sends_the_survivor_walking_toward_it() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    let survivor = state.survivors[0].id;
    let (tx, ty) = find_a_forest_tile(&state);

    sim::apply_command(&mut state, 1, &PlayerCommand::ChopTile { survivor, x: tx, y: ty });

    let s = state.survivors.iter().find(|s| s.id == survivor).unwrap();
    assert_eq!(s.chop_target, Some((tx, ty)));
    assert_eq!(s.assigned_building, None, "a manual chop unassigns them, same as MoveSurvivor");
    assert!(!s.carrying_wood);
}

#[test]
fn chop_tile_is_a_noop_on_a_non_forest_tile() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    let survivor = state.survivors[0].id;
    // The furnace's own tile is guaranteed clear of forest (mapgen keeps it
    // that way) — a safe "definitely not choppable" target.
    let (fx, fy) = (FURNACE_X, FURNACE_Y);
    assert_ne!(state.tile(fx, fy).map(|t| t.terrain), Some(Terrain::Forest));

    sim::apply_command(&mut state, 1, &PlayerCommand::ChopTile { survivor, x: fx, y: fy });

    let s = state.survivors.iter().find(|s| s.id == survivor).unwrap();
    assert_eq!(s.chop_target, None, "no chop errand should start on a non-forest tile");
}

#[test]
fn chopping_a_manually_targeted_tile_credits_the_stockpile_immediately() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    let survivor = state.survivors[0].id;
    let (tx, ty) = find_a_forest_tile(&state);
    let deposit_before = state.tile(tx, ty).unwrap().deposit;
    let wood_before = state.stock.wood;

    sim::apply_command(&mut state, 1, &PlayerCommand::ChopTile { survivor, x: tx, y: ty });
    for _ in 0..(TICKS_PER_DAY * 2) {
        sim::tick(&mut state);
        let s = state.survivors.iter().find(|s| s.id == survivor).unwrap();
        if s.chop_target.is_none() {
            break; // chopped and done — a manual errand never sets carrying_wood
        }
    }

    let s = state.survivors.iter().find(|s| s.id == survivor).unwrap();
    assert_eq!(s.chop_target, None);
    assert!(!s.carrying_wood, "a manual chop never carries — it credits on the spot");
    assert_eq!(state.stock.wood, wood_before + 1.0, "one manually chopped log");
    assert_eq!(state.tile(tx, ty).unwrap().deposit, deposit_before - 1);
}

#[test]
fn chop_tile_overrides_an_existing_job_assignment() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = {
        let mut spot = None;
        'outer: for y in 0..MAP_H as u8 {
            for x in 0..MAP_W as u8 {
                if state.can_place(BuildingKind::Sawmill, x, y).is_ok() {
                    spot = Some((x, y));
                    break 'outer;
                }
            }
        }
        spot.expect("a valid sawmill spot exists")
    };
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y, facing: 0 });
    let sawmill_id = state.buildings.last().unwrap().id;
    // Finishing drops the auto-crew drafted at placement time to
    // `max_workers()`, but doesn't clear it — drain it so the sole named
    // assignment below has room (same reason `assignment_tests.rs`'s
    // `place()` helper does this).
    sim::finish_all_construction(&mut state);
    let cur = state.find_building(sawmill_id).unwrap().workers as i8;
    sim::apply_command(&mut state, 1, &PlayerCommand::AdjustWorkers { building: sawmill_id, delta: -cur });
    let survivor = state.survivors[0].id;
    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::AssignSurvivor { survivor, building: Some(sawmill_id) },
    );
    assert_eq!(state.find_building(sawmill_id).unwrap().workers, 1);

    let (tx, ty) = find_a_forest_tile(&state);
    sim::apply_command(&mut state, 1, &PlayerCommand::ChopTile { survivor, x: tx, y: ty });

    assert_eq!(
        state.find_building(sawmill_id).unwrap().workers, 0,
        "the sawmill loses its worker to the manual chop errand"
    );
    let s = state.survivors.iter().find(|s| s.id == survivor).unwrap();
    assert_eq!(s.assigned_building, None);
    assert_eq!(s.chop_target, Some((tx, ty)));
}

#[test]
fn chop_tile_pulls_the_leader_off_a_furnace_building_errand() {
    let mut state = sim::new_game(SEED, 12); // real (unbuilt-furnace) bootstrap
    let leader = state.survivors[0].id;
    let furnace_id = state.buildings.iter().find(|b| b.kind == BuildingKind::Furnace).unwrap().id;
    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::AssignSurvivor { survivor: leader, building: Some(furnace_id) },
    );
    sim::tick(&mut state); // let the auto-pick furnace errand start
    assert_eq!(state.survivors[0].assigned_building, Some(furnace_id));

    let (tx, ty) = find_a_forest_tile(&state);
    sim::apply_command(&mut state, 1, &PlayerCommand::ChopTile { survivor: leader, x: tx, y: ty });

    let s = &state.survivors[0];
    assert_eq!(s.assigned_building, None, "the manual command ends the furnace assignment");
    assert_eq!(s.chop_target, Some((tx, ty)), "redirected to the manually chosen tile");
    assert!(state.find_building(furnace_id).unwrap().under_construction(), "still unbuilt");
}
