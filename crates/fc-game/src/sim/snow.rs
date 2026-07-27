//! V0.19: snow and roads — the weather's mark on the map itself.
//!
//! Everything before this treated the ground as uniform: a survivor crossed
//! any tile at the same speed, and a blizzard was purely a temperature event.
//! Now weather leaves something behind. Snow piles up on every tile (much
//! faster during a blizzard), and what it costs is TIME: crossing deep drift
//! is slow, crossing a cleared road is fast, and a road that fills in stops
//! being a road until somebody clears it.
//!
//! Snow never blocks. See [`crate::types::Tile::speed_factor`] for why that
//! is an invariant rather than a balance choice.

use crate::types::*;

use super::{push_event, survivor_contribution};

/// How many whole units of a `per_tick` rate are due by the end of `tick`,
/// counting from tick 0. `units_due(t, r) - units_due(t - 1, r)` is the exact
/// number of units to apply on tick `t`, and summed over any run of ticks it
/// always totals `floor(ticks * r)` — unlike a fixed "every N ticks" cadence
/// (`N = (1.0 / r) as u64`), which silently over-delivers whenever `r`
/// doesn't divide evenly into 1.0. That's not a hypothetical: `750.0 / 34.0`
/// (`TICKS_PER_DAY` against the blizzard snowfall rate) truncates to 22
/// rather than 22.06, so a fixed-interval cadence built from it fires every
/// 22 ticks forever — a rate of 750/22 ≈ 34.09/day, not 34, permanently.
/// Shared by the snowfall, trampling and Snow Crew cadences below, all of
/// which have the same "a tick's worth is less than one whole unit" problem.
/// Takes a PER-DAY rate and does the division itself, in integer arithmetic
/// on thousandths. Both details matter: `(tick as f32 * per_tick)` drifts at
/// exactly the boundary that matters — `52.0 / 750.0 * 750.0` is 51.99999 in
/// f32, so a full blizzard day delivered 51 units instead of 52 and the rate
/// silently came up one short every single day.
fn units_due(tick: u64, per_day: f32) -> u64 {
    let milli_per_day = (per_day * 1000.0).round() as u128;
    ((tick as u128) * milli_per_day / (TICKS_PER_DAY as u128 * 1000)) as u64
}

/// Per-tick snow accumulation across the whole map, plus the trampling that
/// keeps trodden ground a little clearer. Called from `sim::tick`.
pub fn tick_snowfall(state: &mut GameState) {
    // The central world has no weather at all (`tick` returns before the
    // blizzard/event block for it), so nothing should quietly bury it.
    if state.central {
        return;
    }
    let per_day = if state.blizzard_active() {
        SNOW_FALL_BLIZZARD_PER_DAY
    } else {
        SNOW_FALL_PER_DAY
    };
    // Snow is a byte per tile, and a tick's worth of fall is a small
    // fraction of one unit — accumulating it directly would truncate to zero
    // every tick and nothing would ever pile up. So fall is applied via
    // `units_due` instead: exactly the ticks where the running total crosses
    // a whole number get +1, which lands on the exact `per_day` rate (see
    // `units_due`'s doc for why a fixed interval doesn't).
    if per_day <= 0.0 {
        return;
    }
    let prev = state.tick.saturating_sub(1);
    let units = units_due(state.tick, per_day).saturating_sub(units_due(prev, per_day));
    if units > 0 {
        let add = units.min(SNOW_MAX as u64) as u8;
        for t in state.tiles.iter_mut() {
            t.snow = t.snow.saturating_add(add).min(SNOW_MAX);
        }
    }

    // Trampling: the tile each survivor is standing on packs down. Same
    // `units_due` cadence, on the trample rate.
    let trample_per_day = SNOW_TRAMPLE_PER_DAY.max(0.01);
    let trample_due =
        units_due(state.tick, trample_per_day) > units_due(prev, trample_per_day);
    if trample_due {
        // Deduplicated: several survivors sharing one tile still pack it
        // down by one unit, not one unit EACH — trampling models the path
        // staying clear, not a race to stomp it flattest. A plain Vec here
        // (the original shape) would apply `saturating_sub(1)` once per
        // survivor on the tile, over-clearing anywhere people cluster (a
        // doorway, the Furnace, a Tent) far past `SNOW_TRAMPLE_PER_DAY`.
        let here: std::collections::HashSet<usize> = state
            .survivors
            .iter()
            .filter_map(|s| {
                let (tx, ty) = (s.x.floor(), s.y.floor());
                in_bounds(tx as i32, ty as i32).then(|| tile_index(tx as u8, ty as u8))
            })
            .collect();
        for idx in here {
            if let Some(t) = state.tiles.get_mut(idx) {
                t.snow = t.snow.saturating_sub(1);
            }
        }
    }
}

