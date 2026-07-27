use crate::rng::Rng;
use crate::types::*;
use super::*;

/// Bind one named survivor (by index) to `building`, doing everything an
/// assignment implies: raise the building's headcount, point the survivor at
/// it, drop any in-progress chop errand, and reset training on a KIND change.
///
/// Extracted in V0.20 because three commands need exactly this and only one
/// of them used to do it. `Place`'s auto-crew and `AdjustWorkers`' `+` both
/// used to just increment `Building::workers` — an ANONYMOUS worker, bound to
/// nobody. The building then reported "1 working" while every survivor stood
/// around with an empty workplace and walked nowhere, because movement keys
/// off `assigned_building`, which no one had. The count and the world
/// disagreed, and the player could see both.
fn bind_survivor(state: &mut GameState, s_idx: usize, building: u32) {
    let Some(kind) = state.buildings.iter().find(|b| b.id == building).map(|b| b.kind) else {
        return;
    };
    if let Some(b) = state.buildings.iter_mut().find(|b| b.id == building) {
        b.workers += 1;
    }
    let s = &mut state.survivors[s_idx];
    s.assigned_building = Some(building);
    // Reassigning away from the Furnace drops any in-progress chop errand —
    // they're not building it anymore. A log already chopped and being
    // carried home is credited now rather than destroyed: the tree is gone
    // either way.
    let carried = s.carrying_wood;
    s.chop_target = None;
    s.carrying_wood = false;
    // A different building KIND resets training progress; moving within the
    // same kind keeps it, since the trade is what's learned.
    if s.trained_kind != Some(kind) {
        s.trained_kind = Some(kind);
        s.xp = 0.0;
    }
    if carried {
        state.stock.wood += 1.0;
    }
}

/// The idle survivors a new crew should be drawn from, lowest id first —
/// deterministic (ids only ever grow and the roster is never reordered), so
/// the same seed always crews the same site with the same people.
fn idle_ids(state: &GameState, want: usize) -> Vec<u32> {
    let mut ids: Vec<u32> = state
        .survivors
        .iter()
        .filter(|s| s.assigned_building.is_none())
        .map(|s| s.id)
        .collect();
    ids.sort_unstable();
    ids.truncate(want);
    ids
}

/// Bind up to `want` idle survivors to `building`. Returns how many actually
/// took the job.
fn crew_from_idle(state: &mut GameState, building: u32, want: usize) -> usize {
    let ids = idle_ids(state, want);
    let mut taken = 0;
    for id in ids {
        if let Some(idx) = state.survivors.iter().position(|s| s.id == id) {
            bind_survivor(state, idx, building);
            taken += 1;
        }
    }
    taken
}

