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
            if level > 1 {
                push_event(state, format!("{} upgraded to L{}.", kind.name(), level));
            } else {
                push_event(state, format!("{} complete.", kind.name()));
            }
        }
    }

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
        let per_day = kind.production_per_worker_day();
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
            BuildingKind::HunterHut => state.stock.food += amount,
            BuildingKind::Greenhouse => state.stock.food += amount,
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
                    let idx = tile_index(bx, by);
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
    let portion = FOOD_PER_SURVIVOR_DAY / TICKS_PER_DAY as f32 * kitchen_efficiency * rationing_factor;
    let insulation_bonus = if state.has_tech(Tech::Insulation) {
        TECH_INSULATION_WARMTH
    } else {
        0.0
    };
    let mut deaths: Vec<(u32, String, &'static str)> = Vec::new();
    let disease = state.disease_active();

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
    // Deferred mutations the chop/carry cycle collects instead of touching
    // `state.tiles`/`state.buildings`/`state.stock` directly from inside the
    // survivors loop (applied once the loop's mutable borrow of
    // `state.survivors` ends).
    let mut chopped_tiles: Vec<(u8, u8)> = Vec::new();
    let mut logs_delivered: u32 = 0;
    let mut manual_wood_gained: u32 = 0;
    for s in state.survivors.iter_mut() {
        // --- Movement: move_target (player-issued walk) takes priority,
        // then a furnace-building chop errand (`chop_target`), then the
        // assigned building's location; with none of the three, stand put.
        let goal = s.move_target.map(|(x, y)| (x as f32 + 0.5, y as f32 + 0.5))
            .or_else(|| s.chop_target.map(|(x, y)| (x as f32 + 0.5, y as f32 + 0.5)))
            .or_else(|| {
                s.assigned_building
                    .and_then(|id| building_lookup.get(&id))
                    .map(|(_, bx, by, _)| (*bx as f32 + 0.5, *by as f32 + 0.5))
            });
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
                }
            } else {
                s.x += dx / dist * SURVIVOR_SPEED_PER_TICK;
                s.y += dy / dist * SURVIVOR_SPEED_PER_TICK;
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
                s.xp += 1.0 / TICKS_PER_DAY as f32;
            }
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
    for (i, s) in state.survivors.iter_mut().enumerate() {
        s.hunger = (s.hunger + hunger_per_tick).min(120.0);
        if s.hunger >= 25.0 && state.stock.food >= portion {
            state.stock.food -= portion;
            s.hunger = (s.hunger - 0.4).max(0.0);
        }
        if s.hunger >= 80.0 {
            starving_present = true;
        }

        let bonus = if lit && i < warm_slots {
            12.0 + 6.0 * level as f32
        } else if i < warm_slots + shelter_slots {
            6.0
        } else if lit {
            3.0 // huddling near the open furnace
        } else {
            0.0
        } + insulation_bonus;
        let eff = temp + bonus;
        if eff < 0.0 {
            s.hp -= (-eff).min(40.0) * 0.35 / tph;
        } else if eff >= 5.0 && s.hunger < 60.0 {
            s.hp = (s.hp + 3.0 / tph).min(100.0);
        }
        if s.hunger >= 80.0 {
            s.hp -= 4.0 * ((s.hunger - 80.0) / 20.0) / tph;
        }
        if care_per_tick > 0.0 {
            s.hp = (s.hp + care_per_tick).min(100.0);
        }
        if disease {
            s.hp -= DISEASE_HP_PER_DAY / TICKS_PER_DAY as f32;
        }
        if s.hp <= 0.0 {
            // Attribute the death to the most likely cause for the event log.
            let cause = if s.hunger >= 80.0 {
                "starved"
            } else if disease {
                "succumbed to illness"
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
        if state.blizzard_active() {
            delta -= per_tick(MORALE_BLIZZARD_PER_DAY);
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
        state.survivors.retain(|s| s.hp > 0.0);
        state.morale = (state.morale - MORALE_DEATH_PENALTY * deaths.len() as f32).clamp(0.0, 100.0);
        for (_, name, cause) in deaths {
            push_event(state, format!("{} has {}.", name, cause));
        }
        // Still needed as a fallback for anonymous-only overflow (shouldn't
        // normally trigger now that every named slot above is freed first).
        clamp_workers(state);
    }

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

    // --- Defeat ---
    if state.survivors.is_empty() {
        state.phase = GamePhase::Lost;
        state.pings.clear();
        push_event(state, "The last survivor has perished. The city falls silent.");
    }

    state.rng = rng.0;
    state.event_rng = erng.0;
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
