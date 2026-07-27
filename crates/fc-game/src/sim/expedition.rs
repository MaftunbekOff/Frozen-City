//! V0.18: expedition lifecycle — launch, recall, and the return that pays
//! out. See [`crate::types::Expedition`] for the data model and, in
//! particular, for why party members are lifted out of `GameState.survivors`
//! for the whole trip instead of staying in the roster behind an "away" flag.
//!
//! Every roll here draws from `GameState.expedition_rng`, a stream of its own,
//! so launching a party never re-sequences arrivals, births, weather or
//! infection (the same isolation `illness_rng` buys the V0.17 systems).

use crate::rng::Rng;
use crate::types::*;

use super::push_event;

/// Apply `PlayerCommand::LaunchExpedition`. The caller has already checked
/// [`GameState::can_launch_expedition`]; this re-checks anyway, since
/// `apply_command` is the authority and a racing tick may have killed a
/// member between the client's click and this call.
pub fn launch(state: &mut GameState, site: ExpeditionSite, members: &[u32]) {
    if state.can_launch_expedition(site, members).is_err() {
        return;
    }
    // Provisions are charged up front, in full, for the WHOLE planned trip —
    // a party that turns back early has still eaten on the road, so a recall
    // never refunds food.
    let provisions =
        site.provisions_per_member_day() * site.days() * members.len() as f32;
    if state.stock.food < provisions {
        return;
    }
    state.stock.food -= provisions;

    let mut party: Vec<Survivor> = Vec::with_capacity(members.len());
    for id in members {
        let Some(idx) = state.survivors.iter().position(|s| s.id == *id) else {
            continue;
        };
        // Free the building slot exactly like the death/migration paths do —
        // an away worker must not hold a post nobody is standing in.
        if let Some(b_id) = state.survivors[idx].assigned_building {
            if let Some(b) = state.buildings.iter_mut().find(|b| b.id == b_id) {
                b.workers = b.workers.saturating_sub(1);
            }
        }
        // The leader may lead an expedition — but the seat travels with them,
        // exactly as it does through the Tunnel (`extract_migrants`), so
        // `leader_alive` never points at someone who isn't in the valley.
        if state.leader == Some(*id) {
            state.leader = None;
            push_event(state, "The leader set out with the expedition.");
        }
        let mut s = state.survivors.remove(idx);
        s.assigned_building = None;
        s.move_target = None;
        s.chop_target = None;
        s.bury_target = None;
        s.carrying_wood = false;
        party.push(s);
    }
    if party.is_empty() {
        // Everyone vanished between validation and here; refund and stop.
        state.stock.food += provisions;
        return;
    }
    let id = state.next_id;
    state.next_id += 1;
    let count = party.len();
    state.expeditions.push(Expedition {
        id,
        site,
        party,
        departed_tick: state.tick,
        return_tick: state.tick + site.trip_ticks(),
        recalled: false,
    });
    push_event(
        state,
        format!("{} set out for the {}.", count, site.name()),
    );
}

/// Apply `PlayerCommand::RecallExpedition`: the party turns around now. They
/// arrive after walking back however far they'd walked out, and are paid only
/// for the days they actually spent out there (see [`resolve_return`]).
pub fn recall(state: &mut GameState, expedition: u32) {
    let now = state.tick;
    let Some(e) = state.expeditions.iter_mut().find(|e| e.id == expedition) else {
        return;
    };
    if e.recalled {
        return;
    }
    e.recalled = true;
    // Home is as far away as they've come: the walk back mirrors the walk
    // out, so a party recalled on day 1 of a 4-day trip is home on day 2, not
    // instantly and not on day 4.
    let out = now.saturating_sub(e.departed_tick);
    e.return_tick = now + out;
    let site = e.site;
    push_event(state, format!("The party at the {} was called home.", site.name()));
}

/// Per-tick expedition upkeep, called from `sim::tick`: returns every party
/// whose `return_tick` has come.
pub fn tick_expeditions(state: &mut GameState) {
    if state.expeditions.is_empty() {
        return;
    }
    let now = state.tick;
    let due: Vec<u32> = state
        .expeditions
        .iter()
        .filter(|e| e.is_due(now))
        .map(|e| e.id)
        .collect();
    for id in due {
        resolve_return(state, id);
    }
}

