//! Pure-simulation tests for V0.20 building interiors: fitting a room out
//! with `PlayerCommand::UpgradeFurnishing` (`FurnishingKind::{Workbench,
//! Seating, Heater, Shelving}` — see `types::furnishing`'s module doc for why
//! these four are a shared vocabulary rather than one enum variant per
//! building), the `UpgradeBuilding` gate `Building::furnishings_keep_pace`
//! composes with, and each fitting's wiring into `sim::tick`.
//!
//! Style mirrors `construction_tests.rs`/`tech_tests.rs`/`fatigue_tests.rs`/
//! `xp_tests.rs`: control-vs-experiment pairs ticked in lockstep, `find_spot`/
//! `place_and_finish` helpers, `seek_time_of_day` to isolate the day-only
//! fatigue-accrual rate from night recovery.

use frozen_city::game::sim;
use frozen_city::game::types::*;

const SEED: u64 = 12345;

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

fn place_and_finish(state: &mut GameState, kind: BuildingKind, x: u8, y: u8) -> u32 {
    sim::apply_command(state, 1, &PlayerCommand::Place { kind, x, y, facing: 0 });
    let id = state.buildings.last().unwrap().id;
    sim::finish_all_construction(state);
    // V0.21: `finish_all_construction` fits a free level-1 workbench, because
    // production now comes from the fitting and every OTHER test file wants a
    // building that actually works. This file is the one that tests the
    // interior itself, so it starts from a genuinely bare room and buys
    // everything through `UpgradeFurnishing`, paying the real costs.
    if let Some(b) = state.buildings.iter_mut().find(|b| b.id == id) {
        b.furnishings.clear();
    }
    id
}

/// `place_and_finish`, then drops whatever anonymous construction crew
/// `Place`/finishing left behind — needed anywhere a test wants a SPECIFIC
/// named survivor to hold the only slot (mirrors `fatigue_tests.rs`'s helper
/// of the same name).
fn place_finish_and_clear_crew(state: &mut GameState, kind: BuildingKind, x: u8, y: u8) -> u32 {
    let id = place_and_finish(state, kind, x, y);
    let cur = state.find_building(id).unwrap().workers as i8;
    sim::apply_command(state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: -cur });
    id
}

/// Lands `state.tick` at the START of `fraction` on whatever day it's
/// currently mid-way through, rounding forward to the next occurrence.
/// Copied verbatim from `fatigue_tests.rs` (each test file keeps its own
/// small helpers, matching this repo's existing convention).
fn seek_time_of_day(state: &mut GameState, fraction: f32) {
    let day_start = (state.tick / TICKS_PER_DAY) * TICKS_PER_DAY;
    let target = day_start + (fraction * TICKS_PER_DAY as f32) as u64;
    state.tick = if target > state.tick { target } else { target + TICKS_PER_DAY };
}

/// The slot index `kind.furnishings()` gives `fitting`, or a panic if `kind`
/// doesn't take it at all — a test-authoring mistake, not a case any of these
/// tests want to silently pass through.
fn slot_of(kind: BuildingKind, fitting: FurnishingKind) -> u8 {
    kind.furnishings()
        .iter()
        .position(|k| *k == fitting)
        .unwrap_or_else(|| panic!("{kind:?} takes no {fitting:?}")) as u8
}

/// Buys `slot`'s fitting all the way to `FURNISHING_MAX_LEVEL`, one
/// `UpgradeFurnishing` command per level — mirrors how a real player climbs
/// the ladder, one purchase at a time.
fn max_out(state: &mut GameState, building: u32, slot: u8) {
    for _ in 0..FURNISHING_MAX_LEVEL {
        sim::apply_command(state, 1, &PlayerCommand::UpgradeFurnishing { building, slot });
    }
}

// --- the most important test in this file: unfurnished == pre-interiors ---

