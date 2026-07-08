//! Pure-simulation tests: determinism, invariants, command validation and the
//! wire protocol framing.

use std::io::Cursor;

use frozen_city::game::sim;
use frozen_city::game::types::*;
use frozen_city::net::protocol::{read_frame, write_frame, ClientMsg, ServerMsg};

#[test]
fn mapgen_is_deterministic() {
    let a = sim::new_game(1234, 12);
    let b = sim::new_game(1234, 12);
    assert_eq!(a, b);
    let c = sim::new_game(9999, 12);
    assert_ne!(a.tiles, c.tiles, "different seeds must differ");
}

#[test]
fn mapgen_has_resources_and_clear_center() {
    let state = sim::new_game(7, 12);
    let forest = state
        .tiles
        .iter()
        .filter(|t| t.terrain == Terrain::Forest)
        .count();
    let coal = state
        .tiles
        .iter()
        .filter(|t| t.terrain == Terrain::Coal)
        .count();
    assert!(forest > 50, "expected a real forest, got {forest}");
    assert!(coal > 10, "expected coal deposits, got {coal}");
    // The furnace area itself must stay clear.
    for y in 29..=34u8 {
        for x in 29..=34u8 {
            if GameState::dist_to_furnace(x, y) < 4.0 {
                assert_eq!(state.tile(x, y).terrain, Terrain::Snow);
            }
        }
    }
    assert_eq!(state.buildings.len(), 1);
    assert_eq!(state.buildings[0].kind, BuildingKind::Furnace);
    assert_eq!(state.survivors.len(), 8);
}

#[test]
fn simulation_is_deterministic() {
    let mut a = sim::new_game(42, 12);
    let mut b = sim::new_game(42, 12);
    for _ in 0..2000 {
        sim::tick(&mut a);
        sim::tick(&mut b);
    }
    assert_eq!(a, b);
}

#[test]
fn long_run_invariants_hold() {
    let mut state = sim::new_game(2024, 30);
    for _ in 0..10_000 {
        sim::tick(&mut state);
        assert!(state.stock.wood >= -0.001, "wood went negative");
        assert!(state.stock.coal >= -0.001, "coal went negative");
        assert!(state.stock.food >= -0.001, "food went negative");
        assert!(state.survivors.len() <= 60);
        assert!(
            state.total_workers() <= state.survivors.len() as u32,
            "more workers than people"
        );
        for s in &state.survivors {
            assert!(s.hp > 0.0 && s.hp <= 100.0);
        }
        if state.phase != GamePhase::Running {
            break;
        }
    }
}

#[test]
fn placing_a_tent_costs_wood_and_blocks_the_spot() {
    let mut state = sim::new_game(5, 12);
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    let wood_before = state.stock.wood;

    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y });
    assert_eq!(state.buildings.len(), 2);
    assert!((state.stock.wood - (wood_before - 15.0)).abs() < 0.001);

    // Same spot again: occupied, silently rejected, no wood spent.
    let wood_after = state.stock.wood;
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y });
    assert_eq!(state.buildings.len(), 2);
    assert_eq!(state.stock.wood, wood_after);
}

#[test]
fn cannot_build_without_wood_or_on_bad_terrain() {
    let mut state = sim::new_game(5, 12);
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    state.stock.wood = 3.0;
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y });
    assert_eq!(state.buildings.len(), 1, "no wood -> no tent");

    state.stock.wood = 100.0;
    // A coal mine demands a coal deposit tile.
    assert!(state.can_place(BuildingKind::CoalMine, x, y).is_err());
    // But works on an actual deposit.
    let (cx, cy) = find_spot(&state, BuildingKind::CoalMine);
    assert!(state.can_place(BuildingKind::CoalMine, cx, cy).is_ok());
}

#[test]
fn worker_assignment_is_clamped() {
    let mut state = sim::new_game(5, 12);
    state.stock.wood = 200.0;
    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y });
    let id = state.buildings.last().unwrap().id;

    sim::apply_command(&mut state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: 100 });
    assert_eq!(
        state.find_building(id).unwrap().workers,
        BuildingKind::Sawmill.max_workers(),
        "clamped to the building maximum"
    );
    sim::apply_command(&mut state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: -100 });
    assert_eq!(state.find_building(id).unwrap().workers, 0);
}

