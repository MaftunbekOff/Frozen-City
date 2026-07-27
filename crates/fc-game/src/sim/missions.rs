//! V0.18: repeatable mission cycles.
//!
//! V0.3's five missions were the road to the Tunnel and nothing after it: the
//! moment the last one completed, the colony's list of things to *aim at*
//! went permanently empty, which is exactly the point most sessions have the
//! least to do. This issues a fresh, harder cycle each time the current one
//! is cleared.
//!
//! The Tunnel's unlock (`TunnelState::unlocked`) is a latch set once in
//! `sim::tick` and never cleared, so replacing the mission list can never
//! re-lock it — a graduated colony stays graduated.

use crate::types::*;

use super::push_event;

/// Scale a target for `cycle` (0 = the original list): each cycle multiplies
/// by `MISSION_CYCLE_TARGET_GROWTH`, so growth compounds rather than adding a
/// flat step that late cycles wouldn't feel.
fn scaled(base: u32, cycle: u32, growth: f32) -> u32 {
    let scale = growth.powi(cycle as i32);
    ((base as f32 * scale).round() as u32).max(base)
}

/// The mission list for `cycle`, derived from the same five kinds the opening
/// cycle uses. `SurviveDays` is the one target that must be read as "days
/// from now" rather than "day number", so it's built against `day_now`.
///
/// Every kind must stay COMPLETABLE IN PRINCIPLE at every cycle up to
/// `MAX_MISSION_CYCLES` — checked one by one, since `scaled`'s compounding
/// growth (1.6^cycle) will eventually outrun any kind that has a hard ceiling:
/// - `Population`: `MAX_POPULATION` (60) is a hard, unconditionally enforced
///   cap (every arrival/birth/migrant/expedition-return path in `tick.rs`/
///   `players.rs`/`command.rs` gates on it) — population can NEVER exceed it,
///   so `t(10)` is clamped to it below. Without the clamp, cycle 4 alone asks
///   for 66 (1.6^4 * 10, rounded) — the mission (and every cycle after it,
///   since `tick_mission_cycles` never advances past an uncompletable one)
///   would simply never clear again.
/// - `BuildTents`/`Sawmills`: bounded by buildable space (thousands of tiles
///   on the 64x64 map) and by wood income, not by any hard game rule — slow
///   at high cycles, never literally impossible, so left unclamped.
/// - `StockpileCoal`: nothing in the sim ever caps `Stockpile.coal` — purely
///   a matter of banking time, so also left unclamped.
/// - `SurviveDays`: unbounded by construction (every day lived satisfies a
///   strictly larger `day()`), never at risk.
pub fn cycle_missions(cycle: u32, day_now: u32) -> Vec<Mission> {
    let t = |base: u32| scaled(base, cycle, MISSION_CYCLE_TARGET_GROWTH);
    let r = |base: u32| scaled(base, cycle, MISSION_CYCLE_REWARD_GROWTH);
    let population_target = t(10).min(MAX_POPULATION as u32);
    vec![
        Mission { kind: MissionKind::BuildTents(t(2)), reward_wood: r(20), reward_coal: 0, reward_food: 0, done: false },
        Mission { kind: MissionKind::Population(population_target), reward_wood: 0, reward_coal: 0, reward_food: r(20), done: false },
        Mission { kind: MissionKind::Sawmills(t(1)), reward_wood: r(20), reward_coal: 0, reward_food: 0, done: false },
        Mission { kind: MissionKind::StockpileCoal(t(60)), reward_wood: r(20), reward_coal: 0, reward_food: 0, done: false },
        // Always a fresh horizon: "survive N more days from today", never a
        // day number already behind the colony (which would complete itself
        // the instant it was issued).
        Mission {
            kind: MissionKind::SurviveDays(day_now + t(4)),
            reward_wood: 0,
            reward_coal: 0,
            reward_food: r(30),
            done: false,
        },
    ]
}

/// Called from `sim::tick` right after the mission/Tunnel block: when every
/// mission in the current cycle is done, issue the next (harder) one.
pub fn tick_mission_cycles(state: &mut GameState) {
    // The central world has no personal progression at all (`new_game_central`
    // clears the list, and an empty list must stay empty — `all_missions_done`
    // is false on it by design).
    if state.central || state.missions.is_empty() {
        return;
    }
    if !state.all_missions_done() {
        return;
    }
    if state.mission_cycle >= MAX_MISSION_CYCLES {
        return;
    }
    // Only ever issue a new cycle once the Tunnel gate has actually latched —
    // otherwise a colony that cleared the opening five could have them
    // replaced in the same tick, before `tick` reads `all_missions_done` for
    // the unlock, and the Tunnel would never open.
    if !state.tunnel.unlocked {
        return;
    }
    state.mission_cycle += 1;
    let cycle = state.mission_cycle;
    let day = state.day();
    state.missions = cycle_missions(cycle, day);
    push_event(
        state,
        format!("The council set out new goals for the city (round {}).", cycle + 1),
    );
}
