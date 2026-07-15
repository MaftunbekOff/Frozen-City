//! Pure-simulation tests for V0.11: population lifecycle (birth, death,
//! corpses/graves, player-initiated burial), profession-gating (a mismatched
//! survivor is merely inefficient EXCEPT at a few skill-gated buildings like
//! Hospital, where they're nearly useless), and the thirst/water resource
//! (Well production, Kitchen/Rationing water discounts, death by thirst).
//! Style mirrors `wildlife_tests.rs` / `building_tests.rs` / `tech_tests.rs`:
//! control-vs-experiment comparisons ticked in lockstep, plus direct
//! `state.events`-log assertions for one-shot occurrences (mirrors
//! `furnace_bootstrap_tests.rs`'s arrival tests).

use frozen_city::game::sim;
use frozen_city::game::types::*;

const SEED: u64 = 12345;

/// Places `kind` at `(x, y)`, instantly finishes its construction and staffs
/// it to exactly `workers`, returning the new building's id. Mirrors
/// `building_tests.rs`'s helper of the same name.
fn place_and_staff(state: &mut GameState, kind: BuildingKind, x: u8, y: u8, workers: i8) -> u32 {
    sim::apply_command(state, 1, &PlayerCommand::Place { kind, x, y });
    let id = state.buildings.last().unwrap().id;
    sim::finish_all_construction(state);
    let cur = state.find_building(id).unwrap().workers as i8;
    sim::apply_command(state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: workers - cur });
    id
}

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

// --- Well / water economy ---

#[test]
fn staffed_well_produces_water() {
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);
    for state in [&mut control, &mut experiment] {
        state.stock.wood = 500.0;
    }

    let (x, y) = find_spot(&experiment, BuildingKind::Well);
    place_and_staff(&mut experiment, BuildingKind::Well, x, y, 2);

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    // V0.11: a fresh world starts with a real (`START_WATER`) buffer, drawn
    // down by ordinary drinking — so "no Well" means the control's water can
    // only ever drift downward from that starting point, never up.
    assert!(
        control.stock.water <= 30.0,
        "a world with no Well should never gain water: got {}",
        control.stock.water
    );
    assert!(
        experiment.stock.water > control.stock.water,
        "a staffed Well should produce more water than the unstaffed control: \
         control={}, experiment={}",
        control.stock.water,
        experiment.stock.water
    );
}

#[test]
fn staffed_kitchen_reduces_water_consumption() {
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);

    // Plenty of water, and neither run builds a Well, so the only thing
    // moving the stockpile is survivor drinking.
    control.stock.water = 1000.0;
    experiment.stock.water = 1000.0;
    experiment.stock.wood = 500.0;

    let (x, y) = find_spot(&experiment, BuildingKind::Kitchen);
    place_and_staff(&mut experiment, BuildingKind::Kitchen, x, y, 1);

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        experiment.stock.water > control.stock.water,
        "a staffed kitchen (0.75x water consumption) should leave more water than the control: \
         control={}, experiment={}",
        control.stock.water,
        experiment.stock.water
    );
}

#[test]
fn rationing_reduces_water_consumption() {
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);
    experiment.techs.push(Tech::Rationing);

    control.stock.water = 1000.0;
    experiment.stock.water = 1000.0;

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        experiment.stock.water > control.stock.water,
        "Rationing should also discount water consumption, not just food: control={}, experiment={}",
        control.stock.water,
        experiment.stock.water
    );
}

