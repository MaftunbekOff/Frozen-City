//! Frozen mirrors of every pre-current save format, so worlds saved by older
//! binaries keep loading after `Building`/`Survivor`/`PlayerInfo`/`GameState`
//! grew new fields. Bincode is positional — any field added to the live types
//! makes old bytes undecodable as them — so `persist::load_at` falls back to
//! decoding the right mirror (by magic header) and migrating forward,
//! V1 -> V2 -> V3 -> V4 (the live format), one `From` hop at a time.
//!
//! These structs must never change again: they ARE their on-disk layout.
//! Types shared unchanged across versions (Tile, Mission, ...) are reused
//! from `types` directly; only the ones that grew fields between versions are
//! mirrored. Serialize is derived solely so tests can fabricate old bytes.

use serde::{Deserialize, Serialize};

use fc_game::types::{
    Building, CaravanOffer, ChatLine, GameEvent, GamePhase, GameState, GuestPermission,
    LedgerEntry, Mission, Ping, PlayerInfo, Profession, Role, Stockpile, Survivor, Tech, Tile,
    TunnelState,
};

#[derive(Serialize, Deserialize)]
pub struct SurvivorV1 {
    pub id: u32,
    pub name: String,
    pub hp: f32,
    pub hunger: f32,
    pub assigned_building: Option<u32>,
}

/// `Survivor`'s shape in V2 AND V3 (unchanged between them — it only grew
/// position/movement/profession/xp fields in V4). Reused by `GameStateV2`
/// and `GameStateV3` below instead of duplicating the struct.
#[derive(Serialize, Deserialize, Clone)]
pub struct SurvivorV3 {
    pub id: u32,
    pub name: String,
    pub hp: f32,
    pub hunger: f32,
    pub assigned_building: Option<u32>,
    pub owner: Option<i64>,
}

