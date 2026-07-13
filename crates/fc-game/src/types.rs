//! Shared, serializable game data. This is the single source of truth that the
//! server simulates and broadcasts to every client each tick.
//!
//! The data model is split across `types/` submodules by domain — the map
//! ([`world`]), buildings ([`building`]), survivors and players ([`people`]),
//! resources ([`economy`]), progression ([`progression`]), transient events
//! ([`events`]), player commands ([`command`]) and the [`GameState`] aggregate
//! ([`state`]) — but every item is re-exported here, so the whole codebase
//! keeps referring to them as `game::types::X`. The shared balance constants
//! and the two spatial free functions live in this root module.

mod building;
mod command;
mod economy;
mod events;
mod people;
mod progression;
mod state;
mod world;

pub use building::*;
pub use command::*;
pub use economy::*;
pub use events::*;
pub use people::*;
pub use progression::*;
pub use state::*;
pub use world::*;

pub const MAP_W: usize = 64;
pub const MAP_H: usize = 64;

/// Simulation rate: 5 ticks per second.
pub const TICK_MS: u64 = 200;
/// One in-game day lasts 150 real seconds.
pub const TICKS_PER_DAY: u64 = 750;
/// Tick-of-day at which newcomers may arrive (morning, t = 0.3).
pub const ARRIVAL_TICK: u64 = 225;

pub const FURNACE_X: u8 = 31;
pub const FURNACE_Y: u8 = 31;

pub const DEFAULT_WIN_DAYS: u32 = 12;
/// Day-based victory must never fire before the longest `SurviveDays` mission
/// can complete, or the Tunnel could be permanently unreachable on that world.
pub const MIN_WIN_DAYS: u32 = 4;
pub const MAX_POPULATION: i32 = 60;
/// How many survivors one account may bring through the Tunnel into the
/// central world, total. Re-entering tops the group back up to this cap (from
/// the personal world's population), so repeat visits can't flood the shared
/// map with one account's settlers.
pub const CENTRAL_MIGRANTS_PER_ACCOUNT: usize = 5;

pub const FOOD_PER_SURVIVOR_DAY: f32 = 1.2;
pub const FURNACE_COAL_PER_DAY_PER_LEVEL: f32 = 12.0;
/// Extra HP per in-game day each staffed hospital worker restores to survivors.
pub const HOSPITAL_CARE_PER_WORKER_DAY: f32 = 2.0;
/// Food-consumption multiplier while any kitchen is staffed (lower = thriftier).
pub const KITCHEN_FOOD_EFFICIENCY: f32 = 0.75;
/// Construction wood-cost multiplier while any warehouse is staffed — salvaged
/// and well-organized materials mean less goes to waste on a new build.
pub const WAREHOUSE_BUILD_DISCOUNT: f32 = 0.80;
/// Wood burns less efficiently than coal.
pub const WOOD_FUEL_PENALTY: f32 = 1.5;
pub const DEMOLISH_REFUND: f32 = 0.4;
pub const TENT_CAPACITY: usize = 4;

// --- V0.8: bino qurilishi va darajalar (construction & upgrades) ---

/// Binolar 1 dan shu darajagacha, bittalab yangilanadi.
pub const BUILDING_MAX_LEVEL: u8 = 10;
/// Bitta qurilish maydonchasi (yangi bino yoki yangilash) sig'diradigan
/// ustalar soni — `AdjustWorkers` qurilish paytida shu bilan cheklanadi.
pub const CONSTRUCTION_CREW_MAX: u8 = 3;
/// 1-daraja binoni tiklash uchun kerak usta-kunlar, bazaviy yog'och narxining
/// har birligiga (Chodir 15y ≈ 0.3 usta-kun; Issiqxona 35y ≈ 0.7).
pub const BUILD_WORKDAYS_PER_WOOD: f32 = 0.02;
/// 1-darajadan yuqori har bir daraja uchun ishlab-chiqarish/effekt bonusi
/// (+12%/daraja, L10 ≈ +108%).
pub const LEVEL_PRODUCTION_BONUS: f32 = 0.12;
/// Chodir 1-darajadan yuqori har bir darajada qo'shimcha joy oladi.
pub const TENT_CAPACITY_PER_LEVEL: usize = 1;

// --- V0.7: survivor management (positions/movement, leader, professions,
// XP/levels, morale) ---

/// Constant walking speed for server-authoritative survivor movement, in
/// tiles per real second. The map is open (no obstacles), so movement is a
/// straight line toward the current goal — no pathfinding needed.
pub const SURVIVOR_SPEED_TILES_PER_SEC: f32 = 2.5;
/// Distance covered per tick at [`SURVIVOR_SPEED_TILES_PER_SEC`], derived
/// from [`TICK_MS`] so the two constants can never drift apart.
pub const SURVIVOR_SPEED_PER_TICK: f32 = SURVIVOR_SPEED_TILES_PER_SEC * (TICK_MS as f32 / 1000.0);
/// How close (in tiles) a survivor must get to their goal to snap to it and
/// stop, so they don't perpetually overshoot-and-correct in a tiny jitter.
pub const ARRIVAL_EPSILON: f32 = SURVIVOR_SPEED_PER_TICK;

/// Colony-wide production multiplier while a leader is alive.
pub const LEADER_PRODUCTION_BONUS: f32 = 1.08;
/// Colony-wide production multiplier while the city mourns a dead leader.
pub const MOURNING_PRODUCTION_PENALTY: f32 = 0.85;
/// A dead leader's city mourns for one full in-game day before command
/// authority over the caravan choice reverts to "nobody's here to decide".
pub const MOURNING_DURATION_TICKS: u64 = TICKS_PER_DAY;