#[test]
fn severe_thirst_can_kill_and_is_recorded_as_the_cause() {
    let mut control = sim::new_game_bootstrapped(SEED, 30);
    let mut experiment = sim::new_game_bootstrapped(SEED, 30);
    control.stock.water = 9999.0;
    experiment.stock.water = 0.0;
    let starting_pop = experiment.survivors.len();

    for _ in 0..(TICKS_PER_DAY * 4) {
        // Reset food every tick in both runs so hunger can never be the
        // killer here — isolates thirst as the one lethal factor under test.
        control.stock.food = 9999.0;
        experiment.stock.food = 9999.0;
        control.stock.water = 9999.0;
        sim::tick(&mut control);
        sim::tick(&mut experiment);
        if experiment.survivors.len() < starting_pop {
            break;
        }
    }

    assert_eq!(
        control.survivors.len(),
        starting_pop,
        "the abundant-water control should lose nobody over the same run"
    );
    assert!(
        experiment.survivors.len() < starting_pop,
        "prolonged zero water should eventually kill someone"
    );
    assert!(
        experiment.events.iter().any(|e| e.text.contains("thirst")),
        "the death should be attributed to thirst: {:?}",
        experiment.events
    );
}

// --- Profession-gating ---

#[test]
fn skilled_mismatch_penalty_makes_a_non_medic_hospital_worker_much_less_effective() {
    let mut anonymous = sim::new_game_bootstrapped(SEED, 12);
    let mut named_mismatch = sim::new_game_bootstrapped(SEED, 12);

    for state in [&mut anonymous, &mut named_mismatch] {
        for s in &mut state.survivors {
            s.hp = 40.0;
        }
        state.stock.wood = 500.0;
    }

    let (x, y) = find_spot(&anonymous, BuildingKind::Hospital);
    place_and_staff(&mut anonymous, BuildingKind::Hospital, x, y, 2);

    let (x, y) = find_spot(&named_mismatch, BuildingKind::Hospital);
    let id = place_and_staff(&mut named_mismatch, BuildingKind::Hospital, x, y, 0);
    // Name two specific, deliberately non-Medic, non-leader survivors into
    // the two slots — the leader's universal profession-match bypass would
    // otherwise confound the comparison, and `is_skilled_at` only grants
    // Medic/Hospital, so both named workers should collapse to
    // `SKILLED_MISMATCH_PENALTY` instead of the anonymous pool's neutral 1.0x.
    let leader_id = named_mismatch.leader;
    let workers: Vec<u32> = named_mismatch
        .survivors
        .iter()
        .filter(|s| s.profession != Profession::Medic && Some(s.id) != leader_id)
        .take(2)
        .map(|s| s.id)
        .collect();
    assert_eq!(workers.len(), 2, "bootstrapped world should have at least 2 non-Medic, non-leader survivors");
    for w in &workers {
        named_mismatch.survivors.iter_mut().find(|s| s.id == *w).unwrap().profession = Profession::Lumberjack;
        sim::apply_command(
            &mut named_mismatch,
            1,
            &PlayerCommand::AssignSurvivor { survivor: *w, building: Some(id) },
        );
    }
    assert_eq!(named_mismatch.find_building(id).unwrap().workers, 2);

    for _ in 0..(TICKS_PER_DAY / 2) {
        sim::tick(&mut anonymous);
        sim::tick(&mut named_mismatch);
    }

    let anonymous_hp: f32 = anonymous.survivors.iter().map(|s| s.hp).sum();
    let mismatch_hp: f32 = named_mismatch.survivors.iter().map(|s| s.hp).sum();
    assert!(
        anonymous_hp > mismatch_hp + 1.0,
        "two anonymous Hospital workers (1.0x each) should out-heal two named, skill-gated \
         mismatched specialists (0.1x each): anonymous={anonymous_hp}, mismatch={mismatch_hp}"
    );
}

