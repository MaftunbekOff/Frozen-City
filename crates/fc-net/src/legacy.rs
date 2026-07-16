//! Frozen mirrors of every pre-current save format, so worlds saved by older
//! binaries keep loading after `Building`/`Survivor`/`PlayerInfo`/`GameState`
//! grew new fields. Bincode is positional — any field added to the live types
//! makes old bytes undecodable as them — so `persist::load_at` falls back to
//! decoding the right mirror (by magic header) and migrating forward,
//! V1 -> V2 -> V3 -> V4 -> V5 -> V6 -> V7 -> V8 -> V9 -> V10 -> V11 -> V12
//! (the live format), one `From` hop at a time.
//!
//! These structs must never change again: they ARE their on-disk layout.
//! Types shared unchanged across versions (Tile, Mission, ...) are reused
//! from `types` directly; only the ones that grew (or, as of V12, shrank)
//! fields between versions are mirrored. Serialize is derived solely so
//! tests can fabricate old bytes.

use serde::{Deserialize, Serialize};

use fc_game::types::{
    Building, CaravanOffer, ChatLine, Corpse, COW_START, DEER_START, GameEvent, GamePhase,
    GameState, Grave, GuestPermission, LedgerEntry, Livestock, Mission, Ping, PlayerInfo,
    Profession, RABBIT_START, Role, SHEEP_START, Stockpile, Survivor, Tech, Tile, TradeCaravan,
    TunnelMigrant, TunnelState, Wildlife,
};

/// `Stockpile`'s shape through V7 — three f32s, no `fur`/`cloth` (added in V8
/// for the Tailor Shop / wildlife resource system). Bincode is positional, so
/// once the live `Stockpile` grew two fields, every legacy `GameStateVn`
/// (n = 1..=7) that used to reuse `types::Stockpile` directly needed its own
/// frozen mirror of the old 3-field shape instead.
#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct StockpileV7 {
    pub wood: f32,
    pub coal: f32,
    pub food: f32,
}

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

/// `Survivor`'s shape in V4 AND V5 (unchanged between them — it only grew
/// `chop_target`/`carrying_wood` in V6, the Furnace's chop-and-carry
/// construction cycle). Reused by `GameStateV4` and `GameStateV5` below
/// instead of duplicating the struct.
#[derive(Serialize, Deserialize, Clone)]
pub struct SurvivorV5 {
    pub id: u32,
    pub name: String,
    pub hp: f32,
    pub hunger: f32,
    pub assigned_building: Option<u32>,
    pub owner: Option<i64>,
    pub x: f32,
    pub y: f32,
    pub move_target: Option<(u8, u8)>,
    pub profession: Profession,
    pub xp: f32,
    pub trained_kind: Option<fc_game::types::BuildingKind>,
}

/// Convenience for tests fabricating legacy bytes from a live `GameState`
/// (which no longer has a `Survivor` shape assignable field-for-field to
/// `SurvivorV5` now that V6 added the chop/carry fields) — drops exactly
/// the fields V5 predates.
impl From<&Survivor> for SurvivorV5 {
    fn from(s: &Survivor) -> SurvivorV5 {
        SurvivorV5 {
            id: s.id,
            name: s.name.clone(),
            hp: s.hp,
            hunger: s.hunger,
            assigned_building: s.assigned_building,
            owner: s.owner,
            x: s.x,
            y: s.y,
            move_target: s.move_target,
            profession: s.profession,
            xp: s.xp,
            trained_kind: s.trained_kind,
        }
    }
}

/// `Survivor`'s shape through V8 (unchanged since V6, i.e. every field the
/// live `Survivor` has today except `thirst`/`bury_target`, added in V9 for
/// the death/burial and thirst systems). Reused by `GameStateV6` and
/// `GameStateV7` below instead of duplicating the struct — both predate this
/// V9 change and, now that the live `Survivor` type's shape has moved on,
/// can no longer safely reuse it directly (same reasoning as `StockpileV7`).
#[derive(Serialize, Deserialize, Clone)]
pub struct SurvivorV8 {
    pub id: u32,
    pub name: String,
    pub hp: f32,
    pub hunger: f32,
    pub assigned_building: Option<u32>,
    pub owner: Option<i64>,
    pub x: f32,
    pub y: f32,
    pub move_target: Option<(u8, u8)>,
    pub profession: Profession,
    pub xp: f32,
    pub trained_kind: Option<fc_game::types::BuildingKind>,
    pub chop_target: Option<(u8, u8)>,
    pub carrying_wood: bool,
}

