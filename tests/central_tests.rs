//! Pure-simulation tests for the central world (the Global World through the
//! Tunnel): settler migration in/out, per-account command authority, and the
//! "permanent meeting place" tick rules (no hunger, no death, no win/lose).
//! Mirrors the style of `roles_tests.rs` / `mission_tests.rs`.

use frozen_city::game::sim;
use frozen_city::game::types::*;

#[test]
fn central_world_starts_empty_and_flagged() {
    let state = sim::new_game_central(5);

    assert!(state.central);
    assert!(state.survivors.is_empty(), "settlers only arrive through the Tunnel");
    assert!(state.missions.is_empty(), "no personal-world progression in the hub");
    assert!(!state.tunnel.unlocked);
    assert_eq!(state.phase, GamePhase::Running);
}

#[test]
fn empty_central_world_never_loses_or_wins_by_days() {
    let mut state = sim::new_game_central(5);

    // One tick with zero survivors would flag an ordinary world Lost.
    sim::tick(&mut state);
    assert_eq!(state.phase, GamePhase::Running, "an empty hub must not be a defeat");

    // Jump to just before the day-count victory would fire and cross it.
    state.tick = state.win_days as u64 * TICKS_PER_DAY - 1;
    sim::tick(&mut state);
    sim::tick(&mut state);
    assert_eq!(state.phase, GamePhase::Running, "no day-count victory in the hub");
}

#[test]
fn central_settlers_never_hunger_or_die() {
    let mut state = sim::new_game_central(5);
    let migrants = vec![Survivor {
        id: 1,
        name: "Anna".into(),
        hp: 1.0, // would freeze/starve to death in one bad day anywhere else
        hunger: 119.0,
        assigned_building: None,
        owner: None,
        x: 0.0,
        y: 0.0,
        move_target: None,
        profession: Profession::from_id_hash(1),
        xp: 0.0,
        trained_kind: None,
        chop_target: None,
        carrying_wood: false,
        thirst: 119.0, // would die of thirst in one bad day anywhere else
        bury_target: None,
        fatigue: 0.0,
        sick_left: 0.0,
        age_days: 30.0,
        partner: None,
    }];
    sim::inject_migrants(&mut state, 42, "Aziz", migrants);
    state.stock.food = 0.0;
    state.furnace_level = 0; // no heat at all

    for _ in 0..(2 * TICKS_PER_DAY) {
        sim::tick(&mut state);
    }

    assert_eq!(state.survivors.len(), 1, "settlers are a presence, not mouths to feed");
    let s = &state.survivors[0];
    assert_eq!(s.hp, 1.0, "hp must not drift in the hub");
    assert_eq!(s.hunger, 119.0, "hunger must not drift in the hub");
    assert_eq!(s.thirst, 119.0, "thirst must not drift in the hub");
}

#[test]
fn inject_reids_sets_owner_and_caps_per_account() {
    let mut state = sim::new_game_central(5);
    let make = |id: u32| Survivor {
        id,
        name: format!("S{id}"),
        hp: 90.0,
        hunger: 10.0,
        assigned_building: Some(7), // stale reference from the source world
        owner: None,
        x: 0.0,
        y: 0.0,
        move_target: None,
        profession: Profession::from_id_hash(id),
        xp: 0.0,
        trained_kind: None,
        chop_target: None,
        carrying_wood: false,
        thirst: 10.0,
        bury_target: None,
        fatigue: 0.0,
        sick_left: 0.0,
        age_days: 30.0,
        partner: None,
    };

    // Two batches from "different personal worlds" with clashing ids.
    let settled = sim::inject_migrants(&mut state, 1, "Aziz", vec![make(1), make(2), make(3)]);
    assert_eq!(settled, 3);
    let settled = sim::inject_migrants(&mut state, 2, "Vali", vec![make(1), make(2)]);
    assert_eq!(settled, 2);

    let ids: Vec<u32> = state.survivors.iter().map(|s| s.id).collect();
    let mut deduped = ids.clone();
    deduped.sort_unstable();
    deduped.dedup();
    assert_eq!(ids.len(), deduped.len(), "settler ids must be re-issued uniquely: {ids:?}");
    assert!(
        state.survivors.iter().all(|s| s.assigned_building.is_none()),
        "stale building assignments from the source world must be cleared"
    );
    assert_eq!(state.owned_settlers(1), 3);
    assert_eq!(state.owned_settlers(2), 2);

    // Overfilling one account stops at the cap.
    let over: Vec<Survivor> = (10..20).map(make).collect();
    let settled = sim::inject_migrants(&mut state, 1, "Aziz", over);
    assert_eq!(
        state.owned_settlers(1),
        CENTRAL_MIGRANTS_PER_ACCOUNT,
        "an account may never exceed the settler cap"
    );
    assert_eq!(settled, CENTRAL_MIGRANTS_PER_ACCOUNT - 3);
}