#[test]
fn matching_medic_outperforms_a_skill_gated_mismatch_at_the_hospital() {
    let mut medic_world = sim::new_game_bootstrapped(SEED, 12);
    let mut mismatch_world = sim::new_game_bootstrapped(SEED, 12);

    for state in [&mut medic_world, &mut mismatch_world] {
        for s in &mut state.survivors {
            s.hp = 40.0;
        }
        state.stock.wood = 500.0;
    }

    let leader_id = medic_world.leader;
    let (x, y) = find_spot(&medic_world, BuildingKind::Hospital);
    let id = place_and_staff(&mut medic_world, BuildingKind::Hospital, x, y, 0);
    let medic = medic_world
        .survivors
        .iter()
        .find(|s| Some(s.id) != leader_id)
        .expect("bootstrapped world has more than one survivor")
        .id;
    medic_world.survivors.iter_mut().find(|s| s.id == medic).unwrap().profession = Profession::Medic;
    sim::apply_command(&mut medic_world, 1, &PlayerCommand::AssignSurvivor { survivor: medic, building: Some(id) });

    let leader_id = mismatch_world.leader;
    let (x, y) = find_spot(&mismatch_world, BuildingKind::Hospital);
    let id = place_and_staff(&mut mismatch_world, BuildingKind::Hospital, x, y, 0);
    let non_medic = mismatch_world
        .survivors
        .iter()
        .find(|s| Some(s.id) != leader_id)
        .expect("bootstrapped world has more than one survivor")
        .id;
    mismatch_world.survivors.iter_mut().find(|s| s.id == non_medic).unwrap().profession = Profession::Lumberjack;
    sim::apply_command(
        &mut mismatch_world,
        1,
        &PlayerCommand::AssignSurvivor { survivor: non_medic, building: Some(id) },
    );

    for _ in 0..(TICKS_PER_DAY / 2) {
        sim::tick(&mut medic_world);
        sim::tick(&mut mismatch_world);
    }

    let medic_hp: f32 = medic_world.survivors.iter().map(|s| s.hp).sum();
    let mismatch_hp: f32 = mismatch_world.survivors.iter().map(|s| s.hp).sum();
    assert!(
        medic_hp > mismatch_hp,
        "a named Medic (1.5x match bonus) should out-heal a skill-gated mismatch (0.1x): \
         medic={medic_hp}, mismatch={mismatch_hp}"
    );
}

// --- Death, corpses, burial, graves ---

#[test]
fn dying_survivor_leaves_a_corpse_at_their_position() {
    let mut state = sim::new_game(SEED, 12);
    let id = state.survivors[0].id;
    let name = state.survivors[0].name.clone();
    let (x, y) = (state.survivors[0].x, state.survivors[0].y);
    state.survivors[0].hp = 0.0;

    sim::tick(&mut state);

    assert!(state.survivors.iter().all(|s| s.id != id), "the dead survivor should be removed from the roster");
    assert_eq!(state.corpses.len(), 1, "a corpse should be left behind: {:?}", state.corpses);
    let corpse = &state.corpses[0];
    assert_eq!(corpse.id, id);
    assert_eq!(corpse.name, name);
    assert_eq!((corpse.x, corpse.y), (x, y));
    assert!(corpse.being_buried_by.is_none());
}

#[test]
fn bury_command_claims_a_corpse_and_a_second_claim_is_rejected() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    let dead_id = state.survivors[0].id;
    state.survivors[0].hp = 0.0;
    sim::tick(&mut state);
    let corpse_id = state.corpses[0].id;
    assert_eq!(corpse_id, dead_id);

    let buryer = state.survivors[0].id;
    let other = state.survivors[1].id;
    sim::apply_command(&mut state, 1, &PlayerCommand::Bury { survivor: buryer, corpse: corpse_id });

    let s = state.survivors.iter().find(|s| s.id == buryer).unwrap();
    assert_eq!(s.bury_target, Some(corpse_id));
    assert_eq!(state.corpses[0].being_buried_by, Some(buryer));

    // A second player can't pile onto the same body.
    sim::apply_command(&mut state, 1, &PlayerCommand::Bury { survivor: other, corpse: corpse_id });
    let other_s = state.survivors.iter().find(|s| s.id == other).unwrap();
    assert_eq!(other_s.bury_target, None, "a corpse already claimed must reject a second buryer");
    assert_eq!(state.corpses[0].being_buried_by, Some(buryer), "the original claim should be unchanged");
}

