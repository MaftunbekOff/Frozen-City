//! V0.17 balance probe (dev tool, not shipped logic): the fatigue curve for a
//! worker with a Tent bunk versus one sleeping rough, and the illness curve
//! with and without a staffed Hospital.
use fc_game::{sim, types::*};

fn fatigue_curve(tents: usize, label: &str) {
    let mut st = sim::new_game_bootstrapped(9, 60);
    st.stock.food = 9e5;
    st.stock.water = 9e5;
    st.stock.coal = 9e5;
    st.stock.wood = 9e5;
    st.furnace_level = 8;
    for b in st.buildings.iter_mut() {
        b.level = 8;
    }
    for (i, (x, y)) in [(30u8, 30u8), (32, 30)].iter().enumerate() {
        if i < tents {
            sim::apply_command(&mut st, 0, &PlayerCommand::Place { kind: BuildingKind::Tent, x: *x, y: *y, facing: 0 });
        }
    }
    sim::apply_command(&mut st, 0, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x: 30, y: 27, facing: 0 });
    sim::finish_all_construction(&mut st);
    let mill = st.buildings.iter().find(|b| b.kind == BuildingKind::Sawmill).unwrap().id;
    // Everyone works: the fatigue question is about a fully-staffed colony.
    for s in st.survivors.iter_mut() {
        s.assigned_building = Some(mill);
    }
    println!("--- {label} (pop {}, housing {}) ---", st.survivors.len(), st.housing_capacity());
    for _ in 0..8 {
        for _ in 0..TICKS_PER_DAY {
            sim::tick(&mut st);
        }
        let pop = st.survivors.len().max(1);
        let avg = st.survivors.iter().map(|s| s.fatigue).sum::<f32>() / pop as f32;
        let worst = st.survivors.iter().map(|s| s.fatigue).fold(0.0f32, f32::max);
        println!(
            "day {:>2} pop {:>2} avg_fatigue {:>5.1} worst {:>5.1} exhausted {} output_scale {:>4.2}",
            st.day(), st.survivors.len(), avg, worst, st.exhausted_count(),
            st.survivors.first().map(|s| s.fatigue_factor()).unwrap_or(1.0)
        );
    }
}

fn illness_curve(hospital: bool) {
    let mut st = sim::new_game_bootstrapped(11, 60);
    st.stock.food = 9e5;
    st.stock.water = 9e5;
    st.stock.coal = 9e5;
    st.furnace_level = 8;
    if hospital {
        sim::apply_command(&mut st, 0, &PlayerCommand::Place { kind: BuildingKind::Hospital, x: 33, y: 31, facing: 0 });
        sim::finish_all_construction(&mut st);
        let hosp = st.buildings.iter().find(|b| b.kind == BuildingKind::Hospital).unwrap().id;
        let medic = st.survivors.iter().find(|s| s.profession == Profession::Medic).map(|s| s.id);
        if let Some(id) = medic {
            sim::apply_command(&mut st, 0, &PlayerCommand::AssignSurvivor { survivor: id, building: Some(hosp) });
        }
    }
    st.survivors[0].sick_left = SICKNESS_TICKS;
    let patient = st.survivors[0].id;
    let start_hp = st.survivors[0].hp;
    let find = |st: &GameState| st.survivors.iter().find(|s| s.id == patient).cloned();
    let mut days = 0.0;
    while find(&st).map(|s| s.is_sick()).unwrap_or(false) && days < 6.0 {
        sim::tick(&mut st);
        days += 1.0 / TICKS_PER_DAY as f32;
    }
    println!(
        "hospital={:<5} recovery {:.2} days, hp {:.0} -> {:.0}, alive {}, colony sick {}",
        hospital, days, start_hp,
        find(&st).map(|s| s.hp).unwrap_or(0.0), find(&st).is_some(), st.sick_count()
    );
}

fn main() {
    fatigue_curve(2, "bunks for everyone");
    fatigue_curve(0, "no tents (sleeping rough)");
    illness_curve(false);
    illness_curve(true);
}