/// Production bonus when a survivor's profession matches the building kind
/// they're working (e.g. a Lumberjack in a Sawmill).
pub const PROFESSION_MATCH_BONUS: f32 = 1.25;

/// In-game days of work at the same building kind to reach each XP level.
/// Level N requires the CUMULATIVE total below (not N's slice alone).
pub const XP_DAYS_LEVEL_1: f32 = 1.0;
pub const XP_DAYS_LEVEL_2: f32 = 3.0;
pub const XP_DAYS_LEVEL_3: f32 = 6.0;
/// Per-level contribution multiplier bonus (level 1 -> +5%, 2 -> +10%, 3 -> +15%).
pub const XP_LEVEL_BONUS_PER_LEVEL: f32 = 0.05;
pub const XP_MAX_LEVEL: u8 = 3;

/// XP level (0..=XP_MAX_LEVEL) from accrued in-game work-days, thresholded by
/// `XP_DAYS_LEVEL_*` (cumulative totals, not per-level slices). Shared by the
/// sim's contribution math and the client's roster/appearance tiers, so the
/// two can never disagree about a survivor's level.
pub fn xp_level(xp: f32) -> u8 {
    if xp >= XP_DAYS_LEVEL_3 {
        3
    } else if xp >= XP_DAYS_LEVEL_2 {
        2
    } else if xp >= XP_DAYS_LEVEL_1 {
        1
    } else {
        0
    }
}

/// Morale starts here on a fresh world and is what every existing balance
/// test implicitly assumes (multiplier 1.0 — see `morale_multiplier`), so a
/// new world's production math is unchanged by this feature.
pub const MORALE_START: f32 = 70.0;
/// Morale drifts toward this baseline (±1/day) absent any other adjustment.
pub const MORALE_BASELINE: f32 = 60.0;
pub const MORALE_DEATH_PENALTY: f32 = 10.0;
pub const MORALE_STARVATION_PER_DAY: f32 = 3.0;
pub const MORALE_BLIZZARD_PER_DAY: f32 = 2.0;
pub const MORALE_KITCHEN_PER_DAY: f32 = 2.0;
pub const MORALE_HOSPITAL_PER_DAY: f32 = 2.0;
pub const MORALE_LEADER_PER_DAY: f32 = 1.0;
pub const MORALE_DRIFT_PER_DAY: f32 = 1.0;

/// Rolling chat log length kept in the snapshot.
pub const MAX_CHAT: usize = 40;
/// Longest chat message accepted (characters, after sanitizing).
pub const MAX_CHAT_LEN: usize = 200;
/// Longest player display name accepted (characters, after sanitizing).
pub const MAX_NAME_LEN: usize = 24;
/// Maximum simultaneous map pings kept in the snapshot (global cap).
pub const MAX_PINGS: usize = 12;
/// Maximum live pings a single player may hold, so one spammer can't evict
/// everyone else's markers before they naturally expire.
pub const MAX_PINGS_PER_PLAYER: usize = 4;
/// A map ping fades and is dropped after this many ticks (~5 s at 5 Hz).
pub const PING_TTL_TICKS: u64 = 25;

// --- V0.3: the Tunnel (see `progression::TunnelState`) ---
pub const TUNNEL_STAGES: u8 = 3;
/// How many `InvestTunnel` contributions complete one stage.
pub const TUNNEL_INVESTS_PER_STAGE: u32 = 5;
pub const TUNNEL_INVEST_WOOD: f32 = 15.0;
pub const TUNNEL_INVEST_COAL: f32 = 12.0;

// --- V0.3: technology-tree effects (see `progression::Tech`) ---
/// Flat extra warmth for every survivor once Insulation is researched.
pub const TECH_INSULATION_WARMTH: f32 = 4.0;
/// Furnace fuel-need multiplier with Efficient Furnace.
pub const TECH_FURNACE_EFFICIENCY: f32 = 0.75;
/// Production multiplier with Better Tools.
pub const TECH_TOOLS_PRODUCTION: f32 = 1.25;
/// Food-consumption multiplier with Rationing.
pub const TECH_RATIONING_FOOD: f32 = 0.85;
/// Hospital-care multiplier with Medicine.
pub const TECH_MEDICINE_CARE: f32 = 1.5;

// --- V0.3: dynamic events (see `events::CaravanOffer` / `events::GameEvent`) ---

/// Events never fire before this in-game day, so a fresh colony (and short
/// deterministic tests) is never disrupted while finding its feet.
pub const EVENT_GRACE_DAY: u32 = 3;

/// Per-day chance a sickness breaks out, and how long/hard it hits.
pub const DISEASE_CHANCE: f32 = 0.16;
pub const DISEASE_TICKS: u64 = 2 * TICKS_PER_DAY;
/// Extra HP lost per in-game day while a disease is active (offset by hospitals).
pub const DISEASE_HP_PER_DAY: f32 = 6.0;

/// Per-day chance of a blizzard, its duration, and the extra cold it brings.
pub const BLIZZARD_CHANCE: f32 = 0.14;
pub const BLIZZARD_TICKS: u64 = TICKS_PER_DAY;
pub const BLIZZARD_COLD: f32 = -8.0;

/// Per-day chance a refugee caravan arrives, and the offer's shape.
pub const CARAVAN_CHANCE: f32 = 0.22;
pub const CARAVAN_FOOD_PER_PERSON: u32 = 4;
/// How long the player has to decide (half an in-game day).
pub const CARAVAN_EXPIRE_TICKS: u64 = TICKS_PER_DAY / 2;

pub fn tile_index(x: u8, y: u8) -> usize {
    y as usize * MAP_W + x as usize
}

pub fn in_bounds(x: i32, y: i32) -> bool {
    x >= 0 && y >= 0 && (x as usize) < MAP_W && (y as usize) < MAP_H
}