#[test]
fn furnishing_factor_is_the_identity_before_anything_is_bought() {
    for kind in BuildingKind::BUILDABLE.into_iter().chain([BuildingKind::Furnace, BuildingKind::Tunnel]) {
        let b = Building {
            id: 0,
            kind,
            x: 0,
            y: 0,
            workers: 0,
            progress: 0.0,
            owner: None,
            owner_account: None,
            level: 1,
            build_left: 0.0,
            furnishings: Vec::new(),
            facing: 0,
        };
        for fk in FurnishingKind::ALL {
            assert_eq!(b.furnishing_of(fk), 0, "{kind:?}/{fk:?}: unbought must read as level 0");
            assert_eq!(b.furnishing_factor(fk), 1.0, "{kind:?}/{fk:?}: unbought must be a true no-op factor");
        }
    }
}

/// THE most important test in this file. Every effect reads a neutral value
/// (factor 1.0 / level 0 / zero relief) when nothing has been bought — proven
/// above directly on the pure accessors; this proves it end-to-end, through
/// actual production, morale, fatigue and xp over a full day, comparing a
/// colony that never touches `UpgradeFurnishing` against a bit-for-bit twin
/// whose buildings carry an explicitly zero-filled `furnishings` vec instead
/// of an absent one. If the two ever diverge, either some factor stopped
/// being exactly 1.0, or `Building::furnishing_level`'s "short/empty vec
/// reads as all-zero" contract (see its doc comment) broke somewhere.
#[test]
fn unfurnished_colony_matches_an_explicitly_zeroed_one_bit_for_bit() {
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);
    for s in [&mut control, &mut experiment] {
        s.stock.wood = 500.0;
    }

    // Kitchen: staffed effect + morale. Sawmill: production + a named,
    // training survivor for xp. Identically placed, staffed and assigned in
    // both worlds; neither ever issues `UpgradeFurnishing`.
    let (kx, ky) = find_spot(&control, BuildingKind::Kitchen);
    let control_kitchen = place_and_finish(&mut control, BuildingKind::Kitchen, kx, ky);
    let exp_kitchen = place_and_finish(&mut experiment, BuildingKind::Kitchen, kx, ky);
    sim::apply_command(&mut control, 1, &PlayerCommand::AdjustWorkers { building: control_kitchen, delta: 1 });
    sim::apply_command(&mut experiment, 1, &PlayerCommand::AdjustWorkers { building: exp_kitchen, delta: 1 });

    let (sx, sy) = find_spot(&control, BuildingKind::Sawmill);
    let control_sawmill = place_and_finish(&mut control, BuildingKind::Sawmill, sx, sy);
    let exp_sawmill = place_and_finish(&mut experiment, BuildingKind::Sawmill, sx, sy);
    let control_survivor = control.survivors[0].id;
    let exp_survivor = experiment.survivors[0].id;
    sim::apply_command(&mut control, 1, &PlayerCommand::AssignSurvivor { survivor: control_survivor, building: Some(control_sawmill) });
    sim::apply_command(&mut experiment, 1, &PlayerCommand::AssignSurvivor { survivor: exp_survivor, building: Some(exp_sawmill) });

    // The only deliberate difference going in: experiment's buildings carry a
    // present-but-zero furnishings vec instead of an empty one.
    for b in experiment.buildings.iter_mut() {
        let n = b.kind.furnishings().len();
        if n > 0 {
            b.furnishings = vec![0; n];
        }
    }
    assert!(control.buildings.iter().all(|b| b.furnishings.is_empty()));
    assert!(
        experiment.buildings.iter().any(|b| !b.furnishings.is_empty()),
        "sanity: the twin must actually differ going in"
    );

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert_eq!(control.stock, experiment.stock, "production must be bit-identical");
    assert_eq!(control.morale, experiment.morale, "morale must be bit-identical");
    for (c, e) in control.survivors.iter().zip(&experiment.survivors) {
        assert_eq!(
            (c.fatigue, c.xp, c.hp),
            (e.fatigue, e.xp, e.hp),
            "per-survivor fatigue/xp/hp must be bit-identical"
        );
    }
}

// --- buying and upgrading a fitting ---