/// Convenience for tests fabricating legacy bytes from a live `GameState`
/// (which no longer has a `Survivor` shape assignable field-for-field to
/// `SurvivorV3` now that V4 added new ones) — drops exactly the fields V3
/// predates.
impl From<&Survivor> for SurvivorV3 {
    fn from(s: &Survivor) -> SurvivorV3 {
        SurvivorV3 {
            id: s.id,
            name: s.name.clone(),
            hp: s.hp,
            hunger: s.hunger,
            assigned_building: s.assigned_building,
            owner: s.owner,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct PlayerInfoV1 {
    pub id: u64,
    pub name: String,
    pub color: u8,
    pub cursor: Option<(f32, f32)>,
    pub built: u32,
    pub demolished: u32,
    pub role: Role,
}

/// `Building`'s shape in both V1 and V2 (unchanged between them — it only
/// grew `owner_account` in V3). Reused by both `GameStateV1` and
/// `GameStateV2` below instead of duplicating the struct.
#[derive(Serialize, Deserialize, Clone)]
pub struct BuildingV2 {
    pub id: u32,
    pub kind: fc_game::types::BuildingKind,
    pub x: u8,
    pub y: u8,
    pub workers: u8,
    pub progress: f32,
    pub owner: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub struct GameStateV1 {
    pub tick: u64,
    pub win_days: u32,
    pub tiles: Vec<Tile>,
    pub buildings: Vec<BuildingV2>,
    pub survivors: Vec<SurvivorV1>,
    pub stock: Stockpile,
    pub furnace_level: u8,
    pub furnace_lit: bool,
    pub cold_snap: bool,
    pub players: Vec<PlayerInfoV1>,
    pub phase: GamePhase,
    pub events: Vec<GameEvent>,
    pub total_events: u64,
    pub chat: Vec<ChatLine>,
    pub total_chat: u64,
    pub pings: Vec<Ping>,
    pub missions: Vec<Mission>,
    pub tunnel: TunnelState,
    pub graduated: bool,
    pub techs: Vec<Tech>,
    pub disease_until: u64,
    pub blizzard_until: u64,
    pub pending_event: Option<CaravanOffer>,
    pub event_rng: u64,
    pub guest_perm: GuestPermission,
    pub owner_id: Option<u64>,
    pub next_id: u32,
    pub rng: u64,
}

/// V2 (post-central-world, pre-account-ownership/contribution-ledger) shape:
/// what `persist.rs` wrote under the `FCWORLD2` header before this change.
/// `PlayerInfo` is unchanged since V2 (still has `account`), so it's reused
/// from `types` directly; `Survivor` is mirrored as `SurvivorV3` (unchanged
/// itself between V2 and V3, but no longer safe to reuse `types::Survivor`
/// now that V4 gave it new fields); `Building` differs too (`owner_account`
/// didn't exist yet) and so does `GameState` itself (no `central_ledger`).
#[derive(Serialize, Deserialize)]
pub struct GameStateV2 {
    pub tick: u64,
    pub win_days: u32,
    pub tiles: Vec<Tile>,
    pub buildings: Vec<BuildingV2>,
    pub survivors: Vec<SurvivorV3>,
    pub stock: Stockpile,
    pub furnace_level: u8,
    pub furnace_lit: bool,
    pub cold_snap: bool,
    pub players: Vec<PlayerInfo>,
    pub phase: GamePhase,
    pub events: Vec<GameEvent>,
    pub total_events: u64,
    pub chat: Vec<ChatLine>,
    pub total_chat: u64,
    pub pings: Vec<Ping>,
    pub missions: Vec<Mission>,
    pub tunnel: TunnelState,
    pub graduated: bool,
    pub central: bool,
    pub techs: Vec<Tech>,
    pub disease_until: u64,
    pub blizzard_until: u64,
    pub pending_event: Option<CaravanOffer>,
    pub event_rng: u64,
    pub guest_perm: GuestPermission,
    pub owner_id: Option<u64>,
    pub next_id: u32,
    pub rng: u64,
}

impl From<GameStateV1> for GameStateV2 {
    fn from(v1: GameStateV1) -> GameStateV2 {
        GameStateV2 {
            tick: v1.tick,
            win_days: v1.win_days,
            tiles: v1.tiles,
            buildings: v1.buildings,
            survivors: v1
                .survivors
                .into_iter()
                .map(|s| SurvivorV3 {
                    id: s.id,
                    name: s.name,
                    hp: s.hp,
                    hunger: s.hunger,
                    assigned_building: s.assigned_building,
                    // V1 predates the central world, so no survivor there
                    // can be an account-owned settler.
                    owner: None,
                })
                .collect(),
            stock: v1.stock,
            furnace_level: v1.furnace_level,
            furnace_lit: v1.furnace_lit,
            cold_snap: v1.cold_snap,
            players: v1
                .players
                .into_iter()
                .map(|p| PlayerInfo {
                    id: p.id,
                    name: p.name,
                    color: p.color,
                    cursor: p.cursor,
                    built: p.built,
                    demolished: p.demolished,
                    role: p.role,
                    // Unknown at load time; set again the next time the
                    // account's connection joins.
                    account: None,
                })
                .collect(),
            phase: v1.phase,
            events: v1.events,
            total_events: v1.total_events,
            chat: v1.chat,
            total_chat: v1.total_chat,
            pings: v1.pings,
            missions: v1.missions,
            tunnel: v1.tunnel,
            graduated: v1.graduated,
            central: false,
            techs: v1.techs,
            disease_until: v1.disease_until,
            blizzard_until: v1.blizzard_until,
            pending_event: v1.pending_event,
            event_rng: v1.event_rng,
            guest_perm: v1.guest_perm,
            owner_id: v1.owner_id,
            next_id: v1.next_id,
            rng: v1.rng,
        }
    }
}

impl From<GameStateV2> for GameStateV3 {
    fn from(v2: GameStateV2) -> GameStateV3 {
        GameStateV3 {
            tick: v2.tick,
            win_days: v2.win_days,
            tiles: v2.tiles,
            buildings: v2
                .buildings
                .into_iter()
                .map(|b| Building {
                    id: b.id,
                    kind: b.kind,
                    x: b.x,
                    y: b.y,
                    workers: b.workers,
                    progress: b.progress,
                    owner: b.owner,
                    // V2 predates account-based central-world building
                    // ownership: every V2 building (central or not) stays
                    // demolishable exactly as it always was (session-id
                    // check), never permanently locked to nobody.
                    owner_account: None,
                })
                .collect(),
            survivors: v2.survivors,
            stock: v2.stock,
            furnace_level: v2.furnace_level,
            furnace_lit: v2.furnace_lit,
            cold_snap: v2.cold_snap,
            players: v2.players,
            phase: v2.phase,
            events: v2.events,
            total_events: v2.total_events,
            chat: v2.chat,
            total_chat: v2.total_chat,
            pings: v2.pings,
            missions: v2.missions,
            tunnel: v2.tunnel,
            graduated: v2.graduated,
            central: v2.central,
            techs: v2.techs,
            disease_until: v2.disease_until,
            blizzard_until: v2.blizzard_until,
            pending_event: v2.pending_event,
            event_rng: v2.event_rng,
            guest_perm: v2.guest_perm,
            owner_id: v2.owner_id,
            next_id: v2.next_id,
            rng: v2.rng,
            // V2 predates the central-world contribution ledger; nothing to
            // carry forward (and it's empty in every non-central world
            // anyway).
            central_ledger: Vec::new(),
        }
    }
}

/// V3 (pre-V0.7 "survivor management") shape: what `persist.rs` wrote under
/// the `FCWORLD3` header before this change — identical to the live
/// `GameState` minus positions/movement/leader/professions/xp/morale.
/// `Survivor` is mirrored as `SurvivorV3`; `Building` already has
/// `owner_account` as of V3, so it's reused from `types` directly (it's
/// `PlayerInfo` that hasn't changed since V2, not `Building`).
#[derive(Serialize, Deserialize)]
pub struct GameStateV3 {
    pub tick: u64,
    pub win_days: u32,
    pub tiles: Vec<Tile>,
    pub buildings: Vec<Building>,
    pub survivors: Vec<SurvivorV3>,
    pub stock: Stockpile,
    pub furnace_level: u8,
    pub furnace_lit: bool,
    pub cold_snap: bool,
    pub players: Vec<PlayerInfo>,
    pub phase: GamePhase,
    pub events: Vec<GameEvent>,
    pub total_events: u64,
    pub chat: Vec<ChatLine>,
    pub total_chat: u64,
    pub pings: Vec<Ping>,
    pub missions: Vec<Mission>,
    pub tunnel: TunnelState,
    pub graduated: bool,
    pub central: bool,
    pub techs: Vec<Tech>,
    pub disease_until: u64,
    pub blizzard_until: u64,
    pub pending_event: Option<CaravanOffer>,
    pub event_rng: u64,
    pub guest_perm: GuestPermission,
    pub owner_id: Option<u64>,
    pub next_id: u32,
    pub rng: u64,
    pub central_ledger: Vec<LedgerEntry>,
}

impl From<GameStateV3> for GameState {
    fn from(v3: GameStateV3) -> GameState {
        GameState {
            tick: v3.tick,
            win_days: v3.win_days,
            tiles: v3.tiles,
            // `Building` is unchanged between V3 and the live format — no
            // per-field remap needed, unlike the V2 -> V3 hop above.
            buildings: v3.buildings,
            survivors: v3
                .survivors
                .into_iter()
                .map(|s| {
                    let (x, y) = GameState::spawn_position(s.id);
                    Survivor {
                        id: s.id,
                        name: s.name,
                        hp: s.hp,
                        hunger: s.hunger,
                        assigned_building: s.assigned_building,
                        owner: s.owner,
                        // V3 predates positions/movement/professions/xp — see
                        // the brief's migration defaults: deterministic
                        // furnace-adjacent spawn, no walk target, profession
                        // derived from the id (no RNG stream survives a
                        // save/load round trip to draw a fresh one from), and
                        // a clean training slate.
                        x,
                        y,
                        move_target: None,
                        profession: Profession::from_id_hash(s.id),
                        xp: 0.0,
                        trained_kind: None,
                    }
                })
                .collect(),
            stock: v3.stock,
            furnace_level: v3.furnace_level,
            furnace_lit: v3.furnace_lit,
            cold_snap: v3.cold_snap,
            players: v3.players,
            phase: v3.phase,
            events: v3.events,
            total_events: v3.total_events,
            chat: v3.chat,
            total_chat: v3.total_chat,
            pings: v3.pings,
            missions: v3.missions,
            tunnel: v3.tunnel,
            graduated: v3.graduated,
            central: v3.central,
            techs: v3.techs,
            disease_until: v3.disease_until,
            blizzard_until: v3.blizzard_until,
            pending_event: v3.pending_event,
            event_rng: v3.event_rng,
            guest_perm: v3.guest_perm,
            owner_id: v3.owner_id,
            next_id: v3.next_id,
            rng: v3.rng,
            central_ledger: v3.central_ledger,
            // V3 predates leader/mourning/morale entirely — see the brief's
            // migration defaults: no leader, no mourning in progress, and
            // morale starts fresh at the same baseline a brand-new world
            // gets (an old save resuming under V0.7 shouldn't start with
            // depressed morale it never actually earned).
            leader: None,
            mourning_until: 0,
            morale: fc_game::types::MORALE_START,
        }
    }
}

/// V2 -> V4 straight through V3, needed by `persist::load_at`'s one-hop
/// `FCWORLD2` path (a V2 file goes all the way to the live format in a
/// single `.into()`).
impl From<GameStateV2> for GameState {
    fn from(v2: GameStateV2) -> GameState {
        GameStateV3::from(v2).into()
    }
}

/// V1 -> V4 straight through V2 -> V3, for `persist::load_at`'s one-shot
/// fallback path (a V1 file goes all the way to the live format in a single
/// `.into()`).
impl From<GameStateV1> for GameState {
    fn from(v1: GameStateV1) -> GameState {
        GameStateV2::from(v1).into()
    }
}
