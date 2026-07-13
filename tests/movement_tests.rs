//! Pure-simulation tests for V0.7 server-authoritative survivor positions and
//! movement (`MoveSurvivor`). Mirrors the style of `assignment_tests.rs`:
//! build a scenario with `sim::new_game` + `sim::apply_command`/`sim::tick`,
//! then assert on the resulting `GameState`.

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

#[test]
fn new_survivors_spawn_near_the_furnace() {
    let state = sim::new_game(SEED, 12);
    let (fx, fy) = GameState::furnace_center();
    for s in &state.survivors {
        let d = ((s.x - fx).powi(2) + (s.y - fy).powi(2)).sqrt();
        assert!(
            d < 10.0,
            "survivor {} spawned too far from the furnace: ({}, {}) vs furnace ({fx}, {fy})",
            s.id,
            s.x,
            s.y
        );
    }
}

#[test]
fn move_survivor_sets_target_and_unassigns_work() {
    let mut state = sim::new_game(SEED, 12);
    let sawmill = {
        state.stock.wood = 500.0;
        let (x, y) = find_spot(&state, BuildingKind::Sawmill);
        sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y });
        state.buildings.last().unwrap().id
    };
    // V0.8: maydonchani bitirib, avto-brigadani bo'shatamiz — test bitgan
    // binodagi yagona nomlangan slotni kuzatadi.
    sim::finish_all_construction(&mut state);
    let cur = state.find_building(sawmill).unwrap().workers as i8;
    if cur > 0 {
        sim::apply_command(&mut state, 1, &PlayerCommand::AdjustWorkers { building: sawmill, delta: -cur });
    }
    let survivor = state.survivors[0].id;
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor, building: Some(sawmill) });
    assert_eq!(state.find_building(sawmill).unwrap().workers, 1);

    sim::apply_command(&mut state, 1, &PlayerCommand::MoveSurvivor { survivor, x: 5, y: 5 });

    let s = state.survivors.iter().find(|s| s.id == survivor).unwrap();
    assert_eq!(s.move_target, Some((5, 5)), "move target should be recorded");
    assert_eq!(s.assigned_building, None, "a manually walked survivor becomes idle");
    assert_eq!(
        state.find_building(sawmill).unwrap().workers,
        0,
        "the vacated work slot must be freed"
    );
}

#[test]
fn move_survivor_rejects_out_of_bounds_and_unknown_ids() {
    let mut state = sim::new_game(SEED, 12);
    let survivor = state.survivors[0].id;

    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::MoveSurvivor { survivor, x: MAP_W as u8, y: 0 },
    );
    assert_eq!(
        state.survivors.iter().find(|s| s.id == survivor).unwrap().move_target,
        None,
        "an out-of-bounds target must be rejected"
    );

    sim::apply_command(&mut state, 1, &PlayerCommand::MoveSurvivor { survivor: 99999, x: 1, y: 1 });
    // No panic, no stray state — nothing to assert beyond "it didn't crash".
}

#[test]
fn survivor_walks_toward_move_target_and_arrives() {
    let mut state = sim::new_game(SEED, 12);
    let survivor_id = state.survivors[0].id;
    // Pin a known starting position close to the target so arrival happens
    // within a bounded number of ticks.
    {
        let s = state.survivors.iter_mut().find(|s| s.id == survivor_id).unwrap();
        s.x = 10.0;
        s.y = 10.0;
    }
    sim::apply_command(&mut state, 1, &PlayerCommand::MoveSurvivor { survivor: survivor_id, x: 15, y: 10 });

    let mut arrived = false;
    for _ in 0..500 {
        sim::tick(&mut state);
        let s = state.survivors.iter().find(|s| s.id == survivor_id).unwrap();
        if s.move_target.is_none() {
            arrived = true;
            // Snapped to the goal tile's center.
            assert!((s.x - 15.5).abs() < 0.01, "x should snap to the goal: got {}", s.x);
            assert!((s.y - 10.5).abs() < 0.01, "y should snap to the goal: got {}", s.y);
            break;
        }
    }
    assert!(arrived, "the survivor should have reached (15, 10) within 500 ticks");
}

#[test]
fn survivor_moves_at_the_constant_configured_speed() {
    let mut state = sim::new_game(SEED, 12);
    let survivor_id = state.survivors[0].id;
    {
        let s = state.survivors.iter_mut().find(|s| s.id == survivor_id).unwrap();
        s.x = 0.5;
        s.y = 0.5;
    }
    // Far enough away that one tick's movement is a pure straight-line step,
    // not an arrival snap.
    sim::apply_command(&mut state, 1, &PlayerCommand::MoveSurvivor { survivor: survivor_id, x: 40, y: 0 });
    sim::tick(&mut state);

    let s = state.survivors.iter().find(|s| s.id == survivor_id).unwrap();
    let moved = s.x - 0.5;
    assert!(
        (moved - SURVIVOR_SPEED_PER_TICK).abs() < 1e-4,
        "expected exactly one tick's worth of movement ({SURVIVOR_SPEED_PER_TICK}), got {moved}"
    );
    assert!((s.y - 0.5).abs() < 1e-4, "straight line: y should not move when the goal is due east");
}

#[test]
fn idle_survivor_with_no_target_or_job_stays_put() {
    let mut state = sim::new_game(SEED, 12);
    let survivor_id = state.survivors[0].id;
    let (x0, y0) = {
        let s = state.survivors.iter().find(|s| s.id == survivor_id).unwrap();
        (s.x, s.y)
    };
    for _ in 0..20 {
        sim::tick(&mut state);
    }
    let s = state.survivors.iter().find(|s| s.id == survivor_id).unwrap();
    assert_eq!((s.x, s.y), (x0, y0), "an unassigned, unmoved survivor should not drift");
}

#[test]
fn assigned_survivor_walks_toward_their_building() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (bx, by) = find_spot(&state, BuildingKind::Sawmill);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x: bx, y: by });
    let sawmill = state.buildings.last().unwrap().id;
    // V0.8: bitirib, avto-brigadani bo'shatamiz — nomlangan biriktirma uchun
    // joy ochilsin.
    sim::finish_all_construction(&mut state);
    {
        let cur = state.find_building(sawmill).unwrap().workers as i8;
        if cur > 0 {
            sim::apply_command(&mut state, 1, &PlayerCommand::AdjustWorkers { building: sawmill, delta: -cur });
        }
    }
    let survivor_id = state.survivors[0].id;
    // Start far from the building so the walk takes multiple ticks.
    {
        let s = state.survivors.iter_mut().find(|s| s.id == survivor_id).unwrap();
        s.x = 0.5;
        s.y = 0.5;
    }
    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::AssignSurvivor { survivor: survivor_id, building: Some(sawmill) },
    );

    let goal = (bx as f32 + 0.5, by as f32 + 0.5);
    let mut arrived = false;
    for _ in 0..2000 {
        sim::tick(&mut state);
        let s = state.survivors.iter().find(|s| s.id == survivor_id).unwrap();
        if (s.x - goal.0).abs() < 0.01 && (s.y - goal.1).abs() < 0.01 {
            arrived = true;
            break;
        }
    }
    assert!(arrived, "an assigned survivor should walk to their building's location");
}