#[test]
fn buying_a_fitting_charges_exact_cost_and_never_reenters_construction() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    let id = place_and_finish(&mut state, BuildingKind::Sawmill, x, y);
    let slot = slot_of(BuildingKind::Sawmill, FurnishingKind::Workbench);

    let wood_before = state.stock.wood;
    sim::apply_command(&mut state, 1, &PlayerCommand::UpgradeFurnishing { building: id, slot });
    let charged = wood_before - state.stock.wood;
    assert!(
        (charged - FurnishingKind::Workbench.cost_wood(1)).abs() < 1e-6,
        "must charge exactly cost_wood(1): charged {charged}"
    );
    assert_eq!(state.find_building(id).unwrap().furnishing_level(slot as usize), 1);
    assert!(
        !state.find_building(id).unwrap().under_construction(),
        "buying a fitting must never put the building back under construction"
    );

    // Insufficient wood: refused outright, no debt, no partial charge.
    state.stock.wood = 1.0;
    sim::apply_command(&mut state, 1, &PlayerCommand::UpgradeFurnishing { building: id, slot });
    assert_eq!(state.find_building(id).unwrap().furnishing_level(slot as usize), 1, "insufficient wood must not upgrade");
    assert_eq!(state.stock.wood, 1.0, "wood must never go negative");

    // A construction site refuses the purchase outright too, even flush with wood.
    state.stock.wood = 500.0;
    let (x2, y2) = find_spot(&state, BuildingKind::Sawmill);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x: x2, y: y2, facing: 0 });
    let site_id = state.buildings.last().unwrap().id;
    assert!(state.find_building(site_id).unwrap().under_construction());
    let wood_before = state.stock.wood;
    sim::apply_command(&mut state, 1, &PlayerCommand::UpgradeFurnishing { building: site_id, slot });
    assert_eq!(state.find_building(site_id).unwrap().furnishing_level(slot as usize), 0, "a construction site can't be furnished yet");
    assert_eq!(state.stock.wood, wood_before, "a refused furnishing purchase must not charge wood");
}

#[test]
fn furnishing_climbs_the_ladder_to_max_then_refuses() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 100_000.0;
    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    let id = place_and_finish(&mut state, BuildingKind::Sawmill, x, y);
    let slot = slot_of(BuildingKind::Sawmill, FurnishingKind::Workbench);

    for level in 1..=FURNISHING_MAX_LEVEL {
        let wood_before = state.stock.wood;
        sim::apply_command(&mut state, 1, &PlayerCommand::UpgradeFurnishing { building: id, slot });
        assert_eq!(state.find_building(id).unwrap().furnishing_level(slot as usize), level);
        let charged = wood_before - state.stock.wood;
        assert!(
            (charged - FurnishingKind::Workbench.cost_wood(level)).abs() < 1e-6,
            "level {level}: must charge exactly cost_wood({level})"
        );
    }

    let wood_at_max = state.stock.wood;
    sim::apply_command(&mut state, 1, &PlayerCommand::UpgradeFurnishing { building: id, slot });
    assert_eq!(
        state.find_building(id).unwrap().furnishing_level(slot as usize),
        FURNISHING_MAX_LEVEL,
        "maxed fitting must refuse a further purchase"
    );
    assert_eq!(state.stock.wood, wood_at_max, "a refused purchase past the max must not charge wood");
}

#[test]
fn out_of_range_slot_and_fittingless_kind_are_harmless_noops() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 500.0;

    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    let id = place_and_finish(&mut state, BuildingKind::Sawmill, x, y);
    let slots = BuildingKind::Sawmill.furnishings().len() as u8;
    let wood_before = state.stock.wood;
    sim::apply_command(&mut state, 1, &PlayerCommand::UpgradeFurnishing { building: id, slot: slots });
    sim::apply_command(&mut state, 1, &PlayerCommand::UpgradeFurnishing { building: id, slot: 250 });
    assert!(state.find_building(id).unwrap().furnishings.is_empty(), "an out-of-range slot must never grow the vec");
    assert_eq!(state.stock.wood, wood_before);

    let (tx, ty) = find_spot(&state, BuildingKind::Tent);
    let tent_id = place_and_finish(&mut state, BuildingKind::Tent, tx, ty);
    assert!(BuildingKind::Tent.furnishings().is_empty(), "sanity: a Tent takes no fittings at all");
    let wood_before = state.stock.wood;
    sim::apply_command(&mut state, 1, &PlayerCommand::UpgradeFurnishing { building: tent_id, slot: 0 });
    assert!(state.find_building(tent_id).unwrap().furnishings.is_empty());
    assert_eq!(state.stock.wood, wood_before);

    sim::apply_command(&mut state, 1, &PlayerCommand::UpgradeFurnishing { building: 999_999, slot: 0 });
    assert_eq!(state.stock.wood, wood_before, "an unknown building id must be a no-op");
}