/// Bring one party home: roll hazards, credit the haul, put the survivors
/// back in the roster. Split out from [`tick_expeditions`] so tests can drive
/// a single return directly.
pub fn resolve_return(state: &mut GameState, expedition: u32) {
    let Some(idx) = state.expeditions.iter().position(|e| e.id == expedition) else {
        return;
    };
    let e = state.expeditions.remove(idx);
    let mut rng = Rng(state.expedition_rng);
    let now = state.tick;
    let days = e.days_travelled(now);
    let site = e.site;

    // --- Haul: base site rate x party size x days actually travelled, then
    // the party's own quality (profession/XP/condition) as a multiplier.
    let base = site.haul_per_member_day();
    let mut skill = 0.0f32;
    for s in &e.party {
        // An expedition is general labour: everyone contributes their
        // condition and their level, but no profession matches "the road", so
        // this deliberately does NOT use `survivor_contribution` (which is
        // building-shaped) — it uses the same factors minus the trade bonus.
        skill += s.fatigue_factor()
            * s.age_work_factor()
            * (1.0 + xp_level(s.xp) as f32 * XP_LEVEL_BONUS_PER_LEVEL);
    }
    let haul = ExpeditionHaul {
        wood: base.wood * skill * days,
        coal: base.coal * skill * days,
        food: base.food * skill * days,
        fur: base.fur * skill * days,
    };
    state.stock.wood += haul.wood;
    state.stock.coal += haul.coal;
    state.stock.food += haul.food;
    state.stock.fur += haul.fur;

    // --- Hazards: one roll per member per day out there.
    let mut hurt: Vec<String> = Vec::new();
    let mut party = e.party;
    for s in party.iter_mut() {
        let rolls = days.round().max(1.0) as u32;
        let mut struck = false;
        for _ in 0..rolls {
            if rng.chance(site.hazard_per_member_day()) {
                struck = true;
                s.hp = (s.hp - EXPEDITION_HAZARD_HP).max(EXPEDITION_MIN_RETURN_HP);
                if rng.chance(EXPEDITION_HAZARD_SICK_CHANCE) {
                    s.sick_left = SICKNESS_TICKS;
                }
            }
        }
        if struck {
            hurt.push(s.name.clone());
        }
        s.fatigue = (s.fatigue + EXPEDITION_FATIGUE_PER_DAY * days).clamp(0.0, 100.0);
        s.xp += EXPEDITION_XP_PER_DAY * days;
        s.age_days += days;
        // Re-place them at the Tunnel-side edge of the colony: they walk back
        // in rather than teleporting to wherever they stood four days ago.
        let (x, y) = GameState::spawn_position(s.id);
        s.x = x;
        s.y = y;
        s.move_target = None;
    }

    // --- Rescue: the Ruined Town is the only site with anyone left to find.
    // Drawn from the SAME local `rng` (backed by `expedition_rng`) as the
    // decision roll just above, mirroring how an accepted caravan generates
    // its refugees from `event_rng` (`command.rs`'s `RespondEvent` handler)
    // rather than `rng` — a rescue is still an expedition outcome, not an
    // arrival, and must never re-sequence the main stream (see the module
    // doc). This used to draw from `state.rng` directly, which quietly broke
    // that guarantee on every Ruined Town trip that actually rescued someone.
    let mut rescued = 0u32;
    if rng.chance(site.rescue_chance()) {
        let room = (MAX_POPULATION as usize).saturating_sub(state.survivors.len() + party.len());
        if room > 0 {
            let s = super::new_survivor(&mut rng, &mut state.next_id);
            party.push(s);
            rescued = 1;
        }
    }

    state.survivors.extend(party);

    // --- Morale: a whole party coming home lifts the city; a hurt one
    // doesn't. Both are one-off event adjustments, like a death, not rates.
    let delta = if hurt.is_empty() {
        MORALE_EXPEDITION_RETURN
    } else {
        -MORALE_EXPEDITION_HURT
    };
    state.morale = (state.morale + delta).clamp(0.0, 100.0);

    let what = if e.recalled { "returned early" } else { "returned" };
    push_event(
        state,
        format!(
            "The party {} from the {}: {} wood, {} coal, {} food, {} fur.",
            what,
            site.name(),
            haul.wood as i64,
            haul.coal as i64,
            haul.food as i64,
            haul.fur as i64,
        ),
    );
    for name in hurt {
        push_event(state, format!("{} came back hurt.", name));
    }
    if rescued > 0 {
        push_event(state, "They brought someone home from the ruins.");
    }
    state.expedition_rng = rng.0;
}
