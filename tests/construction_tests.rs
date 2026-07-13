//! V0.8 qurilish va bino-daraja tizimi invariantlari: joylashtirish qurilish
//! maydonchasi yaratadi (avto-brigada bilan), ustalar ishlagandagina bitadi,
//! bitmagan bino ishlab chiqarmaydi/uy bermaydi, yangilash zanjiri 10-
//! darajagacha bittalab boradi va har daraja ishlab chiqarishni oshiradi.
//!
//! Uslub `building_tests.rs`ni takrorlaydi: ssenariy qur, kun (yoki qism)
//! aylantir, `GameState`ga qarab tasdiqla — control/experiment juftligi bilan.

use frozen_city::game::sim;
use frozen_city::game::types::*;

const SEED: u64 = 777;

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
fn placement_starts_a_site_with_auto_crew() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let idle_before = state.idle_workers();
    assert!(idle_before > 0, "fresh world should have idle survivors");

    let (x, y) = find_spot(&state, BuildingKind::HunterHut);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::HunterHut, x, y });

    let b = state.buildings.last().unwrap();
    assert_eq!(b.kind, BuildingKind::HunterHut);
    assert!(b.under_construction(), "a fresh placement must be a construction site");
    assert_eq!(b.level, 1);
    assert_eq!(
        b.workers as u32,
        idle_before.min(CONSTRUCTION_CREW_MAX as u32),
        "an auto-crew should be drafted from the idle pool up to CONSTRUCTION_CREW_MAX"
    );
    assert!(
        (b.build_left - BuildingKind::HunterHut.build_workdays()).abs() < 1e-6,
        "a new site owes exactly build_workdays() of work"
    );
}

#[test]
fn a_site_without_crew_never_progresses_and_produces_nothing() {
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);
    experiment.stock.wood = 500.0;
    control.stock.wood = 500.0;

    let (x, y) = find_spot(&experiment, BuildingKind::HunterHut);
    sim::apply_command(
        &mut experiment,
        1,
        &PlayerCommand::Place { kind: BuildingKind::HunterHut, x, y },
    );
    let id = experiment.buildings.last().unwrap().id;
    // Brigadani butunlay bo'shatamiz.
    sim::apply_command(
        &mut experiment,
        1,
        &PlayerCommand::AdjustWorkers { building: id, delta: -(CONSTRUCTION_CREW_MAX as i8) },
    );
    assert_eq!(experiment.find_building(id).unwrap().workers, 0);

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    let b = experiment.find_building(id).unwrap();
    assert!(b.under_construction(), "no crew, no progress");
    // Ishchilar band emas va bino ishlamayapti — oziq iqtisodi control bilan
    // aynan bir xil bo'lishi kerak (faqat yog'och narxi farq qiladi).
    assert!(
        (experiment.stock.food - control.stock.food).abs() < 0.01,
        "an unfinished site must not produce: control={}, experiment={}",
        control.stock.food,
        experiment.stock.food
    );
}

#[test]
fn crew_finishes_the_site_and_it_starts_working() {
    // Ikkala olam ham bir xil joyga maydoncha ochadi; controlda brigada
    // bo'shatiladi (hech qachon bitmaydi), experimentda ustalar ishlaydi —
    // farq faqat bitgan binoning ishlab chiqarishi bo'ladi.
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);
    control.stock.wood = 500.0;
    experiment.stock.wood = 500.0;

    let (x, y) = find_spot(&experiment, BuildingKind::HunterHut);
    for s in [&mut control, &mut experiment] {
        sim::apply_command(s, 1, &PlayerCommand::Place { kind: BuildingKind::HunterHut, x, y });
    }
    let id = experiment.buildings.last().unwrap().id;
    sim::apply_command(
        &mut control,
        1,
        &PlayerCommand::AdjustWorkers { building: id, delta: -(CONSTRUCTION_CREW_MAX as i8) },
    );

    // 0.5 usta-kun / 3 usta ≈ 0.17 kun — bir kun katta zaxira bilan yetadi.
    let mut ticks = 0u32;
    while experiment.find_building(id).unwrap().under_construction() {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
        ticks += 1;
        assert!(ticks <= TICKS_PER_DAY as u32, "site should finish well within a day");
    }

    let b = experiment.find_building(id).unwrap();
    assert!(
        b.workers <= b.kind.max_workers(),
        "completion must trim the crew to what the building can employ"
    );

    // Bitgach ovchilar oziq keltira boshlaydi — control esa hali maydoncha.
    sim::apply_command(&mut experiment, 1, &PlayerCommand::AdjustWorkers { building: id, delta: 2 });
    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }
    assert!(
        experiment.stock.food > control.stock.food + 5.0,
        "a finished, staffed hut must out-produce the never-finished control: control={}, experiment={}",
        control.stock.food,
        experiment.stock.food
    );
}

