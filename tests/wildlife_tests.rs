//! Pure-simulation tests for V0.10: the deer/rabbit wildlife resource system
//! (HunterHut hunting -> food + fur) and the Tailor Shop (fur -> cloth,
//! staffed cloth -> passive warmth). Style mirrors `building_tests.rs` /
//! `tech_tests.rs`: control-vs-experiment comparisons between two otherwise-
//! identical `new_game` worlds, ticked in lockstep so the only difference is
//! the mechanic under test.

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

#[test]
fn wildlife_starts_nonzero_and_regenerates_toward_cap() {
    let state = sim::new_game(SEED, 12);
    assert_eq!(state.wildlife.deer, DEER_START, "a fresh world starts with a real deer population");
    assert_eq!(state.wildlife.rabbit, RABBIT_START, "a fresh world starts with a real rabbit population");

    let mut state = sim::new_game(SEED, 12);
    state.wildlife.deer = 5.0; // well below DEER_CAP
    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut state);
    }
    assert!(state.wildlife.deer > 5.0, "deer should regrow over a day: got {}", state.wildlife.deer);
    assert!(state.wildlife.deer < DEER_CAP, "one day shouldn't reach the cap from 5.0");
}

#[test]
fn wildlife_never_exceeds_cap() {
    let mut state = sim::new_game(SEED, 12);
    state.wildlife.deer = DEER_CAP;
    state.wildlife.rabbit = RABBIT_CAP;
    for _ in 0..(TICKS_PER_DAY * 5) {
        sim::tick(&mut state);
        assert!(state.wildlife.deer <= DEER_CAP, "deer exceeded its cap: {}", state.wildlife.deer);
        assert!(state.wildlife.rabbit <= RABBIT_CAP, "rabbit exceeded its cap: {}", state.wildlife.rabbit);
    }
}

#[test]
fn staffed_hunter_hut_produces_food_and_fur_and_depletes_wildlife() {
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);
    control.stock.wood = 500.0;
    experiment.stock.wood = 500.0;

    let (x, y) = find_spot(&experiment, BuildingKind::HunterHut);
    place_and_staff(&mut experiment, BuildingKind::HunterHut, x, y, 2);

    let control_wildlife_before = control.wildlife.deer + control.wildlife.rabbit;
    let experiment_wildlife_before = experiment.wildlife.deer + experiment.wildlife.rabbit;

    // A few hours (not a full day) isolates hunting's depletion from
    // regen's noise, which would otherwise partially mask the delta over a
    // longer run.
    let ticks_per_hour = TICKS_PER_DAY / 24;
    for _ in 0..(ticks_per_hour * 6) {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert!(
        experiment.stock.fur > control.stock.fur,
        "a staffed HunterHut should produce fur: control={}, experiment={}",
        control.stock.fur,
        experiment.stock.fur
    );
    assert!(
        experiment.stock.food > control.stock.food,
        "a staffed HunterHut should produce more food than the control: control={}, experiment={}",
        control.stock.food,
        experiment.stock.food
    );
    let control_wildlife_after = control.wildlife.deer + control.wildlife.rabbit;
    let experiment_wildlife_after = experiment.wildlife.deer + experiment.wildlife.rabbit;
    assert!(
        experiment_wildlife_after - experiment_wildlife_before
            < control_wildlife_after - control_wildlife_before,
        "hunting should net-deplete wildlife relative to the unhunted control"
    );
}

#[test]
fn hunter_hut_gracefully_idles_when_wildlife_is_exhausted() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    state.wildlife.deer = 0.0;
    state.wildlife.rabbit = 0.0;

    let (x, y) = find_spot(&state, BuildingKind::HunterHut);
    place_and_staff(&mut state, BuildingKind::HunterHut, x, y, 2);

    let fur_before = state.stock.fur;
    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut state);
        assert!(state.wildlife.deer >= 0.0, "deer population must never go negative");
        assert!(state.wildlife.rabbit >= 0.0, "rabbit population must never go negative");
    }

    // Wildlife starts and stays at 0.0 (logistic regen from 0 never leaves
    // 0), so hunting has nothing to draw from the whole run — no panic, no
    // negative stock, just idle production, same as an empty coal deposit.
    // Fur has no other producer, so it staying put is a clean signal; food
    // isn't (survivors keep eating regardless of hunting), so that half is
    // checked via a control/experiment pair below instead.
    assert_eq!(state.wildlife.deer, 0.0);
    assert_eq!(state.wildlife.rabbit, 0.0);
    assert_eq!(state.stock.fur, fur_before, "an exhausted HunterHut should produce no fur");

    let mut control = sim::new_game(SEED, 12);
    control.stock.wood = 500.0;
    control.wildlife.deer = 0.0;
    control.wildlife.rabbit = 0.0;
    // No HunterHut placed at all — isolates ordinary hunger consumption from
    // any hunting income.
    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
    }
    assert!(
        (state.stock.food - control.stock.food).abs() < 0.01,
        "an exhausted HunterHut should add no food income beyond ordinary consumption: \
         no-hut={}, exhausted-hut={}",
        control.stock.food,
        state.stock.food
    );
}