/// Validate and apply a player command. Invalid commands are silently ignored
/// (the client pre-validates, so this only happens on races or tampering).
pub fn apply_command(state: &mut GameState, player: u64, cmd: &PlayerCommand) {
    if state.phase != GamePhase::Running {
        return;
    }
    if !state.can_issue(player, cmd) {
        return;
    }
    match cmd {
        PlayerCommand::Place { kind, x, y, facing } => {
            if state.can_place(*kind, *x, *y).is_ok() {
                // V0.8: kasalxona/oshxona kabi, ombor darajasi ham
                // chegirmani kuchaytiradi — eng yuqori darajali ishchili
                // ombordan olinadi.
                let warehouse_level_factor = state
                    .buildings
                    .iter()
                    .filter(|b| b.kind == BuildingKind::Warehouse && b.workers > 0 && !b.under_construction())
                    .map(|b| b.level_factor())
                    .fold(0.0f32, f32::max);
                let discount = if warehouse_level_factor > 0.0 {
                    (1.0 - (1.0 - WAREHOUSE_BUILD_DISCOUNT) * warehouse_level_factor).max(0.1)
                } else {
                    1.0
                };
                let spent_wood = kind.cost_wood() as f32 * discount;
                // V0.8: qarzga qurilmaydi — resurs yetmasa buyruq e'tiborsiz
                // qoladi (client menyusi bu shartni oldindan ko'rsatadi).
                if state.stock.wood < spent_wood {
                    return;
                }
                state.stock.wood -= spent_wood;
                let id = state.next_id;
                state.next_id += 1;
                // Central-world buildings are owned by the placing ACCOUNT
                // (so only that account may later demolish them, across any
                // of its sessions); elsewhere ownership stays session-based
                // as before (`owner_account: None`).
                let owner_account = if state.central { state.player_account(player) } else { None };
                state.buildings.push(Building {
                    id,
                    kind: *kind,
                    x: *x,
                    y: *y,
                    // Crewed below, by NAME — see `bind_survivor`.
                    workers: 0,
                    progress: 0.0,
                    owner: Some(player),
                    owner_account,
                    level: 1,
                    build_left: kind.build_workdays(),
                    // A new building starts bare: every fitting is bought
                    // afterwards (`UpgradeFurnishing`). An empty vec reads
                    // as all-zero everywhere (`furnishing_level`).
                    furnishings: Vec::new(),
                    // Visual only; clamp defensively so a hand-crafted client
                    // can't smuggle an out-of-range value into the world.
                    facing: *facing % 4,
                });
                // V0.8: a new building starts as a construction site and pulls
                // an automatic crew off the idle pool (as many as there are,
                // up to the cap; with nobody free the site simply waits for
                // the player to assign someone).
                //
                // V0.20: that crew is drawn BY NAME. It used to be an
                // anonymous bump of `workers`, which made the site claim a
                // worker the colony could see standing idle somewhere else —
                // the count said "1 building", the idle pool dropped to 0, and
                // the survivor it had supposedly taken walked nowhere and
                // reported an empty workplace. Naming them makes the picture
                // and the mechanic the same thing: they walk over, they tire,
                // they train, and the roster says where they are.
                //
                // The CENTRAL world still crews nothing: every settler there
                // belongs to an account, so conscripting one automatically is
                // exactly what `AdjustWorkers` is forbidden for there.
                if !state.central {
                    crew_from_idle(state, id, CONSTRUCTION_CREW_MAX as usize);
                }
                // Central-world economy v1: charge the placing account's
                // ledger for what it spent, so a showcase can reflect it.
                if let Some(acc) = owner_account {
                    state.credit_ledger(acc, |t| t.wood_spent += spent_wood);
                }
                // Attribute the placement to the player, if they're in the roster.
                let name = state.player(player).map(|p| p.name.clone());
                if let Some(p) = state.players.iter_mut().find(|p| p.id == player) {
                    p.built += 1;
                }
                if let Some(name) = name {
                    push_action_event(state, format!("{} started building a {}.", name, kind.name()));
                }
            }
        }
        PlayerCommand::Demolish { building } => {
            if let Some(i) = state
                .buildings
                .iter()
                .position(|b| b.id == *building && b.kind.buildable())
            {
                let b = state.buildings.remove(i);
                // Refund scales with the building's level — the same way its
                // upgrade cost did on the way up (`upgrade_cost_wood`) — so
                // tearing down a heavily-upgraded building isn't worth
                // identical salvage to a brand-new one.
                state.stock.wood += b.kind.cost_wood() as f32 * DEMOLISH_REFUND * b.level_factor();
                // The building's own worker slots go with it; don't leave any
                // named survivor pointing at a building id that no longer exists.
                for s in state.survivors.iter_mut() {
                    if s.assigned_building == Some(b.id) {
                        s.assigned_building = None;
                    }
                }
                // Attribute the demolition to the player, if they're in the roster.
                let name = state.player(player).map(|p| p.name.clone());
                if let Some(p) = state.players.iter_mut().find(|p| p.id == player) {
                    p.demolished += 1;
                }
                if let Some(name) = name {
                    push_action_event(state, format!("{} demolished a {}.", name, b.kind.name()));
                }
            }
        }
        PlayerCommand::AdjustWorkers { building, delta } => {
            // V0.20: `+` and `-` move PEOPLE now, not just a number. This used
            // to add anonymous headcount — the building's count went up and
            // the idle pool went down, but nobody walked over, nobody tired,
            // nobody trained, and the roster still said their workplace was
            // empty. Same incoherence `Place`'s auto-crew had; same fix.
            let Some(i) = state.buildings.iter().position(|b| b.id == *building) else {
                return;
            };
            let (busy, kind, cur) = {
                let b = &state.buildings[i];
                (b.under_construction(), b.kind, b.workers as i32)
            };
            // V0.8: a construction site takes a crew up to `CONSTRUCTION_CREW_MAX`
            // (even for kinds that employ nobody once finished, like a Tent);
            // a finished building takes its own trade's slots.
            let max = if busy { CONSTRUCTION_CREW_MAX as i32 } else { kind.max_workers() as i32 };
            let target = (cur + *delta as i32).clamp(0, max);
            if target > cur {
                crew_from_idle(state, *building, (target - cur) as usize);
            } else if target < cur {
                let mut to_drop = (cur - target) as usize;
                // Let go of the most recently added first (highest id), so
                // repeatedly tapping `-` undoes `+` in the order it happened.
                let mut named: Vec<u32> = state
                    .survivors
                    .iter()
                    .filter(|s| s.assigned_building == Some(*building))
                    .map(|s| s.id)
                    .collect();
                named.sort_unstable_by(|a, b| b.cmp(a));
                // Any headcount beyond the named survivors is anonymous slack
                // — from a test, a tool, or a save written before V0.20. Shed
                // that first: there is no one to send home for it.
                let anonymous = (cur as usize).saturating_sub(named.len());
                let shed_anonymous = anonymous.min(to_drop);
                to_drop -= shed_anonymous;
                for id in named.into_iter().take(to_drop) {
                    if let Some(s) = state.survivors.iter_mut().find(|s| s.id == id) {
                        s.assigned_building = None;
                    }
                }
                state.buildings[i].workers = target as u8;
            }
        }
        PlayerCommand::AssignSurvivor { survivor, building } => {
            let Some(s_idx) = state.survivors.iter().position(|s| s.id == *survivor) else {
                return;
            };
            let prev = state.survivors[s_idx].assigned_building;
            match building {
                Some(new_id) => {
                    let Some(target) = state.buildings.iter().find(|b| b.id == *new_id) else {
                        return;
                    };
                    // V0.8: qurilish maydonchasi nomlangan ustalarni brigada
                    // capigacha oladi (jumladan max_workers == 0 turlar ham,
                    // masalan Chodir maydonchasi); bitgan bino esa o'z kasb
                    // o'rinlari bilan cheklanadi.
                    let capacity = if target.under_construction() {
                        CONSTRUCTION_CREW_MAX
                    } else {
                        target.kind.max_workers()
                    };
                    if capacity == 0 || prev == Some(*new_id) {
                        return;
                    }
                    if target.workers >= capacity {
                        return;
                    }
                    let new_kind = target.kind;
                    // `Building.workers` is a HEADCOUNT: named survivors
                    // assigned here, plus anonymous slack filled from the idle
                    // pool. Naming someone who is ALREADY inside that
                    // anonymous count (which is everyone, once
                    // `idle_workers()` has hit zero) must therefore convert a
                    // slot, never add one — otherwise one person is counted
                    // twice and the building reads "2 working" with a single
                    // survivor on site. That was exactly the case after
                    // `Place` auto-crewed a site from the idle pool and the
                    // player then assigned that same survivor to it by name.
                    let vacate = if prev.is_some() {
                        // Moving between buildings: the old one gives up the
                        // slot, as it always did.
                        prev
                    } else if state.idle_workers() > 0 {
                        // Genuine slack in the colony — this is a new pair of
                        // hands, so the headcount really does grow.
                        None
                    } else {
                        // No slack: this survivor is already occupying an
                        // anonymous slot somewhere. Prefer the target itself
                        // (the common case, and the only one the player can
                        // see); otherwise take the lowest-id building that has
                        // one, which is deterministic and always exists —
                        // with no idle workers and no named assignment, some
                        // building's anonymous count must be covering them.
                        let anonymous_at = |b: &Building| {
                            let named = state
                                .survivors
                                .iter()
                                .filter(|s| s.assigned_building == Some(b.id))
                                .count();
                            b.workers as usize > named
                        };
                        let mut ids: Vec<u32> = state
                            .buildings
                            .iter()
                            .filter(|b| anonymous_at(b))
                            .map(|b| b.id)
                            .collect();
                        ids.sort_unstable();
                        if ids.contains(new_id) { Some(*new_id) } else { ids.first().copied() }
                    };
                    if let Some(vacate_id) = vacate {
                        if let Some(b) = state.buildings.iter_mut().find(|b| b.id == vacate_id) {
                            b.workers = b.workers.saturating_sub(1);
                        }
                    }
                    if let Some(b) = state.buildings.iter_mut().find(|b| b.id == *new_id) {
                        b.workers += 1;
                    }
                    state.survivors[s_idx].assigned_building = Some(*new_id);
                    // Reassigning away from the Furnace (its only source)
                    // drops any in-progress chop errand — they're not
                    // building it anymore, so nothing should keep walking
                    // them toward a tree or back. If they'd already chopped
                    // a log and were mid-walk home, credit it to the
                    // stockpile now — the tree is already gone either way,
                    // so dropping the log too would just destroy it.
                    if state.survivors[s_idx].carrying_wood {
                        state.stock.wood += 1.0;
                    }
                    state.survivors[s_idx].chop_target = None;
                    state.survivors[s_idx].carrying_wood = false;
                    // A different building KIND resets training progress —
                    // reassigning within the same kind (e.g. one Sawmill to
                    // another) keeps it, since the trade is what's learned,
                    // not the specific building.
                    let s = &mut state.survivors[s_idx];
                    if s.trained_kind != Some(new_kind) {
                        s.trained_kind = Some(new_kind);
                        s.xp = 0.0;
                    }
                }
                None => {
                    let Some(prev_id) = prev else { return };
                    if let Some(b) = state.buildings.iter_mut().find(|b| b.id == prev_id) {
                        b.workers = b.workers.saturating_sub(1);
                    }
                    state.survivors[s_idx].assigned_building = None;
                    if state.survivors[s_idx].carrying_wood {
                        state.stock.wood += 1.0;
                    }
                    state.survivors[s_idx].chop_target = None;
                    state.survivors[s_idx].carrying_wood = false;
                    // Plain unassignment does NOT reset xp/trained_kind —
                    // only an assignment to a genuinely different kind does
                    // (see the `Some(new_id)` arm above). A temporarily
                    // idled survivor keeps their training.
                }
            }
        }
        PlayerCommand::MoveSurvivor { survivor, x, y } => {
            if !in_bounds(*x as i32, *y as i32) {
                return;
            }
            let Some(s_idx) = state.survivors.iter().position(|s| s.id == *survivor) else {
                return;
            };
            // Walking a survivor is a manual override: they become idle
            // (unassigned from work) exactly like `AssignSurvivor { building:
            // None }` does, freeing whatever slot they held.
            if let Some(prev_id) = state.survivors[s_idx].assigned_building {
                if let Some(b) = state.buildings.iter_mut().find(|b| b.id == prev_id) {
                    b.workers = b.workers.saturating_sub(1);
                }
            }
            state.survivors[s_idx].assigned_building = None;
            // A log already chopped and mid-carry isn't dropped on the
            // ground when redirected — it's credited immediately, same as a
            // manual `ChopTile` chop (see below).
            if state.survivors[s_idx].carrying_wood {
                state.stock.wood += 1.0;
            }
            let s = &mut state.survivors[s_idx];
            s.chop_target = None;
            s.carrying_wood = false;
            s.move_target = Some((*x, *y));
        }
        PlayerCommand::ChopTile { survivor, x, y } => {
            if !in_bounds(*x as i32, *y as i32) {
                return;
            }
            let idx = tile_index(*x, *y);
            if state.tiles[idx].terrain != Terrain::Forest || state.tiles[idx].deposit == 0 {
                return;
            }
            let Some(s_idx) = state.survivors.iter().position(|s| s.id == *survivor) else {
                return;
            };
            // A manual chop is the same kind of override `MoveSurvivor` is —
            // it replaces whatever they were doing, including a
            // Furnace-building errand already in progress (see `tick.rs`'s
            // chop/carry block, which only treats `chop_target` as a Furnace
            // delivery while `assigned_building` still points at it).
            if let Some(prev_id) = state.survivors[s_idx].assigned_building {
                if let Some(b) = state.buildings.iter_mut().find(|b| b.id == prev_id) {
                    b.workers = b.workers.saturating_sub(1);
                }
            }
            // Redirecting to a new tree drops a log already chopped and
            // mid-carry the same way — credited immediately rather than lost.
            if state.survivors[s_idx].carrying_wood {
                state.stock.wood += 1.0;
            }
            let s = &mut state.survivors[s_idx];
            s.assigned_building = None;
            s.move_target = None;
            s.chop_target = Some((*x, *y));
            s.carrying_wood = false;
        }
        PlayerCommand::Bury { survivor, corpse } => {
            let Some(s_idx) = state.survivors.iter().position(|s| s.id == *survivor) else {
                return;
            };
            let Some(c_idx) = state.corpses.iter().position(|c| c.id == *corpse) else {
                return;
            };
            // Already claimed by someone else — a second player can't pile
            // onto the same body.
            if state.corpses[c_idx].being_buried_by.is_some() {
                return;
            }
            // Same override rules as ChopTile/MoveSurvivor: drop whatever
            // they were doing, credit any wood already mid-carry.
            if let Some(prev_id) = state.survivors[s_idx].assigned_building {
                if let Some(b) = state.buildings.iter_mut().find(|b| b.id == prev_id) {
                    b.workers = b.workers.saturating_sub(1);
                }
            }
            if state.survivors[s_idx].carrying_wood {
                state.stock.wood += 1.0;
            }
            let s = &mut state.survivors[s_idx];
            s.assigned_building = None;
            s.move_target = None;
            s.chop_target = None;
            s.carrying_wood = false;
            s.bury_target = Some(*corpse);
            state.corpses[c_idx].being_buried_by = Some(*survivor);
        }
        PlayerCommand::SetLeader { survivor } => {
            if state.survivors.iter().any(|s| s.id == *survivor) {
                state.leader = Some(*survivor);
                // A freshly appointed leader ends mourning immediately —
                // `leader_multiplier()` already gives a living leader
                // priority over an active mourning penalty, so leaving
                // `mourning_until` untouched would just let the client's
                // (mourning_active()-driven) UI disagree with the
                // production math for the rest of the window.
                state.mourning_until = 0;
                let name = state.survivors.iter().find(|s| s.id == *survivor).unwrap().name.clone();
                push_event(state, format!("{} has been chosen as leader.", name));
            }
        }
        PlayerCommand::SetFurnaceLevel { level } => {
            // Can't dial in a burn level before the furnace itself exists —
            // it's still a construction site until `build_left` runs out
            // (`tick.rs` lights it and sets the level to 1 at that point).
            // A LATER V0.9 level upgrade (`b.level` 1-10) also re-sets
            // `build_left`, but by then `state.furnace_level` is already
            // nonzero, so that alone wouldn't re-block this (mirrors
            // `tick.rs`'s `furnace_info`/construction-loop gating, same
            // `state.furnace_level == 0` signal) — but a rough "gulxan"
            // (levels 1-6) has no damper to dial in either way: only once
            // it's grown into an established `Pech` (`b.level >= 7`,
            // matching `render/buildings.rs`'s two-tier model) is the burn
            // setting under player control at all.
            let furnace = state.buildings.iter().find(|b| b.kind == BuildingKind::Furnace);
            let still_building = state.furnace_level == 0
                && furnace.is_some_and(Building::under_construction);
            let too_young = furnace.is_none_or(|b| b.level < 7);
            if !still_building && !too_young {
                state.furnace_level = (*level).min(3);
            }
        }
        PlayerCommand::InvestTunnel => {
            if state.tunnel.unlocked
                && state.tunnel.stage < TUNNEL_STAGES
                && state.stock.wood >= TUNNEL_INVEST_WOOD
                && state.stock.coal >= TUNNEL_INVEST_COAL
            {
                state.stock.wood -= TUNNEL_INVEST_WOOD;
                state.stock.coal -= TUNNEL_INVEST_COAL;
                state.tunnel.progress += 1.0 / TUNNEL_INVESTS_PER_STAGE as f32;
                if state.tunnel.progress >= 1.0 - 1e-4 {
                    state.tunnel.progress = 0.0;
                    state.tunnel.stage += 1;
                    if state.tunnel.stage >= TUNNEL_STAGES {
                        state.tunnel.stage = TUNNEL_STAGES;
                        state.phase = GamePhase::Won;
                        state.graduated = true;
                        push_event(state, "The Tunnel breaks through - the Global World awaits. Victory!");
                    } else {
                        push_event(state, format!("Tunnel stage {}/{} excavated.", state.tunnel.stage, TUNNEL_STAGES));
                    }
                }
            }
        }
        PlayerCommand::Research { tech } => {
            if !state.has_tech(*tech)
                && state.stock.wood >= tech.cost_wood() as f32
                && state.stock.coal >= tech.cost_coal() as f32
            {
                state.stock.wood -= tech.cost_wood() as f32;
                state.stock.coal -= tech.cost_coal() as f32;
                state.techs.push(*tech);
                push_event(state, format!("Researched: {}.", tech.name()));
            }
        }
        PlayerCommand::RespondEvent { accept } => {
            // V0.7: a caravan choice needs someone to actually make the
            // call. Without a living leader, player input is ignored and the
            // offer is left standing to auto-resolve to reject at its
            // existing deadline (`tick`'s pending-event expiry, unchanged).
            if !state.leader_alive() {
                return;
            }
            if let Some(offer) = state.pending_event.take() {
                if *accept {
                    let pop = state.survivors.len() as i32;
                    let space = (state.housing_capacity() as i32 + 2 - pop).max(0);
                    let take = (offer.count as i32).min(space).min(MAX_POPULATION - pop).max(0) as u32;
                    // Charge only for the refugees actually admitted, never the
                    // full offer, so a tight housing/pop cap doesn't waste food.
                    let cost = (take * CARAVAN_FOOD_PER_PERSON) as f32;
                    if take > 0 && state.stock.food >= cost {
                        state.stock.food -= cost;
                        let mut erng_local = Rng(state.event_rng);
                        for _ in 0..take {
                            let s = new_survivor(&mut erng_local, &mut state.next_id);
                            state.survivors.push(s);
                        }
                        state.event_rng = erng_local.0;
                        push_event(state, format!("{} refugees joined the city.", take));
                    } else {
                        push_event(state, "There was no room or food for the caravan.");
                    }
                } else {
                    push_event(state, "The caravan was turned away.");
                }
            }
        }
        PlayerCommand::UpgradeBuilding { building } => {
            // Bo'sh ishchilar soni mutatsiyadan OLDIN — brigada to'ldirishda
            // aynan shu zaxiradan olinadi.
            let idle = state.idle_workers();
            if let Some(i) = state.buildings.iter().position(|b| b.id == *building) {
                let (kind, level, busy) = {
                    let b = &state.buildings[i];
                    (b.kind, b.level, b.under_construction())
                };
                // Faqat bitgan, yangilansa bo'ladigan va maksimumga yetmagan
                // bino yangilanadi; yangilashlar zanjir — keyingisi faqat
                // oldingisi bitgach. `upgradeable` (`buildable`dan farqli) —
                // Pech ham shu yo'ldan o'sadi, garchi qayta joylashtirib
                // yoki buzib bo'lmasa ham.
                if !kind.upgradeable() || busy || level >= BUILDING_MAX_LEVEL {
                    return;
                }
                // V0.20: a room has to be furnished before it can be enlarged
                // — every fitting it takes must have kept pace with its level
                // (see `Building::furnishings_keep_pace`). Vacuously true for
                // anything with no interior, so Tents/Walls/the Furnace climb
                // exactly as they always did.
                if !state.buildings[i].furnishings_keep_pace() {
                    return;
                }
                let cost = kind.upgrade_cost_wood(level + 1) as f32;
                if state.stock.wood < cost {
                    return;
                }
                state.stock.wood -= cost;
                if state.central {
                    if let Some(acc) = state.buildings[i].owner_account {
                        state.credit_ledger(acc, |t| t.wood_spent += cost);
                    }
                }
                let b = &mut state.buildings[i];
                b.level += 1;
                b.build_left = kind.upgrade_workdays(b.level);
                // Mavjud ishchilar ustaga aylanadi; brigada to'lmagan bo'lsa
                // bo'sh ishchilardan avtomatik to'ldiriladi (Place'dagi kabi) —
                // lekin MARKAZIY olamda emas: `Place` singari, u yerda har bir
                // aholi akkauntga tegishli, shuning uchun boshqa akkauntning
                // bo'sh ko'chmanchisini so'rovsiz jalb qilib bo'lmaydi (egasi
                // o'zi `AssignSurvivor` bilan biriktiradi).
                let idle = if state.central { 0 } else { idle };
                let add = (CONSTRUCTION_CREW_MAX.saturating_sub(b.workers) as u32).min(idle) as u8;
                b.workers += add;
                let target = b.level;
                let name = state.player(player).map(|p| p.name.clone());
                if let Some(name) = name {
                    push_action_event(
                        state,
                        format!("{} started upgrading a {} to L{}.", name, kind.name(), target),
                    );
                }
            }
        }
        PlayerCommand::DispatchTradeCaravan { good, amount, selling } => {
            // Only one caravan on the road at a time (single-slot, same
            // convention as `pending_migrant`), and the Tunnel must at least
            // be breached (not necessarily fully excavated — see
            // `CARAVAN_TRIP_TICKS`'s doc comment).
            if !state.tunnel.unlocked
                || state.pending_caravan.is_some()
                || *amount == 0
                || *amount > CARAVAN_MAX_AMOUNT
            {
                return;
            }
            if *selling {
                if good.amount_in(&state.stock) < *amount as f32 {
                    return;
                }
                // Loaded onto the caravan immediately — it leaves the
                // stockpile now, not on return, same as a manual chop
                // crediting wood the moment it's cut.
                good.credit(&mut state.stock, -(*amount as f32));
                let gold = *amount as f32 * good.sell_price();
                state.pending_caravan = Some(TradeCaravan {
                    selling: true,
                    good: *good,
                    amount: *amount,
                    gold,
                    departed_tick: state.tick,
                    return_tick: state.tick + CARAVAN_TRIP_TICKS,
                });
                push_event(state, format!("A caravan departed through the Tunnel to sell {} {}.", amount, good.name()));
            } else {
                let cost = *amount as f32 * good.buy_price();
                if state.stock.gold < cost {
                    return;
                }
                state.stock.gold -= cost;
                state.pending_caravan = Some(TradeCaravan {
                    selling: false,
                    good: *good,
                    amount: *amount,
                    gold: 0.0,
                    departed_tick: state.tick,
                    return_tick: state.tick + CARAVAN_TRIP_TICKS,
                });
                push_event(state, format!("A caravan departed through the Tunnel to buy {} {}.", amount, good.name()));
            }
        }
        PlayerCommand::RelocateBuilding { building, x, y } => {
            if state.can_relocate(*building, *x, *y).is_ok() {
                // Bo'sh ishchilar soni mutatsiyadan OLDIN, xuddi
                // UpgradeBuilding'dagi kabi.
                let idle = state.idle_workers();
                if let Some(i) = state.buildings.iter().position(|b| b.id == *building) {
                    let kind = state.buildings[i].kind;
                    let b = &mut state.buildings[i];
                    b.x = *x;
                    b.y = *y;
                    // Free — no wood charged (see `GameState::can_relocate`'s
                    // doc comment) — just a discounted rebuild timer;
                    // `level` and any named worker assignments carry over
                    // unchanged.
                    b.build_left = kind.build_workdays() * RELOCATE_WORKDAYS_FACTOR;
                    let idle = if state.central { 0 } else { idle };
                    let add = (CONSTRUCTION_CREW_MAX.saturating_sub(b.workers) as u32).min(idle) as u8;
                    b.workers += add;
                    let name = state.player(player).map(|p| p.name.clone());
                    if let Some(name) = name {
                        push_action_event(state, format!("{} started relocating a {}.", name, kind.name()));
                    }
                }
            }
        }
        PlayerCommand::RotateBuilding { building } => {
            if state.can_rotate(*building).is_ok() {
                // Idle count snapshotted before the mutation, same as
                // `RelocateBuilding`/`UpgradeBuilding`.
                let idle = state.idle_workers();
                if let Some(i) = state.buildings.iter().position(|b| b.id == *building) {
                    let kind = state.buildings[i].kind;
                    let b = &mut state.buildings[i];
                    b.facing = (b.facing + 1) % 4;
                    // Modelled on relocation: free of wood, but re-squaring the
                    // structure re-enters construction at the discounted timer.
                    b.build_left = kind.build_workdays() * RELOCATE_WORKDAYS_FACTOR;
                    let idle = if state.central { 0 } else { idle };
                    let add = (CONSTRUCTION_CREW_MAX.saturating_sub(b.workers) as u32).min(idle) as u8;
                    b.workers += add;
                    let name = state.player(player).map(|p| p.name.clone());
                    if let Some(name) = name {
                        push_action_event(state, format!("{} started turning a {}.", name, kind.name()));
                    }
                }
            }
        }
        // V0.18: each new mechanic keeps its logic in its own module (see
        // `sim::expedition` / `sim::lawbook`); this match only routes.
        PlayerCommand::LaunchExpedition { site, members } => {
            super::expedition::launch(state, *site, members)
        }
        PlayerCommand::RecallExpedition { expedition } => {
            super::expedition::recall(state, *expedition)
        }
        // V0.18: relocate + re-face in one commit (see `RelocateFacing`'s doc
        // for why it can't be two commands). Everything but the extra
        // `facing` write is `RelocateBuilding`'s arm verbatim.
        PlayerCommand::RelocateFacing { building, x, y, facing } => {
            if state.can_relocate(*building, *x, *y).is_ok() {
                let idle = state.idle_workers();
                if let Some(i) = state.buildings.iter().position(|b| b.id == *building) {
                    let kind = state.buildings[i].kind;
                    let b = &mut state.buildings[i];
                    b.x = *x;
                    b.y = *y;
                    // Visual only; clamped defensively so a hand-crafted
                    // client can't smuggle an out-of-range value in (same
                    // guard `Place` uses).
                    b.facing = *facing % 4;
                    b.build_left = kind.build_workdays() * RELOCATE_WORKDAYS_FACTOR;
                    let idle = if state.central { 0 } else { idle };
                    let add = (CONSTRUCTION_CREW_MAX.saturating_sub(b.workers) as u32).min(idle) as u8;
                    b.workers += add;
                    let name = state.player(player).map(|p| p.name.clone());
                    if let Some(name) = name {
                        push_action_event(state, format!("{} started relocating a {}.", name, kind.name()));
                    }
                }
            }
        }
        // V0.19: roads and snow. One command per drag — see `BuildRoad`.
        PlayerCommand::BuildRoad { tiles } => {
            let mut laid = 0u32;
            for (x, y) in tiles.iter().take(MAX_ROAD_TILES_PER_COMMAND) {
                if state.can_lay_road(*x, *y).is_err() {
                    continue;
                }
                // Never on credit: the drag simply stops where the wood does.
                if state.stock.wood < ROAD_COST_WOOD {
                    break;
                }
                state.stock.wood -= ROAD_COST_WOOD;
                let idx = tile_index(*x, *y);
                state.tiles[idx].road = true;
                // Laying a road clears what had settled on it — the crew is
                // standing right there.
                state.tiles[idx].snow = 0;
                laid += 1;
            }
            if laid > 0 {
                let name = state.player(player).map(|p| p.name.clone());
                if let Some(name) = name {
                    push_action_event(state, format!("{} laid {} tiles of road.", name, laid));
                }
            }
        }
        PlayerCommand::RemoveRoad { tiles } => {
            let mut torn = 0u32;
            for (x, y) in tiles.iter().take(MAX_ROAD_TILES_PER_COMMAND) {
                let idx = tile_index(*x, *y);
                if state.tiles.get(idx).is_none_or(|t| !t.road) {
                    continue;
                }
                state.tiles[idx].road = false;
                state.stock.wood += ROAD_COST_WOOD * ROAD_REFUND;
                torn += 1;
            }
            if torn > 0 {
                let name = state.player(player).map(|p| p.name.clone());
                if let Some(name) = name {
                    push_action_event(state, format!("{} tore up {} tiles of road.", name, torn));
                }
            }
        }
        PlayerCommand::ClearSnow { survivor, x, y } => {
            if state.tile(*x, *y).is_none() {
                return;
            }
            let Some(s) = state.survivors.iter_mut().find(|s| s.id == *survivor) else {
                return;
            };
            // A manual order always overrides the standing job, exactly like
            // `MoveSurvivor`/`ChopTile`/`Bury`.
            s.assigned_building = None;
            s.move_target = Some((*x, *y));
            s.chop_target = None;
            s.bury_target = None;
            s.carrying_wood = false;
            let id = s.id;
            // One order per tile: re-issuing just re-points whoever is going.
            match state.clear_orders.iter_mut().find(|o| o.x == *x && o.y == *y) {
                Some(o) => o.survivor = id,
                None => state.clear_orders.push(ClearOrder {
                    x: *x,
                    y: *y,
                    survivor: id,
                    work_left: CLEAR_SNOW_WORKDAYS * TICKS_PER_DAY as f32,
                }),
            }
        }
        // V0.20: buy or improve one fitting. Unlike `UpgradeBuilding` this
        // never re-enters construction — the room keeps working while the
        // table is carried in.
        PlayerCommand::UpgradeFurnishing { building, slot } => {
            let slot = *slot as usize;
            let Some(i) = state.buildings.iter().position(|b| b.id == *building) else {
                return;
            };
            let Some((next, cost)) = state.buildings[i].next_furnishing_step(slot) else {
                return;
            };
            if state.buildings[i].under_construction() {
                return;
            }
            if state.stock.wood < cost {
                return;
            }
            state.stock.wood -= cost;
            if state.central {
                if let Some(acc) = state.buildings[i].owner_account {
                    state.credit_ledger(acc, |t| t.wood_spent += cost);
                }
            }
            let kind = state.buildings[i].kind;
            let b = &mut state.buildings[i];
            // Grow the vec lazily to exactly the slot count — a building
            // migrated from before interiors arrives with an empty one.
            let slots = kind.furnishings().len();
            if b.furnishings.len() < slots {
                b.furnishings.resize(slots, 0);
            }
            b.furnishings[slot] = next;
            let fitting = kind.furnishings()[slot];
            let name = state.player(player).map(|p| p.name.clone());
            if let Some(name) = name {
                push_action_event(
                    state,
                    format!("{} fitted a {} in the {}.", name, fitting.name(), kind.name()),
                );
            }
        }
        PlayerCommand::EnactLaw { law } => super::lawbook::enact(state, *law),
        PlayerCommand::RepealLaw { law } => super::lawbook::repeal(state, *law),
    }
}