// --- the four effects, each observed control-vs-experiment ---

#[test]
fn workbench_raises_production() {
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);
    for s in [&mut control, &mut experiment] {
        s.stock.wood = 500.0;
    }

    let (x, y) = find_spot(&control, BuildingKind::HunterHut);
    let control_id = place_and_finish(&mut control, BuildingKind::HunterHut, x, y);
    let exp_id = place_and_finish(&mut experiment, BuildingKind::HunterHut, x, y);
    sim::apply_command(&mut control, 1, &PlayerCommand::AdjustWorkers { building: control_id, delta: 2 });
    sim::apply_command(&mut experiment, 1, &PlayerCommand::AdjustWorkers { building: exp_id, delta: 2 });
    assert_eq!(control.find_building(control_id).unwrap().workers, 2, "control HunterHut should be fully staffed");
    assert_eq!(experiment.find_building(exp_id).unwrap().workers, 2, "experiment HunterHut should be fully staffed");

    max_out(&mut experiment, exp_id, slot_of(BuildingKind::HunterHut, FurnishingKind::Workbench));

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        experiment.stock.food > control.stock.food,
        "a maxed Workbench (+24%) must out-produce an identically-staffed unfurnished hut: control={}, experiment={}",
        control.stock.food,
        experiment.stock.food
    );
}

#[test]
fn seating_raises_morale_while_staffed() {
    let mut control = sim::new_game_bootstrapped(SEED, 12);
    let mut experiment = sim::new_game_bootstrapped(SEED, 12);
    for s in [&mut control, &mut experiment] {
        s.stock.wood = 500.0;
    }

    let (x, y) = find_spot(&control, BuildingKind::Kitchen);
    let control_id = place_and_finish(&mut control, BuildingKind::Kitchen, x, y);
    let exp_id = place_and_finish(&mut experiment, BuildingKind::Kitchen, x, y);
    sim::apply_command(&mut control, 1, &PlayerCommand::AdjustWorkers { building: control_id, delta: 1 });
    sim::apply_command(&mut experiment, 1, &PlayerCommand::AdjustWorkers { building: exp_id, delta: 1 });

    max_out(&mut experiment, exp_id, slot_of(BuildingKind::Kitchen, FurnishingKind::Seating));

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        experiment.morale > control.morale,
        "a maxed Seating fitting must lift morale beyond the identically-staffed unfurnished control: control={}, experiment={}",
        control.morale,
        experiment.morale
    );
}

