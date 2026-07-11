//! Frozen mirror of the V1 (pre-central-world) save format, so worlds saved
//! by older binaries keep loading after `Survivor`/`PlayerInfo`/`GameState`
//! grew new fields. Bincode is positional — any field added to the live types
//! makes old bytes undecodable as them — so `persist::load_at` falls back to
//! decoding these mirrors and converting.
//!
//! These structs must never change again: they ARE the on-disk V1 layout.
//! Types that V1 shares with the live format (Tile, Building, Mission, ...)
//! are reused from `types` directly; only the three that grew fields are
//! mirrored. Serialize is derived solely so tests can fabricate V1 bytes.

use serde::{Deserialize, Serialize};

use crate::game::types::{
    Building, CaravanOffer, ChatLine, GameEvent, GamePhase, GameState, GuestPermission, Mission,
    Ping, PlayerInfo, Role, Stockpile, Survivor, Tech, Tile, TunnelState,
};

#[derive(Serialize, Deserialize)]
pub struct SurvivorV1 {
    pub id: u32,
    pub name: String,
    pub hp: f32,
    pub hunger: f32,
    pub assigned_building: Option<u32>,
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

#[derive(Serialize, Deserialize)]
pub struct GameStateV1 {
    pub tick: u64,
    pub win_days: u32,
    pub tiles: Vec<Tile>,
    pub buildings: Vec<Building>,
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

impl From<GameStateV1> for GameState {
    fn from(v1: GameStateV1) -> GameState {
        GameState {
            tick: v1.tick,
            win_days: v1.win_days,
            tiles: v1.tiles,
            buildings: v1.buildings,
            survivors: v1
                .survivors
                .into_iter()
                .map(|s| Survivor {
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