#[test]
fn burial_completes_removes_corpse_leaves_a_grave_and_frees_the_survivor() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.survivors[0].hp = 0.0;
    sim::tick(&mut state);
    let corpse_id = state.corpses[0].id;
    let (cx, cy) = (state.corpses[0].x, state.corpses[0].y);

    let buryer = state.survivors[0].id;
    // Stand the buryer right on top of the corpse so the walk-there step is
    // instant — isolates the burial-progress mechanic from movement/pathing.
    state.survivors.iter_mut().find(|s| s.id == buryer).unwrap().x = cx;
    state.survivors.iter_mut().find(|s| s.id == buryer).unwrap().y = cy;
    sim::apply_command(&mut state, 1, &PlayerCommand::Bury { survivor: buryer, corpse: corpse_id });

    assert!(state.graves.is_empty(), "no grave yet — burial just started");
    // BURY_DURATION_TICKS worth of standing-there ticks completes it; a
    // couple of extra ticks covers the arrival tick itself.
    for _ in 0..(BURY_DURATION_TICKS + 2) {
        sim::tick(&mut state);
    }

    assert!(state.corpses.is_empty(), "the buried corpse should be gone: {:?}", state.corpses);
    assert_eq!(state.graves.len(), 1, "a grave should be left where the burial completed: {:?}", state.graves);
    assert_eq!((state.graves[0].x, state.graves[0].y), (cx, cy));
    let s = state.survivors.iter().find(|s| s.id == buryer).unwrap();
    assert_eq!(s.bury_target, None, "the buryer should be freed once burial completes");
    assert!(
        state.events.iter().any(|e| e.text.contains("laid to rest")),
        "a burial-complete event should be logged: {:?}",
        state.events
    );
}

#[test]
fn unburied_corpse_decays_into_a_grave() {
    // A bootstrapped (8-survivor) world, not the default lone-survivor one —
    // killing the ONLY survivor would empty the roster and immediately end
    // the game (`GamePhase::Lost`), which freezes `tick()` into a no-op
    // before decay ever gets a chance to run.
    let mut state = sim::new_game_bootstrapped(SEED, 30);
    state.survivors[0].hp = 0.0;
    sim::tick(&mut state);
    assert_eq!(state.corpses.len(), 1, "sanity: a corpse exists after death");
    let (cx, cy) = (state.corpses[0].x, state.corpses[0].y);

    // Nobody ever buries it — left alone, it should fade into a Grave on its
    // own once `CORPSE_DECAY_TICKS` have passed since death. Food/water are
    // topped up every tick so the rest of the colony survives the wait
    // uneventfully — this test is about decay timing, not survival.
    for _ in 0..(CORPSE_DECAY_TICKS + 2) {
        state.stock.food = 999.0;
        state.stock.water = 999.0;
        sim::tick(&mut state);
    }

    assert!(state.corpses.is_empty(), "an unburied corpse should decay away: {:?}", state.corpses);
    assert_eq!(state.graves.len(), 1, "decay should leave a grave: {:?}", state.graves);
    assert_eq!((state.graves[0].x, state.graves[0].y), (cx, cy));
    assert!(
        state.events.iter().any(|e| e.text.contains("faded into the snow")),
        "a decay event should be logged: {:?}",
        state.events
    );
}

#[test]
fn grave_fades_after_grave_fade_ticks() {
    // Bootstrapped + topped-up resources for the same reason as
    // `unburied_corpse_decays_into_a_grave`: a starved-out or frozen-out
    // population would flip `GamePhase` away from `Running` and freeze
    // `tick()`'s no-op guard long before `GRAVE_FADE_TICKS` (3 in-game days)
    // elapses.
    let mut state = sim::new_game_bootstrapped(SEED, 30);
    state.graves.push(Grave { x: 5.0, y: 5.0, created_tick: state.tick });

    for _ in 0..(GRAVE_FADE_TICKS - 1) {
        state.stock.food = 999.0;
        state.stock.water = 999.0;
        sim::tick(&mut state);
        assert_eq!(state.graves.len(), 1, "the grave should still be visible before its fade deadline");
    }
    state.stock.food = 999.0;
    state.stock.water = 999.0;
    sim::tick(&mut state);

    assert!(state.graves.is_empty(), "the grave should have faded by now: {:?}", state.graves);
}

