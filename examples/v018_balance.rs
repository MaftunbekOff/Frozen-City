//! V0.18 balance probe (dev tool, not shipped logic): what an expedition
//! actually pays per person-day compared with staying home and working, what
//! each law does to a colony's output/morale over a week, and how fast a
//! population ages into children and elders.
//!
//! Mirrors `v017_balance.rs`'s shape: a few seeded colonies, printed curves,
//! no assertions — the tests own correctness, this owns "is the number
//! sensible".
use fc_game::{sim, types::*};

/// A colony with food, fuel, a lit furnace and — crucially — HOUSING, so a
/// probe measures the thing it is probing instead of measuring frostbite.
///
/// The Tents are not optional comfort: `tick.rs` only grants the big
/// `12.0 + 6.0 * furnace_level` warmth bonus to survivors holding a warm
/// housing slot. With no Tent at all, everyone falls back to the flat 3.0
/// "huddling by the open fire" bonus no matter how strong the furnace is, and
/// since ambient temperature drops ~1.2 °C per day the whole colony freezes
/// within a few days — which is exactly what an earlier version of this probe
/// accidentally measured.
fn comfortable(seed: u64) -> GameState {
    let mut st = sim::new_game_bootstrapped(seed, 60);
    st.stock.food = 9e5;
    st.stock.water = 9e5;
    st.stock.coal = 9e5;
    st.stock.wood = 9e5;
    // `GameState::furnace_level` is the 0..=3 burn-power dial (`SetFurnaceLevel`
    // clamps it there); the 1..=10 tier is `Building::level`, set below. Using
    // 8 here would be a value the real game can never produce.
    st.furnace_level = 3;
    for b in st.buildings.iter_mut() {
        b.level = BUILDING_MAX_LEVEL;
    }
    for (x, y) in [(29u8, 29u8), (33, 29), (29, 33), (33, 33)] {
        sim::apply_command(&mut st, 0, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y, facing: 0 });
    }
    sim::finish_all_construction(&mut st);
    st
}

/// Send a party to `site` and report what it brought back per person-day,
/// next to what the same people would have produced at a Sawmill at home.
fn expedition_yield(site: ExpeditionSite) {
    let mut st = comfortable(21);
    // Fast-forward past `EXPEDITION_MIN_DAY` so the launch is legal.
    for _ in 0..(TICKS_PER_DAY * EXPEDITION_MIN_DAY as u64) {
        sim::tick(&mut st);
    }
    // Only healthy adults may travel (`can_launch_expedition`), and by day 3 a
    // seeded colony usually has someone ill — picking blindly off the front of
    // the roster silently refuses the whole launch.
    let party: Vec<u32> = st
        .survivors
        .iter()
        .filter(|s| !s.is_sick() && s.stage() != LifeStage::Child)
        .take(EXPEDITION_MAX_PARTY)
        .map(|s| s.id)
        .collect();
    let before = (st.stock.wood, st.stock.coal, st.stock.food, st.stock.fur);
    let pop_before = st.survivors.len();
    sim::apply_command(
        &mut st,
        0,
        &PlayerCommand::LaunchExpedition { site, members: party.clone() },
    );
    let away = st.people_away();
    // Run until they are home again (plus a day of slack).
    for _ in 0..((site.days() as u64 + 1) * TICKS_PER_DAY) {
        sim::tick(&mut st);
    }
    let (w, c, f, fu) = (
        st.stock.wood - before.0,
        st.stock.coal - before.1,
        st.stock.food - before.2,
        st.stock.fur - before.3,
    );
    let person_days = (away as f32 * site.days()).max(1.0);
    let hurt = st.survivors.iter().filter(|s| party.contains(&s.id) && s.hp < 60.0).count();
    println!(
        "{:<16} party {} x {:.0}d -> wood {:>7.1} coal {:>7.1} food {:>7.1} fur {:>6.1} \
         | per person-day: w {:>5.1} c {:>5.1} f {:>5.1} | hurt {} | pop {} -> {}",
        site.name(), away, site.days(), w, c, f, fu,
        w / person_days, c / person_days, f / person_days,
        hurt, pop_before, st.survivors.len(),
    );
}

