use crate::rng::Rng;
use crate::types::*;
use super::*;

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
        PlayerCommand::Place { kind, x, y } => {
            if state.can_place(*kind, *x, *y).is_ok() {
                let warehouse_staffed = state
                    .buildings
                    .iter()
                    .any(|b| b.kind == BuildingKind::Warehouse && b.workers > 0 && !b.under_construction());
                let discount = if warehouse_staffed {
                    WAREHOUSE_BUILD_DISCOUNT
                } else {
                    1.0
                };
                let spent_wood = kind.cost_wood() as f32 * discount;
                // V0.8: qarzga qurilmaydi — resurs yetmasa buyruq e'tiborsiz
                // qoladi (client menyusi bu shartni oldindan ko'rsatadi).
                if state.stock.wood < spent_wood {
                    return;
                }
                // V0.8: yangi bino qurilish maydonchasi sifatida boshlanadi —
                // bo'sh ishchilardan avtomatik usta-brigada tuziladi (kam
                // bo'lsa bori bilan; hech kim bo'lmasa qurilish o'z-o'zidan
                // siljimaydi, o'yinchi keyin +/- bilan biriktiradi).
                // MARKAZIY olamda brigada tuzilmaydi: u yerda har bir aholi
                // akkauntga tegishli, anonim jalb qilish `AdjustWorkers`
                // taqiqlangani bilan bir xil sabablarga ko'ra mumkin emas —
                // egasi o'z ko'chmanchisini saytga o'zi biriktiradi.
                let crew = if state.central {
                    0
                } else {
                    (state.idle_workers()).min(CONSTRUCTION_CREW_MAX as u32) as u8
                };
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
                    workers: crew,
                    progress: 0.0,
                    owner: Some(player),
                    owner_account,
                    level: 1,
                    build_left: kind.build_workdays(),
                });
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
                .position(|b| b.id == *building && b.kind != BuildingKind::Furnace)
            {
                let b = state.buildings.remove(i);
                state.stock.wood += b.kind.cost_wood() as f32 * DEMOLISH_REFUND;
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
            let idle = state.idle_workers() as i32;
            // Named assignments (`AssignSurvivor`) are a floor this anonymous
            // +/- can't dip below — it may only drain the anonymous slack.
            let named = state
                .survivors
                .iter()
                .filter(|s| s.assigned_building == Some(*building))
                .count() as i32;
            if let Some(b) = state.buildings.iter_mut().find(|b| b.id == *building) {
                // V0.8: qurilish maydonchasi o'z sig'imi bilan ishlaydi —
                // ustalar brigada capigacha, bitgan bino esa o'z kasb
                // o'rinlarigacha oladi.
                let max = if b.under_construction() {
                    CONSTRUCTION_CREW_MAX as i32
                } else {
                    b.kind.max_workers() as i32
                };
                let cur = b.workers as i32;
                let target = (cur + *delta as i32).clamp(0, max).max(named);
                let new = if target > cur {
                    cur + (target - cur).min(idle)
                } else {
                    target
                };
                b.workers = new as u8;
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
                    if let Some(prev_id) = prev {
                        if let Some(b) = state.buildings.iter_mut().find(|b| b.id == prev_id) {
                            b.workers = b.workers.saturating_sub(1);
                        }
                    }
                    if let Some(b) = state.buildings.iter_mut().find(|b| b.id == *new_id) {
                        b.workers += 1;
                    }
                    state.survivors[s_idx].assigned_building = Some(*new_id);
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
            let s = &mut state.survivors[s_idx];
            s.assigned_building = None;
            s.move_target = Some((*x, *y));
        }
        PlayerCommand::SetLeader { survivor } => {
            if state.survivors.iter().any(|s| s.id == *survivor) {
                state.leader = Some(*survivor);
                let name = state.survivors.iter().find(|s| s.id == *survivor).unwrap().name.clone();
                push_event(state, format!("{} has been chosen as leader.", name));
            }
        }
        PlayerCommand::SetFurnaceLevel { level } => {
            state.furnace_level = (*level).min(3);
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
                // Faqat bitgan, qurilsa bo'ladigan va maksimumga yetmagan
                // bino yangilanadi; yangilashlar zanjir — keyingisi faqat
                // oldingisi bitgach.
                if !kind.buildable() || busy || level >= BUILDING_MAX_LEVEL {
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
                // bo'sh ishchilardan avtomatik to'ldiriladi (Place'dagi kabi).
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
    }
}

/// A single named survivor's production share at the building they're
/// assigned to, in the same "1.0 = one anonymous worker" units `AdjustWorkers`
/// headcount uses. Composes the profession-match bonus with the XP/level
/// bonus (see `xp_level` / `XP_LEVEL_BONUS_PER_LEVEL`) — both are per-survivor
/// multipliers layered on top of the flat baseline of 1.0.
pub(crate) fn survivor_contribution(s: &Survivor, kind: BuildingKind) -> f32 {
    let profession_factor =
        if s.profession.matching_building() == kind { PROFESSION_MATCH_BONUS } else { 1.0 };
    let level_factor = if s.trained_kind == Some(kind) {
        1.0 + xp_level(s.xp) as f32 * XP_LEVEL_BONUS_PER_LEVEL
    } else {
        1.0
    };
    profession_factor * level_factor
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
    let mut finished: Vec<(u32, u8)> = Vec::new();
    for b in state.buildings.iter_mut() {
        if b.build_left > 0.0 {
            b.build_left = 0.0;
            b.workers = b.workers.min(b.kind.max_workers());
            finished.push((b.id, b.kind.max_workers()));
        }
    }
    for (id, max) in finished {
        let mut named = 0u8;
        for s in state.survivors.iter_mut() {
            if s.assigned_building == Some(id) {
                named += 1;
                if named > max {
                    s.assigned_building = None;
                }
            }
        }
    }
}