#[test]
fn heater_slows_fatigue_only_for_workers_assigned_there() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 500.0;

    let (cx, cy) = find_spot(&state, BuildingKind::CoalMine);
    let coal_mine = place_finish_and_clear_crew(&mut state, BuildingKind::CoalMine, cx, cy);
    let (sx, sy) = find_spot(&state, BuildingKind::Sawmill);
    let sawmill = place_finish_and_clear_crew(&mut state, BuildingKind::Sawmill, sx, sy);
    assert!(
        BuildingKind::Sawmill.furnishings().iter().all(|k| *k != FurnishingKind::Heater),
        "sanity: the Sawmill takes no Heater at all, so it can never accidentally benefit"
    );

    max_out(&mut state, coal_mine, slot_of(BuildingKind::CoalMine, FurnishingKind::Heater));

    let heated = state.survivors[0].id;
    let cold = state.survivors[1].id;
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor: heated, building: Some(coal_mine) });
    sim::apply_command(&mut state, 1, &PlayerCommand::AssignSurvivor { survivor: cold, building: Some(sawmill) });

    // Land solidly inside the day window and reset fatigue right there — same
    // technique as `fatigue_tests.rs`'s accrual test, so every tick below
    // accrues at exactly one of the two (possibly heater-reduced) work rates,
    // with no night-recovery mixed in.
    seek_time_of_day(&mut state, 0.26);
    for s in &mut state.survivors {
        s.fatigue = 0.0;
    }
    let ticks = TICKS_PER_DAY / 8;
    for _ in 0..ticks {
        assert!(!state.is_night(), "sanity: must stay inside the day window throughout");
        sim::tick(&mut state);
    }

    let heated_fatigue = state.survivors.iter().find(|s| s.id == heated).unwrap().fatigue;
    let cold_fatigue = state.survivors.iter().find(|s| s.id == cold).unwrap().fatigue;
    let expected = |per_day: f32| per_day / TICKS_PER_DAY as f32 * ticks as f32;
    // Heater maxed: relief = 3 * 0.07 = 0.21 (well above the formula's 0.2
    // floor, so the floor itself never engages here — see the review notes).
    let relief = FurnishingKind::Heater.per_level() * FURNISHING_MAX_LEVEL as f32;

    assert!(
        (heated_fatigue - expected(FATIGUE_WORK_PER_DAY * (1.0 - relief))).abs() < 0.1,
        "a heated worker must accrue fatigue at the reduced rate: expected {}, got {heated_fatigue}",
        expected(FATIGUE_WORK_PER_DAY * (1.0 - relief))
    );
    assert!(
        (cold_fatigue - expected(FATIGUE_WORK_PER_DAY)).abs() < 0.1,
        "a worker with no stove must accrue fatigue at the ordinary rate: expected {}, got {cold_fatigue}",
        expected(FATIGUE_WORK_PER_DAY)
    );
    assert!(heated_fatigue < cold_fatigue, "the heated worker must tire more slowly than the one with no stove");
}

#[test]
fn shelving_speeds_xp_accrual() {
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);
    for s in [&mut control, &mut experiment] {
        s.stock.wood = 500.0;
    }

    let (x, y) = find_spot(&control, BuildingKind::Sawmill);
    let control_id = place_finish_and_clear_crew(&mut control, BuildingKind::Sawmill, x, y);
    let exp_id = place_finish_and_clear_crew(&mut experiment, BuildingKind::Sawmill, x, y);

    let control_survivor = control.survivors[0].id;
    let exp_survivor = experiment.survivors[0].id;
    sim::apply_command(&mut control, 1, &PlayerCommand::AssignSurvivor { survivor: control_survivor, building: Some(control_id) });
    sim::apply_command(&mut experiment, 1, &PlayerCommand::AssignSurvivor { survivor: exp_survivor, building: Some(exp_id) });

    max_out(&mut experiment, exp_id, slot_of(BuildingKind::Sawmill, FurnishingKind::Shelving));

    for _ in 0..(TICKS_PER_DAY / 2) {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    let control_xp = control.survivors.iter().find(|s| s.id == control_survivor).unwrap().xp;
    let exp_xp = experiment.survivors.iter().find(|s| s.id == exp_survivor).unwrap().xp;
    assert!(
        exp_xp > control_xp,
        "maxed Shelving (+45%) must speed xp accrual: control={control_xp}, experiment={exp_xp}"
    );

    // Sanity against the exact formulas (mirrors `xp_tests.rs`'s style):
    // control accrues at the plain rate, experiment at
    // `1.0 + Shelving.per_level() * FURNISHING_MAX_LEVEL`.
    let plain = (TICKS_PER_DAY / 2) as f32 / TICKS_PER_DAY as f32;
    let boosted = plain * (1.0 + FurnishingKind::Shelving.per_level() * FURNISHING_MAX_LEVEL as f32);
    assert!((control_xp - plain).abs() < 0.01, "control xp should match the plain (unboosted) formula: got {control_xp}");
    assert!((exp_xp - boosted).abs() < 0.01, "experiment xp should match the Shelving-boosted formula: got {exp_xp}");
}

// --- the interior gate on UpgradeBuilding ---