/// Convenience for tests fabricating legacy bytes from a live `GameState`
/// (which now has `thirst`/`bury_target` `SurvivorV8` predates) — drops
/// exactly the fields V8 predates.
impl From<&Survivor> for SurvivorV8 {
    fn from(s: &Survivor) -> SurvivorV8 {
        SurvivorV8 {
            id: s.id,
            name: s.name.clone(),
            hp: s.hp,
            hunger: s.hunger,
            assigned_building: s.assigned_building,
            owner: s.owner,
            x: s.x,
            y: s.y,
            move_target: s.move_target,
            profession: s.profession,
            xp: s.xp,
            trained_kind: s.trained_kind,
            chop_target: s.chop_target,
            carrying_wood: s.carrying_wood,
        }
    }
}

/// `Stockpile`'s shape through V8 — five f32s, no `water` (added in V9 for
/// the thirst/Well system). Bincode is positional, so once the live
/// `Stockpile` grew a field, `GameStateV8` (which predates V9) needed its
/// own frozen mirror of the old shape instead of reusing the live type,
/// same reasoning as `StockpileV7`.
#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct StockpileV8 {
    pub wood: f32,
    pub coal: f32,
    pub food: f32,
    pub fur: f32,
    pub cloth: f32,
}

/// `Stockpile`'s shape through V10 — six f32s, no `gold` (added in V11 for
/// the Tunnel trade caravan). Reused by `GameStateV9` and `GameStateV10`
/// instead of duplicating the struct, same reasoning as `StockpileV7`/`V8`.
#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct StockpileV10 {
    pub wood: f32,
    pub coal: f32,
    pub food: f32,
    pub fur: f32,
    pub cloth: f32,
    pub water: f32,
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

/// `Building`'s shape in V3 AND V4 (unchanged between them — it only grew
/// `level`/`build_left` in V5). Reused by `GameStateV3` and `GameStateV4`
/// below instead of duplicating the struct.
#[derive(Serialize, Deserialize, Clone)]
pub struct BuildingV4 {
    pub id: u32,
    pub kind: fc_game::types::BuildingKind,
    pub x: u8,
    pub y: u8,
    pub workers: u8,
    pub progress: f32,
    pub owner: Option<u64>,
    pub owner_account: Option<i64>,
}

