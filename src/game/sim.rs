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
        guest_perm: GuestPermission::Build,
        owner_id: None,
        next_id,
        rng: rng.0,
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
    // Ownership is bound to the FIRST player ever to join, tracked in
    // `owner_id` — NOT the momentary roster size. If the owner is mid-reconnect
    // (roster briefly empty), a fresh joiner must not be able to seize the world.
    let role = if state.owner_id.is_none_or(|o| o == id) {
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
        name: name.to_string(),
        color,
        cursor: None,
        built: 0,
        demolished: 0,
        role,
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

pub fn set_cursor(state: &mut GameState, id: u64, x: f32, y: f32) {
    if let Some(p) = state.players.iter_mut().find(|p| p.id == id) {
        p.cursor = Some((x, y));
    }
}

/// Append a chat line from `player_id`, sanitizing and length-capping the text.
/// Silently dropped if the player isn't connected or the text is empty after sanitizing.
pub fn push_chat(state: &mut GameState, player_id: u64, text: &str) {
    let Some(p) = state.player(player_id) else {
        return;
    };
    let name = p.name.clone();
    let color = p.color;

    // Strip control chars AND invisible format/bidi characters: a bare
    // `is_control()` misses U+202E (RIGHT-TO-LEFT OVERRIDE), zero-width joiners
    // and friends, which would let a chat line reorder or hide text for every
    // viewer on the shared server.
    let is_bad = |c: char| {
        c.is_control()
            || matches!(c,
                '\u{200B}'..='\u{200F}'   // zero-width space/joiners, LRM/RLM
                | '\u{202A}'..='\u{202E}' // bidi embeddings & overrides
                | '\u{2060}'..='\u{2069}' // word-joiner, bidi isolates
                | '\u{061C}'              // arabic letter mark
                | '\u{FEFF}'              // zero-width no-break space (BOM)
            )
    };
    // Cap stacked combining marks ("zalgo" text): a handful of accents on one
    // base character is normal typing/IME behavior, but dozens turn a single
    // line into multi-row visual noise for every viewer on the shared server.
    let is_combining = |c: char| {
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
    };
    // Order matters: strip bad chars, THEN cap stacked combining marks, and
    // only THEN apply the length budget — so a zalgo-heavy prefix can't eat the
    // whole MAX_CHAT_LEN budget and silently discard legitimate trailing text.
    let sanitized: String = text
        .trim()
        .chars()
        .filter(|c| !is_bad(*c))
        .scan(0u32, |run, c| {
            *run = if is_combining(c) { *run + 1 } else { 0 };
            Some((c, *run))
        })
        .filter(|(_, run)| *run <= 2)
        .map(|(c, _)| c)
        .take(MAX_CHAT_LEN)
        .collect();
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
/// Silently ignored if the player isn't connected or the game is over (a frozen
/// world never advances its tick, so a post-game ping could never expire).
pub fn add_ping(state: &mut GameState, player_id: u64, x: f32, y: f32) {
    if state.phase != GamePhase::Running {
        return;
    }
    let Some(p) = state.player(player_id) else {
        return;
    };
    let color = p.color;

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
                state.stock.wood -= kind.cost_wood() as f32;
                let id = state.next_id;
                state.next_id += 1;
                state.buildings.push(Building {
                    id,
                    kind: *kind,
                    x: *x,
                    y: *y,
                    workers: 0,
                    progress: 0.0,
                    owner: Some(player),
                });
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
            if let Some(b) = state.buildings.iter_mut().find(|b| b.id == *building) {
                let max = b.kind.max_workers() as i32;
                let cur = b.workers as i32;
                let target = (cur + *delta as i32).clamp(0, max);
                let new = if target > cur {
                    cur + (target - cur).min(idle)
                } else {
                    target
                };
                b.workers = new as u8;
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
                        push_event(state, "The Tunnel breaks through - the Global World awaits. Victory!");
                    } else {
                        push_event(state, format!("Tunnel stage {}/{} excavated.", state.tunnel.stage, TUNNEL_STAGES));
                    }
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
    state.tick += 1;

    // --- Expire stale map pings ---
    let tick = state.tick;
    state
        .pings
        .retain(|p| tick.saturating_sub(p.tick) < PING_TTL_TICKS);

    // --- Midnight: day rollover ---
    if state.tick % TICKS_PER_DAY == 0 {
        let day = state.day();
        if day > state.win_days {
            state.phase = GamePhase::Won;
            state.pings.clear();
            push_event(
                state,
                format!("The city has survived {} days. Victory!", state.win_days),
            );
            state.rng = rng.0;
            return;
        }
        state.cold_snap = day >= 3 && rng.chance(0.30);
        if state.cold_snap {
            push_event(state, "Forecast: a brutal cold snap will strike tonight!");
        }
    }

    // --- Furnace fuel ---
    if state.furnace_level > 0 {
        let need_coal =
            state.furnace_level as f32 * FURNACE_COAL_PER_DAY_PER_LEVEL / TICKS_PER_DAY as f32;
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
    for i in 0..state.buildings.len() {
        let (kind, bx, by, workers) = {
            let b = &state.buildings[i];
            (b.kind, b.x, b.y, b.workers)
        };
        if workers == 0 {
            continue;
        }
        let per_day = kind.production_per_worker_day();
        if per_day == 0.0 {
            continue;
        }
        let amount = workers as f32 * per_day / TICKS_PER_DAY as f32;
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
    let care_per_tick = hospital_workers as f32 * HOSPITAL_CARE_PER_WORKER_DAY / TICKS_PER_DAY as f32;
    let portion = FOOD_PER_SURVIVOR_DAY / TICKS_PER_DAY as f32
        * if kitchen_staffed { KITCHEN_FOOD_EFFICIENCY } else { 1.0 };
    let mut deaths: Vec<(String, bool)> = Vec::new();

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
        };
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
        if s.hp <= 0.0 {
            deaths.push((s.name.clone(), s.hunger >= 80.0));
        }
    }

    if !deaths.is_empty() {
        state.survivors.retain(|s| s.hp > 0.0);
        for (name, starved) in deaths {
            let cause = if starved { "starved" } else { "froze to death" };
            push_event(state, format!("{} has {}.", name, cause));
        }
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
