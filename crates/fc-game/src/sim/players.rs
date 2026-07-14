use crate::types::*;
use super::*;

pub fn player_joined(state: &mut GameState, id: u64, name: &str) {
    player_joined_as(state, id, name, None);
}

/// Like [`player_joined`], but records which account the connection signed in
/// with — in the central world that's what links a player to the settlers
/// they own.
pub fn player_joined_as(state: &mut GameState, id: u64, name: &str, account: Option<i64>) {
    // A joining player's display name is just as untrusted as a chat line —
    // it's stored and then rendered verbatim in the roster, in chat
    // attribution ("<name> built a Tent"), and in the event log, so it must
    // go through the same sanitizer before it's kept.
    let name = sanitize_name(name);

    // Ownership is bound to the FIRST player ever to join, tracked in
    // `owner_id` — NOT the momentary roster size. If the owner is mid-reconnect
    // (roster briefly empty), a fresh joiner must not be able to seize the world.
    // The central world is the exception: it belongs to no one, so nobody ever
    // gets the Owner role (which would grant kick/policy power over every
    // account on the shared map) — authority there follows settler ownership,
    // see `GameState::can_issue`.
    let role = if state.central {
        Role::Guest
    } else if state.owner_id.is_none_or(|o| o == id) {
        state.owner_id = Some(id);
        Role::Owner
    } else {
        Role::Guest
    };
    // Pick the lowest palette slot not currently in use so simultaneously
    // connected players stay visually distinct (chat tint, cursor, ping color);
    // only fall back to reuse once all 8 colors are taken.
    let color = (0u8..8)
        .find(|c| !state.players.iter().any(|p| p.color == *c))
        .unwrap_or((state.players.len() % 8) as u8);
    state.players.push(PlayerInfo {
        id,
        name: name.clone(),
        color,
        cursor: None,
        built: 0,
        demolished: 0,
        role,
        account,
    });
    // Join/leave churn is cosmetic so a flood of connects can never evict a
    // genuine system event (death, weather, victory) from the capped log.
    push_action_event(state, format!("{} joined the city.", name));
}

pub fn player_left(state: &mut GameState, id: u64) {
    if let Some(p) = state.players.iter().find(|p| p.id == id) {
        let name = p.name.clone();
        push_action_event(state, format!("{} left the city.", name));
    }
    state.players.retain(|p| p.id != id);
}

/// Owner-only: change what guests in this world are allowed to do.
pub fn set_guest_permission(state: &mut GameState, perm: GuestPermission) {
    state.guest_perm = perm;
    let label = match perm {
        GuestPermission::ViewOnly => "view only",
        GuestPermission::Build => "build",
        GuestPermission::Full => "full",
    };
    push_event(state, format!("Guests can now: {}.", label));
}

/// Owner-only: remove `target` from the roster. No-op if they're not present
/// (already disconnected, or the id never joined).
pub fn kick_player(state: &mut GameState, target: u64) {
    if let Some(pos) = state.players.iter().position(|p| p.id == target) {
        let name = state.players.remove(pos).name;
        push_event(state, format!("{} was removed by the owner.", name));
    }
}