// --- Births ---

#[test]
fn birth_can_occur_once_the_grace_period_and_thresholds_are_met() {
    let mut state = sim::new_game_bootstrapped(SEED, 90);
    state.stock.wood = 500.0;
    // 8 bootstrapped survivors need real housing before either arrivals or
    // births have any `space` to grow into (`housing_capacity() + 2 - pop`)
    // — build enough Tents that this never blocks the mechanic under test.
    for _ in 0..5 {
        let (x, y) = find_spot(&state, BuildingKind::Tent);
        sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y });
    }
    sim::finish_all_construction(&mut state);

    // Force the birth gate's own resource/morale thresholds to stay
    // satisfied for the whole run — isolates the birth-chance roll itself
    // from food/morale drifting out of range over a long run, which are
    // covered by their own dedicated tests elsewhere.
    let mut saw_birth = false;
    for _ in 0..(TICKS_PER_DAY * 60) {
        state.stock.food = 999.0;
        state.stock.water = 999.0;
        state.morale = 100.0;
        sim::tick(&mut state);
        if state.events.iter().any(|e| e.text.contains("newborn")) {
            saw_birth = true;
            break;
        }
    }

    // 60 days gives ~55 independent post-grace-day tries at BIRTH_CHANCE
    // (0.12); the odds of never rolling a hit are 0.88^55 =~ 0.0006 —
    // deterministic given SEED, and vanishingly unlikely to flake.
    assert!(saw_birth, "expected at least one newborn over 60 days of favorable conditions");
}

#[test]
fn no_birth_before_the_grace_day_even_with_perfect_conditions() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 500.0;
    for _ in 0..5 {
        let (x, y) = find_spot(&state, BuildingKind::Tent);
        sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y });
    }
    sim::finish_all_construction(&mut state);

    for _ in 0..(TICKS_PER_DAY * (BIRTH_GRACE_DAY as u64 - 1)) {
        state.stock.food = 999.0;
        state.stock.water = 999.0;
        state.morale = 100.0;
        sim::tick(&mut state);
    }

    assert!(
        !state.events.iter().any(|e| e.text.contains("newborn")),
        "no birth should occur before BIRTH_GRACE_DAY, no matter how favorable conditions are: {:?}",
        state.events
    );
}

// --- Guest permissions / central-world restrictions ---

#[test]
fn central_world_settlers_never_get_buried() {
    let state = sim::new_game_central(SEED);
    assert!(
        !state.can_issue(1, &PlayerCommand::Bury { survivor: 1, corpse: 1 }),
        "settlers in the central world never die, so Bury must never be issuable there"
    );
}

#[test]
fn guest_may_bury_under_build_but_not_view_only() {
    let mut state = sim::new_game(SEED, 12);
    sim::player_joined(&mut state, 1, "Owner");
    sim::player_joined(&mut state, 2, "Guest");
    assert_eq!(state.guest_perm, GuestPermission::Build, "default policy is Build");

    assert!(
        state.can_issue(2, &PlayerCommand::Bury { survivor: 1, corpse: 1 }),
        "guests should be able to help with burial under Build, same as ChopTile/MoveSurvivor"
    );

    sim::set_guest_permission(&mut state, GuestPermission::ViewOnly);
    assert!(
        !state.can_issue(2, &PlayerCommand::Bury { survivor: 1, corpse: 1 }),
        "ViewOnly guests may not issue any world-changing command"
    );
}