#[test]
fn upgrades_chain_one_level_at_a_time_to_max() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 100_000.0;

    let (x, y) = find_spot(&state, BuildingKind::HunterHut);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::HunterHut, x, y });
    let id = state.buildings.last().unwrap().id;
    sim::finish_all_construction(&mut state);

    // L2: narx aynan formulaga mos tushadi va sayt yana qurilishga o'tadi.
    let wood_before = state.stock.wood;
    sim::apply_command(&mut state, 1, &PlayerCommand::UpgradeBuilding { building: id });
    {
        let b = state.find_building(id).unwrap();
        assert_eq!(b.level, 2);
        assert!(b.under_construction(), "an upgrade is a construction site again");
        assert!(
            (wood_before - state.stock.wood - BuildingKind::HunterHut.upgrade_cost_wood(2) as f32)
                .abs()
                < 0.01,
            "upgrade must charge upgrade_cost_wood(2)"
        );
    }

    // Qurilish bitmaguncha navbatdagi yangilash rad etiladi (zanjir sharti).
    let wood_mid = state.stock.wood;
    sim::apply_command(&mut state, 1, &PlayerCommand::UpgradeBuilding { building: id });
    assert_eq!(state.find_building(id).unwrap().level, 2, "busy site must refuse an upgrade");
    assert_eq!(state.stock.wood, wood_mid, "a refused upgrade must not charge wood");

    // Zanjir maksimumgacha: har qadamda bitiramiz, keyin ko'taramiz.
    sim::finish_all_construction(&mut state);
    for target in 3..=BUILDING_MAX_LEVEL {
        sim::apply_command(&mut state, 1, &PlayerCommand::UpgradeBuilding { building: id });
        assert_eq!(state.find_building(id).unwrap().level, target);
        sim::finish_all_construction(&mut state);
    }

    // Maksimumda: rad, pul ushlanmaydi.
    let wood_at_max = state.stock.wood;
    sim::apply_command(&mut state, 1, &PlayerCommand::UpgradeBuilding { building: id });
    assert_eq!(state.find_building(id).unwrap().level, BUILDING_MAX_LEVEL);
    assert_eq!(state.stock.wood, wood_at_max);
}

#[test]
fn upgrade_requires_wood() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::HunterHut);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::HunterHut, x, y });
    let id = state.buildings.last().unwrap().id;
    sim::finish_all_construction(&mut state);

    state.stock.wood = 0.0;
    sim::apply_command(&mut state, 1, &PlayerCommand::UpgradeBuilding { building: id });
    assert_eq!(state.find_building(id).unwrap().level, 1, "no wood, no upgrade");
    assert_eq!(state.stock.wood, 0.0, "wood must never go negative");
}

#[test]
fn higher_level_produces_more() {
    let mut control = sim::new_game(SEED, 12);
    let mut experiment = sim::new_game(SEED, 12);
    for s in [&mut control, &mut experiment] {
        s.stock.wood = 500.0;
        let (x, y) = find_spot(s, BuildingKind::HunterHut);
        sim::apply_command(s, 1, &PlayerCommand::Place { kind: BuildingKind::HunterHut, x, y });
        let id = s.buildings.last().unwrap().id;
        sim::finish_all_construction(s);
        sim::apply_command(s, 1, &PlayerCommand::AdjustWorkers { building: id, delta: 2 });
        assert_eq!(s.find_building(id).unwrap().workers, 2);
    }
    // Faqat experiment darajasi ko'tariladi (to'g'ridan-to'g'ri, qurilishsiz —
    // sof ishlab-chiqarish farqini o'lchash uchun).
    let id = experiment.buildings.last().unwrap().id;
    experiment.buildings.iter_mut().find(|b| b.id == id).unwrap().level = 5;

    for _ in 0..TICKS_PER_DAY {
        sim::tick(&mut control);
        sim::tick(&mut experiment);
    }

    // L5 = +48% ishlab chiqarish: 2 ishchi * 10/kun * 0.48 ≈ +9.6 oziq/kun.
    assert!(
        experiment.stock.food > control.stock.food + 4.0,
        "L5 hut must clearly out-produce L1: control={}, experiment={}",
        control.stock.food,
        experiment.stock.food
    );
}

#[test]
fn tents_house_nobody_while_building_and_more_when_leveled() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    let base = state.housing_capacity();

    let (x, y) = find_spot(&state, BuildingKind::Tent);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y });
    assert_eq!(state.housing_capacity(), base, "an unfinished tent shelters nobody");

    sim::finish_all_construction(&mut state);
    assert_eq!(state.housing_capacity(), base + TENT_CAPACITY);

    let id = state.buildings.last().unwrap().id;
    state.buildings.iter_mut().find(|b| b.id == id).unwrap().level = 4;
    assert_eq!(
        state.housing_capacity(),
        base + TENT_CAPACITY + 3 * TENT_CAPACITY_PER_LEVEL,
        "each level above 1 must add TENT_CAPACITY_PER_LEVEL slots"
    );
}

#[test]
fn placement_without_wood_is_refused() {
    let mut state = sim::new_game(SEED, 12);
    let buildings_before = state.buildings.len();
    // Joy yog'och yetarli paytda tanlanadi (can_place narxni ham tekshiradi),
    // keyin zaxira kamaytirilib joylashtirish urinib ko'riladi.
    state.stock.wood = 500.0;
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    state.stock.wood = 5.0;
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y });
    assert_eq!(state.buildings.len(), buildings_before, "no wood, no site");
    assert_eq!(state.stock.wood, 5.0, "wood must never go negative");
}