/// A single named survivor's production share at the building they're
/// assigned to, in the same "1.0 = one anonymous worker" units `AdjustWorkers`
/// headcount uses. Composes the profession-match bonus with the XP/level
/// bonus (see `xp_level` / `XP_LEVEL_BONUS_PER_LEVEL`) — both are per-survivor
/// multipliers layered on top of the flat baseline of 1.0.
///
/// The current leader (`leader`, i.e. `GameState.leader`) always gets the
/// profession-match bonus regardless of their own trade — leading makes them
/// a generalist for as long as they hold the seat, not a permanent change to
/// `Survivor::profession` (which never changes after spawn); appoint someone
/// else, or let the seat go empty, and they're back to their own trade's
/// normal match/mismatch behavior next tick.
pub(crate) fn survivor_contribution(s: &Survivor, kind: BuildingKind, leader: Option<u32>) -> f32 {
    let is_match = leader == Some(s.id) || s.profession.matches_building(kind);
    let profession_factor = if is_match {
        PROFESSION_MATCH_BONUS
    } else if kind.requires_specialist() {
        // V0.11: a specialized trade (currently just Medic/Hospital) isn't
        // picked up informally the way general labor is — a mismatched
        // survivor here produces almost nothing instead of the ordinary
        // 1.0x baseline every other mismatch keeps. The leader exemption
        // above still applies first (a leader stays a generalist).
        SKILLED_MISMATCH_PENALTY
    } else {
        1.0
    };
    let level_factor = if s.trained_kind == Some(kind) {
        1.0 + xp_level(s.xp) as f32 * XP_LEVEL_BONUS_PER_LEVEL
    } else {
        1.0
    };
    // V0.17: condition factors — tiredness scales the share down past
    // `FATIGUE_TIRED`, illness cuts it hard (a sick survivor stays at their
    // post but gets little done). Both are 1.0 for a rested, healthy
    // survivor, so every pre-V0.17 balance expectation is unchanged on a
    // colony that sleeps and stays well.
    // V0.18: age joins the same per-survivor condition band — a child
    // contributes nothing (they still eat and still need a bunk), an elder
    // contributes less. 1.0 for every adult, so pre-V0.18 balance holds for
    // any colony of working-age people.
    let condition_factor = s.fatigue_factor()
        * if s.is_sick() { SICK_WORK_FACTOR } else { 1.0 }
        * s.age_work_factor();
    profession_factor * level_factor * condition_factor
}
// `xp_level` endi `types`da yashaydi (klient ko'rinish-darajalari ham xuddi
// shu funksiyani ishlatadi) — bu yerdagi chaqiruvlar `use crate::types::*`
// orqali o'sha yagona nusxaga boradi.

