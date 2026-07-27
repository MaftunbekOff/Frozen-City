use crate::rng::Rng;
use crate::types::*;
use super::*;

/// Sawmills harvest forest tiles within this Chebyshev radius.
pub const SAWMILL_RADIUS: i32 = 4;

/// Advance the world by one tick (200 ms of real time).
pub fn tick(state: &mut GameState) {
    if state.phase != GamePhase::Running {
        return;
    }
    let mut rng = Rng(state.rng);
    let mut erng = Rng(state.event_rng);
    state.tick += 1;

    // --- Expire stale map pings ---
    let tick = state.tick;
    state
        .pings
        .retain(|p| tick.saturating_sub(p.tick) < PING_TTL_TICKS);

    // --- Midnight: day rollover ---
    // The central world is a permanent meeting place, not a survival run: no
    // day-count victory, no weather forecast drama, no disease/blizzard
    // events — days just pass.
    if state.tick.is_multiple_of(TICKS_PER_DAY) && !state.central {
        let day = state.day();
        if day > state.win_days {
            state.phase = GamePhase::Won;
            state.pings.clear();
            push_event(
                state,
                format!("The city has survived {} days. Victory!", state.win_days),
            );
            state.rng = rng.0;
            state.event_rng = erng.0;
            return;
        }
        state.cold_snap = day >= 3 && rng.chance(0.30);
        if state.cold_snap {
            push_event(state, "Forecast: a brutal cold snap will strike tonight!");
        }

        if state.day() >= EVENT_GRACE_DAY && !state.disease_active() && erng.chance(DISEASE_CHANCE) {
            state.disease_until = state.tick + DISEASE_TICKS;
            push_event(state, "A sickness is spreading through the city.");
        }
        if state.day() >= EVENT_GRACE_DAY && !state.blizzard_active() && erng.chance(BLIZZARD_CHANCE) {
            state.blizzard_until = state.tick + BLIZZARD_TICKS;
            push_event(state, "A blizzard is closing in — brace for deep cold.");
        }
    }

    // --- Furnace fuel ---
    if state.furnace_level > 0 {
        let furnace_factor = if state.has_tech(Tech::EfficientFurnace) {
            TECH_FURNACE_EFFICIENCY
        } else {
            1.0
        };
        let need_coal = state.furnace_level as f32 * FURNACE_COAL_PER_DAY_PER_LEVEL
            / TICKS_PER_DAY as f32
            * furnace_factor;
        let lit = if state.stock.coal >= need_coal {
            state.stock.coal -= need_coal;
            true
        } else {
            let need_wood = need_coal * WOOD_FUEL_PENALTY;
            if state.stock.wood >= need_wood {
                state.stock.wood -= need_wood;
                true
            } else {
                false
            }
        };
        if state.furnace_lit && !lit {
            push_event(state, "The furnace has gone out — no fuel!");
        }
        state.furnace_lit = lit;
    } else {
        state.furnace_lit = false;
    }

    // --- V0.8 Construction: ustalar bitmagan maydonchalarni bitiradi. Sayt
    // bitgunicha hech narsa ishlab chiqarmaydi va hech kimni sig'dirmaydi
    // (production/housing hisoblari `under_construction`ni tekshiradi). ---
    for i in 0..state.buildings.len() {
        let (crew, left, kind) = {
            let b = &state.buildings[i];
            (b.workers, b.build_left, b.kind)
        };
        // While the Furnace is still on its very first (unlit) construction,
        // `build_left` is cleared by chop-and-carry log deliveries instead
        // (see the dedicated block further down) — the generic worker-days
        // tick-down below doesn't apply yet. Once it's been lit at least
        // once (`state.furnace_level > 0`, which — unlike the moment-to-
        // moment `furnace_lit` fuel flag above — never resets back to 0),
        // any further `build_left` came from a V0.9 level-1-10 upgrade
        // (`UpgradeBuilding`) and progresses exactly like every other
        // building's construction.
        if left <= 0.0
            || crew == 0
            || (kind == BuildingKind::Furnace && state.furnace_level == 0)
        {
            continue;
        }
        let mut finished: Option<(BuildingKind, u32, u8, u8)> = None;
        {
            let b = &mut state.buildings[i];
            b.build_left -= crew as f32 / TICKS_PER_DAY as f32;
            if b.build_left <= 0.0 {
                b.build_left = 0.0;
                // Bitgan bino o'zi band qila oladigan ishchinigina saqlab
                // qoladi; ortiqcha ustalar bo'sh ishchilar safiga qaytadi.
                b.workers = b.workers.min(b.kind.max_workers());
                finished = Some((b.kind, b.id, b.kind.max_workers(), b.level));
            }
        }
        if let Some((kind, id, max, level)) = finished {
            // Sig'imdan ortiq NOMLANGAN ustalar ham bo'shatiladi, workers >=
            // named-floor invarianti buzilmasin (birinchi `max`tasi qoladi —
            // deterministik).
            let mut named = 0u8;
            for s in state.survivors.iter_mut() {
                if s.assigned_building == Some(id) {
                    named += 1;
                    if named > max {
                        s.assigned_building = None;
                    }
                }
            }
            // The Furnace's very first ignition never reaches this arm (it's
            // excluded above while `state.furnace_level == 0`, and completes
            // via the dedicated chop-and-carry block instead) — so by the
            // time a Furnace shows up here, `level` is always a genuine V0.9
            // upgrade (>= 2), same as any other building's.
            //
            // V0.14: a relocated building also completes through this same
            // generic arm (see `RelocateBuilding`) and its own start-of-move
            // event already told the player what's happening — `level` alone
            // can't distinguish "just relocated" from "just upgraded" here,
            // so a relocated building already above L1 logs as "upgraded"
            // even though its level didn't change. Cosmetic only; harmless.
            if level > 1 {
                push_event(state, format!("{} upgraded to L{}.", kind.name(), level));
            } else {
                push_event(state, format!("{} complete.", kind.name()));
            }
        }
    }

    // --- V0.10: wildlife regen (deer/rabbit) ---
    // Runs unconditionally, whether or not a HunterHut exists — same as
    // forest/coal deposits existing on the map before any Sawmill/CoalMine
    // is built. Logistic growth toward each cap: proportional to current
    // population, so a population hunted near zero regenerates very slowly
    // (graceful degradation instead of a hard floor/extinction state).
    let deer_growth = DEER_REGEN_PER_DAY / TICKS_PER_DAY as f32
        * state.wildlife.deer
        * (1.0 - state.wildlife.deer / DEER_CAP).max(0.0);
    state.wildlife.deer = (state.wildlife.deer + deer_growth).clamp(0.0, DEER_CAP);
    let rabbit_growth = RABBIT_REGEN_PER_DAY / TICKS_PER_DAY as f32
        * state.wildlife.rabbit
        * (1.0 - state.wildlife.rabbit / RABBIT_CAP).max(0.0);
    state.wildlife.rabbit = (state.wildlife.rabbit + rabbit_growth).clamp(0.0, RABBIT_CAP);

    // --- V0.12: livestock regen (cow/sheep) ---
    // Same logistic-growth shape as wildlife regen, just domesticated
    // instead of hunted — runs unconditionally, whether or not a Farmhouse
    // exists yet, same "the resource exists before the building that uses
    // it" convention.
    let cow_growth = COW_REGEN_PER_DAY / TICKS_PER_DAY as f32
        * state.livestock.cow
        * (1.0 - state.livestock.cow / COW_CAP).max(0.0);
    state.livestock.cow = (state.livestock.cow + cow_growth).clamp(0.0, COW_CAP);
    let sheep_growth = SHEEP_REGEN_PER_DAY / TICKS_PER_DAY as f32
        * state.livestock.sheep
        * (1.0 - state.livestock.sheep / SHEEP_CAP).max(0.0);
    state.livestock.sheep = (state.livestock.sheep + sheep_growth).clamp(0.0, SHEEP_CAP);

    // --- Production ---
    // Colony-wide multipliers (tools/leader/morale) composed once per tick,
    // not once per building — see `GameState::colony_production_multiplier`.
    let colony_factor = state.colony_production_multiplier();
    for i in 0..state.buildings.len() {
        let (b_id, kind, bx, by, workers, owner_account, level_factor, under_construction) = {
            let b = &state.buildings[i];
            (b.id, b.kind, b.x, b.y, b.workers, b.owner_account, b.level_factor(), b.under_construction())
        };
        // V0.8: qurilish maydonchasi hali bino emas.
        if under_construction {
            continue;
        }
        if workers == 0 {
            continue;
        }
        // V0.21: the rate comes from the workbench inside, not from the
        // building itself — see `FurnishingKind::cycle`. A room with no
        // workbench yields NOTHING: there are no tools to work with. A
        // level-1 one restores exactly the throughput this building always
        // had, and each level above shortens the cycle.
        let Some(cycle) = state.buildings[i].production_cycle() else {
            continue;
        };
        let per_day = cycle.per_day();
        if per_day == 0.0 {
            continue;
        }
        // Effective worker-units: named survivors assigned here each
        // contribute their own profession/XP-boosted share; any remaining
        // anonymous headcount (workers beyond the named ones, filled via
        // `AdjustWorkers`) contributes a flat 1.0 each, same as before this
        // feature existed. With zero named assignments (every existing
        // balance test) this sum is exactly `workers as f32` — unchanged.
        let mut units = 0.0f32;
        let mut named = 0u8;
        for s in &state.survivors {
            if s.assigned_building == Some(b_id) {
                named += 1;
                units += survivor_contribution(s, kind, state.leader);
            }
        }
        units += (workers.saturating_sub(named)) as f32;
        // V0.8: bino darajasi ishlab chiqarishni kuchaytiradi (`level_factor`).
        let amount = units * per_day / TICKS_PER_DAY as f32 * colony_factor * level_factor;
        // Central-world economy v1: credit whatever this building actually
        // adds to the shared stock this tick to its owning account's ledger
        // — measured as an actual delta (not just `amount`) so Sawmill/
        // CoalMine's discrete, depletion-bounded output is credited exactly,
        // never more than what really landed in the stockpile.
        let wood_before = state.stock.wood;
        let coal_before = state.stock.coal;
        let food_before = state.stock.food;
        match kind {
            BuildingKind::HunterHut => {
                state.buildings[i].progress += amount;
                while state.buildings[i].progress >= 1.0 {
                    // Deer preferred while available (better food+fur yield
                    // per unit); falls back to rabbit once deer are too
                    // scarce to support a hunt-unit — graceful degradation
                    // instead of a hard stop the moment deer alone run low.
                    if state.wildlife.deer >= DEER_HUNT_PER_UNIT {
                        state.wildlife.deer -= DEER_HUNT_PER_UNIT;
                        state.stock.food += DEER_FOOD_PER_UNIT;
                        state.stock.fur += DEER_FUR_PER_UNIT;
                        state.buildings[i].progress -= 1.0;
                    } else if state.wildlife.rabbit >= RABBIT_HUNT_PER_UNIT {
                        state.wildlife.rabbit -= RABBIT_HUNT_PER_UNIT;
                        state.stock.food += RABBIT_FOOD_PER_UNIT;
                        state.stock.fur += RABBIT_FUR_PER_UNIT;
                        state.buildings[i].progress -= 1.0;
                    } else {
                        // Wildlife too scarce this tick — hold progress like
                        // an empty coal deposit does; hunters keep trying
                        // next tick as population regrows.
                        state.buildings[i].progress = 1.0;
                        break;
                    }
                }
            }
            BuildingKind::TailorShop => {
                state.buildings[i].progress += amount;
                while state.buildings[i].progress >= 1.0 {
                    if state.stock.fur >= FUR_PER_CLOTH {
                        state.stock.fur -= FUR_PER_CLOTH;
                        state.stock.cloth += 1.0;
                        state.buildings[i].progress -= 1.0;
                    } else {
                        state.buildings[i].progress = 1.0;
                        break;
                    }
                }
            }
            BuildingKind::Greenhouse => state.stock.food += amount,
            // V0.11: flat and uncapped like Greenhouse — no map-tile deposit
            // to exhaust, unlike Sawmill/CoalMine.
            BuildingKind::Well => state.stock.water += amount,
            BuildingKind::Farmhouse => {
                state.buildings[i].progress += amount;
                while state.buildings[i].progress >= 1.0 {
                    // Cow preferred while available (better food yield per
                    // unit); falls back to sheep once cows are too scarce to
                    // support a farm-unit — same graceful degradation as
                    // HunterHut's deer/rabbit fallback.
                    if state.livestock.cow >= COW_HARVEST_PER_UNIT {
                        state.livestock.cow -= COW_HARVEST_PER_UNIT;
                        state.stock.food += COW_FOOD_PER_UNIT;
                        state.buildings[i].progress -= 1.0;
                    } else if state.livestock.sheep >= SHEEP_HARVEST_PER_UNIT {
                        state.livestock.sheep -= SHEEP_HARVEST_PER_UNIT;
                        state.stock.food += SHEEP_FOOD_PER_UNIT;
                        state.buildings[i].progress -= 1.0;
                    } else {
                        // Livestock too scarce this tick — hold progress like
                        // an empty coal deposit does; farmers keep trying
                        // next tick as the herd regrows.
                        state.buildings[i].progress = 1.0;
                        break;
                    }
                }
            }
            BuildingKind::Sawmill => {
                state.buildings[i].progress += amount;
                while state.buildings[i].progress >= 1.0 {
                    if take_forest_unit(&mut state.tiles, bx, by, SAWMILL_RADIUS) {
                        state.buildings[i].progress -= 1.0;
                        state.stock.wood += 1.0;
                    } else {
                        // Nothing left to cut nearby; hold progress.
                        state.buildings[i].progress = 1.0;
                        break;
                    }
                }
            }
            BuildingKind::CoalMine => {
                state.buildings[i].progress += amount;
                while state.buildings[i].progress >= 1.0 {
                    // V0.22: a mine covers a 3x3 room and only needs SOME
                    // coal beneath it (see `footprint_is_clear`), so it draws
                    // from the richest tile it sits on rather than assuming
                    // the corner one is a seam.
                    let Some(idx) = state.richest_coal_under(kind, bx, by) else {
                        state.buildings[i].progress = 1.0;
                        break;
                    };
                    if state.tiles[idx].deposit > 0 {
                        state.tiles[idx].deposit -= 1;
                        state.stock.coal += 1.0;
                        state.buildings[i].progress -= 1.0;
                    } else {
                        state.buildings[i].progress = 1.0;
                        break;
                    }
                }
            }
            _ => {}
        }
        if state.central {
            if let Some(acc) = owner_account {
                let dwood = state.stock.wood - wood_before;
                let dcoal = state.stock.coal - coal_before;
                let dfood = state.stock.food - food_before;
                if dwood != 0.0 || dcoal != 0.0 || dfood != 0.0 {
                    state.credit_ledger(acc, |t| {
                        t.wood += dwood;
                        t.coal += dcoal;
                        t.food += dfood;
                    });
                }
            }
        }
    }

    // --- V0.19: snow falls on the map, crews and shovels take it back off.
    // Before the movement block below, so this tick's walking already feels
    // the ground as it is right now rather than as it was last tick.
    snow::tick_snowfall(state);
    snow::tick_snow_crews(state);
    snow::tick_clear_orders(state);

    // --- V0.18: life cycle (aging, pairing, old age). Runs BEFORE the
    // survivor loop below on purpose: an old-age death is written as `hp = 0`
    // and picked up by that loop's existing death detection in the same tick,
    // so every cause of death still flows through exactly one death path
    // (corpse, mourning, morale, worker slots, partner cleanup).
    lifecycle::tick_lifecycle(state);

    // --- Survivors: hunger, warmth, health ---
    let temp = state.temperature();
    let lit = state.furnace_lit;
    let level = state.furnace_level;
    let radius = state.heat_radius();
    let mut warm_slots = 0usize;
    let mut shelter_slots = 0usize;
    for b in &state.buildings {
        // V0.8: daraja va qurilish holatiga qarab (bitmagan chodir joy
        // bermaydi, yuqori daraja ko'proq sig'diradi) — `housing_slots`.
        let slots = b.housing_slots();
        if slots > 0 {
            if lit && GameState::dist_to_furnace(b.x, b.y) <= radius {
                warm_slots += slots;
            } else {
                shelter_slots += slots;
            }
        }
    }
    let tph = TICKS_PER_DAY as f32 / 24.0; // ticks per in-game hour
    let hunger_per_tick = 100.0 / TICKS_PER_DAY as f32;
    // Hospital effect strength has a real scalar hook (`care_per_tick`
    // below), so Medic/XP bonuses apply here exactly like a production
    // building: named Hospital workers contribute their boosted share,
    // remaining anonymous headcount contributes 1.0 each (identical to plain
    // `hospital_workers` when nobody is named — same neutrality property as
    // the main production loop). Kitchen's effect is still a flat boolean
    // toggle in terms of WHO staffs it (`kitchen_staffed`) — a matching Cook
    // currently grants nothing beyond any other worker — documented, not a
    // bug; but it DOES now scale with the staffed Kitchen's own level, same
    // as Hospital (`kitchen_level_factor`, below).
    let mut hospital_units = 0.0f32;
    for b in state
        .buildings
        .iter()
        .filter(|b| b.kind == BuildingKind::Hospital && !b.under_construction())
    {
        let named: Vec<&Survivor> =
            state.survivors.iter().filter(|s| s.assigned_building == Some(b.id)).collect();
        let mut units = 0.0f32;
        for s in &named {
            units += survivor_contribution(s, BuildingKind::Hospital, state.leader);
        }
        units += b.workers.saturating_sub(named.len() as u8) as f32;
        // V0.8: kasalxona darajasi parvarish kuchini ham oshiradi.
        hospital_units += units * b.level_factor();
    }
    let kitchen_staffed = state.buildings.iter()
        .any(|b| b.kind == BuildingKind::Kitchen && b.workers > 0 && !b.under_construction());
    // V0.8: kasalxona kabi, oshxona darajasi ham samaradorlikni oshiradi —
    // eng yuqori darajali ishchili oshxonaning `level_factor()`i olinadi.
    let kitchen_level_factor = state.buildings.iter()
        .filter(|b| b.kind == BuildingKind::Kitchen && b.workers > 0 && !b.under_construction())
        .map(|b| b.level_factor())
        .fold(0.0f32, f32::max);
    let medicine_factor = if state.has_tech(Tech::Medicine) {
        TECH_MEDICINE_CARE
    } else {
        1.0
    };
    let care_per_tick = hospital_units * HOSPITAL_CARE_PER_WORKER_DAY
        / TICKS_PER_DAY as f32
        * medicine_factor;
    let rationing_factor = if state.has_tech(Tech::Rationing) {
        TECH_RATIONING_FOOD
    } else {
        1.0
    };
    // The base discount (1.0 - KITCHEN_FOOD_EFFICIENCY) grows with the best
    // staffed kitchen's level, floored at 10% of a portion so it can never
    // reach (or cross) zero at high levels.
    let kitchen_efficiency = if kitchen_level_factor > 0.0 {
        (1.0 - (1.0 - KITCHEN_FOOD_EFFICIENCY) * kitchen_level_factor).max(0.1)
    } else {
        1.0
    };
    // V0.18: the law book joins Kitchen efficiency and Rationing as another
    // colony-wide multiplier on the same portion (Extra Rations feeds people
    // better and costs more; Communal Meals is the reverse) — 1.0 with an
    // empty book, so an unlegislated colony eats exactly as it always did.
    let portion = FOOD_PER_SURVIVOR_DAY / TICKS_PER_DAY as f32
        * kitchen_efficiency
        * rationing_factor
        * state.law_food_multiplier();
    // V0.11: thirst mirrors hunger's Kitchen/Rationing discount shape
    // exactly (a kitchen serves both food and drink; the same Rationing
    // tech now covers "eats and drinks more carefully") — reuses the same
    // `kitchen_level_factor` computed above, just a different efficiency
    // constant.
    let water_rationing_factor = if state.has_tech(Tech::Rationing) {
        TECH_RATIONING_WATER
    } else {
        1.0
    };
    let water_kitchen_efficiency = if kitchen_level_factor > 0.0 {
        (1.0 - (1.0 - KITCHEN_WATER_EFFICIENCY) * kitchen_level_factor).max(0.1)
    } else {
        1.0
    };
    let water_portion =
        WATER_PER_SURVIVOR_DAY / TICKS_PER_DAY as f32 * water_kitchen_efficiency * water_rationing_factor;
    let insulation_bonus = if state.has_tech(Tech::Insulation) {
        TECH_INSULATION_WARMTH
    } else {
        0.0
    };
    // V0.10: Tailor Shop warmth — active only while at least one TailorShop
    // is staffed AND there's cloth to consume; stacks additively with
    // Insulation (garments are a separate, renewable-but-costly warmth
    // source, not a replacement for the one-time tech unlock). Computed once
    // per tick, colony-wide (not per-survivor), same granularity the
    // furnace's `need_coal` check already uses.
    let tailoring_staffed = state.buildings.iter().any(|b| {
        b.kind == BuildingKind::TailorShop && b.workers > 0 && !b.under_construction()
    });
    let cloth_upkeep_per_tick = CLOTH_UPKEEP_PER_DAY / TICKS_PER_DAY as f32;
    let tailoring_bonus = if tailoring_staffed && state.stock.cloth >= cloth_upkeep_per_tick {
        state.stock.cloth -= cloth_upkeep_per_tick;
        TAILORING_WARMTH
    } else {
        0.0
    };
    let mut deaths: Vec<(u32, String, &'static str)> = Vec::new();
    // V0.18: training rate under the current law book, resolved before the
    // movement/XP loop borrows `survivors` mutably (1.0 with an empty book).
    let law_xp = state.law_xp_multiplier();
    // V0.20: per-building training bonus from shelving, snapshotted for the
    // same borrow reason. Absent = 1.0, so an unfurnished building trains at
    // exactly the old rate.
    let shelving_xp: std::collections::HashMap<u32, f32> = state
        .buildings
        .iter()
        .filter(|b| b.furnishing_of(FurnishingKind::Shelving) > 0)
        .map(|b| (b.id, b.furnishing_factor(FurnishingKind::Shelving)))
        .collect();

    // V0.22: where each assigned survivor actually STANDS inside their
    // building. Workers take the fittings' stations in order — one at the
    // stove, one at the workbench — instead of every one of them walking to
    // the same tile centre, which is what a 3x3 room made look absurd (three
    // people stacked on one square in the middle of an empty floor).
    // Snapshotted here because the movement loop below borrows `survivors`
    // mutably and this needs to read both vecs.
    let worker_stations: std::collections::HashMap<u32, (f32, f32)> = {
        let mut out = std::collections::HashMap::new();
        for b in &state.buildings {
            for (i, s) in state
                .survivors
                .iter()
                .filter(|s| s.assigned_building == Some(b.id))
                .enumerate()
            {
                out.insert(s.id, b.worker_station(i));
            }
        }
        out
    };

    // --- Movement + XP accrual: needs a building lookup snapshot taken
    // before the loop below (can't borrow `state.buildings` immutably while
    // iterating `state.survivors` mutably). `central` settlers move and earn
    // XP too — a settler's assigned building keeps working while their owner
    // is away, so there's no reason their position/training should freeze.
    let building_lookup: std::collections::HashMap<u32, (BuildingKind, u8, u8, u8)> = state
        .buildings
        .iter()
        .map(|b| (b.id, (b.kind, b.x, b.y, b.workers)))
        .collect();
    // Furnace snapshot for the chop/carry cycle below — `None`/not-building
    // once it's lit, which quietly stops the whole dance for good (a V0.9
    // level upgrade afterward re-sets `build_left` but must NOT restart the
    // chop/carry dance — that crew just contributes worker-days like any
    // other building's, see `tick`'s construction loop). Taken once up
    // front for the same reason as `building_lookup`: reading
    // `state.buildings` inside the survivors loop below would conflict with
    // iterating `state.survivors` mutably.
    let furnace_info = state
        .buildings
        .iter()
        .find(|b| b.kind == BuildingKind::Furnace)
        .map(|b| (b.id, b.x, b.y, b.under_construction() && state.furnace_level == 0));
    // V0.11: corpse position snapshot for the burial errand below — same
    // reasoning as `building_lookup`/`furnace_info` (can't borrow
    // `state.corpses` while iterating `state.survivors` mutably).
    let corpse_lookup: std::collections::HashMap<u32, (f32, f32)> =
        state.corpses.iter().map(|c| (c.id, (c.x, c.y))).collect();
    // V0.15: daily-routine snapshot (see `routine_goal`) — same
    // borrow-splitting reason as the lookups above. Any FINISHED Kitchen
    // qualifies for meals regardless of staffing (people still gather
    // there); every FINISHED Tent is a candidate bunk, nearest one wins.
    //
    // Deliberately offset just past the tile's south edge, NOT the tile
    // center (`bx + 0.5, by + 0.5`) that `assigned_building`'s own goal
    // uses: a survivor sitting exactly on a building's center renders
    // clipped inside solid models like the Tent's (a closed prism, unlike
    // the flatter Kitchen), AND becomes permanently unclickable (the
    // client's building-vs-survivor click tie-break always favors the
    // building at that exact distance — see `input.rs`'s `resolve_world_click`).
    // Standing just outside reads as "waiting at the door" and keeps both
    // working. See `gather_points` for the shared formula.
    let (kitchen_pos, tent_positions) = gather_points(state);
    // V0.17: who has a bunk tonight — snapshotted before the survivors loop
    // for the same borrow-splitting reason as the lookups above.
    let bunked = state.bunked_ids();
    // Snapshotted (not passed as `&GameState`) for the same borrow-splitting
    // reason as the lookups above — the survivors loop below holds `state`
    // mutably the whole time.
    let routine_time_of_day = state.time_of_day();
    let routine_is_night = state.is_night();
    // V0.17: nobody sleeps while the furnace is still dark — the whole colony
    // is one survivor and one errand at that point (see the sleep leg below).
    let routine_furnace_lit = state.furnace_lit;
    // Deferred mutations the chop/carry cycle collects instead of touching
    // `state.tiles`/`state.buildings`/`state.stock` directly from inside the
    // survivors loop (applied once the loop's mutable borrow of
    // `state.survivors` ends).
    let mut chopped_tiles: Vec<(u8, u8)> = Vec::new();
    let mut logs_delivered: u32 = 0;
    let mut manual_wood_gained: u32 = 0;
    // V0.11: corpse ids actively being worked on this tick (a survivor with
    // `bury_target` set who has arrived at the body) — applied after the
    // loop for the same borrow-splitting reason as the chop/carry deferrals.
    let mut burying_now: Vec<u32> = Vec::new();
    for s in state.survivors.iter_mut() {
        // --- Movement: move_target (player-issued walk) takes priority,
        // then a furnace-building chop errand (`chop_target`), then a
        // burial errand (`bury_target`), then the assigned building's
        // location; with none of the five, the V0.15 daily routine (meal/
        // sleep) gets a say — only ever reached when truly nobody and
        // nothing is telling this survivor where to be.
        let goal = s.move_target.map(|(x, y)| (x as f32 + 0.5, y as f32 + 0.5))
            .or_else(|| s.chop_target.map(|(x, y)| (x as f32 + 0.5, y as f32 + 0.5)))
            .or_else(|| s.bury_target.and_then(|id| corpse_lookup.get(&id)).copied())
            // V0.17: the night shift goes to bed. Placed ABOVE the assigned
            // building on purpose — this is the only way an assigned
            // survivor's `fatigue` ever gets the fast Tent recovery rate,
            // and it costs the colony no output at all (production reads
            // `Building.workers`, never survivor positions).
            .or_else(|| {
                // Never while carrying a log home to an unlit furnace: that
                // errand IS the opening crisis, and parking the carrier in a
                // Tent overnight would stall it until morning.
                if s.carrying_wood || !routine_furnace_lit {
                    return None;
                }
                sleep_goal(routine_is_night, bunked.contains(&s.id), s, &tent_positions)
            })
            .or_else(|| {
                // V0.22: head for YOUR station inside the room, not the
                // building's corner tile (see `worker_stations` above).
                s.assigned_building.and(worker_stations.get(&s.id).copied())
            })
            .or_else(|| routine_goal(routine_time_of_day, kitchen_pos));
        let mut arrived_at_chop_target = false;
        let mut arrived_carrying_home = false;
        if let Some((gx, gy)) = goal {
            let (dx, dy) = (gx - s.x, gy - s.y);
            let dist = (dx * dx + dy * dy).sqrt();
            if dist <= ARRIVAL_EPSILON {
                s.x = gx;
                s.y = gy;
                if s.move_target.is_some() {
                    s.move_target = None; // arrived: clear a walk goal, then stand idle
                } else if s.chop_target.is_some() {
                    arrived_at_chop_target = true;
                } else if s.carrying_wood {
                    arrived_carrying_home = true;
                } else if let Some(corpse_id) = s.bury_target {
                    burying_now.push(corpse_id);
                }
            } else {
                // V0.19: the ground now has an opinion. Speed scales with the
                // tile underfoot (deep snow is a crawl, a cleared road is
                // quick), and the direction is chosen one tile at a time by
                // comparing the neighbours' cost — which is what makes people
                // walk ALONG a road instead of straight across the drift
                // beside it.
                //
                // This is deliberately a local heuristic, not a pathfinder:
                // the map has no obstacles and snow never blocks (see
                // `Tile::speed_factor`), so cost is finite everywhere and a
                // greedy step can never trap anyone. A real search would buy
                // prettier routes around nothing at all.
                let speed = state
                    .tiles
                    .get(tile_of(s.x, s.y))
                    .map_or(1.0, |t| t.speed_factor())
                    * SURVIVOR_SPEED_PER_TICK;
                let (sx, sy) = road_step(&state.tiles, s.x, s.y, gx, gy);
                s.x += sx * speed;
                s.y += sy * speed;
            }
        }

        // --- Chop/carry cycle: `chop_target` drives two different errands.
        // A Furnace-building one (`assigned_building` still points at a
        // still-unbuilt Furnace) auto-picks a new tree every time it's idle
        // and carries each chopped log home to count toward
        // `FURNACE_LOGS_NEEDED`. A manual one (`PlayerCommand::ChopTile`,
        // `assigned_building` cleared when it was issued) is one-shot —
        // chopping credits the stockpile immediately, since there's no
        // assigned building to carry the log back to, and the survivor then
        // just stands there idle rather than auto-continuing.
        let furnace_errand = furnace_info.filter(|&(furnace_id, _, _, still_building)| {
            still_building && s.assigned_building == Some(furnace_id)
        });
        if arrived_at_chop_target {
            if let Some(pos) = s.chop_target.take() {
                chopped_tiles.push(pos);
                if furnace_errand.is_some() {
                    s.carrying_wood = true;
                } else {
                    manual_wood_gained += 1;
                }
            }
        } else if arrived_carrying_home {
            // Only a Furnace errand ever sets `carrying_wood`, so this only
            // ever fires for that case.
            s.carrying_wood = false;
            logs_delivered += 1;
        } else if let Some((_, fx, fy, _)) = furnace_errand {
            if s.chop_target.is_none() && !s.carrying_wood {
                s.chop_target = find_forest_tile(&state.tiles, fx, fy, FURNACE_CHOP_RADIUS);
            }
        }

        // --- XP: accrues while assigned to a building that's actually
        // staffed/working (at least one worker, which — since this survivor
        // is one of them — is always true when they're assigned; the check
        // guards the theoretical case of a stale assignment surviving a
        // clamp). The kind-switch reset happens at assignment time in
        // `apply_command` (`AssignSurvivor`), not here — by the time `tick`
        // sees it, `trained_kind` already matches the current building.
        if let Some((kind, _, _, workers)) = s.assigned_building.and_then(|id| building_lookup.get(&id)) {
            if *workers > 0 && s.trained_kind == Some(*kind) {
                // V0.18: Apprenticeship (and any future law touching training)
                // scales the rate — 1.0 with an empty book. V0.20: so does
                // the room's own shelving — 1.0 without any.
                let shelves = s
                    .assigned_building
                    .and_then(|id| shelving_xp.get(&id))
                    .copied()
                    .unwrap_or(1.0);
                s.xp += law_xp * shelves / TICKS_PER_DAY as f32;
            }
        }
    }

    // V0.11: apply this tick's burial progress — deferred for the same
    // borrow-splitting reason as the chop/carry mutations below.
    // `Corpse.bury_left` mirrors `Building.build_left`'s "remaining work"
    // shape: ticks down by exactly one tick's worth per tick a survivor
    // stands there actively working it, completing (removing the corpse,
    // leaving a `Grave`, freeing the burying survivor) once it reaches 0.
    if !burying_now.is_empty() {
        let mut completed: Vec<(u32, String, f32, f32, Option<u32>)> = Vec::new();
        for corpse_id in burying_now {
            if let Some(c) = state.corpses.iter_mut().find(|c| c.id == corpse_id) {
                c.bury_left -= 1.0;
                if c.bury_left <= 0.0 {
                    completed.push((c.id, c.name.clone(), c.x, c.y, c.being_buried_by));
                }
            }
        }
        for (corpse_id, name, x, y, buryer) in completed {
            state.corpses.retain(|c| c.id != corpse_id);
            state.graves.push(Grave { x, y, created_tick: tick });
            if let Some(buryer_id) = buryer {
                if let Some(s) = state.survivors.iter_mut().find(|s| s.id == buryer_id) {
                    s.bury_target = None;
                }
            }
            push_event(state, format!("{} was laid to rest.", name));
        }
    }

    // Apply the chop/carry cycle's deferred mutations (collected above
    // instead of touching `state.tiles`/`state.buildings` while
    // `state.survivors` was borrowed mutably): each chopped tile loses one
    // unit of wood, and each delivered log reduces the Furnace's remaining
    // `build_left` by exactly one — completing (lighting) it the moment
    // `FURNACE_LOGS_NEEDED` round trips have landed.
    for (tx, ty) in chopped_tiles {
        let idx = tile_index(tx, ty);
        if state.tiles[idx].terrain == Terrain::Forest && state.tiles[idx].deposit > 0 {
            state.tiles[idx].deposit -= 1;
            if state.tiles[idx].deposit == 0 {
                state.tiles[idx].terrain = Terrain::Snow;
            }
        }
    }
    // A manually-chopped log (`PlayerCommand::ChopTile`, not part of a
    // Furnace-building errand) has nowhere assigned to carry it to — credit
    // the stockpile the instant it's chopped instead.
    if manual_wood_gained > 0 {
        state.stock.wood += manual_wood_gained as f32;
    }
    if logs_delivered > 0 {
        if let Some(fi) = state.buildings.iter().position(|b| b.kind == BuildingKind::Furnace) {
            state.buildings[fi].build_left =
                (state.buildings[fi].build_left - logs_delivered as f32).max(0.0);
            if state.buildings[fi].build_left <= 0.0 {
                let id = state.buildings[fi].id;
                for s in state.survivors.iter_mut() {
                    if s.assigned_building == Some(id) {
                        // Furnace.max_workers() == 0 once built — nobody
                        // stays crewing a lit furnace, same as the generic
                        // construction-complete arm above frees an
                        // over-capacity crew. Also drop any leftover
                        // chop-errand state so a survivor mid-trip when the
                        // LAST log lands doesn't keep walking toward a tree
                        // (or home) forever with nothing tracking them.
                        s.assigned_building = None;
                        s.chop_target = None;
                        s.carrying_wood = false;
                    }
                }
                state.buildings[fi].workers = 0;
                state.furnace_lit = true;
                state.furnace_level = 1;
                push_event(state, "The furnace is lit! Others will come seeking its warmth.".to_string());
            }
        }
    }

    // Central-world settlers are a presence, not mouths to feed: they never
    // hunger, freeze, sicken or die (their owner may be offline for weeks —
    // returning to a starved-out group would make migration pointless).
    // Everything above (production, furnace, movement, XP) still runs so an
    // assigned settler keeps contributing to the communal stock and training.
    if state.central {
        state.rng = rng.0;
        state.event_rng = erng.0;
        return;
    }

    let mut starving_present = false;
    let mut thirsty_present = false;
    // V0.17: colony-wide condition flags for the morale block below, plus the
    // names of everyone whose illness ended this tick (pushed as events after
    // the loop, which needs `state` back).
    let mut exhausted_present = false;
    let mut sick_present = false;
    let mut recoveries: Vec<String> = Vec::new();
    // V0.17: hospital care now does two jobs — the flat HP top-up it always
    // did (`care_per_tick`, computed above) and burning illness down faster
    // than its natural course. Both draw on the same staffed-hospital units.
    let illness_recovery_per_tick =
        1.0 + hospital_units * HOSPITAL_RECOVERY_PER_UNIT_DAY / TICKS_PER_DAY as f32;
    let fatigue_night = state.is_night();
    // V0.18: the law book's two fatigue dials, resolved once before the loop
    // (they're whole-`state` queries; inside the loop `survivors` is borrowed
    // mutably). Both are exactly 1.0 with an empty book.
    let law_fatigue = state.law_fatigue_multiplier();
    let law_rest = state.law_rest_multiplier();
    // V0.20: per-building heater relief, snapshotted before the loop borrows
    // `survivors` mutably (reading `buildings` inside it would be fine, but
    // the lookup is per-survivor and this is once per building).
    let heater_relief: std::collections::HashMap<u32, f32> = state
        .buildings
        .iter()
        .filter(|b| b.furnishing_of(FurnishingKind::Heater) > 0)
        .map(|b| {
            (
                b.id,
                FurnishingKind::Heater.per_level() * b.furnishing_of(FurnishingKind::Heater) as f32,
            )
        })
        .collect();
    for (i, s) in state.survivors.iter_mut().enumerate() {
        // --- V0.17: fatigue. Awake work/idle accrual by day, recovery by
        // night at one of two rates depending on whether this survivor holds
        // a Tent bunk (`bunked`, the same set the sleep walk above used).
        let fatigue_per_day = if fatigue_night {
            if bunked.contains(&s.id) {
                -FATIGUE_TENT_REST_PER_DAY * law_rest
            } else {
                -FATIGUE_ROUGH_REST_PER_DAY * law_rest
            }
        } else if let Some(b_id) = s.assigned_building {
            // V0.20: a stove where you work takes the edge off the shift.
            // `heater_relief` is 0.0 for an unfurnished (or heater-less)
            // building, so this is exactly `FATIGUE_WORK_PER_DAY * law_fatigue`
            // everywhere it was before.
            let relief = heater_relief.get(&b_id).copied().unwrap_or(0.0);
            FATIGUE_WORK_PER_DAY * law_fatigue * (1.0 - relief).max(0.2)
        } else {
            FATIGUE_IDLE_PER_DAY * law_fatigue
        };
        s.fatigue = (s.fatigue + fatigue_per_day / TICKS_PER_DAY as f32).clamp(0.0, 100.0);
        if s.fatigue >= FATIGUE_EXHAUSTED {
            exhausted_present = true;
        }

        // --- V0.17: illness runs its course (faster with a staffed
        // Hospital), draining HP the whole time. Recovery is logged once,
        // on the tick it completes.
        if s.is_sick() {
            sick_present = true;
            s.sick_left = (s.sick_left - illness_recovery_per_tick).max(0.0);
            // Per DAY, unlike the hunger/thirst/cold drains just below, which
            // are all written as per-hour rates against `tph`.
            s.hp -= SICK_HP_PER_DAY / TICKS_PER_DAY as f32;
            if !s.is_sick() {
                recoveries.push(s.name.clone());
            }
        }

        s.hunger = (s.hunger + hunger_per_tick).min(120.0);
        // V0.18: a child eats a smaller share of the same portion (water is
        // deliberately not age-scaled — thirst doesn't care how old you are).
        let s_portion = portion * s.food_factor();
        if s.hunger >= 25.0 && state.stock.food >= s_portion {
            state.stock.food -= s_portion;
            s.hunger = (s.hunger - 0.4).max(0.0);
        }
        if s.hunger >= 80.0 {
            starving_present = true;
        }

        // V0.11: thirst mirrors hunger's exact shape (same accrual rate,
        // same clamp, same "-0.4 per successful drink" reduction) — only
        // the thresholds differ (20/65 vs hunger's 25/80), calibrated so
        // thirst becomes critical sooner than hunger (see
        // `WATER_PER_SURVIVOR_DAY`'s doc comment).
        s.thirst = (s.thirst + hunger_per_tick).min(120.0);
        if s.thirst >= 20.0 && state.stock.water >= water_portion {
            state.stock.water -= water_portion;
            s.thirst = (s.thirst - 0.4).max(0.0);
        }
        if s.thirst >= 65.0 {
            thirsty_present = true;
        }

        let bonus = if lit && i < warm_slots {
            12.0 + 6.0 * level as f32
        } else if i < warm_slots + shelter_slots {
            6.0
        } else if lit {
            3.0 // huddling near the open furnace
        } else {
            0.0
        } + insulation_bonus + tailoring_bonus;
        let eff = temp + bonus;
        if eff < 0.0 {
            s.hp -= (-eff).min(40.0) * 0.35 / tph;
        } else if eff >= 5.0 && s.hunger < 60.0 {
            s.hp = (s.hp + 3.0 / tph).min(100.0);
        }
        if s.hunger >= 80.0 {
            s.hp -= 4.0 * ((s.hunger - 80.0) / 20.0) / tph;
        }
        if s.thirst >= 65.0 {
            s.hp -= 4.0 * ((s.thirst - 65.0) / 20.0) / tph;
        }
        if care_per_tick > 0.0 {
            s.hp = (s.hp + care_per_tick).min(100.0);
        }
        // V0.17: an active outbreak no longer drains everyone's HP directly —
        // `disease_until` is now just the window during which the infection
        // roll below fires; the HP cost lands on the people who actually fell
        // ill (`sick_left`, handled at the top of this loop).
        if s.hp <= 0.0 {
            // Attribute the death to the most likely cause for the event
            // log — thirst checked first since it's calibrated to become
            // critical sooner than hunger (see the threshold comment above).
            let cause = if s.thirst >= 65.0 {
                "died of thirst"
            } else if s.hunger >= 80.0 {
                "starved"
            } else if s.is_sick() {
                // V0.17: attributed to THIS survivor's own illness, not to a
                // colony-wide outbreak flag they may never have caught.
                "succumbed to illness"
            } else if s.stage() == LifeStage::Elder {
                // V0.18: an elder who wasn't hungry, thirsty or ill is
                // overwhelmingly likely to be here because `lifecycle`'s
                // once-daily old-age roll zeroed their HP this tick. An elder
                // who genuinely froze is mislabeled — event-log text only,
                // and the alternative (a persisted "dying of old age" flag on
                // every survivor) costs a save-format field to fix a caption.
                "died of old age"
            } else {
                "froze to death"
            };
            deaths.push((s.id, s.name.clone(), cause));
        }
    }

    // --- Morale: smooth per-tick version of the per-day adjustments in the
    // brief (each rate divided by TICKS_PER_DAY so a day's worth of ticks
    // sums to the stated daily amount). Death penalties are applied
    // separately, once per death, in the block below — they're an event, not
    // a rate. Every input here is a per-tick snapshot already computed above
    // (staffed Kitchen/Hospital, leader, blizzard) or just finished
    // (starving_present), so this reads as "what happened THIS tick".
    {
        let per_tick = |per_day: f32| per_day / TICKS_PER_DAY as f32;
        let mut delta = 0.0f32;
        if starving_present {
            delta -= per_tick(MORALE_STARVATION_PER_DAY);
        }
        if thirsty_present {
            delta -= per_tick(MORALE_THIRST_PER_DAY);
        }
        if state.blizzard_active() {
            delta -= per_tick(MORALE_BLIZZARD_PER_DAY);
        }
        // V0.17: an exhausted or bedridden colony is a miserable one — both
        // stack additively with the hunger/thirst/blizzard penalties above,
        // the same way those already stack with each other.
        if exhausted_present {
            delta -= per_tick(MORALE_EXHAUSTION_PER_DAY);
        }
        if sick_present {
            delta -= per_tick(MORALE_SICK_PER_DAY);
        }
        if kitchen_staffed {
            delta += per_tick(MORALE_KITCHEN_PER_DAY);
        }
        // Hospital "staffed" for morale purposes mirrors the Kitchen check
        // above (any worker present), independent of the profession/XP-
        // boosted `hospital_units` used for the healing rate itself.
        if state.buildings.iter().any(|b| b.kind == BuildingKind::Hospital && b.workers > 0) {
            delta += per_tick(MORALE_HOSPITAL_PER_DAY);
        }
        if state.leader_alive() {
            delta += per_tick(MORALE_LEADER_PER_DAY);
        }
        // V0.18: the law book's own standing morale term — a sum of the
        // enacted laws' daily adjustments (Extra Rations lifts, Long Shifts
        // grinds), joining the additive terms above rather than multiplying
        // them. Exactly 0.0 with an empty book.
        delta += per_tick(state.law_morale_per_day());
        // V0.20: somewhere to sit down. Only counts in a building that is
        // actually staffed and finished — an empty room full of chairs
        // cheers nobody up. Exactly 0.0 with no seating anywhere.
        let seating: f32 = state
            .buildings
            .iter()
            .filter(|b| b.workers > 0 && !b.under_construction())
            .map(|b| {
                FurnishingKind::Seating.per_level() * b.furnishing_of(FurnishingKind::Seating) as f32
            })
            .sum();
        delta += per_tick(seating);
        // Slow drift toward the baseline, on top of the specific adjustments
        // above — moves at most `MORALE_DRIFT_PER_DAY` worth per day, and
        // never overshoots past the baseline in one tick.
        let drift_cap = per_tick(MORALE_DRIFT_PER_DAY);
        let toward_baseline = (MORALE_BASELINE - state.morale).clamp(-drift_cap, drift_cap);
        state.morale = (state.morale + delta + toward_baseline).clamp(0.0, 100.0);
    }

    if !deaths.is_empty() {
        // Free each dying survivor's own named slot before they're removed,
        // so the building they worked at loses exactly its own vacancy —
        // not an arbitrary one picked by `clamp_workers` below, which has no
        // way to tell a named slot from anonymous fill.
        for (id, _, _) in &deaths {
            if let Some(b_id) = state.survivors.iter().find(|s| s.id == *id).and_then(|s| s.assigned_building) {
                if let Some(b) = state.buildings.iter_mut().find(|b| b.id == b_id) {
                    b.workers = b.workers.saturating_sub(1);
                }
            }
        }
        // V0.11: leave a corpse at the death location — must happen before
        // `retain` removes them below (need their position/name). Burial is
        // player-initiated (`PlayerCommand::Bury`); left alone, it fades
        // into a `Grave` on its own after `CORPSE_DECAY_TICKS`.
        for (id, name, _) in &deaths {
            if let Some(s) = state.survivors.iter().find(|s| s.id == *id) {
                state.corpses.push(Corpse {
                    id: *id,
                    name: name.clone(),
                    x: s.x,
                    y: s.y,
                    died_tick: tick,
                    bury_left: BURY_DURATION_TICKS as f32,
                    being_buried_by: None,
                });
            }
        }
        // The leader's death starts mourning instead of just clearing the
        // seat — checked before the removal below so `leader` still points
        // at a real (if about-to-die) survivor here.
        if let Some(leader_id) = state.leader {
            if let Some((_, name, _)) = deaths.iter().find(|(id, _, _)| *id == leader_id) {
                let name = name.clone();
                state.leader = None;
                state.mourning_until = state.tick + MOURNING_DURATION_TICKS;
                push_event(state, format!("The leader {} has died - the city mourns.", name));
            }
        }
        // V0.18: nobody's partner may outlive them as a dangling id — cleared
        // before the removal, while both sides are still in the roster.
        let dead_ids: Vec<u32> = deaths.iter().map(|(id, _, _)| *id).collect();
        lifecycle::clear_partner_links(state, &dead_ids);
        state.survivors.retain(|s| s.hp > 0.0);
        // V0.18: Funeral Rites softens the blow and charges wood for it. Both
        // are no-ops (x1.0, 0.0) with an empty law book.
        let funeral_wood = state.law_funeral_wood() * deaths.len() as f32;
        if funeral_wood > 0.0 {
            state.stock.wood = (state.stock.wood - funeral_wood).max(0.0);
        }
        let death_penalty =
            MORALE_DEATH_PENALTY * deaths.len() as f32 * state.law_death_morale_multiplier();
        state.morale = (state.morale - death_penalty).clamp(0.0, 100.0);
        for (_, name, cause) in deaths {
            push_event(state, format!("{} has {}.", name, cause));
        }
        // Still needed as a fallback for anonymous-only overflow (shouldn't
        // normally trigger now that every named slot above is freed first).
        clamp_workers(state);
    }

    // --- V0.17: recoveries. Logged after the death block so someone who
    // shook off the illness and then died of the cold in the same tick isn't
    // announced as recovered — `retain` above already dropped them.
    for name in recoveries {
        if state.survivors.iter().any(|s| s.name == name) {
            push_event(state, format!("{} has recovered.", name));
        }
    }

    // --- V0.17: the once-daily infection roll. Draws from `illness_rng`, a
    // stream of its own, so it can never re-sequence the existing
    // `rng`/`event_rng` outcomes (arrivals, births, weather). Two sources
    // compose: the outbreak window (`disease_until`, V0.3's event, which now
    // only opens the door instead of draining everyone's HP) and contagion
    // from each survivor already carrying it — so an illness can keep
    // circulating after the outbreak officially "fades".
    if state.tick % TICKS_PER_DAY == ILLNESS_TICK && !state.survivors.is_empty() {
        let mut irng = Rng(state.illness_rng);
        let outbreak = state.disease_active();
        let sick_now = state.sick_count();
        if outbreak || sick_now > 0 {
            // V0.18: Quarantine (and any future law touching contagion) scales
            // the whole daily chance — 1.0 with an empty book.
            let law_contagion = state.law_contagion_multiplier();
            let base = (if outbreak { OUTBREAK_INFECT_CHANCE_PER_DAY } else { 0.0 }
                + sick_now as f32 * CONTAGION_CHANCE_PER_SICK_PER_DAY)
                * law_contagion;
            let mut fell_ill: Vec<String> = Vec::new();
            for s in state.survivors.iter_mut() {
                if s.is_sick() {
                    continue;
                }
                let exhausted = s.fatigue >= FATIGUE_EXHAUSTED;
                // V0.18: children and elders catch it more easily — composes
                // multiplicatively with exhaustion, so an exhausted elder is
                // scaled by both rather than by whichever branch won.
                let chance = (base
                    * if exhausted { EXHAUSTION_SICK_MULTIPLIER } else { 1.0 }
                    * if s.is_frail() { FRAIL_SICK_MULTIPLIER } else { 1.0 })
                    .min(MAX_INFECT_CHANCE_PER_DAY);
                if irng.chance(chance) {
                    s.sick_left = SICKNESS_TICKS;
                    fell_ill.push(s.name.clone());
                }
            }
            for name in fell_ill {
                push_event(state, format!("{} has fallen ill.", name));
            }
        }
        state.illness_rng = irng.0;
    }

    // --- V0.11: corpse decay -> Grave, Grave fade ---
    // Unburied for too long becomes the same fading trace a proper burial
    // leaves — "vaqtinchalik iz", never a permanent obstruction. Runs every
    // tick (not just once/day), since it's purely a `tick - died_tick`
    // comparison.
    if !state.corpses.is_empty() {
        let mut decayed: Vec<Corpse> = Vec::new();
        state.corpses.retain(|c| {
            if tick.saturating_sub(c.died_tick) >= CORPSE_DECAY_TICKS {
                decayed.push(c.clone());
                false
            } else {
                true
            }
        });
        if !decayed.is_empty() {
            for c in &decayed {
                state.graves.push(Grave { x: c.x, y: c.y, created_tick: tick });
                push_event(state, format!("{}'s grave has faded into the snow.", c.name));
            }
            // Defensive: free anyone whose burial errand just decayed out
            // from under them (an extremely narrow race — decay takes days,
            // a burial takes seconds — but leaves no dangling reference
            // either way).
            for s in state.survivors.iter_mut() {
                if let Some(cid) = s.bury_target {
                    if decayed.iter().any(|c| c.id == cid) {
                        s.bury_target = None;
                    }
                }
            }
        }
    }
    state.graves.retain(|g| tick.saturating_sub(g.created_tick) < GRAVE_FADE_TICKS);

    // --- Morning arrivals ---
    // Nobody comes seeking a furnace that isn't lit yet — the leader has to
    // finish building it first (see the Construction section above).
    if state.tick % TICKS_PER_DAY == ARRIVAL_TICK
        && state.day() >= 2
        && state.furnace_lit
        && rng.chance(0.55)
    {
        let pop = state.survivors.len() as i32;
        let space = state.housing_capacity() as i32 + 2 - pop;
        let n = (1 + rng.below(3) as i32).min(space).min(MAX_POPULATION - pop);
        if n > 0 {
            for _ in 0..n {
                let s = new_survivor(&mut rng, &mut state.next_id);
                state.survivors.push(s);
            }
            let plural = if n == 1 { "" } else { "s" };
            push_event(state, format!("{} newcomer{} arrived seeking shelter.", n, plural));
        }
    }

    // --- V0.11: births ---
    // The colony grows from within once it's stable, not just from outsiders
    // arriving — gated on morale/food surplus so a struggling colony
    // (already losing people to starvation/cold) never also gets a birth
    // pulling it further underwater. Requires at least 2 survivors
    // (abstracted "a pairing exists somewhere in the roster") — never fires
    // on a lone survivor. Deliberately much rarer than arrivals/caravans — a
    // slow background trickle on top of external growth, not a replacement
    // for it.
    if state.tick % TICKS_PER_DAY == BIRTH_TICK
        && state.day() >= BIRTH_GRACE_DAY
        && state.furnace_lit
        && state.survivors.len() >= 2
        && state.morale >= BIRTH_MIN_MORALE
        && state.stock.food >= BIRTH_MIN_FOOD_STOCK
        // V0.18: a colony with a couple in it is markedly more fertile; one
        // with none keeps V0.11's plain `BIRTH_CHANCE` as the floor, so this
        // only ever raises the rate, never gates births behind pairing.
        && rng.chance(BIRTH_CHANCE * lifecycle::birth_chance_multiplier(state))
    {
        let pop = state.survivors.len() as i32;
        let space = state.housing_capacity() as i32 + 2 - pop;
        if space > 0 && pop < MAX_POPULATION {
            let mut s = new_survivor(&mut rng, &mut state.next_id);
            // V0.18: born here — the ONLY way into the colony at age 0
            // (`new_survivor` otherwise draws a grown arrival's age).
            s.age_days = 0.0;
            let name = s.name.clone();
            state.survivors.push(s);
            push_event(state, format!("A newborn, {}, has joined the city.", name));
        }
    }

    if state.tick % TICKS_PER_DAY == ARRIVAL_TICK
        && state.day() >= EVENT_GRACE_DAY
        && state.pending_event.is_none()
        && erng.chance(CARAVAN_CHANCE)
    {
        let count = 2 + erng.below(3);
        state.pending_event = Some(CaravanOffer {
            count,
            food_cost: count * CARAVAN_FOOD_PER_PERSON,
            expires: state.tick + CARAVAN_EXPIRE_TICKS,
        });
        push_event(
            state,
            format!(
                "A caravan of {} refugees asks for shelter (needs {} food). Answer them.",
                count,
                count * CARAVAN_FOOD_PER_PERSON
            ),
        );
    }

    // --- Event lifecycle: expiry and endings ---
    if let Some(o) = state.pending_event {
        if state.tick >= o.expires {
            state.pending_event = None;
            push_event(state, "The caravan gave up waiting and moved on.");
        }
    }
    if state.disease_until != 0 && state.tick == state.disease_until {
        state.disease_until = 0;
        push_event(state, "The sickness fades.");
    }
    if state.blizzard_until != 0 && state.tick == state.blizzard_until {
        state.blizzard_until = 0;
        push_event(state, "The blizzard passes.");
    }

    // --- Missions & Tunnel ---
    for i in 0..state.missions.len() {
        if state.missions[i].done {
            continue;
        }
        let kind = state.missions[i].kind;
        if state.mission_current(kind) >= kind.target() {
            state.missions[i].done = true;
            let (rw, rc, rf) = (
                state.missions[i].reward_wood,
                state.missions[i].reward_coal,
                state.missions[i].reward_food,
            );
            state.stock.wood += rw as f32;
            state.stock.coal += rc as f32;
            state.stock.food += rf as f32;
            push_event(state, format!("Mission complete: {} {}.", kind.label(), kind.target()));
        }
    }
    if !state.tunnel.unlocked && state.all_missions_done() {
        state.tunnel.unlocked = true;
        push_event(state, "All missions complete - the Tunnel can now be excavated!");
    }
    // V0.18: once the current cycle is cleared (and the Tunnel gate above has
    // latched), issue a fresh, harder one — see `sim::missions`.
    missions::tick_mission_cycles(state);

    // --- Tunnel migrants ---
    // Once the Tunnel is unlocked it starts letting travelers through — this
    // is independent of `InvestTunnel`'s excavation-stage megaproject above;
    // the tunnel doesn't need to be fully dug to admit people, just breached.
    // Unlike the caravan offer this needs no leader decision: it resolves
    // automatically the moment the colony has room, checked every tick (not
    // just on the day it spawned) so building a tent while travelers wait
    // lets them in immediately — see `render`'s waiting figures at the
    // tunnel mouth for as long as `pending_migrant` is set.
    if state.tunnel.unlocked
        && state.pending_migrant.is_none()
        && state.tick % TICKS_PER_DAY == ARRIVAL_TICK
        && state.day() >= EVENT_GRACE_DAY
        && erng.chance(TUNNEL_MIGRANT_CHANCE)
    {
        let count = 1 + erng.below(2);
        state.pending_migrant = Some(TunnelMigrant {
            count,
            expires: state.tick + TUNNEL_MIGRANT_WAIT_TICKS,
        });
        let plural = if count == 1 { "" } else { "s" };
        push_event(
            state,
            format!("{} traveler{} emerged from the Tunnel, waiting to be let in.", count, plural),
        );
    }
    if let Some(m) = state.pending_migrant {
        let pop = state.survivors.len() as i32;
        let space = state.housing_capacity() as i32 + 2 - pop;
        if m.count as i32 <= space && m.count as i32 <= MAX_POPULATION - pop {
            for _ in 0..m.count {
                let s = new_survivor(&mut rng, &mut state.next_id);
                state.survivors.push(s);
            }
            state.pending_migrant = None;
            let plural = if m.count == 1 { "" } else { "s" };
            push_event(state, format!("{} traveler{} joined the colony.", m.count, plural));
        } else if state.tick >= m.expires {
            state.pending_migrant = None;
            push_event(state, "With nowhere to stay, the travelers turned back through the Tunnel.");
        }
    }

    // --- V0.13: trade caravan return ---
    // A fixed-duration round trip (see `CARAVAN_TRIP_TICKS`) — the goods (or
    // gold) already left the stockpile at dispatch, so this only ever
    // CREDITS something back, never fails or expires.
    if let Some(c) = state.pending_caravan {
        if state.tick >= c.return_tick {
            state.pending_caravan = None;
            if c.selling {
                state.stock.gold += c.gold;
                push_event(
                    state,
                    format!("The caravan returned with {} gold from selling {} {}.", c.gold as i64, c.amount, c.good.name()),
                );
            } else {
                c.good.credit(&mut state.stock, c.amount as f32);
                push_event(state, format!("The caravan returned with {} {}.", c.amount, c.good.name()));
            }
        }
    }

    // --- V0.18: expeditions come home ---
    // Placed after every survivor-facing system above so a party that returns
    // this tick isn't immediately hungry/tired/frozen on arrival — they land
    // in the roster and start living a normal life next tick. Also before the
    // defeat check below, so a colony whose last people were away is not
    // declared lost on the very tick they walk back in.
    expedition::tick_expeditions(state);

    // --- Defeat ---
    // V0.18: an empty valley is only a defeat when there is genuinely nobody
    // left — a party still on the road is coming back, and `can_launch_
    // expedition` already refuses to send out so many that the colony empties
    // on purpose. Without this a colony whose last home-stayers died while a
    // party was out would be declared lost seconds before they walked in.
    if state.survivors.is_empty() && state.people_away() == 0 {
        state.phase = GamePhase::Lost;
        state.pings.clear();
        push_event(state, "The last survivor has perished. The city falls silent.");
    }

    state.rng = rng.0;
    state.event_rng = erng.0;
}

/// V0.19: index of the tile a survivor at `(x, y)` is standing on. Out-of-
/// bounds floats collapse to tile 0, which is harmless: the callers use it
/// only to look up a speed/cost, and a survivor is never outside the map (see
/// `sim::set_cursor`'s clamp and `spawn_position`'s).
fn tile_of(x: f32, y: f32) -> usize {
    let tx = (x.floor() as i32).clamp(0, MAP_W as i32 - 1) as u8;
    let ty = (y.floor() as i32).clamp(0, MAP_H as i32 - 1) as u8;
    tile_index(tx, ty)
}

/// V0.19: pick this tick's unit step toward `(gx, gy)`, preferring cheap
/// ground. Scores the eight neighbouring tiles (plus standing still, which
/// only wins if every neighbour is worse than being here) by
/// `cost(neighbour) + distance(neighbour -> goal)` and heads for the best.
///
/// Ties break toward the straight line, so on uniform ground — every tile the
/// same cost, which is exactly what a fresh map is — this reduces to the old
/// straight-line walk and nothing about existing movement changes.
fn road_step(tiles: &[Tile], x: f32, y: f32, gx: f32, gy: f32) -> (f32, f32) {
    let (dx, dy) = (gx - x, gy - y);
    let dist = (dx * dx + dy * dy).sqrt();
    let straight = if dist > 0.0 { (dx / dist, dy / dist) } else { (0.0, 0.0) };
    if tiles.is_empty() {
        // Delta snapshots ship an empty tile grid (see `GameState::tile`);
        // a consumer ticking one of those still has to move people.
        return straight;
    }
    let (cx, cy) = (x.floor() as i32, y.floor() as i32);
    // Close enough that neighbour-picking would overshoot the goal: just aim
    // at it. Without this, the last step could wander off a road it is
    // standing on rather than stepping onto the goal tile.
    if dist <= 1.5 {
        return straight;
    }
    let mut best: Option<(f32, (f32, f32))> = None;
    for (ndx, ndy) in [
        (0i32, -1i32), (1, -1), (1, 0), (1, 1),
        (0, 1), (-1, 1), (-1, 0), (-1, -1),
    ] {
        let (nx, ny) = (cx + ndx, cy + ndy);
        if !in_bounds(nx, ny) {
            continue;
        }
        let Some(tile) = tiles.get(tile_index(nx as u8, ny as u8)) else {
            continue;
        };
        // Centre of the candidate tile, and how far the goal still is from
        // there — the heuristic half of the score.
        let (mx, my) = (nx as f32 + 0.5, ny as f32 + 0.5);
        let remaining = ((gx - mx).powi(2) + (gy - my).powi(2)).sqrt();
        // Diagonal moves cover more ground, so they pay proportionally more
        // of the tile's cost — otherwise diagonals would always look cheap.
        let leg = if ndx != 0 && ndy != 0 { std::f32::consts::SQRT_2 } else { 1.0 };
        let score = tile.move_cost() * leg + remaining;
        // A small bias toward the straight-line direction settles ties in
        // favour of the old behaviour (see this fn's doc).
        let alignment = straight.0 * ndx as f32 + straight.1 * ndy as f32;
        let score = score - alignment * 0.001;
        if best.is_none_or(|(b, _)| score < b) {
            let len = (ndx * ndx + ndy * ndy) as f32;
            let len = len.sqrt().max(1.0);
            best = Some((score, (ndx as f32 / len, ndy as f32 / len)));
        }
    }
    best.map(|(_, dir)| dir).unwrap_or(straight)
}

/// A Kitchen's meal-gathering point (if a finished one exists) and every
/// finished Tent's bunk point — see `gather_points`.
pub type GatherPoints = (Option<(f32, f32)>, Vec<(f32, f32)>);

/// The Kitchen's meal-gathering point (if a finished one exists) and every
/// finished Tent's bunk point, both offset just past the building's south
/// edge — see the doc comment at this fn's call site in `tick` for why not
/// the tile center. Shared by `routine_goal` (movement) and
/// `survivor_is_at_meal` (V0.16 render-only pose query) so the two stay in
/// exact sync — the client must ask exactly the same question `tick` asked
/// when it originally sent a survivor here.
pub fn gather_points(state: &GameState) -> GatherPoints {
    let kitchen_pos = state
        .buildings
        .iter()
        .find(|b| b.kind == BuildingKind::Kitchen && !b.under_construction())
        .map(|b| (b.x as f32 + 0.5, b.y as f32 + 1.15));
    let tent_positions: Vec<(f32, f32)> = state
        .buildings
        .iter()
        .filter(|b| b.kind == BuildingKind::Tent && !b.under_construction())
        .map(|b| (b.x as f32 + 0.5, b.y as f32 + 1.15))
        .collect();
    (kitchen_pos, tent_positions)
}

/// V0.16: client-only render query — is `s` *currently* sitting at the
/// Kitchen's meal gathering point (arrived, not still walking there)? Reuses
/// `gather_points`/`routine_goal`'s exact windows and idle-check so a seated
/// pose only ever appears on the survivor `routine_goal` itself would have
/// parked here — a render nicety, never a gameplay signal (no stockpile or
/// assignment effect, mirroring `routine_goal`'s own doc comment below).
pub fn survivor_is_at_meal(state: &GameState, s: &Survivor) -> bool {
    survivor_is_at_meal_with(state, &gather_points(state), s)
}

/// The same query as [`survivor_is_at_meal`], against a `gather_points`
/// result the caller already has. The client asks this for every survivor
/// on every frame (`render::survivors`'s `sync_survivors`); rescanning the
/// building list — and reallocating the tent vector — once per survivor is
/// pure waste when they all see the same buildings.
pub fn survivor_is_at_meal_with(state: &GameState, points: &GatherPoints, s: &Survivor) -> bool {
    if s.move_target.is_some()
        || s.chop_target.is_some()
        || s.bury_target.is_some()
        || s.assigned_building.is_some()
    {
        return false; // something else is telling them where to be
    }
    let kitchen_pos = points.0;
    match routine_goal(state.time_of_day(), kitchen_pos) {
        Some((gx, gy)) => {
            let (dx, dy) = (gx - s.x, gy - s.y);
            (dx * dx + dy * dy).sqrt() <= ARRIVAL_EPSILON && Some((gx, gy)) == kitchen_pos
        }
        None => false,
    }
}

/// V0.15: where an otherwise-idle survivor (nothing else in the movement
/// priority chain claimed them — see `tick`'s `goal` computation) heads at
/// meal/sleep time: the Kitchen during `BREAKFAST_WINDOW`/`LUNCH_WINDOW` if
/// one's finished, their nearest finished Tent overnight (`is_night`).
/// Purely a cosmetic walk destination — it doesn't touch the stockpile,
/// assignment, or any persisted field; a survivor who arrives just stands
/// there like any other idle survivor until the window passes or something
/// else (a player command, a work assignment) claims them. Returns `None`
/// outside those windows, or when there's nowhere fitting to go yet.
///
/// V0.17: the overnight leg moved out into [`sleep_goal`], which now applies
/// to WORKING survivors too (see `tick`'s movement chain) — production is
/// headcount-based, not presence-based, so walking a night shift home to bed
/// costs the colony nothing and is what makes `Survivor::fatigue` recoverable.
/// What's left here is the meal leg, still idle-only.
fn routine_goal(time_of_day: f32, kitchen: Option<(f32, f32)>) -> Option<(f32, f32)> {
    let mealtime = (BREAKFAST_WINDOW.0..BREAKFAST_WINDOW.1).contains(&time_of_day)
        || (LUNCH_WINDOW.0..LUNCH_WINDOW.1).contains(&time_of_day);
    if mealtime {
        if let Some(pos) = kitchen {
            return Some(pos);
        }
    }
    // The overnight leg lives in `sleep_goal` and is dispatched from `tick`'s
    // movement chain instead — it needs to know whether this survivor holds a
    // bunk (`bunked_ids`), which is colony-wide state this fn has no view of.
    // Answering "go to the nearest Tent" here too would walk a bunkless
    // survivor to a bed they don't have while the sim rested them at the
    // rough rate.
    None
}

/// V0.17: the Tent bunk a survivor heads to overnight — their nearest
/// finished Tent, but only if they hold one of the colony's
/// `housing_capacity()` bunks (`has_bunk`, from `GameState::bunked_ids`).
/// Someone with no bed sleeps rough wherever they stand, which is exactly
/// what the slower `FATIGUE_ROUGH_REST_PER_DAY` recovery rate models — the
/// picture and the mechanic come from this one predicate.
pub fn sleep_goal(
    is_night: bool,
    has_bunk: bool,
    s: &Survivor,
    tents: &[(f32, f32)],
) -> Option<(f32, f32)> {
    if !is_night || !has_bunk {
        return None;
    }
    tents.iter().copied().min_by(|a, b| {
        let da = (a.0 - s.x).powi(2) + (a.1 - s.y).powi(2);
        let db = (b.0 - s.x).powi(2) + (b.1 - s.y).powi(2);
        da.total_cmp(&db)
    })
}

/// V0.17: client-only render query (sibling of [`survivor_is_at_meal`]) —
/// is `s` asleep in a Tent bunk right now, i.e. night, they hold a bunk, and
/// they've arrived at it? Nothing else may be claiming them (a player walk,
/// a chop or burial errand); an assigned building does NOT disqualify them,
/// since the night shift walks home too.
pub fn survivor_is_resting(state: &GameState, s: &Survivor) -> bool {
    survivor_is_resting_with(state, &gather_points(state), &state.bunked_ids(), s)
}

/// The same query as [`survivor_is_resting`], against snapshots the caller
/// already has — the client asks it per survivor per frame, same reason
/// `survivor_is_at_meal_with` exists.
pub fn survivor_is_resting_with(
    state: &GameState,
    points: &GatherPoints,
    bunked: &std::collections::HashSet<u32>,
    s: &Survivor,
) -> bool {
    if s.move_target.is_some()
        || s.chop_target.is_some()
        || s.bury_target.is_some()
        || s.carrying_wood
        || !state.furnace_lit
    {
        return false; // mirrors `tick`'s own sleep-leg guards exactly
    }
    match sleep_goal(state.is_night(), bunked.contains(&s.id), s, &points.1) {
        Some((gx, gy)) => {
            let (dx, dy) = (gx - s.x, gy - s.y);
            (dx * dx + dy * dy).sqrt() <= ARRIVAL_EPSILON
        }
        None => false,
    }
}

/// After deaths, make sure assigned workers never exceed the population.
fn clamp_workers(state: &mut GameState) {
    let pop = state.survivors.len() as u32;
    let mut total = state.total_workers();
    if total <= pop {
        return;
    }
    for b in state.buildings.iter_mut().rev() {
        while total > pop && b.workers > 0 {
            b.workers -= 1;
            total -= 1;
        }
    }
}