#[test]
fn sawmill_produces_wood_and_eats_the_forest() {
    let mut state = sim::new_game(5, 12);
    state.stock.wood = 100.0;
    let (x, y) = find_sawmill_spot_near_forest(&state);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y });
    let id = state.buildings.last().unwrap().id;
    sim::apply_command(&mut state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: 2 });

    let wood_before = state.stock.wood;
    let forest_before = state.forest_near(x, y, 4);
    for _ in 0..(TICKS_PER_DAY as usize) {
        sim::tick(&mut state);
    }
    // Furnace may burn wood if coal runs dry, so compare against production.
    let forest_after = state.forest_near(x, y, 4);
    assert!(
        forest_after < forest_before,
        "forest should be consumed ({forest_before} -> {forest_after})"
    );
    assert!(
        state.stock.wood > wood_before - 40.0,
        "wood income should offset most consumption"
    );
}

#[test]
fn demolish_refunds_and_furnace_is_protected() {
    let mut state = sim::new_game(5, 12);
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y });
    let id = state.buildings.last().unwrap().id;
    let wood = state.stock.wood;
    sim::apply_command(&mut state, 1, &PlayerCommand::Demolish { building: id });
    assert_eq!(state.buildings.len(), 1);
    assert!((state.stock.wood - (wood + 6.0)).abs() < 0.001, "40% of 15");

    sim::apply_command(&mut state, 1, &PlayerCommand::Demolish { building: 0 });
    assert_eq!(state.buildings.len(), 1, "the furnace cannot be demolished");
}

#[test]
fn furnace_goes_out_without_fuel() {
    let mut state = sim::new_game(5, 12);
    state.stock.coal = 0.0;
    state.stock.wood = 0.0;
    assert!(state.furnace_lit);
    sim::tick(&mut state);
    assert!(!state.furnace_lit, "no fuel, no fire");
    assert!(state.heat_radius() == 0.0);
}

#[test]
fn city_survives_to_victory_on_short_run() {
    let mut state = sim::new_game(31337, 1);
    for _ in 0..(TICKS_PER_DAY * 2) {
        sim::tick(&mut state);
        if state.phase == GamePhase::Won {
            break;
        }
    }
    assert_eq!(state.phase, GamePhase::Won);
}

#[test]
fn freezing_city_is_lost() {
    let mut state = sim::new_game(11, 12);
    state.furnace_level = 0;
    state.stock = Stockpile { wood: 0.0, coal: 0.0, food: 0.0 };
    for s in &mut state.survivors {
        s.hp = 0.5;
        s.hunger = 100.0;
    }
    for _ in 0..(TICKS_PER_DAY * 4) {
        sim::tick(&mut state);
        if state.phase == GamePhase::Lost {
            break;
        }
    }
    assert_eq!(state.phase, GamePhase::Lost);
    assert!(state.survivors.is_empty());
}

#[test]
fn commands_after_game_end_are_ignored() {
    let mut state = sim::new_game(5, 12);
    state.phase = GamePhase::Lost;
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y });
    assert_eq!(state.buildings.len(), 1);
}

#[test]
fn protocol_frames_roundtrip() {
    let state = sim::new_game(77, 12);
    let msgs = vec![
        ServerMsg::Welcome { player_id: 3, state: state.clone() },
        ServerMsg::State { state, tiles_included: true },
    ];
    let mut buf = Vec::new();
    for m in &msgs {
        write_frame(&mut buf, m).unwrap();
    }
    let mut cur = Cursor::new(buf);
    for m in &msgs {
        let back: ServerMsg = read_frame(&mut cur).unwrap();
        match (m, &back) {
            (ServerMsg::Welcome { player_id: a, state: sa }, ServerMsg::Welcome { player_id: b, state: sb }) => {
                assert_eq!(a, b);
                assert_eq!(sa, sb);
            }
            (ServerMsg::State { state: sa, .. }, ServerMsg::State { state: sb, .. }) => {
                assert_eq!(sa, sb);
            }
            _ => panic!("message kind mismatch"),
        }
    }

    let cmd = ClientMsg::Cmd(PlayerCommand::Place { kind: BuildingKind::Sawmill, x: 10, y: 20 });
    let mut buf = Vec::new();
    write_frame(&mut buf, &cmd).unwrap();
    let back: ClientMsg = read_frame(&mut Cursor::new(buf)).unwrap();
    match back {
        ClientMsg::Cmd(PlayerCommand::Place { kind, x, y }) => {
            assert_eq!(kind, BuildingKind::Sawmill);
            assert_eq!((x, y), (10, 20));
        }
        _ => panic!("wrong message"),
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

fn find_sawmill_spot_near_forest(state: &GameState) -> (u8, u8) {
    for y in 0..MAP_H as u8 {
        for x in 0..MAP_W as u8 {
            if state.can_place(BuildingKind::Sawmill, x, y).is_ok()
                && state.forest_near(x, y, 4) > 100
            {
                return (x, y);
            }
        }
    }
    panic!("no sawmill spot near forest");
}
