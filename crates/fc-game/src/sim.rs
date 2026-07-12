//! Pure, deterministic simulation. No Bevy, no I/O — the server thread calls
//! `tick` / `apply_command`, tests call them directly.

use super::rng::Rng;
use super::types::*;

const NAMES: [&str; 24] = [
    "Anna", "Boris", "Clara", "Dmitri", "Edda", "Finn", "Greta", "Henrik", "Ilya", "Jonas",
    "Katya", "Lena", "Mikkel", "Nadia", "Oskar", "Petra", "Ravi", "Sonja", "Tomas", "Ulla",
    "Viktor", "Wanda", "Yuri", "Zoya",
];

const START_SURVIVORS: usize = 8;
const START_WOOD: f32 = 60.0;
const START_COAL: f32 = 40.0;
const START_FOOD: f32 = 25.0;
/// Sawmills harvest forest tiles within this Chebyshev radius.
pub const SAWMILL_RADIUS: i32 = 4;

pub fn new_game(seed: u64, win_days: u32) -> GameState {
    let mut rng = Rng::new(seed);
    let mut tiles = vec![
        Tile { terrain: Terrain::Snow, deposit: 0 };
        MAP_W * MAP_H
    ];

    // Forest blobs.
    for _ in 0..14 {
        if let Some((cx, cy)) = pick_blob_center(&mut rng, 9) {
            let steps = 25 + rng.below(21);
            blob_walk(&mut tiles, &mut rng, cx, cy, steps, Terrain::Forest, 40, 80, 6);
        }
    }
    // Coal deposits.
    for _ in 0..6 {
        if let Some((cx, cy)) = pick_blob_center(&mut rng, 12) {
            let steps = 8 + rng.below(9);
            blob_walk(&mut tiles, &mut rng, cx, cy, steps, Terrain::Coal, 250, 500, 8);
        }
    }
    // Scattered lone trees.
    for _ in 0..80 {
        let x = rng.below(MAP_W as u32) as u8;
        let y = rng.below(MAP_H as u32) as u8;
        let idx = tile_index(x, y);
        if tiles[idx].terrain == Terrain::Snow && GameState::dist_to_furnace(x, y) >= 6.0 {
            tiles[idx] = Tile {
                terrain: Terrain::Forest,
                deposit: 25 + rng.below(26) as u16,
            };
        }
    }

    let mut next_id: u32 = 1;
    let mut survivors = Vec::new();
    for _ in 0..START_SURVIVORS {
        survivors.push(new_survivor(&mut rng, &mut next_id));
    }

    let furnace = Building {
        id: 0,
        kind: BuildingKind::Furnace,
        x: FURNACE_X,
        y: FURNACE_Y,
        workers: 0,
        progress: 0.0,
        owner: None,
        owner_account: None,
    };

    let mut state = GameState {
        // Start mid-morning of day 1.
        tick: ARRIVAL_TICK,
        win_days,
        tiles,
        buildings: vec![furnace],
        survivors,
        stock: Stockpile {
            wood: START_WOOD,
            coal: START_COAL,
            food: START_FOOD,
        },
        furnace_level: 1,
        furnace_lit: true,
        cold_snap: false,
        players: Vec::new(),
        phase: GamePhase::Running,
        events: Vec::new(),
        total_events: 0,
        chat: Vec::new(),
        total_chat: 0,
        pings: Vec::new(),
        missions: vec![
            Mission { kind: MissionKind::BuildTents(2),     reward_wood: 20, reward_coal: 0, reward_food: 0,  done: false },
            Mission { kind: MissionKind::Population(10),    reward_wood: 0,  reward_coal: 0, reward_food: 20, done: false },
            Mission { kind: MissionKind::Sawmills(1),       reward_wood: 20, reward_coal: 0, reward_food: 0,  done: false },
            Mission { kind: MissionKind::StockpileCoal(60), reward_wood: 20, reward_coal: 0, reward_food: 0,  done: false },
            Mission { kind: MissionKind::SurviveDays(4),    reward_wood: 0,  reward_coal: 0, reward_food: 30, done: false },
        ],
        tunnel: TunnelState::default(),
        graduated: false,
        central: false,
        techs: Vec::new(),
        disease_until: 0,
        blizzard_until: 0,
        pending_event: None,
        event_rng: Rng::new(seed.rotate_left(21) ^ 0x00E0_0DE0_1234_5678).0,
        guest_perm: GuestPermission::Build,
        owner_id: None,
        next_id,
        rng: rng.0,
        central_ledger: Vec::new(),
    };
    push_event(
        &mut state,
        format!(
            "The last furnace is lit. Survive until day {}.",
            win_days
        ),
    );
    state
}