/// V0.8 test/vosita yordamchisi: barcha qurilish maydonchalarini bir zumda
/// bitiradi — tickdagi tugash mantig'i kabi ortiqcha ustalarni (sig'imdan
/// oshgan nomlangan biriktirmalar bilan birga) bo'shatadi, lekin voqea
/// yozmaydi. Sim o'zi hech qachon chaqirmaydi; bitgan binoning EFFEKTINI
/// sinaydigan testlar qurilish bosqichini shu bilan o'tkazib yuboradi.
pub fn finish_all_construction(state: &mut GameState) {
    let mut finished: Vec<(u32, u8, BuildingKind)> = Vec::new();
    for b in state.buildings.iter_mut() {
        if b.build_left > 0.0 {
            b.build_left = 0.0;
            b.workers = b.workers.min(b.kind.max_workers());
            finished.push((b.id, b.kind.max_workers(), b.kind));
        }
    }
    for (id, max, kind) in finished {
        let mut named = 0u8;
        for s in state.survivors.iter_mut() {
            if s.assigned_building == Some(id) {
                named += 1;
                if named > max {
                    s.assigned_building = None;
                }
            }
        }
        // Mirrors the tick.rs completion arm's two top-level fields (minus
        // the event, matching this helper's existing no-event-logging
        // contract) so a test that skips construction via this shortcut
        // sees the same lit/level state a real tick would have produced —
        // but only for the genuine first ignition. A V0.9 level upgrade
        // (`furnace_level` already > 0) must NOT reset the player's chosen
        // burn intensity back down to 1.
        if kind == BuildingKind::Furnace && state.furnace_level == 0 {
            state.furnace_lit = true;
            state.furnace_level = 1;
        }
        // V0.21: a finished building is not an OPERATIONAL one — production
        // comes from the workbench inside it (`FurnishingKind::cycle`), and a
        // bare room yields nothing. This helper exists so a test that cares
        // about a working building's effect can skip the parts it isn't
        // testing, so it fits the first workbench too (free, like the
        // construction it also skips). Tests that care about the interior
        // itself drive `UpgradeFurnishing` and see the real costs.
        if let Some(slot) = kind.furnishings().iter().position(|f| *f == FurnishingKind::Workbench) {
            if let Some(b) = state.buildings.iter_mut().find(|b| b.id == id) {
                if b.furnishings.len() < kind.furnishings().len() {
                    b.furnishings.resize(kind.furnishings().len(), 0);
                }
                if b.furnishings[slot] == 0 {
                    b.furnishings[slot] = 1;
                }
            }
        }
    }
}