#[test]
fn extract_prefers_idle_and_frees_staffed_slots() {
    // Needs several idle survivors alongside the two pinned to the sawmill
    // (extract 2 idle without touching them, then drain the rest) — that's
    // the bootstrapped population, not the 1-survivor furnace-building
    // opening.
    let mut state = sim::new_game_bootstrapped(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    let start_pop = state.survivors.len();
    assert!(start_pop >= 5, "sanity: default start population needs room for 2 pinned + idle");

    // Stand up a sawmill and pin it to its `max_workers()` (2), full up.
    // (V0.8: joylash maydoncha ochadi — bu test bitgan binoning slotlarini
    // sinaydi, shuning uchun darhol bitirib, avto-brigadani bo'shatamiz.)
    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y, facing: 0 });
    let sawmill = state.buildings.iter().find(|b| b.kind == BuildingKind::Sawmill).unwrap().id;
    sim::finish_all_construction(&mut state);
    let cur = state.find_building(sawmill).unwrap().workers as i8;
    if cur > 0 {
        sim::apply_command(&mut state, 1, &PlayerCommand::AdjustWorkers { building: sawmill, delta: -cur });
    }
    // Two pinned, not one: with only one assigned survivor, the population
    // floor (`extract_migrants` never drains below 1) always leaves exactly
    // enough idle survivors to satisfy any request on its own — the single
    // pinned one would never actually need to be reached. Two pinned makes
    // the fallback-into-the-assigned-pool path unavoidable once idle runs
    // low, so it's still exercised under the new floor.
    // Pin the LAST two survivors, not the first: V0.16 extracts the leader
    // (the starting leader is `survivors[0]`, see `mapgen`) first, so pinning
    // the leader here would collide with that. Leaving the leader idle keeps
    // this test focused on the idle-before-assigned preference it exists for.
    let pinned: Vec<u32> = state.survivors.iter().rev().take(2).map(|s| s.id).collect();
    for &id in &pinned {
        sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor: id, building: Some(sawmill) });
    }
    assert_eq!(state.find_building(sawmill).unwrap().workers, 2);

    // Taking fewer than the idle pool must not touch either pinned worker.
    let taken = sim::extract_migrants(&mut state, 2);
    assert_eq!(taken.len(), 2);
    assert!(taken.iter().all(|s| !pinned.contains(&s.id)), "idle survivors go first");
    assert_eq!(state.find_building(sawmill).unwrap().workers, 2);

    // Draining "everyone" now must (a) fall back to the pinned pool once
    // idle runs out, freeing at least one staffed slot, and (b) still never
    // fully empty the world — one survivor always stays behind.
    let remaining_before = state.survivors.len();
    let rest = sim::extract_migrants(&mut state, 1000);
    assert_eq!(rest.len(), remaining_before - 1, "leaves exactly one survivor behind");
    assert_eq!(state.survivors.len(), 1, "extract_migrants never empties a personal world");
    assert!(
        state.find_building(sawmill).unwrap().workers < 2,
        "at least one migrating worker must free their staffed slot"
    );
    assert!(
        rest.iter().any(|s| pinned.contains(&s.id)),
        "falls back to the assigned pool once idle runs out"
    );
    assert!(rest.iter().all(|s| s.assigned_building.is_none()));
}