#[test]
fn upgrade_gate_blocks_until_furnished_then_frees_the_climb_to_max() {
    let mut state = sim::new_game_bootstrapped(SEED, 12);
    state.stock.wood = 1_000_000.0;
    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    let id = place_and_finish(&mut state, BuildingKind::Sawmill, x, y);
    let slots: Vec<u8> = (0..BuildingKind::Sawmill.furnishings().len() as u8).collect();

    assert!(
        !state.find_building(id).unwrap().furnishings_keep_pace(),
        "sanity: a bare room's fittings can never be keeping pace"
    );
    let wood_before = state.stock.wood;
    sim::apply_command(&mut state, 1, &PlayerCommand::UpgradeBuilding { building: id });
    assert_eq!(state.find_building(id).unwrap().level, 1, "an unfurnished building must not climb past level 1");
    assert_eq!(state.stock.wood, wood_before, "a refused upgrade must not charge wood");

    // Transitioning level L -> L+1 needs every slot at >= min(L,
    // FURNISHING_MAX_LEVEL), so the FURNISHING_MAX_LEVEL-th gated step is the
    // one that finally maxes every fitting (see `required_furnishing_level`).
    for target in 2..=(FURNISHING_MAX_LEVEL + 1) {
        sim::apply_command(&mut state, 1, &PlayerCommand::UpgradeBuilding { building: id });
        assert_eq!(
            state.find_building(id).unwrap().level,
            target - 1,
            "must stay blocked until every slot keeps pace with the CURRENT level"
        );

        for &slot in &slots {
            sim::apply_command(&mut state, 1, &PlayerCommand::UpgradeFurnishing { building: id, slot });
        }
        assert!(state.find_building(id).unwrap().furnishings_keep_pace());

        sim::apply_command(&mut state, 1, &PlayerCommand::UpgradeBuilding { building: id });
        assert_eq!(state.find_building(id).unwrap().level, target, "furnished to keep pace, the upgrade must go through");
        sim::finish_all_construction(&mut state);
    }
    assert_eq!(
        state.find_building(id).unwrap().furnishings,
        vec![FURNISHING_MAX_LEVEL; slots.len()],
        "every slot should be fully maxed by now"
    );

    // Fittings are maxed — the gate opens for good, all the way to BUILDING_MAX_LEVEL.
    for target in (FURNISHING_MAX_LEVEL + 2)..=BUILDING_MAX_LEVEL {
        sim::apply_command(&mut state, 1, &PlayerCommand::UpgradeBuilding { building: id });
        assert_eq!(state.find_building(id).unwrap().level, target, "once fittings are maxed, upgrades must carry on unblocked");
        sim::finish_all_construction(&mut state);
    }
    assert_eq!(state.find_building(id).unwrap().level, BUILDING_MAX_LEVEL);
}

// --- central-world ownership ---

#[test]
fn central_world_only_the_owning_account_may_furnish_its_building() {
    let mut state = sim::new_game_central(5);
    sim::player_joined_as(&mut state, 10, "Aziz", Some(1));
    sim::player_joined_as(&mut state, 20, "Vali", Some(2));

    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    sim::apply_command(&mut state, 10, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y, facing: 0 });
    let id = state.buildings.iter().find(|b| b.kind == BuildingKind::Sawmill).unwrap().id;
    sim::finish_all_construction(&mut state);
    // Start from a bare room — `finish_all_construction` fits a free level-1
    // workbench for every other test file's benefit (see `place_and_finish`),
    // and this test needs the "nothing bought yet" state to prove a stranger
    // cannot buy the first one.
    if let Some(b) = state.buildings.iter_mut().find(|b| b.id == id) {
        b.furnishings.clear();
    }
    state.stock.wood = 500.0;
    let slot = slot_of(BuildingKind::Sawmill, FurnishingKind::Workbench);

    assert!(
        !state.can_issue(20, &PlayerCommand::UpgradeFurnishing { building: id, slot }),
        "a different account must never furnish someone else's central building"
    );
    let wood_before = state.stock.wood;
    sim::apply_command(&mut state, 20, &PlayerCommand::UpgradeFurnishing { building: id, slot });
    assert_eq!(state.find_building(id).unwrap().furnishing_level(slot as usize), 0, "the stranger's attempt must be a no-op");
    assert_eq!(state.stock.wood, wood_before);

    assert!(state.can_issue(10, &PlayerCommand::UpgradeFurnishing { building: id, slot }));
    sim::apply_command(&mut state, 10, &PlayerCommand::UpgradeFurnishing { building: id, slot });
    assert_eq!(state.find_building(id).unwrap().furnishing_level(slot as usize), 1, "the owning account must succeed");
}