/// A week under one law, printed against the same week with an empty book.
fn law_week(law: Option<Law>) {
    let mut st = comfortable(33);
    // Greenhouses, not Sawmills: a Sawmill's output is bounded by the forest
    // that happens to stand near it (`take_forest_unit`), so it saturates
    // within a day or two and every law then reads as "no effect". A
    // Greenhouse is the flat, uncapped producer — production law effects show
    // up in its food output undistorted. Staffed to their real `max_workers`
    // (assigning everyone would just be undone by `clamp_workers`).
    for (kind, x, y) in [
        (BuildingKind::Greenhouse, 29u8, 28u8),
        (BuildingKind::Greenhouse, 33, 28),
    ] {
        sim::apply_command(&mut st, 0, &PlayerCommand::Place { kind, x, y, facing: 0 });
    }
    sim::finish_all_construction(&mut st);
    let houses: Vec<u32> = st
        .buildings
        .iter()
        .filter(|b| b.kind == BuildingKind::Greenhouse)
        .map(|b| b.id)
        .collect();
    let mut ids = st.survivors.iter().map(|s| s.id).collect::<Vec<_>>().into_iter();
    for house in houses {
        for _ in 0..BuildingKind::Greenhouse.max_workers() {
            if let Some(id) = ids.next() {
                sim::apply_command(
                    &mut st,
                    0,
                    &PlayerCommand::AssignSurvivor { survivor: id, building: Some(house) },
                );
            }
        }
    }
    // The same warm-up in BOTH branches, law or no law: the measured window
    // has to start on the same in-game day, or the comparison is really just
    // measuring how much forest was still standing when it began.
    for _ in 0..(TICKS_PER_DAY * LAW_MIN_DAY as u64) {
        sim::tick(&mut st);
    }
    if let Some(law) = law {
        sim::apply_command(&mut st, 0, &PlayerCommand::EnactLaw { law });
        assert!(st.has_law(law), "the probe must actually enact {}", law.name());
    }
    let food_before = st.stock.food;
    let xp_before: f32 = st.survivors.iter().map(|s| s.xp).sum();
    for _ in 0..(TICKS_PER_DAY * 7) {
        sim::tick(&mut st);
    }
    let pop = st.survivors.len().max(1);
    println!(
        "{:<16} food {:>+8.1} xp {:>+6.2} prod_mult {:>4.2} morale {:>5.1} \
         avg_fatigue {:>5.1} sick {} pop {}",
        law.map(|l| l.name()).unwrap_or("(no law)"),
        // Net of what the colony ate, which is the point: Extra Rations must
        // show up as a WORSE net than Communal Meals at identical output.
        st.stock.food - food_before,
        st.survivors.iter().map(|s| s.xp).sum::<f32>() - xp_before,
        st.colony_production_multiplier(),
        st.morale,
        st.survivors.iter().map(|s| s.fatigue).sum::<f32>() / pop as f32,
        st.sick_count(),
        st.survivors.len(),
    );
}

/// How the colony's age profile drifts over a long run — the question being
/// whether a city ever ends up mostly non-working (children + elders).
fn age_profile(days: u64) {
    let mut st = comfortable(47);
    println!("--- age profile over {days} days ---");
    for day in 0..days {
        for _ in 0..TICKS_PER_DAY {
            sim::tick(&mut st);
        }
        if day % 10 != 9 {
            continue;
        }
        let (mut kids, mut adults, mut elders) = (0, 0, 0);
        for s in &st.survivors {
            match s.stage() {
                LifeStage::Child => kids += 1,
                LifeStage::Adult => adults += 1,
                LifeStage::Elder => elders += 1,
            }
        }
        let couples = st.survivors.iter().filter(|s| s.partner.is_some()).count() / 2;
        println!(
            "day {:>3} pop {:>2} (children {} adults {} elders {}) couples {} morale {:>5.1}",
            st.day(), st.survivors.len(), kids, adults, elders, couples, st.morale,
        );
    }
}

fn main() {
    println!("=== expeditions ===");
    for site in ExpeditionSite::ALL {
        expedition_yield(site);
    }
    println!("\n=== laws (one week each) ===");
    law_week(None);
    for law in Law::ALL {
        law_week(Some(law));
    }
    println!();
    age_profile(120);
}