#[test]
fn extract_takes_the_leader_first_so_they_cross_with_the_player() {
    // V0.16: crossing to the Global World brings the player's leader along.
    let mut state = sim::new_game_bootstrapped(7, 12);
    sim::player_joined(&mut state, 1, "Owner");

    // Appoint an ASSIGNED survivor as leader: assigned survivors are normally
    // extracted only after the whole idle pool, so this proves the leader
    // jumps the queue rather than merely happening to be idle.
    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y, facing: 0 });
    let sawmill = state.buildings.iter().find(|b| b.kind == BuildingKind::Sawmill).unwrap().id;
    sim::finish_all_construction(&mut state);
    let boss = state.survivors.last().unwrap().id;
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor: boss, building: Some(sawmill) });
    sim::apply_command(&mut state, 1, &PlayerCommand::SetLeader { survivor: boss });
    assert_eq!(state.leader, Some(boss));

    let taken = sim::extract_migrants(&mut state, 1);
    assert_eq!(taken.len(), 1);
    assert_eq!(taken[0].id, boss, "the leader heads the migration group");
    assert_eq!(state.leader, None, "the leader vacated the personal city on crossing");
}

#[test]
fn central_authority_follows_settler_ownership() {
    let mut state = sim::new_game_central(5);
    // Aziz (account 1) and Vali (account 2), each with one settler.
    sim::player_joined_as(&mut state, 10, "Aziz", Some(1));
    sim::player_joined_as(&mut state, 20, "Vali", Some(2));
    let mk = |id: u32| Survivor {
        id,
        name: format!("S{id}"),
        hp: 90.0,
        hunger: 10.0,
        assigned_building: None,
        owner: None,
        x: 0.0,
        y: 0.0,
        move_target: None,
        profession: Profession::from_id_hash(id),
        xp: 0.0,
        trained_kind: None,
        chop_target: None,
        carrying_wood: false,
        thirst: 10.0,
        bury_target: None,
        fatigue: 0.0,
        sick_left: 0.0,
        age_days: 30.0,
        partner: None,
    };
    sim::inject_migrants(&mut state, 1, "Aziz", vec![mk(1)]);
    sim::inject_migrants(&mut state, 2, "Vali", vec![mk(2)]);
    let azizs = state.survivors.iter().find(|s| s.owner == Some(1)).unwrap().id;
    let valis = state.survivors.iter().find(|s| s.owner == Some(2)).unwrap().id;

    // Nobody is Owner in the hub — not even the first joiner.
    assert!(state.owner_id.is_none());
    assert!(state.players.iter().all(|p| p.role == Role::Guest));

    let assign_own = PlayerCommand::AssignSurvivor { survivor: azizs, building: None };
    let assign_foreign = PlayerCommand::AssignSurvivor { survivor: valis, building: None };
    assert!(state.can_issue(10, &assign_own), "your settler: allowed");
    assert!(!state.can_issue(10, &assign_foreign), "someone else's settler: denied");
    assert!(state.can_issue(20, &assign_foreign), "...but their owner may");

    assert!(
        !state.can_issue(10, &PlayerCommand::AdjustWorkers { building: 0, delta: 1 }),
        "the anonymous worker pool has no meaning where every settler is owned"
    );
    assert!(!state.can_issue(10, &PlayerCommand::SetFurnaceLevel { level: 3 }));
    assert!(!state.can_issue(10, &PlayerCommand::InvestTunnel));
    assert!(!state.can_issue(10, &PlayerCommand::Research { tech: Tech::Tools }));
    assert!(!state.can_issue(10, &PlayerCommand::RespondEvent { accept: true }));
    assert!(state.can_issue(10, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x: 1, y: 1, facing: 0 }));

    // And the full apply path respects it: Vali's settler stays put when Aziz
    // tries to move him onto a building.
    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    sim::apply_command(&mut state, 10, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y, facing: 0 });
    let sawmill = state.buildings.iter().find(|b| b.kind == BuildingKind::Sawmill).unwrap().id;
    sim::apply_command(
        &mut state,
        10,
        &PlayerCommand::AssignSurvivor { survivor: valis, building: Some(sawmill) },
    );
    assert_eq!(
        state.survivors.iter().find(|s| s.id == valis).unwrap().assigned_building,
        None,
        "a foreign assignment attempt must be a no-op"
    );
    sim::apply_command(
        &mut state,
        10,
        &PlayerCommand::AssignSurvivor { survivor: azizs, building: Some(sawmill) },
    );
    assert_eq!(
        state.survivors.iter().find(|s| s.id == azizs).unwrap().assigned_building,
        Some(sawmill),
        "your own settler assignment must work"
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