// --- V0.21: production comes from the fitting, not the building ---
//
// The building still decides WHICH resource and still owns the deposit
// capping around it; the workbench inside decides the RATE. A room with no
// workbench has no tools and produces nothing at all — which is the whole
// reason furnishing a workplace is a decision rather than a bonus.

#[test]
fn a_bare_workshop_produces_nothing_however_many_people_stand_in_it() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::Greenhouse);
    // `place_and_finish` here strips the free workbench (see its doc) — this
    // is a genuinely bare room.
    let id = place_and_finish(&mut state, BuildingKind::Greenhouse, x, y);
    sim::apply_command(&mut state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: 2 });
    assert!(state.find_building(id).unwrap().workers > 0, "sanity: staffed");
    assert!(
        state.find_building(id).unwrap().production_cycle().is_none(),
        "no workbench, no cycle"
    );

    let before = state.stock.food;
    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut state);
    }
    assert!(
        state.stock.food <= before,
        "a bare Greenhouse must add no food at all (the colony still eats): {before} -> {}",
        state.stock.food
    );
}

#[test]
fn the_first_workbench_restores_exactly_the_buildings_old_throughput() {
    // The calibration that keeps V0.21 from being a balance change: a
    // level-1 workbench reproduces `production_per_worker_day` precisely.
    // Buying the first one makes the trade WORK; levels above it make it
    // better.
    for kind in [BuildingKind::Greenhouse, BuildingKind::Sawmill, BuildingKind::Well] {
        let slot = slot_of(kind, FurnishingKind::Workbench);
        let cycle = FurnishingKind::Workbench
            .cycle(kind, 1)
            .expect("a producer's workbench always has a cycle");
        assert!(
            (cycle.per_day() - kind.production_per_worker_day()).abs() < 0.01,
            "{kind:?}: level-1 workbench should yield exactly {}/day, got {}",
            kind.production_per_worker_day(),
            cycle.per_day()
        );
        let _ = slot;
    }
}

#[test]
fn each_workbench_level_shortens_the_cycle_and_raises_throughput() {
    let kind = BuildingKind::Greenhouse;
    let mut last_ticks = f32::MAX;
    let mut last_per_day = 0.0;
    for level in 1..=FURNISHING_MAX_LEVEL {
        let c = FurnishingKind::Workbench.cycle(kind, level).expect("producer");
        assert!(c.ticks < last_ticks, "level {level} must run a shorter cycle than {}", level - 1);
        assert!(c.per_day() > last_per_day, "level {level} must out-produce {}", level - 1);
        assert!(c.seconds() > 0.0, "a cycle always takes real time");
        last_ticks = c.ticks;
        last_per_day = c.per_day();
    }
    assert!(
        FurnishingKind::Workbench.cycle(kind, 0).is_none(),
        "level 0 is 'not bought', not 'runs slowly'"
    );
}

#[test]
fn only_the_workbench_produces_and_only_where_the_building_yields_something() {
    // A Kitchen's value is an EFFECT (it makes the city eat thriftily), not a
    // stockpile credit — so even its workbench has no cycle. And no other
    // fitting produces anywhere.
    assert!(
        FurnishingKind::Workbench.cycle(BuildingKind::Kitchen, 3).is_none(),
        "a Kitchen credits no resource, so nothing in it runs a production cycle"
    );
    for kind in [FurnishingKind::Seating, FurnishingKind::Heater, FurnishingKind::Shelving] {
        assert!(
            kind.cycle(BuildingKind::Greenhouse, 3).is_none(),
            "{kind:?} is not a producer anywhere"
        );
    }
}