/// Convenience for tests fabricating legacy bytes from a live `GameState`
/// (whose `Building` now has V5's `level`/`build_left` fields) — drops
/// exactly the fields V4 predates.
impl From<&Building> for BuildingV4 {
    fn from(b: &Building) -> BuildingV4 {
        BuildingV4 {
            id: b.id,
            kind: b.kind,
            x: b.x,
            y: b.y,
            workers: b.workers,
            progress: b.progress,
            owner: b.owner,
            owner_account: b.owner_account,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct GameStateV1 {
    pub tick: u64,
    pub win_days: u32,
    pub tiles: Vec<Tile>,
    pub buildings: Vec<BuildingV2>,
    pub survivors: Vec<SurvivorV1>,
    pub stock: StockpileV7,
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
    pub stock: StockpileV7,
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
                .map(|b| BuildingV4 {
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
/// the `FCWORLD3` header before this change — identical to the V4 shape
/// minus positions/movement/leader/professions/xp/morale. `Survivor` is
/// mirrored as `SurvivorV3`, `Building` as `BuildingV4` (its shape was the
/// same in V3 and V4).
#[derive(Serialize, Deserialize)]
pub struct GameStateV3 {
    pub tick: u64,
    pub win_days: u32,
    pub tiles: Vec<Tile>,
    pub buildings: Vec<BuildingV4>,
    pub survivors: Vec<SurvivorV3>,
    pub stock: StockpileV7,
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

/// V4 (V0.7 "survivor management", pre-building-levels) shape: what
/// `persist.rs` wrote under the `FCWORLD4` header before this change —
/// identical to V5 minus `Building::level`/`build_left` (mirrored as
/// `BuildingV4`). `Survivor` is mirrored as `SurvivorV5` (unchanged itself
/// between V4 and V5, but no longer safe to reuse `types::Survivor` now
/// that V6 gave it chop/carry fields); `PlayerInfo` is unchanged since V4,
/// so it's reused from `types` directly.
#[derive(Serialize, Deserialize)]
pub struct GameStateV4 {
    pub tick: u64,
    pub win_days: u32,
    pub tiles: Vec<Tile>,
    pub buildings: Vec<BuildingV4>,
    pub survivors: Vec<SurvivorV5>,
    pub stock: StockpileV7,
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
    pub leader: Option<u32>,
    pub mourning_until: u64,
    pub morale: f32,
}

impl From<GameStateV3> for GameStateV4 {
    fn from(v3: GameStateV3) -> GameStateV4 {
        GameStateV4 {
            tick: v3.tick,
            win_days: v3.win_days,
            tiles: v3.tiles,
            // `Building` is unchanged between V3 and V4 — no remap needed.
            buildings: v3.buildings,
            survivors: v3
                .survivors
                .into_iter()
                .map(|s| {
                    let (x, y) = GameState::spawn_position(s.id);
                    SurvivorV5 {
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

/// V5 (pre-chop/carry, i.e. every field the live `GameState` has today
/// except `Survivor::chop_target`/`carrying_wood`) shape: what `persist.rs`
/// wrote under the `FCWORLD5` header before this change. `Building` is
/// unchanged since V5, so it's reused from `types` directly; `Survivor` is
/// mirrored as `SurvivorV5`.
#[derive(Serialize, Deserialize)]
pub struct GameStateV5 {
    pub tick: u64,
    pub win_days: u32,
    pub tiles: Vec<Tile>,
    pub buildings: Vec<Building>,
    pub survivors: Vec<SurvivorV5>,
    pub stock: StockpileV7,
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
    pub leader: Option<u32>,
    pub mourning_until: u64,
    pub morale: f32,
}

impl From<GameStateV4> for GameStateV5 {
    fn from(v4: GameStateV4) -> GameStateV5 {
        GameStateV5 {
            tick: v4.tick,
            win_days: v4.win_days,
            tiles: v4.tiles,
            buildings: v4
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
                    owner_account: b.owner_account,
                    // V4 predates building levels/construction: every old
                    // building loads as a FINISHED level-1 building — exactly
                    // how it behaved before the feature existed.
                    level: 1,
                    build_left: 0.0,
                })
                .collect(),
            survivors: v4.survivors,
            stock: v4.stock,
            furnace_level: v4.furnace_level,
            furnace_lit: v4.furnace_lit,
            cold_snap: v4.cold_snap,
            players: v4.players,
            phase: v4.phase,
            events: v4.events,
            total_events: v4.total_events,
            chat: v4.chat,
            total_chat: v4.total_chat,
            pings: v4.pings,
            missions: v4.missions,
            tunnel: v4.tunnel,
            graduated: v4.graduated,
            central: v4.central,
            techs: v4.techs,
            disease_until: v4.disease_until,
            blizzard_until: v4.blizzard_until,
            pending_event: v4.pending_event,
            event_rng: v4.event_rng,
            guest_perm: v4.guest_perm,
            owner_id: v4.owner_id,
            next_id: v4.next_id,
            rng: v4.rng,
            central_ledger: v4.central_ledger,
            leader: v4.leader,
            mourning_until: v4.mourning_until,
            morale: v4.morale,
        }
    }
}

/// V6 (pre-Tunnel-migrants, i.e. every field the live `GameState` has today
/// except `pending_migrant`) shape: what `persist.rs` wrote under the
/// `FCWORLD6` header before this change. `Building` is unchanged since V6,
/// so it's reused from `types` directly; `Survivor` is mirrored as
/// [`SurvivorV8`] (the live `Survivor` now has `thirst`/`bury_target`).
#[derive(Serialize, Deserialize)]
pub struct GameStateV6 {
    pub tick: u64,
    pub win_days: u32,
    pub tiles: Vec<Tile>,
    pub buildings: Vec<Building>,
    pub survivors: Vec<SurvivorV8>,
    pub stock: StockpileV7,
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
    pub leader: Option<u32>,
    pub mourning_until: u64,
    pub morale: f32,
}

impl From<GameStateV5> for GameStateV6 {
    fn from(v5: GameStateV5) -> GameStateV6 {
        GameStateV6 {
            tick: v5.tick,
            win_days: v5.win_days,
            tiles: v5.tiles,
            buildings: v5.buildings,
            survivors: v5
                .survivors
                .into_iter()
                .map(|s| SurvivorV8 {
                    id: s.id,
                    name: s.name,
                    hp: s.hp,
                    hunger: s.hunger,
                    assigned_building: s.assigned_building,
                    owner: s.owner,
                    x: s.x,
                    y: s.y,
                    move_target: s.move_target,
                    profession: s.profession,
                    xp: s.xp,
                    trained_kind: s.trained_kind,
                    // V5 predates the Furnace's chop-and-carry construction
                    // cycle — nobody mid-migration is off chopping a tree.
                    chop_target: None,
                    carrying_wood: false,
                })
                .collect(),
            stock: v5.stock,
            furnace_level: v5.furnace_level,
            furnace_lit: v5.furnace_lit,
            cold_snap: v5.cold_snap,
            players: v5.players,
            phase: v5.phase,
            events: v5.events,
            total_events: v5.total_events,
            chat: v5.chat,
            total_chat: v5.total_chat,
            pings: v5.pings,
            missions: v5.missions,
            tunnel: v5.tunnel,
            graduated: v5.graduated,
            central: v5.central,
            techs: v5.techs,
            disease_until: v5.disease_until,
            blizzard_until: v5.blizzard_until,
            pending_event: v5.pending_event,
            event_rng: v5.event_rng,
            guest_perm: v5.guest_perm,
            owner_id: v5.owner_id,
            next_id: v5.next_id,
            rng: v5.rng,
            central_ledger: v5.central_ledger,
            leader: v5.leader,
            mourning_until: v5.mourning_until,
            morale: v5.morale,
        }
    }
}

/// V7 (pre-wildlife/Tailor-Shop, i.e. every field the live `GameState` has
/// today except `wildlife`) shape: what `persist.rs` wrote under the
/// `FCWORLD7` header before this change. `Building` is unchanged since V6,
/// so it's reused from `types` directly; `Survivor` is mirrored as
/// [`SurvivorV8`] and `Stockpile` as [`StockpileV7`] (the live types have
/// since grown `thirst`/`bury_target` and `fur`/`cloth`/`water`
/// respectively).
#[derive(Serialize, Deserialize)]
pub struct GameStateV7 {
    pub tick: u64,
    pub win_days: u32,
    pub tiles: Vec<Tile>,
    pub buildings: Vec<Building>,
    pub survivors: Vec<SurvivorV8>,
    pub stock: StockpileV7,
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
    pub leader: Option<u32>,
    pub mourning_until: u64,
    pub morale: f32,
    pub pending_migrant: Option<TunnelMigrant>,
}

impl From<GameStateV6> for GameStateV7 {
    fn from(v6: GameStateV6) -> GameStateV7 {
        GameStateV7 {
            tick: v6.tick,
            win_days: v6.win_days,
            tiles: v6.tiles,
            buildings: v6.buildings,
            survivors: v6.survivors,
            stock: v6.stock,
            furnace_level: v6.furnace_level,
            furnace_lit: v6.furnace_lit,
            cold_snap: v6.cold_snap,
            players: v6.players,
            phase: v6.phase,
            events: v6.events,
            total_events: v6.total_events,
            chat: v6.chat,
            total_chat: v6.total_chat,
            pings: v6.pings,
            missions: v6.missions,
            tunnel: v6.tunnel,
            graduated: v6.graduated,
            central: v6.central,
            techs: v6.techs,
            disease_until: v6.disease_until,
            blizzard_until: v6.blizzard_until,
            pending_event: v6.pending_event,
            event_rng: v6.event_rng,
            guest_perm: v6.guest_perm,
            owner_id: v6.owner_id,
            next_id: v6.next_id,
            rng: v6.rng,
            central_ledger: v6.central_ledger,
            leader: v6.leader,
            mourning_until: v6.mourning_until,
            morale: v6.morale,
            // V6 predates Tunnel migrants — nobody was mid-wait at the
            // tunnel mouth when this world was saved.
            pending_migrant: None,
        }
    }
}

impl From<GameStateV7> for GameStateV8 {
    fn from(v7: GameStateV7) -> GameStateV8 {
        GameStateV8 {
            tick: v7.tick,
            win_days: v7.win_days,
            tiles: v7.tiles,
            buildings: v7.buildings,
            survivors: v7.survivors,
            stock: StockpileV8 {
                wood: v7.stock.wood,
                coal: v7.stock.coal,
                food: v7.stock.food,
                // V7 predates fur/cloth and the wildlife resource system —
                // nobody could have banked any yet.
                fur: 0.0,
                cloth: 0.0,
            },
            furnace_level: v7.furnace_level,
            furnace_lit: v7.furnace_lit,
            cold_snap: v7.cold_snap,
            players: v7.players,
            phase: v7.phase,
            events: v7.events,
            total_events: v7.total_events,
            chat: v7.chat,
            total_chat: v7.total_chat,
            pings: v7.pings,
            missions: v7.missions,
            tunnel: v7.tunnel,
            graduated: v7.graduated,
            central: v7.central,
            techs: v7.techs,
            disease_until: v7.disease_until,
            blizzard_until: v7.blizzard_until,
            pending_event: v7.pending_event,
            event_rng: v7.event_rng,
            guest_perm: v7.guest_perm,
            owner_id: v7.owner_id,
            next_id: v7.next_id,
            rng: v7.rng,
            central_ledger: v7.central_ledger,
            leader: v7.leader,
            mourning_until: v7.mourning_until,
            morale: v7.morale,
            pending_migrant: v7.pending_migrant,
            // V7 predates the wildlife resource system entirely — an old
            // save resuming under V0.10 must NOT start with zero deer/rabbit
            // (that would strand every existing HunterHut unable to produce
            // until population "grows" from nothing, since logistic growth
            // from 0 never leaves 0). Start at the same levels a brand-new
            // world gets.
            wildlife: Wildlife { deer: DEER_START, rabbit: RABBIT_START },
        }
    }
}

/// V8 (pre-death/burial/thirst, i.e. every field the live `GameState` has
/// today except `corpses`/`graves`) shape: what `persist.rs` wrote under the
/// `FCWORLD8` header before this change. `Building` is unchanged, so it's
/// reused from `types` directly; `Survivor` is mirrored as [`SurvivorV8`]
/// and `Stockpile` as [`StockpileV8`] (the live types have since grown
/// `thirst`/`bury_target` and `water` respectively).
#[derive(Serialize, Deserialize)]
pub struct GameStateV8 {
    pub tick: u64,
    pub win_days: u32,
    pub tiles: Vec<Tile>,
    pub buildings: Vec<Building>,
    pub survivors: Vec<SurvivorV8>,
    pub stock: StockpileV8,
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
    pub leader: Option<u32>,
    pub mourning_until: u64,
    pub morale: f32,
    pub pending_migrant: Option<TunnelMigrant>,
    pub wildlife: Wildlife,
}

/// V9 (pre-livestock, i.e. every field the live `GameState` has today except
/// `livestock`) shape: what `persist.rs` wrote under the `FCWORLD9` header
/// before this change. Neither `Building`, `Survivor` nor `Stockpile` changed
/// shape in this hop (only `GameState` itself grew a field), so they're all
/// reused from `types` directly instead of needing frozen mirrors.
#[derive(Serialize, Deserialize)]
pub struct GameStateV9 {
    pub tick: u64,
    pub win_days: u32,
    pub tiles: Vec<Tile>,
    pub buildings: Vec<Building>,
    pub survivors: Vec<Survivor>,
    pub stock: StockpileV10,
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
    pub leader: Option<u32>,
    pub mourning_until: u64,
    pub morale: f32,
    pub pending_migrant: Option<TunnelMigrant>,
    pub wildlife: Wildlife,
    pub corpses: Vec<Corpse>,
    pub graves: Vec<Grave>,
}

impl From<GameStateV8> for GameStateV9 {
    fn from(v8: GameStateV8) -> GameStateV9 {
        GameStateV9 {
            tick: v8.tick,
            win_days: v8.win_days,
            tiles: v8.tiles,
            buildings: v8.buildings,
            survivors: v8
                .survivors
                .into_iter()
                .map(|s| Survivor {
                    id: s.id,
                    name: s.name,
                    hp: s.hp,
                    hunger: s.hunger,
                    assigned_building: s.assigned_building,
                    owner: s.owner,
                    x: s.x,
                    y: s.y,
                    move_target: s.move_target,
                    profession: s.profession,
                    xp: s.xp,
                    trained_kind: s.trained_kind,
                    chop_target: s.chop_target,
                    carrying_wood: s.carrying_wood,
                    // V8 predates thirst — same "fresh start, no retroactive
                    // penalty" default `xp: 0.0` already uses. Zero is the
                    // SAFE starting state here (unlike `Wildlife`'s
                    // population counters, which need seeding since they're
                    // a production source, not a consumption meter).
                    thirst: 0.0,
                    // Nobody was mid-burial when this world predated the
                    // concept entirely.
                    bury_target: None,
                })
                .collect(),
            stock: StockpileV10 {
                wood: v8.stock.wood,
                coal: v8.stock.coal,
                food: v8.stock.food,
                fur: v8.stock.fur,
                cloth: v8.stock.cloth,
                // V8 predates the thirst/Well system — nobody could have
                // banked any yet.
                water: 0.0,
            },
            furnace_level: v8.furnace_level,
            furnace_lit: v8.furnace_lit,
            cold_snap: v8.cold_snap,
            players: v8.players,
            phase: v8.phase,
            events: v8.events,
            total_events: v8.total_events,
            chat: v8.chat,
            total_chat: v8.total_chat,
            pings: v8.pings,
            missions: v8.missions,
            tunnel: v8.tunnel,
            graduated: v8.graduated,
            central: v8.central,
            techs: v8.techs,
            disease_until: v8.disease_until,
            blizzard_until: v8.blizzard_until,
            pending_event: v8.pending_event,
            event_rng: v8.event_rng,
            guest_perm: v8.guest_perm,
            owner_id: v8.owner_id,
            next_id: v8.next_id,
            rng: v8.rng,
            central_ledger: v8.central_ledger,
            leader: v8.leader,
            mourning_until: v8.mourning_until,
            morale: v8.morale,
            pending_migrant: v8.pending_migrant,
            wildlife: v8.wildlife,
            // V8 predates death/burial entirely — nobody was mid-decay or
            // mid-grave when this world was saved.
            corpses: Vec::new(),
            graves: Vec::new(),
        }
    }
}

/// V10 (pre-trade-caravan, i.e. every field the live `GameState` has today
/// except `pending_caravan`) shape: what `persist.rs` wrote under the
/// `FCWORLD10` header before this change. `Stockpile` predates `gold` too
/// (see [`StockpileV10`]); everything else is unchanged since V9, so it's
/// reused from `types`/`GameStateV9` directly.
#[derive(Serialize, Deserialize)]
pub struct GameStateV10 {
    pub tick: u64,
    pub win_days: u32,
    pub tiles: Vec<Tile>,
    pub buildings: Vec<Building>,
    pub survivors: Vec<Survivor>,
    pub stock: StockpileV10,
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
    pub leader: Option<u32>,
    pub mourning_until: u64,
    pub morale: f32,
    pub pending_migrant: Option<TunnelMigrant>,
    pub wildlife: Wildlife,
    pub corpses: Vec<Corpse>,
    pub graves: Vec<Grave>,
    pub livestock: Livestock,
}

impl From<GameStateV9> for GameStateV10 {
    fn from(v9: GameStateV9) -> GameStateV10 {
        GameStateV10 {
            tick: v9.tick,
            win_days: v9.win_days,
            tiles: v9.tiles,
            buildings: v9.buildings,
            survivors: v9.survivors,
            stock: v9.stock,
            furnace_level: v9.furnace_level,
            furnace_lit: v9.furnace_lit,
            cold_snap: v9.cold_snap,
            players: v9.players,
            phase: v9.phase,
            events: v9.events,
            total_events: v9.total_events,
            chat: v9.chat,
            total_chat: v9.total_chat,
            pings: v9.pings,
            missions: v9.missions,
            tunnel: v9.tunnel,
            graduated: v9.graduated,
            central: v9.central,
            techs: v9.techs,
            disease_until: v9.disease_until,
            blizzard_until: v9.blizzard_until,
            pending_event: v9.pending_event,
            event_rng: v9.event_rng,
            guest_perm: v9.guest_perm,
            owner_id: v9.owner_id,
            next_id: v9.next_id,
            rng: v9.rng,
            central_ledger: v9.central_ledger,
            leader: v9.leader,
            mourning_until: v9.mourning_until,
            morale: v9.morale,
            pending_migrant: v9.pending_migrant,
            wildlife: v9.wildlife,
            corpses: v9.corpses,
            graves: v9.graves,
            // V9 predates the Farmhouse/livestock system entirely — an old
            // save resuming under V0.12+ must NOT start with zero cows/sheep
            // (same "logistic growth from 0 never leaves 0" reasoning as the
            // V7->V8 wildlife default). Start at the same levels a brand-new
            // world gets.
            livestock: Livestock { cow: COW_START, sheep: SHEEP_START },
        }
    }
}

/// V11 (pre-guest-permission-removal, i.e. every field the live `GameState`
/// has today PLUS `guest_perm`) shape: what `persist.rs` wrote under the
/// `FCWORLD11` header before this change. `Stockpile` already has `gold`
/// (added at the V10->V11 hop, see the old trade-caravan comment below) —
/// only `GameState` itself loses a field going into V12, so `Stockpile` is
/// reused from `types` directly, same as V10.
#[derive(Serialize, Deserialize)]
pub struct GameStateV11 {
    pub tick: u64,
    pub win_days: u32,
    pub tiles: Vec<Tile>,
    pub buildings: Vec<Building>,
    pub survivors: Vec<Survivor>,
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
    pub leader: Option<u32>,
    pub mourning_until: u64,
    pub morale: f32,
    pub pending_migrant: Option<TunnelMigrant>,
    pub wildlife: Wildlife,
    pub corpses: Vec<Corpse>,
    pub graves: Vec<Grave>,
    pub livestock: Livestock,
    pub pending_caravan: Option<TradeCaravan>,
}

impl From<GameStateV10> for GameStateV11 {
    fn from(v10: GameStateV10) -> GameStateV11 {
        GameStateV11 {
            tick: v10.tick,
            win_days: v10.win_days,
            tiles: v10.tiles,
            buildings: v10.buildings,
            survivors: v10.survivors,
            stock: Stockpile {
                wood: v10.stock.wood,
                coal: v10.stock.coal,
                food: v10.stock.food,
                fur: v10.stock.fur,
                cloth: v10.stock.cloth,
                water: v10.stock.water,
                // V10 predates the trade caravan entirely — nobody could
                // have banked any gold yet.
                gold: 0.0,
            },
            furnace_level: v10.furnace_level,
            furnace_lit: v10.furnace_lit,
            cold_snap: v10.cold_snap,
            players: v10.players,
            phase: v10.phase,
            events: v10.events,
            total_events: v10.total_events,
            chat: v10.chat,
            total_chat: v10.total_chat,
            pings: v10.pings,
            missions: v10.missions,
            tunnel: v10.tunnel,
            graduated: v10.graduated,
            central: v10.central,
            techs: v10.techs,
            disease_until: v10.disease_until,
            blizzard_until: v10.blizzard_until,
            pending_event: v10.pending_event,
            event_rng: v10.event_rng,
            guest_perm: v10.guest_perm,
            owner_id: v10.owner_id,
            next_id: v10.next_id,
            rng: v10.rng,
            central_ledger: v10.central_ledger,
            leader: v10.leader,
            mourning_until: v10.mourning_until,
            morale: v10.morale,
            pending_migrant: v10.pending_migrant,
            wildlife: v10.wildlife,
            corpses: v10.corpses,
            graves: v10.graves,
            livestock: v10.livestock,
            // V10 predates the trade caravan — nobody could have had one on
            // the road when this world was saved.
            pending_caravan: None,
        }
    }
}

/// V12 dropped `guest_perm` entirely: the tiered guest-permission system
/// (ViewOnly/Build/Full) was retired (2026-07-16) — every guest now has full
/// command authority, same as the owner (see `GameState::can_issue`). Every
/// other field carries straight through unchanged.
impl From<GameStateV11> for GameState {
    fn from(v11: GameStateV11) -> GameState {
        GameState {
            tick: v11.tick,
            win_days: v11.win_days,
            tiles: v11.tiles,
            buildings: v11.buildings,
            survivors: v11.survivors,
            stock: v11.stock,
            furnace_level: v11.furnace_level,
            furnace_lit: v11.furnace_lit,
            cold_snap: v11.cold_snap,
            players: v11.players,
            phase: v11.phase,
            events: v11.events,
            total_events: v11.total_events,
            chat: v11.chat,
            total_chat: v11.total_chat,
            pings: v11.pings,
            missions: v11.missions,
            tunnel: v11.tunnel,
            graduated: v11.graduated,
            central: v11.central,
            techs: v11.techs,
            disease_until: v11.disease_until,
            blizzard_until: v11.blizzard_until,
            pending_event: v11.pending_event,
            event_rng: v11.event_rng,
            owner_id: v11.owner_id,
            next_id: v11.next_id,
            rng: v11.rng,
            central_ledger: v11.central_ledger,
            leader: v11.leader,
            mourning_until: v11.mourning_until,
            morale: v11.morale,
            pending_migrant: v11.pending_migrant,
            wildlife: v11.wildlife,
            corpses: v11.corpses,
            graves: v11.graves,
            livestock: v11.livestock,
            pending_caravan: v11.pending_caravan,
        }
    }
}

/// V10 -> V12 straight through V11, needed by `persist::load_at`'s one-hop
/// `FCWORLD10` path (a V10 file goes all the way to the live format in a
/// single `.into()`).
impl From<GameStateV10> for GameState {
    fn from(v10: GameStateV10) -> GameState {
        GameStateV11::from(v10).into()
    }
}

/// V9 -> V12 straight through V10 -> V11, needed by `persist::load_at`'s
/// one-hop `FCWORLD9` path (a V9 file goes all the way to the live format in
/// a single `.into()`).
impl From<GameStateV9> for GameState {
    fn from(v9: GameStateV9) -> GameState {
        GameStateV10::from(v9).into()
    }
}

/// V8 -> V12 straight through V9 -> V10 -> V11, needed by
/// `persist::load_at`'s one-hop `FCWORLD8` path (a V8 file goes all the way
/// to the live format in a single `.into()`).
impl From<GameStateV8> for GameState {
    fn from(v8: GameStateV8) -> GameState {
        GameStateV9::from(v8).into()
    }
}

/// V7 -> V12 straight through V8 -> V9 -> V10 -> V11, needed by
/// `persist::load_at`'s one-hop `FCWORLD7` path (a V7 file goes all the way
/// to the live format in a single `.into()`).
impl From<GameStateV7> for GameState {
    fn from(v7: GameStateV7) -> GameState {
        GameStateV8::from(v7).into()
    }
}

/// V6 -> V12 straight through V7 -> V8 -> V9 -> V10 -> V11, needed by
/// `persist::load_at`'s one-hop `FCWORLD6` path (a V6 file goes all the way
/// to the live format in a single `.into()`).
impl From<GameStateV6> for GameState {
    fn from(v6: GameStateV6) -> GameState {
        GameStateV7::from(v6).into()
    }
}

/// V5 -> V12 straight through V6 -> V7 -> V8 -> V9 -> V10 -> V11, needed by
/// `persist::load_at`'s one-hop `FCWORLD5` path (a V5 file goes all the way
/// to the live format in a single `.into()`).
impl From<GameStateV5> for GameState {
    fn from(v5: GameStateV5) -> GameState {
        GameStateV6::from(v5).into()
    }
}

/// V4 -> V12 straight through V5 -> V6 -> V7 -> V8 -> V9 -> V10 -> V11,
/// needed by `persist::load_at`'s one-hop `FCWORLD4` path (a V4 file goes
/// all the way to the live format in a single `.into()`).
impl From<GameStateV4> for GameState {
    fn from(v4: GameStateV4) -> GameState {
        GameStateV5::from(v4).into()
    }
}

/// V3 -> V12 straight through V4 -> V5 -> V6 -> V7 -> V8 -> V9 -> V10 -> V11,
/// needed by `persist::load_at`'s one-hop `FCWORLD3` path (a V3 file goes
/// all the way to the live format in a single `.into()`).
impl From<GameStateV3> for GameState {
    fn from(v3: GameStateV3) -> GameState {
        GameStateV4::from(v3).into()
    }
}

/// V2 -> V12 straight through V3 -> V4 -> V5 -> V6 -> V7 -> V8 -> V9 -> V10
/// -> V11, needed by `persist::load_at`'s one-hop `FCWORLD2` path.
impl From<GameStateV2> for GameState {
    fn from(v2: GameStateV2) -> GameState {
        GameStateV3::from(v2).into()
    }
}

/// V1 -> V12 straight through V2 -> V3 -> V4 -> V5 -> V6 -> V7 -> V8 -> V9 ->
/// V10 -> V11, for `persist::load_at`'s one-shot fallback path (a V1 file
/// goes all the way to the live format in a single `.into()`).
impl From<GameStateV1> for GameState {
    fn from(v1: GameStateV1) -> GameState {
        GameStateV2::from(v1).into()
    }
}