/// Remove up to `max` survivors from a personal world for migration through
/// the Tunnel, preferring idle ones so a working city keeps its staffed
/// buildings. Assigned survivors taken anyway (when idle ones run out) free
/// their building slot exactly like the death path does. Returns the removed
/// survivors; the caller (`world_manager`) hands them to the central world's
/// `inject_migrants`.
pub fn extract_migrants(state: &mut GameState, max: usize) -> Vec<Survivor> {
    // Never extract the whole population — a personal world always keeps at
    // least one survivor behind, the same invariant `sim::tick`'s empty-
    // -survivors defeat check assumes everywhere else. This matters most
    // right after a fresh (post-graduation) reset, where the sole starting
    // leader is idle and would otherwise be exactly what gets picked first.
    let max = max.min(state.survivors.len().saturating_sub(1));
    let mut picked: Vec<u32> = state
        .survivors
        .iter()
        .filter(|s| s.assigned_building.is_none())
        .map(|s| s.id)
        .take(max)
        .collect();
    if picked.len() < max {
        let more: Vec<u32> = state
            .survivors
            .iter()
            .filter(|s| s.assigned_building.is_some())
            .map(|s| s.id)
            .take(max - picked.len())
            .collect();
        picked.extend(more);
    }
    let mut out = Vec::with_capacity(picked.len());
    for id in picked {
        let Some(idx) = state.survivors.iter().position(|s| s.id == id) else {
            continue;
        };
        if let Some(b_id) = state.survivors[idx].assigned_building {
            if let Some(b) = state.buildings.iter_mut().find(|b| b.id == b_id) {
                b.workers = b.workers.saturating_sub(1);
            }
        }
        // Unlike the death path, nobody's mourned — they left for the
        // Global World alive and well — but the seat still needs to be
        // vacated so `leader`/`leader_alive` never dangle on a removed id.
        if state.leader == Some(id) {
            state.leader = None;
            push_event(state, "The leader left through the Tunnel; the city has no leader.");
        }
        let mut s = state.survivors.remove(idx);
        s.assigned_building = None;
        out.push(s);
    }
    if !out.is_empty() {
        let plural = if out.len() == 1 { "" } else { "s" };
        push_event(
            state,
            format!("{} settler{} left through the Tunnel.", out.len(), plural),
        );
    }
    out
}

/// Add migrated survivors to the central world as `account`'s settlers.
/// Re-ids each one from this world's own counter (ids are only unique within
/// the world they came from — two personal worlds both have a survivor 1) and
/// enforces the per-account cap even if the caller already checked, so racing
/// entries can't stack one account past it. Returns how many actually settled.
pub fn inject_migrants(
    state: &mut GameState,
    account: i64,
    owner_name: &str,
    migrants: Vec<Survivor>,
) -> usize {
    let mut settled = 0usize;
    for mut s in migrants {
        if state.owned_settlers(account) >= CENTRAL_MIGRANTS_PER_ACCOUNT {
            break;
        }
        s.id = state.next_id;
        state.next_id += 1;
        s.assigned_building = None;
        s.owner = Some(account);
        // Re-id'd, so re-spawn near the arriving world's furnace too — the
        // old position was relative to the personal world they left.
        (s.x, s.y) = GameState::spawn_position(s.id);
        s.move_target = None;
        state.survivors.push(s);
        settled += 1;
    }
    if settled > 0 {
        let plural = if settled == 1 { "" } else { "s" };
        push_event(
            state,
            format!(
                "{} settler{} arrived through the Tunnel with {}.",
                settled, plural, owner_name
            ),
        );
    }
    settled
}

/// Restore a previously-connected player's roster entry (reconnect flow).
/// Preserves their id/name/color/stats but resets the transient cursor.
pub fn player_rejoined(state: &mut GameState, saved: PlayerInfo) {
    let name = saved.name.clone();
    state.players.push(PlayerInfo {
        cursor: None,
        ..saved
    });
    push_action_event(state, format!("{} reconnected.", name));
}

/// Set `id`'s last-known cursor position (world tile coordinates), broadcast
/// to every other client each snapshot. The value comes straight off the
/// wire from whichever client owns `id`, so it must be validated exactly
/// like any other untrusted input before it's stored: a crafted client could
/// otherwise send NaN/inf, which would then propagate to every viewer's
/// renderer. Non-finite input drops the update entirely (stale cursor beats
/// a poisoned one); finite input is clamped into map bounds.
pub fn set_cursor(state: &mut GameState, id: u64, x: f32, y: f32) {
    if !x.is_finite() || !y.is_finite() {
        return;
    }
    let x = x.clamp(0.0, MAP_W as f32);
    let y = y.clamp(0.0, MAP_H as f32);
    if let Some(p) = state.players.iter_mut().find(|p| p.id == id) {
        p.cursor = Some((x, y));
    }
}