/// The one shared central world — the Global World on the far side of the
/// Tunnel. Same procedural map, but it starts with no population (settlers
/// only ever arrive through the Tunnel, owned by their account) and no
/// personal-world progression: no missions, so the Tunnel can never unlock
/// (`all_missions_done` is false on an empty list), and `tick` skips
/// hunger/death/win/lose/events/arrivals for `central` worlds.
pub fn new_game_central(seed: u64) -> GameState {
    let mut state = new_game(seed, DEFAULT_WIN_DAYS);
    state.central = true;
    state.survivors.clear();
    state.missions.clear();
    state.events.clear();
    state.total_events = 0;
    push_event(
        &mut state,
        "The Global World. Settlers arrive through the Tunnel.",
    );
    state
}

fn pick_blob_center(rng: &mut Rng, min_dist: i32) -> Option<(u8, u8)> {
    for _ in 0..40 {
        let x = rng.below(MAP_W as u32) as u8;
        let y = rng.below(MAP_H as u32) as u8;
        if GameState::dist_to_furnace(x, y) >= min_dist as f32 {
            return Some((x, y));
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn blob_walk(
    tiles: &mut [Tile],
    rng: &mut Rng,
    cx: u8,
    cy: u8,
    steps: u32,
    terrain: Terrain,
    dep_lo: u32,
    dep_hi: u32,
    keep_clear: i32,
) {
    let mut x = cx as i32;
    let mut y = cy as i32;
    for _ in 0..steps {
        if in_bounds(x, y) {
            let (ux, uy) = (x as u8, y as u8);
            let idx = tile_index(ux, uy);
            if tiles[idx].terrain == Terrain::Snow
                && GameState::dist_to_furnace(ux, uy) >= keep_clear as f32
            {
                tiles[idx] = Tile {
                    terrain,
                    deposit: (dep_lo + rng.below(dep_hi - dep_lo + 1)) as u16,
                };
            }
        }
        x = (x + rng.range(-1, 1)).clamp(0, MAP_W as i32 - 1);
        y = (y + rng.range(-1, 1)).clamp(0, MAP_H as i32 - 1);
    }
}

fn new_survivor(rng: &mut Rng, next_id: &mut u32) -> Survivor {
    let id = *next_id;
    *next_id += 1;
    Survivor {
        id,
        name: NAMES[rng.below(NAMES.len() as u32) as usize].to_string(),
        hp: 85.0 + rng.below(16) as f32,
        hunger: 20.0 + rng.below(21) as f32,
        assigned_building: None,
        owner: None,
    }
}

/// Shared push path for both event kinds: appends the event, bumps the
/// monotonic counter, then evicts if the capped log overflowed.
fn push_event_inner(state: &mut GameState, text: String, system: bool) {
    let day = state.day();
    state.events.push(GameEvent { day, text, system });
    state.total_events += 1;
    if state.events.len() > 12 {
        // Prefer evicting the oldest cosmetic (non-system) event first, so a
        // burst of "built a Tent" spam can't push a death or victory line out
        // of the log; only fall back to the oldest overall once every event
        // in the log is a system event.
        let evict = state
            .events
            .iter()
            .position(|e| !e.system)
            .unwrap_or(0);
        state.events.remove(evict);
    }
}

/// Push a world/system event (deaths, weather, arrivals, joins, victory,
/// defeat) — protected from eviction by cosmetic player-action spam.
pub fn push_event(state: &mut GameState, text: impl Into<String>) {
    push_event_inner(state, text.into(), true);
}

/// Push a cosmetic player-action event (build/demolish attribution) — the
/// first to be evicted once the capped log overflows.
pub fn push_action_event(state: &mut GameState, text: impl Into<String>) {
    push_event_inner(state, text.into(), false);
}

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

/// Characters that must never survive into text stored and broadcast to every
/// client (chat messages, display names): a bare `is_control()` misses
/// U+202E (RIGHT-TO-LEFT OVERRIDE), zero-width joiners and friends, which
/// would let a line reorder or hide text for every viewer on the shared
/// server.
fn is_unsafe_char(c: char) -> bool {
    c.is_control()
        || matches!(c,
            '\u{200B}'..='\u{200F}'   // zero-width space/joiners, LRM/RLM
            | '\u{202A}'..='\u{202E}' // bidi embeddings & overrides
            | '\u{2060}'..='\u{2069}' // word-joiner, bidi isolates
            | '\u{061C}'              // arabic letter mark
            | '\u{FEFF}'              // zero-width no-break space (BOM)
        )
}

/// Combining marks to cap when stacked ("zalgo" text): a handful of accents
/// on one base character is normal typing/IME behavior, but dozens turn a
/// single line into multi-row visual noise for every viewer on the shared
/// server.
fn is_combining_mark(c: char) -> bool {
    matches!(c,
        '\u{0300}'..='\u{036F}'   // combining diacritical marks
        | '\u{0483}'..='\u{0489}' // Cyrillic combining
        | '\u{0591}'..='\u{05C7}' // Hebrew points
        | '\u{0610}'..='\u{061A}' // Arabic
        | '\u{064B}'..='\u{065F}' // Arabic tashkil
        | '\u{06D6}'..='\u{06ED}' // Arabic
        | '\u{0900}'..='\u{0903}' // Devanagari
        | '\u{093A}'..='\u{094F}' // Devanagari matras
        | '\u{0951}'..='\u{0957}' // Devanagari
        | '\u{0E31}'..='\u{0E3A}' // Thai vowels/tones
        | '\u{0E47}'..='\u{0E4E}' // Thai tones
        | '\u{1AB0}'..='\u{1AFF}' // combining diacritical marks extended
        | '\u{1DC0}'..='\u{1DFF}' // combining diacritical marks supplement
        | '\u{20D0}'..='\u{20FF}' // combining diacritical marks for symbols
        | '\u{FE20}'..='\u{FE2F}' // combining half marks
    )
}

/// Shared sanitizer for any untrusted, free-form text a player supplies that
/// ends up stored and broadcast verbatim to every other client (chat lines,
/// display names). Strips control/invisible-format/bidi characters, THEN
/// caps stacked combining marks, and only THEN applies `max_len` — in that
/// order, so a zalgo-heavy prefix can't eat the whole length budget and
/// silently discard legitimate trailing text.
fn sanitize_text(text: &str, max_len: usize) -> String {
    text.trim()
        .chars()
        .filter(|c| !is_unsafe_char(*c))
        .scan(0u32, |run, c| {
            *run = if is_combining_mark(c) { *run + 1 } else { 0 };
            Some((c, *run))
        })
        .filter(|(_, run)| *run <= 2)
        .map(|(c, _)| c)
        .take(max_len)
        .collect()
}

/// Sanitize an incoming player display name with the same rules as chat text
/// (see [`sanitize_text`]), capped to [`MAX_NAME_LEN`]. Falls back to a safe
/// placeholder if nothing legible survives (e.g. a name made entirely of
/// bidi overrides or zero-width characters).
fn sanitize_name(name: &str) -> String {
    let cleaned = sanitize_text(name, MAX_NAME_LEN);
    if cleaned.trim().is_empty() {
        "Player".to_string()
    } else {
        cleaned
    }
}

/// The chat sanitizer, exposed for text that is broadcast but never stored in
/// the world snapshot (nearby-chat bubbles, `ServerMsg::Bubble`): identical
/// rules to persistent chat, so a bubble can't smuggle in what a chat line
/// can't.
pub fn sanitize_public_text(text: &str) -> String {
    sanitize_text(text, MAX_CHAT_LEN)
}

/// Append a chat line from `player_id`, sanitizing and length-capping the text.
/// Silently dropped if the player isn't connected or the text is empty after sanitizing.
pub fn push_chat(state: &mut GameState, player_id: u64, text: &str) {
    let Some(p) = state.player(player_id) else {
        return;
    };
    let name = p.name.clone();
    let color = p.color;

    let sanitized = sanitize_text(text, MAX_CHAT_LEN);
    if sanitized.trim().is_empty() {
        return;
    }

    state.chat.push(ChatLine {
        player_id,
        name,
        color,
        text: sanitized,
    });
    while state.chat.len() > MAX_CHAT {
        state.chat.remove(0);
    }
    state.total_chat += 1;
}

/// Drop a transient map ping from `player_id` at world tile coordinates `(x, y)`.
/// Silently ignored if the player isn't connected, the game is over (a frozen
/// world never advances its tick, so a post-game ping could never expire), or
/// the coordinates aren't finite (a crafted client sending NaN/inf, which
/// would otherwise be stored and broadcast to every viewer's renderer).
/// Finite coordinates are clamped into map bounds.
pub fn add_ping(state: &mut GameState, player_id: u64, x: f32, y: f32) {
    if state.phase != GamePhase::Running {
        return;
    }
    if !x.is_finite() || !y.is_finite() {
        return;
    }
    let Some(p) = state.player(player_id) else {
        return;
    };
    let color = p.color;
    let x = x.clamp(0.0, MAP_W as f32);
    let y = y.clamp(0.0, MAP_H as f32);

    state.pings.push(Ping {
        player_id,
        color,
        x,
        y,
        tick: state.tick,
    });
    // Per-player cap first (evict this player's own oldest), then the global cap.
    while state.pings.iter().filter(|q| q.player_id == player_id).count() > MAX_PINGS_PER_PLAYER {
        if let Some(pos) = state.pings.iter().position(|q| q.player_id == player_id) {
            state.pings.remove(pos);
        } else {
            break;
        }
    }
    while state.pings.len() > MAX_PINGS {
        state.pings.remove(0);
    }
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
        PlayerCommand::Place { kind, x, y } => {
            if state.can_place(*kind, *x, *y).is_ok() {
                let warehouse_staffed = state
                    .buildings
                    .iter()
                    .any(|b| b.kind == BuildingKind::Warehouse && b.workers > 0);
                let discount = if warehouse_staffed {
                    WAREHOUSE_BUILD_DISCOUNT
                } else {
                    1.0
                };
                let spent_wood = kind.cost_wood() as f32 * discount;
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
                    workers: 0,
                    progress: 0.0,
                    owner: Some(player),
                    owner_account,
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
                    push_action_event(state, format!("{} built a {}.", name, kind.name()));
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
                let max = b.kind.max_workers() as i32;
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
                    if target.kind.max_workers() == 0 || prev == Some(*new_id) {
                        return;
                    }
                    if target.workers >= target.kind.max_workers() {
                        return;
                    }
                    if let Some(prev_id) = prev {
                        if let Some(b) = state.buildings.iter_mut().find(|b| b.id == prev_id) {
                            b.workers = b.workers.saturating_sub(1);
                        }
                    }
                    if let Some(b) = state.buildings.iter_mut().find(|b| b.id == *new_id) {
                        b.workers += 1;
                    }
                    state.survivors[s_idx].assigned_building = Some(*new_id);
                }
                None => {
                    let Some(prev_id) = prev else { return };
                    if let Some(b) = state.buildings.iter_mut().find(|b| b.id == prev_id) {
                        b.workers = b.workers.saturating_sub(1);
                    }
                    state.survivors[s_idx].assigned_building = None;
                }
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
    }
}

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

    // --- Production ---
    let tools_factor = if state.has_tech(Tech::Tools) {
        TECH_TOOLS_PRODUCTION
    } else {
        1.0
    };
    for i in 0..state.buildings.len() {
        let (kind, bx, by, workers, owner_account) = {
            let b = &state.buildings[i];
            (b.kind, b.x, b.y, b.workers, b.owner_account)
        };
        if workers == 0 {
            continue;
        }
        let per_day = kind.production_per_worker_day();
        if per_day == 0.0 {
            continue;
        }
        let amount = workers as f32 * per_day / TICKS_PER_DAY as f32 * tools_factor;
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
        if b.kind == BuildingKind::Tent {
            if lit && GameState::dist_to_furnace(b.x, b.y) <= radius {
                warm_slots += TENT_CAPACITY;
            } else {
                shelter_slots += TENT_CAPACITY;
            }
        }
    }
    let tph = TICKS_PER_DAY as f32 / 24.0; // ticks per in-game hour
    let hunger_per_tick = 100.0 / TICKS_PER_DAY as f32;
    let hospital_workers: u32 = state.buildings.iter()
        .filter(|b| b.kind == BuildingKind::Hospital).map(|b| b.workers as u32).sum();
    let kitchen_staffed = state.buildings.iter()
        .any(|b| b.kind == BuildingKind::Kitchen && b.workers > 0);
    let medicine_factor = if state.has_tech(Tech::Medicine) {
        TECH_MEDICINE_CARE
    } else {
        1.0
    };
    let care_per_tick = hospital_workers as f32 * HOSPITAL_CARE_PER_WORKER_DAY
        / TICKS_PER_DAY as f32
        * medicine_factor;
    let rationing_factor = if state.has_tech(Tech::Rationing) {
        TECH_RATIONING_FOOD
    } else {
        1.0
    };
    let portion = FOOD_PER_SURVIVOR_DAY / TICKS_PER_DAY as f32
        * if kitchen_staffed { KITCHEN_FOOD_EFFICIENCY } else { 1.0 }
        * rationing_factor;
    let insulation_bonus = if state.has_tech(Tech::Insulation) {
        TECH_INSULATION_WARMTH
    } else {
        0.0
    };
    let mut deaths: Vec<(u32, String, &'static str)> = Vec::new();
    let disease = state.disease_active();

    // Central-world settlers are a presence, not mouths to feed: they never
    // hunger, freeze, sicken or die (their owner may be offline for weeks —
    // returning to a starved-out group would make migration pointless).
    // Everything above (production, furnace) still runs so an assigned
    // settler keeps contributing to the communal stock.
    if state.central {
        state.rng = rng.0;
        state.event_rng = erng.0;
        return;
    }

    for (i, s) in state.survivors.iter_mut().enumerate() {
        s.hunger = (s.hunger + hunger_per_tick).min(120.0);
        if s.hunger >= 25.0 && state.stock.food >= portion {
            state.stock.food -= portion;
            s.hunger = (s.hunger - 0.4).max(0.0);
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
        state.survivors.retain(|s| s.hp > 0.0);
        for (_, name, cause) in deaths {
            push_event(state, format!("{} has {}.", name, cause));
        }
        // Still needed as a fallback for anonymous-only overflow (shouldn't
        // normally trigger now that every named slot above is freed first).
        clamp_workers(state);
    }

    // --- Morning arrivals ---
    if state.tick % TICKS_PER_DAY == ARRIVAL_TICK && state.day() >= 2 && rng.chance(0.55) {
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

    // --- Defeat ---
    if state.survivors.is_empty() {
        state.phase = GamePhase::Lost;
        state.pings.clear();
        push_event(state, "The last survivor has perished. The city falls silent.");
    }

    state.rng = rng.0;
    state.event_rng = erng.0;
}

/// Remove one unit of wood from the nearest forest tile within `r` of (cx, cy).
fn take_forest_unit(tiles: &mut [Tile], cx: u8, cy: u8, r: i32) -> bool {
    let mut best: Option<(i32, usize)> = None;
    for dy in -r..=r {
        for dx in -r..=r {
            let (x, y) = (cx as i32 + dx, cy as i32 + dy);
            if !in_bounds(x, y) {
                continue;
            }
            let idx = tile_index(x as u8, y as u8);
            let t = &tiles[idx];
            if t.terrain == Terrain::Forest && t.deposit > 0 {
                let d = dx.abs().max(dy.abs());
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, idx));
                }
            }
        }
    }
    if let Some((_, idx)) = best {
        tiles[idx].deposit -= 1;
        if tiles[idx].deposit == 0 {
            tiles[idx].terrain = Terrain::Snow;
        }
        true
    } else {
        false
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
