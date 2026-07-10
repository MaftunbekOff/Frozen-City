//! Pure-simulation tests for the V0.3 "Missions & Tunnel" feature: mission
//! progress/completion/rewards, unlocking the Tunnel, and investing in it to
//! win. Mirrors the style of `roles_tests.rs` / `sim_tests.rs`.

use frozen_city::game::sim;
use frozen_city::game::types::*;

#[test]
fn missions_start_incomplete() {
    let state = sim::new_game(5, 12);

    assert_eq!(state.missions.len(), 5, "new_game should seed 5 missions");
    assert!(
        state.missions.iter().all(|m| !m.done),
        "no mission should be done at the start: {:?}",
        state.missions
    );
    assert!(!state.tunnel.unlocked, "the Tunnel must start locked");
    assert_eq!(state.tunnel.stage, 0);
    assert_eq!(state.tunnel.progress, 0.0);
}

#[test]
fn building_tents_completes_the_first_mission_and_grants_reward() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");

    let (x1, y1) = find_spot(&state, BuildingKind::Tent);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x: x1, y: y1 });
    let (x2, y2) = find_spot(&state, BuildingKind::Tent);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x: x2, y: y2 });

    assert_eq!(
        state.mission_current(MissionKind::BuildTents(2)),
        2,
        "two tents should satisfy the BuildTents(2) mission's live progress"
    );

    // Capture wood right before the completing tick: production/consumption
    // also move wood a tiny amount over one tick, so assert the reward landed
    // approximately rather than exactly.
    let wood_before = state.stock.wood;
    sim::tick(&mut state);

    assert!(
        state.missions[0].done,
        "the BuildTents mission should be complete: {:?}",
        state.missions[0]
    );
    assert!(
        state.stock.wood >= wood_before + 19.0,
        "wood should jump by ~20 (the mission reward): before={wood_before}, after={}",
        state.stock.wood
    );
}

#[test]
fn population_mission_completes() {
    let mut state = sim::new_game(5, 12);
    let mut next_id = state.survivors.iter().map(|s| s.id).max().unwrap_or(0) + 1;
    while state.survivors.len() < 10 {
        state.survivors.push(Survivor {
            id: next_id,
            name: "Extra".into(),
            hp: 100.0,
            hunger: 0.0,
            assigned_building: None,
        });
        next_id += 1;
    }
    assert!(state.survivors.len() >= 10);

    sim::tick(&mut state);

    assert!(
        state.missions[1].done,
        "the Population mission should be complete: {:?}",
        state.missions[1]
    );
}

#[test]
fn all_missions_done_unlocks_the_tunnel() {
    let mut state = sim::new_game(5, 12);
    for m in state.missions.iter_mut() {
        m.done = true;
    }

    sim::tick(&mut state);

    assert!(state.tunnel.unlocked, "completing every mission must unlock the Tunnel");
}

#[test]
fn investing_completes_the_tunnel_and_wins() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    state.tunnel.unlocked = true;
    state.stock.wood = 10_000.0;
    state.stock.coal = 10_000.0;

    for _ in 0..(TUNNEL_INVESTS_PER_STAGE * TUNNEL_STAGES as u32) {
        sim::apply_command(&mut state, 1, &PlayerCommand::InvestTunnel);
    }

    assert_eq!(state.tunnel.stage, TUNNEL_STAGES, "all stages should be excavated");
    assert_eq!(state.phase, GamePhase::Won, "completing the Tunnel is a victory condition");
}

#[test]
fn invest_tunnel_is_a_noop_while_locked() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    assert!(!state.tunnel.unlocked, "sanity: Tunnel starts locked");
    state.stock.wood = 10_000.0;
    state.stock.coal = 10_000.0;

    let wood_before = state.stock.wood;
    let coal_before = state.stock.coal;
    sim::apply_command(&mut state, 1, &PlayerCommand::InvestTunnel);

    assert_eq!(state.tunnel.stage, 0);
    assert_eq!(state.tunnel.progress, 0.0);
    assert_eq!(state.stock.wood, wood_before, "no wood spent while locked");
    assert_eq!(state.stock.coal, coal_before, "no coal spent while locked");
}

#[test]
fn guest_may_invest_tunnel_under_build_but_not_view_only() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    sim::player_joined(&mut state, 2, "Guest");
    assert_eq!(state.guest_perm, GuestPermission::Build, "default policy is Build");

    assert!(
        state.can_issue(2, &PlayerCommand::InvestTunnel),
        "guests should be able to pitch in on the shared Tunnel goal under Build"
    );

    sim::set_guest_permission(&mut state, GuestPermission::ViewOnly);
    assert!(
        !state.can_issue(2, &PlayerCommand::InvestTunnel),
        "ViewOnly guests may not issue any world-changing command"
    );
}

#[test]
fn min_win_days_never_pre_empts_the_survive_mission() {
    // Day-based victory returns from tick() before missions are evaluated, so
    // the configurable day-victory floor MUST outlast the longest SurviveDays
    // mission or the Tunnel could be permanently unreachable on that world.
    let state = sim::new_game(1, DEFAULT_WIN_DAYS);
    let max_survive = state
        .missions
        .iter()
        .filter_map(|m| match m.kind {
            MissionKind::SurviveDays(n) => Some(n),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    assert!(
        MIN_WIN_DAYS >= max_survive,
        "MIN_WIN_DAYS ({MIN_WIN_DAYS}) must be >= the longest SurviveDays target ({max_survive})"
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