/// A staffed Snow Crew keeps its radius clear. Mirrors the Hospital's shape:
/// the effect scales with `survivor_contribution` units (so a rested, trained
/// crew works visibly faster), and it is applied as an effect in the tick
/// rather than as a credit to the stockpile.
pub fn tick_snow_crews(state: &mut GameState) {
    if state.central {
        return;
    }
    let crews: Vec<(u8, u8, f32)> = state
        .buildings
        .iter()
        .filter(|b| b.kind == BuildingKind::SnowCrew && b.workers > 0 && !b.under_construction())
        .map(|b| {
            let named: f32 = state
                .survivors
                .iter()
                .filter(|s| s.assigned_building == Some(b.id))
                .map(|s| survivor_contribution(s, BuildingKind::SnowCrew, state.leader))
                .sum();
            let named_count = state
                .survivors
                .iter()
                .filter(|s| s.assigned_building == Some(b.id))
                .count() as u8;
            // Anonymous headcount contributes a flat 1.0 each, exactly as it
            // does in the production loop.
            let units = named + b.workers.saturating_sub(named_count) as f32;
            (b.x, b.y, units * b.level_factor())
        })
        .collect();
    if crews.is_empty() {
        return;
    }
    // Budget in snow-units per tick, spent on the deepest tiles in range
    // first: a crew should reopen the buried stretch of road before it
    // bothers tidying ground that was nearly clear anyway.
    for (bx, by, units) in crews {
        let per_day = units * SNOW_CREW_CLEAR_PER_UNIT_DAY;
        let mut budget = per_day / TICKS_PER_DAY as f32;
        if budget < 1.0 {
            // Sub-unit budgets would truncate to nothing every tick (a tiny
            // crew "spending" 0.12 units/tick never actually removes a whole
            // unit of snow, since the spend below floors to u8) — so, same
            // as the snowfall rate above, a tiny crew fires on a `units_due`
            // cadence instead: exactly one whole unit, on the tick(s) where
            // the running total crosses a whole number. That lands on the
            // real `SNOW_CREW_CLEAR_PER_UNIT_DAY` rate over time; a fixed
            // "every N ticks" interval built by truncating `1.0 / budget`
            // (the original shape here) fires slightly too often forever,
            // the same bias `units_due`'s doc explains for the blizzard rate.
            let prev = state.tick.saturating_sub(1);
            if units_due(state.tick, per_day) <= units_due(prev, per_day) {
                continue;
            }
            budget = 1.0;
        }
        let mut targets: Vec<(u8, usize)> = Vec::new();
        for dy in -SNOW_CREW_RADIUS..=SNOW_CREW_RADIUS {
            for dx in -SNOW_CREW_RADIUS..=SNOW_CREW_RADIUS {
                let (tx, ty) = (bx as i32 + dx, by as i32 + dy);
                if !in_bounds(tx, ty) {
                    continue;
                }
                let idx = tile_index(tx as u8, ty as u8);
                let snow = state.tiles[idx].snow;
                if snow > 0 {
                    // Roads first at equal depth: the network is the thing
                    // worth keeping open.
                    let priority = if state.tiles[idx].road { snow.saturating_add(1) } else { snow };
                    targets.push((priority, idx));
                }
            }
        }
        targets.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        for (_, idx) in targets {
            if budget < 1.0 {
                break;
            }
            let take = (state.tiles[idx].snow as f32).min(budget);
            state.tiles[idx].snow -= take as u8;
            budget -= take;
        }
    }
}

/// Hand-shovelling: each `ClearOrder` counts down only while the survivor it
/// names is actually standing on the tile, then clears it. Mirrors the burial
/// block's shape exactly (`Corpse::bury_left`).
pub fn tick_clear_orders(state: &mut GameState) {
    if state.clear_orders.is_empty() {
        return;
    }
    let mut done: Vec<(u8, u8)> = Vec::new();
    let mut abandoned: Vec<usize> = Vec::new();
    for (i, order) in state.clear_orders.iter_mut().enumerate() {
        let Some(s) = state.survivors.iter().find(|s| s.id == order.survivor) else {
            // Whoever was going never arrived (died, or left through the
            // Tunnel) — drop the order rather than leaving it forever.
            abandoned.push(i);
            continue;
        };
        let at = s.x.floor() as i32 == order.x as i32 && s.y.floor() as i32 == order.y as i32;
        if !at {
            continue;
        }
        order.work_left -= 1.0;
        if order.work_left <= 0.0 {
            done.push((order.x, order.y));
        }
    }
    for i in abandoned.into_iter().rev() {
        state.clear_orders.remove(i);
    }
    // Nothing outside this module actually lets two orders land on the same
    // tile (`apply_command`'s `ClearSnow` handler re-points an existing
    // order rather than duplicating it), but this function shouldn't rely on
    // a caller-side invariant it can't see — and if it ever did happen, both
    // orders finishing the same tick would otherwise push the SAME (x, y)
    // twice below, clearing it (harmlessly) and logging "was cleared" twice
    // for one shovel-load of snow. Dedupe first so one tile finishing is
    // always exactly one event.
    done.sort_unstable();
    done.dedup();
    for (x, y) in done {
        let idx = tile_index(x, y);
        if let Some(t) = state.tiles.get_mut(idx) {
            t.snow = 0;
        }
        state.clear_orders.retain(|o| !(o.x == x && o.y == y));
        push_event(state, format!("The snow at ({}, {}) was cleared.", x, y));
    }
}