#[test]
fn staffed_tailor_shop_converts_fur_to_cloth() {
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);
    control.stock.wood = 500.0;
    experiment.stock.wood = 500.0;
    control.stock.fur = 100.0;
    experiment.stock.fur = 100.0;

    let (x, y) = find_spot(&experiment, BuildingKind::TailorShop);
    place_and_staff(&mut experiment, BuildingKind::TailorShop, x, y, 2);

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert_eq!(control.stock.cloth, 0.0, "an unstaffed Tailor Shop world produces no cloth");
    assert!(
        experiment.stock.cloth > 0.0,
        "a staffed Tailor Shop should produce cloth from the seeded fur"
    );
    assert!(
        experiment.stock.fur < control.stock.fur,
        "fur should be consumed converting to cloth: control={}, experiment={}",
        control.stock.fur,
        experiment.stock.fur
    );
    // Roughly FUR_PER_CLOTH fur per cloth (allowing for the day's worth of
    // partial `Building.progress` carried over, hence a generous tolerance).
    let fur_spent = control.stock.fur - experiment.stock.fur;
    let ratio = fur_spent / experiment.stock.cloth;
    assert!(
        (ratio - FUR_PER_CLOTH).abs() < 0.5,
        "fur-per-cloth ratio should be close to FUR_PER_CLOTH ({FUR_PER_CLOTH}): got {ratio}"
    );
}

#[test]
fn tailor_shop_idles_without_fur() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    state.stock.fur = 0.0;

    let (x, y) = find_spot(&state, BuildingKind::TailorShop);
    place_and_staff(&mut state, BuildingKind::TailorShop, x, y, 2);

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut state);
    }

    assert_eq!(state.stock.cloth, 0.0, "no fur means no cloth, never a panic or negative stock");
    assert_eq!(state.stock.fur, 0.0);
}

#[test]
fn tailoring_bonus_reduces_cold_damage_like_insulation() {
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);

    // No furnace heat in either world, so every survivor is exposed to the
    // raw outside temperature and the only remaining difference is the
    // Tailor Shop's warmth bonus.
    control.furnace_level = 0;
    control.furnace_lit = false;
    experiment.furnace_level = 0;
    experiment.furnace_lit = false;

    experiment.stock.wood = 500.0;
    // No fur, so the shop's own production doesn't confound the comparison —
    // only the seeded cloth (and its slow upkeep drain) matters here.
    experiment.stock.fur = 0.0;
    experiment.stock.cloth = 50.0;
    let (x, y) = find_spot(&experiment, BuildingKind::TailorShop);
    place_and_staff(&mut experiment, BuildingKind::TailorShop, x, y, 1);

    for s in &mut control.survivors {
        s.hp = 50.0;
    }
    for s in &mut experiment.survivors {
        s.hp = 50.0;
    }

    let ticks_per_hour = TICKS_PER_DAY / 24;
    for _ in 0..(ticks_per_hour * 6) {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    assert_eq!(control.survivors.len(), experiment.survivors.len(), "nobody should have died in either run");

    let control_hp: f32 = control.survivors.iter().map(|s| s.hp).sum();
    let experiment_hp: f32 = experiment.survivors.iter().map(|s| s.hp).sum();
    assert!(
        experiment_hp >= control_hp,
        "a staffed Tailor Shop with cloth should reduce (or reverse) cold damage: \
         control_hp={control_hp}, experiment_hp={experiment_hp}"
    );
}

#[test]
fn tailoring_bonus_stacks_with_insulation_tech() {
    let mut insulation_only = sim::new_game(SEED, 12);
    let mut both = sim::new_game(SEED, 12);

    for state in [&mut insulation_only, &mut both] {
        state.furnace_level = 0;
        state.furnace_lit = false;
        state.techs.push(Tech::Insulation);
        for s in &mut state.survivors {
            s.hp = 50.0;
        }
    }

    both.stock.wood = 500.0;
    both.stock.fur = 0.0;
    both.stock.cloth = 50.0;
    let (x, y) = find_spot(&both, BuildingKind::TailorShop);
    place_and_staff(&mut both, BuildingKind::TailorShop, x, y, 1);

    let ticks_per_hour = TICKS_PER_DAY / 24;
    for _ in 0..(ticks_per_hour * 6) {
        sim::tick(&mut insulation_only);
        sim::tick(&mut both);
    }

    assert_eq!(
        insulation_only.survivors.len(),
        both.survivors.len(),
        "nobody should have died in either run"
    );

    let insulation_only_hp: f32 = insulation_only.survivors.iter().map(|s| s.hp).sum();
    let both_hp: f32 = both.survivors.iter().map(|s| s.hp).sum();
    assert!(
        both_hp >= insulation_only_hp,
        "Tailor Shop warmth should stack additively on top of Insulation, not be masked by it: \
         insulation_only_hp={insulation_only_hp}, both_hp={both_hp}"
    );
}

#[test]
fn tailor_shop_consumes_cloth_over_time() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    state.stock.fur = 0.0; // no replenishment — isolates the upkeep drain
    state.stock.cloth = 100.0;

    let (x, y) = find_spot(&state, BuildingKind::TailorShop);
    place_and_staff(&mut state, BuildingKind::TailorShop, x, y, 1);

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut state);
    }

    let consumed = 100.0 - state.stock.cloth;
    assert!(
        (consumed - CLOTH_UPKEEP_PER_DAY).abs() < 0.05,
        "cloth upkeep should drain roughly CLOTH_UPKEEP_PER_DAY ({CLOTH_UPKEEP_PER_DAY}) per day: consumed={consumed}"
    );
}
