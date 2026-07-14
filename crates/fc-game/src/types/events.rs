//! Transient world happenings and co-op comms: chat lines, map pings, dynamic
//! events (disease / blizzard / refugee caravan) and the rolling event log.

use serde::{Deserialize, Serialize};

/// One line of co-op text chat, attributed to its author.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ChatLine {
    /// Author's player id; `0` marks a system message.
    pub player_id: u64,
    pub name: String,
    /// Player palette index for coloring (ignored for system lines).
    pub color: u8,
    pub text: String,
}

/// A transient map marker a player drops to draw attention (Alt+click).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Ping {
    pub player_id: u64,
    pub color: u8,
    /// World tile coordinates.
    pub x: f32,
    pub y: f32,
    /// Tick the ping was created; used for TTL expiry.
    pub tick: u64,
}

// --- V0.3: dynamic events (disease, refugee caravan, blizzard) ---

/// A refugee caravan offering shelter to newcomers in exchange for food.
/// Sits in `GameState.pending_event` until answered or it expires.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaravanOffer {
    pub count: u32,
    pub food_cost: u32,
    /// Tick after which the offer lapses (auto-declined).
    pub expires: u64,
}

/// Traveler(s) standing at the Tunnel mouth, waiting to join the colony.
/// Sits in `GameState.pending_migrant` until the colony has room (housing +
/// population cap, checked every tick — see `tick.rs`) or `expires` passes,
/// whichever comes first. Unlike `CaravanOffer` this needs no leader
/// decision: it resolves automatically the moment conditions allow, or the
/// travelers give up and head back through the Tunnel.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TunnelMigrant {
    pub count: u32,
    /// Tick after which the travelers give up and leave.
    pub expires: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GameEvent {
    pub day: u32,
    pub text: String,
    /// True for world/system events (deaths, weather, arrivals, victory) that
    /// must not be evicted from the capped log by cosmetic player-action spam.
    pub system: bool,
}
