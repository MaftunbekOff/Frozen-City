//! Shared, serializable game data. This is the single source of truth that the
//! server simulates and broadcasts to every client each tick.

use serde::{Deserialize, Serialize};

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

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terrain {
    Snow,
    Forest,
    Coal,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Tile {
    pub terrain: Terrain,
    /// Remaining harvestable units (wood for Forest, coal for Coal).
    pub deposit: u16,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuildingKind {
    Furnace,
    Tent,
    Sawmill,
    CoalMine,
    HunterHut,
    Greenhouse,
    Hospital,
    Kitchen,
    Warehouse,
}

impl BuildingKind {
    pub const BUILDABLE: [BuildingKind; 8] = [
        BuildingKind::Tent,
        BuildingKind::Sawmill,
        BuildingKind::CoalMine,
        BuildingKind::HunterHut,
        BuildingKind::Greenhouse,
        BuildingKind::Hospital,
        BuildingKind::Kitchen,
        BuildingKind::Warehouse,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BuildingKind::Furnace => "Furnace",
            BuildingKind::Tent => "Tent",
            BuildingKind::Sawmill => "Sawmill",
            BuildingKind::CoalMine => "Coal Mine",
            BuildingKind::HunterHut => "Hunter's Hut",
            BuildingKind::Greenhouse => "Greenhouse",
            BuildingKind::Hospital => "Hospital",
            BuildingKind::Kitchen => "Kitchen",
            BuildingKind::Warehouse => "Warehouse",
        }
    }

    pub fn letter(self) -> &'static str {
        match self {
            BuildingKind::Furnace => "F",
            BuildingKind::Tent => "T",
            BuildingKind::Sawmill => "S",
            BuildingKind::CoalMine => "C",
            BuildingKind::HunterHut => "H",
            BuildingKind::Greenhouse => "G",
            BuildingKind::Hospital => "+",
            BuildingKind::Kitchen => "K",
            BuildingKind::Warehouse => "W",
        }
    }

    pub fn cost_wood(self) -> u32 {
        match self {
            BuildingKind::Furnace => 0,
            BuildingKind::Tent => 15,
            BuildingKind::Sawmill => 25,
            BuildingKind::CoalMine => 30,
            BuildingKind::HunterHut => 25,
            BuildingKind::Greenhouse => 35,
            BuildingKind::Hospital => 35,
            BuildingKind::Kitchen => 25,
            BuildingKind::Warehouse => 30,
        }
    }

    pub fn max_workers(self) -> u8 {
        match self {
            BuildingKind::Furnace | BuildingKind::Tent => 0,
            BuildingKind::Sawmill => 2,
            BuildingKind::CoalMine => 3,
            BuildingKind::HunterHut => 2,
            BuildingKind::Greenhouse => 2,
            BuildingKind::Hospital => 2,
            BuildingKind::Kitchen => 1,
            BuildingKind::Warehouse => 1,
        }
    }

    /// Units produced per worker per in-game day (resource producers only;
    /// Hospital/Kitchen provide effects handled in the tick, not raw output).
    pub fn production_per_worker_day(self) -> f32 {
        match self {
            BuildingKind::Sawmill => 12.0,
            BuildingKind::CoalMine => 15.0,
            BuildingKind::HunterHut => 10.0,
            // Pricier than the Hunter's Hut but higher output per worker — a
            // sustained late-game food source rather than a strict upgrade.
            BuildingKind::Greenhouse => 13.0,
            _ => 0.0,
        }
    }

    pub fn size(self) -> (u8, u8) {
        match self {
            BuildingKind::Furnace => (2, 2),
            _ => (1, 1),
        }
    }

    pub fn buildable(self) -> bool {
        self != BuildingKind::Furnace
    }

    pub fn description(self) -> &'static str {
        match self {
            BuildingKind::Furnace => "Keep it burning. Consumes coal (or wood).",
            BuildingKind::Tent => "Houses 4 people. Place inside the heat radius.",
            BuildingKind::Sawmill => "Workers harvest nearby forest for wood.",
            BuildingKind::CoalMine => "Must be placed on a coal deposit.",
            BuildingKind::HunterHut => "Workers hunt for food.",
            BuildingKind::Greenhouse => "A high-output indoor farm (more food/worker).",
            BuildingKind::Hospital => "Staffed: heals survivors faster.",
            BuildingKind::Kitchen => "Staffed: the city eats more efficiently.",
            BuildingKind::Warehouse => "Staffed: new construction wastes less wood.",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Building {
    pub id: u32,
    pub kind: BuildingKind,
    pub x: u8,
    pub y: u8,
    pub workers: u8,
    /// Fractional extraction progress (sawmill / coal mine).
    pub progress: f32,
    /// Player id that placed this building; `None` for the starting furnace.
    pub owner: Option<u64>,
    /// Account id that placed this building, set only when it was placed in
    /// the CENTRAL world by an account-authenticated connection; `None`
    /// everywhere else (personal/guest worlds keep using `owner`/session-id
    /// authority) and for legacy central buildings placed before this field
    /// existed (migration default `None` — they stay demolishable by anyone,
    /// same as today, rather than becoming permanently stuck).
    pub owner_account: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Survivor {
    pub id: u32,
    pub name: String,
    /// 0..=100; death at 0.
    pub hp: f32,
    /// 0..=120; starvation damage above 80.
    pub hunger: f32,
    /// Building this survivor is individually assigned to work at, if any.
    /// `None` means they're part of the anonymous pool `AdjustWorkers`
    /// counts but doesn't track by identity. Cleared automatically when the
    /// survivor dies or their building is demolished.
    pub assigned_building: Option<u32>,
    /// Account id (see `net::accounts`) this survivor belongs to. `None` in
    /// personal/shared worlds; `Some` only for settlers brought through the
    /// Tunnel into the central world, where only their owner may command them.
    pub owner: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Stockpile {
    pub wood: f32,
    pub coal: f32,
    pub food: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerInfo {
    pub id: u64,
    pub name: String,
    /// Index into the client-side player palette.
    pub color: u8,
    /// Last known cursor position in world tile coordinates.
    pub cursor: Option<(f32, f32)>,
    /// Buildings this player has placed (kept across reconnects).
    pub built: u32,
    /// Buildings this player has demolished (kept across reconnects).
    pub demolished: u32,
    /// Authority in this world (owner vs guest); kept across reconnects.
    pub role: Role,
    /// Account id this player signed in with, `None` for guests. In the
    /// central world it links a player to the settlers they own (see
    /// `Survivor::owner`); the stable identity across sessions is the
    /// account, not the per-world player id.
    pub account: Option<i64>,
}

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

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GamePhase {
    Running,
    Won,
    Lost,
}

/// One account's running contribution totals in the central world (V0.5
/// "economy v1"): credited for what their owned settlers' staffed buildings
/// produce there, and for what they spend placing buildings there. Purely a
/// ledger — it does not gate anything (yet); it's the data source for
/// `ServerMsg::Showcase`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct ContributionTotals {
    pub wood: f32,
    pub coal: f32,
    pub food: f32,
    /// Wood spent placing buildings (a cost, tracked separately from the
    /// production credits above so a showcase can show "built" vs "produced").
    pub wood_spent: f32,
}

/// One row of the central-world contribution ledger, keyed by account.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct LedgerEntry {
    pub account: i64,
    pub totals: ContributionTotals,
}

/// A player's authority in the world. The first to join owns it; everyone else
/// is a guest, bounded by the world's [`GuestPermission`].
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Owner,
    Guest,
}

/// What guests are allowed to do, chosen by the owner. The owner always has
/// full authority; a world with no connected owner behaves as `Full`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuestPermission {
    /// Chat, ping and move the cursor only — no world changes.
    ViewOnly,
    /// Place buildings, assign workers, and demolish their OWN buildings.
    Build,
    /// Everything the owner can do (except owner-only admin: set policy, kick).
    Full,
}

// --- V0.3: missions & the Tunnel (personal-world progression) ---

/// Each variant carries its completion target. Progress is derived from the
/// live [`GameState`] (see [`GameState::mission_current`]), so missions never
/// need per-tick counters and stay deterministic.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissionKind {
    BuildTents(u32),
    Population(u32),
    Sawmills(u32),
    StockpileCoal(u32),
    SurviveDays(u32),
}

impl MissionKind {
    pub fn target(self) -> u32 {
        match self {
            MissionKind::BuildTents(n)
            | MissionKind::Population(n)
            | MissionKind::Sawmills(n)
            | MissionKind::StockpileCoal(n)
            | MissionKind::SurviveDays(n) => n,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            MissionKind::BuildTents(_) => "Build tents",
            MissionKind::Population(_) => "Reach population",
            MissionKind::Sawmills(_) => "Build sawmills",
            MissionKind::StockpileCoal(_) => "Stockpile coal",
            MissionKind::SurviveDays(_) => "Survive days",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mission {
    pub kind: MissionKind,
    pub reward_wood: u32,
    pub reward_coal: u32,
    pub reward_food: u32,
    pub done: bool,
}

/// The Tunnel: the multi-stage megaproject that graduates a personal world to
/// the Global World. Unlocked once every mission is complete, then advanced by
/// `InvestTunnel` commands until `stage` reaches [`TUNNEL_STAGES`].
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct TunnelState {
    pub unlocked: bool,
    /// 0..=TUNNEL_STAGES; equal to TUNNEL_STAGES means complete.
    pub stage: u8,
    /// Progress within the current stage, 0.0..1.0.
    pub progress: f32,
}

impl Default for TunnelState {
    fn default() -> Self {
        TunnelState {
            unlocked: false,
            stage: 0,
            progress: 0.0,
        }
    }
}

pub const TUNNEL_STAGES: u8 = 3;
/// How many `InvestTunnel` contributions complete one stage.
pub const TUNNEL_INVESTS_PER_STAGE: u32 = 5;
pub const TUNNEL_INVEST_WOOD: f32 = 15.0;
pub const TUNNEL_INVEST_COAL: f32 = 12.0;

// --- V0.3: technology tree (permanent cooperative upgrades) ---

/// One researchable, permanent upgrade. Effects are applied in `sim::tick`;
/// with no tech researched every effect is identity, so determinism holds.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tech {
    Insulation,
    EfficientFurnace,
    Tools,
    Rationing,
    Medicine,
}

impl Tech {
    pub const ALL: [Tech; 5] = [
        Tech::Insulation,
        Tech::EfficientFurnace,
        Tech::Tools,
        Tech::Rationing,
        Tech::Medicine,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Tech::Insulation => "Insulation",
            Tech::EfficientFurnace => "Efficient Furnace",
            Tech::Tools => "Better Tools",
            Tech::Rationing => "Rationing",
            Tech::Medicine => "Medicine",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Tech::Insulation => "Everyone shrugs off the cold a little better.",
            Tech::EfficientFurnace => "The furnace burns 25% less fuel.",
            Tech::Tools => "Workers produce 25% more.",
            Tech::Rationing => "The city eats 15% less food.",
            Tech::Medicine => "Hospitals heal 50% faster.",
        }
    }

    pub fn cost_wood(self) -> u32 {
        match self {
            Tech::Insulation => 40,
            Tech::EfficientFurnace => 30,
            Tech::Tools => 50,
            Tech::Rationing => 35,
            Tech::Medicine => 40,
        }
    }

    pub fn cost_coal(self) -> u32 {
        match self {
            Tech::Insulation => 0,
            Tech::EfficientFurnace => 20,
            Tech::Tools => 10,
            Tech::Rationing => 0,
            Tech::Medicine => 20,
        }
    }
}

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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GameEvent {
    pub day: u32,
    pub text: String,
    /// True for world/system events (deaths, weather, arrivals, victory) that
    /// must not be evicted from the capped log by cosmetic player-action spam.
    pub system: bool,
}

/// Commands a player may issue; the server validates every one of them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum PlayerCommand {
    Place { kind: BuildingKind, x: u8, y: u8 },
    Demolish { building: u32 },
    AdjustWorkers { building: u32, delta: i8 },
    /// Assign (or, with `building: None`, unassign) one named survivor to
    /// work at a specific building. Distinct from `AdjustWorkers`: this
    /// targets an identity, not a headcount.
    AssignSurvivor { survivor: u32, building: Option<u32> },
    SetFurnaceLevel { level: u8 },
    /// Contribute resources toward excavating the Tunnel (once unlocked).
    InvestTunnel,
    /// Spend resources to permanently unlock a technology.
    Research { tech: Tech },
    /// Answer a pending event choice (e.g. a refugee caravan).
    RespondEvent { accept: bool },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GameState {
    pub tick: u64,
    pub win_days: u32,
    /// Row-major MAP_W x MAP_H. May be empty on the wire (tiles are only
    /// included in periodic snapshots); clients keep their last copy.
    pub tiles: Vec<Tile>,
    pub buildings: Vec<Building>,
    pub survivors: Vec<Survivor>,
    pub stock: Stockpile,
    /// 0 = off, 1..=3 heat output level.
    pub furnace_level: u8,
    pub furnace_lit: bool,
    pub cold_snap: bool,
    pub players: Vec<PlayerInfo>,
    pub phase: GamePhase,
    /// Rolling log of the most recent events.
    pub events: Vec<GameEvent>,
    /// Monotonic counter of all events ever pushed (events itself is capped).
    pub total_events: u64,
    /// Rolling co-op chat log (capped at [`MAX_CHAT`]).
    pub chat: Vec<ChatLine>,
    /// Monotonic counter of all chat lines ever pushed (chat itself is capped).
    pub total_chat: u64,
    /// Active map pings; expired each tick after [`PING_TTL_TICKS`].
    pub pings: Vec<Ping>,
    /// Ordered personal-world missions; completing all unlocks the Tunnel.
    pub missions: Vec<Mission>,
    /// The Tunnel megaproject (graduation to the Global World).
    pub tunnel: TunnelState,
    /// Set when the win was earned by completing the Tunnel (graduating to
    /// the Global World), as opposed to surviving to the day-count victory.
    /// A permanent achievement: the server carries it across the post-game
    /// world reset, so graduation keeps unlocking the central world forever.
    pub graduated: bool,
    /// True for the one shared central world (the Global World reached
    /// through the Tunnel): survivors there are account-owned settlers who
    /// never hunger or die, there is no win/lose/events/arrivals, and command
    /// authority follows settler ownership instead of owner/guest roles.
    pub central: bool,
    /// Permanently unlocked technologies.
    pub techs: Vec<Tech>,
    /// Tick until which a disease is active (0 = none).
    pub disease_until: u64,
    /// Tick until which a blizzard is active (0 = none).
    pub blizzard_until: u64,
    /// An unanswered event choice (refugee caravan), if any.
    pub pending_event: Option<CaravanOffer>,
    /// Private RNG stream for events, kept separate from the main sim RNG so
    /// adding events never perturbs mapgen/cold-snap/arrival determinism.
    pub event_rng: u64,
    /// What guests may do in this world, set by the owner.
    pub guest_perm: GuestPermission,
    /// The player id that owns this world. Set once when the very first player
    /// joins and never cleared automatically, so a momentarily-empty roster
    /// (owner mid-reconnect) can't let a stranger seize ownership.
    pub owner_id: Option<u64>,
    pub next_id: u32,
    /// SplitMix64 RNG state.
    pub rng: u64,
    /// Central world only: per-account contribution ledger (see
    /// [`ContributionTotals`]). Always empty in personal/guest worlds.
    pub central_ledger: Vec<LedgerEntry>,
}

pub fn tile_index(x: u8, y: u8) -> usize {
    y as usize * MAP_W + x as usize
}

pub fn in_bounds(x: i32, y: i32) -> bool {
    x >= 0 && y >= 0 && (x as usize) < MAP_W && (y as usize) < MAP_H
}

impl GameState {
    pub fn day(&self) -> u32 {
        (self.tick / TICKS_PER_DAY) as u32 + 1
    }

    /// 0.0 = midnight, 0.5 = midday.
    pub fn time_of_day(&self) -> f32 {
        (self.tick % TICKS_PER_DAY) as f32 / TICKS_PER_DAY as f32
    }

    pub fn is_night(&self) -> bool {
        let t = self.time_of_day();
        !(0.25..0.75).contains(&t)
    }

    /// Outside temperature in degrees Celsius.
    pub fn temperature(&self) -> f32 {
        let day = self.day() as f32;
        let base = -4.0 - 1.2 * (day - 1.0);
        let t = self.time_of_day();
        let diurnal = -6.0 * (std::f32::consts::TAU * t).cos();
        let snap = if self.cold_snap && t >= 0.7 { -10.0 } else { 0.0 };
        let blizzard = if self.blizzard_active() { BLIZZARD_COLD } else { 0.0 };
        base + diurnal + snap + blizzard
    }

    /// Heat radius in tiles around the furnace center; 0 when unlit.
    pub fn heat_radius(&self) -> f32 {
        if self.furnace_lit && self.furnace_level > 0 {
            4.0 + 6.0 * self.furnace_level as f32
        } else {
            0.0
        }
    }

    /// Center of the 2x2 furnace in tile coordinates.
    pub fn furnace_center() -> (f32, f32) {
        (FURNACE_X as f32 + 1.0, FURNACE_Y as f32 + 1.0)
    }

    pub fn tile(&self, x: u8, y: u8) -> &Tile {
        &self.tiles[tile_index(x, y)]
    }

    pub fn building_at(&self, x: u8, y: u8) -> Option<&Building> {
        self.buildings.iter().find(|b| {
            let (w, h) = b.kind.size();
            x >= b.x && x < b.x + w && y >= b.y && y < b.y + h
        })
    }

    pub fn find_building(&self, id: u32) -> Option<&Building> {
        self.buildings.iter().find(|b| b.id == id)
    }

    /// Look up a connected player by id.
    pub fn player(&self, id: u64) -> Option<&PlayerInfo> {
        self.players.iter().find(|p| p.id == id)
    }

    pub fn is_owner(&self, id: u64) -> bool {
        self.player(id).map(|p| p.role == Role::Owner).unwrap_or(false)
    }

    /// Live progress value for a mission, compared against `kind.target()`.
    pub fn mission_current(&self, kind: MissionKind) -> u32 {
        match kind {
            MissionKind::BuildTents(_) => self
                .buildings
                .iter()
                .filter(|b| b.kind == BuildingKind::Tent)
                .count() as u32,
            MissionKind::Population(_) => self.survivors.len() as u32,
            MissionKind::Sawmills(_) => self
                .buildings
                .iter()
                .filter(|b| b.kind == BuildingKind::Sawmill)
                .count() as u32,
            MissionKind::StockpileCoal(_) => self.stock.coal.max(0.0) as u32,
            MissionKind::SurviveDays(_) => self.day(),
        }
    }

    pub fn all_missions_done(&self) -> bool {
        !self.missions.is_empty() && self.missions.iter().all(|m| m.done)
    }

    pub fn has_tech(&self, t: Tech) -> bool {
        self.techs.contains(&t)
    }

    pub fn disease_active(&self) -> bool {
        self.tick < self.disease_until
    }

    pub fn blizzard_active(&self) -> bool {
        self.tick < self.blizzard_until
    }

    /// Whether any connected player currently owns the world.
    pub fn owner_present(&self) -> bool {
        self.players.iter().any(|p| p.role == Role::Owner)
    }

    /// The account `pid` signed in with, if they're connected and logged in.
    pub fn player_account(&self, pid: u64) -> Option<i64> {
        self.player(pid).and_then(|p| p.account)
    }

    /// How many central-world settlers `account` owns.
    pub fn owned_settlers(&self, account: i64) -> usize {
        self.survivors
            .iter()
            .filter(|s| s.owner == Some(account))
            .count()
    }

    /// Mutable access to `account`'s central-world ledger row, creating it
    /// (zeroed) on first use. Callers apply their own delta via the closure —
    /// keeps every call site a one-liner instead of a find-or-insert dance.
    pub fn credit_ledger(&mut self, account: i64, f: impl FnOnce(&mut ContributionTotals)) {
        match self.central_ledger.iter_mut().find(|e| e.account == account) {
            Some(entry) => f(&mut entry.totals),
            None => {
                let mut totals = ContributionTotals::default();
                f(&mut totals);
                self.central_ledger.push(LedgerEntry { account, totals });
            }
        }
    }

    /// `account`'s central-world contribution totals, or zero if they have
    /// none recorded yet (never having placed/staffed anything there).
    pub fn ledger_for(&self, account: i64) -> ContributionTotals {
        self.central_ledger
            .iter()
            .find(|e| e.account == account)
            .map(|e| e.totals)
            .unwrap_or_default()
    }

    /// The single source of truth for command authority, shared by the server
    /// (enforcement), the sim (`apply_command`) and the client (UI greying).
    /// Unknown players (unit tests, trusted local calls) and owner-less worlds
    /// are unrestricted so co-op never dead-ends when the owner leaves.
    pub fn can_issue(&self, pid: u64, cmd: &PlayerCommand) -> bool {
        // The central world has no owner/guest hierarchy at all — authority
        // follows settler ownership. This branch must come before the
        // unknown-player and no-owner fallbacks below: both are unrestricted,
        // and an owner-less shared map full of strangers is exactly where
        // "anyone commands anyone's settlers" must never happen.
        if self.central {
            let account = self.player_account(pid);
            return match cmd {
                // Anyone may add to the shared city; tearing down is limited
                // to whoever placed it. Account-owned central buildings (set
                // from the builder's account at placement time) require that
                // SAME account — any of its connections, any session — while
                // legacy central buildings from before account ownership
                // existed (`owner_account: None`, migration default) stay
                // demolishable by the placing session, as they always were.
                PlayerCommand::Place { .. } => true,
                PlayerCommand::Demolish { building } => {
                    self.find_building(*building).is_some_and(|b| match b.owner_account {
                        Some(owner_acc) => account == Some(owner_acc),
                        None => b.owner == Some(pid),
                    })
                }
                // Only your own settlers, by account identity.
                PlayerCommand::AssignSurvivor { survivor, .. } => {
                    account.is_some()
                        && self
                            .survivors
                            .iter()
                            .find(|s| s.id == *survivor)
                            .is_some_and(|s| s.owner == account)
                }
                // The anonymous +/- pool can't tell whose settler it would
                // move, so it has no meaning where every settler is owned.
                PlayerCommand::AdjustWorkers { .. } => false,
                // Communal fixtures and personal-world progression have no
                // per-player authority in the central world.
                PlayerCommand::SetFurnaceLevel { .. }
                | PlayerCommand::InvestTunnel
                | PlayerCommand::Research { .. }
                | PlayerCommand::RespondEvent { .. } => false,
            };
        }
        let Some(p) = self.player(pid) else {
            return true;
        };
        if p.role == Role::Owner || !self.owner_present() {
            return true;
        }
        match self.guest_perm {
            GuestPermission::Full => true,
            GuestPermission::ViewOnly => false,
            GuestPermission::Build => match cmd {
                // Building, worker assignment and the shared Tunnel/research
                // goals are the cooperative core, so guests may all pitch in.
                PlayerCommand::Place { .. }
                | PlayerCommand::AdjustWorkers { .. }
                | PlayerCommand::AssignSurvivor { .. }
                | PlayerCommand::InvestTunnel
                | PlayerCommand::Research { .. }
                | PlayerCommand::RespondEvent { .. } => true,
                // Guests may only tear down their own buildings, never touch the
                // furnace, under the Build policy.
                PlayerCommand::Demolish { building } => self
                    .find_building(*building)
                    .map(|b| b.owner == Some(pid))
                    .unwrap_or(false),
                PlayerCommand::SetFurnaceLevel { .. } => false,
            },
        }
    }

    pub fn total_workers(&self) -> u32 {
        self.buildings.iter().map(|b| b.workers as u32).sum()
    }

    pub fn idle_workers(&self) -> u32 {
        (self.survivors.len() as u32).saturating_sub(self.total_workers())
    }

    pub fn housing_capacity(&self) -> usize {
        self.buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::Tent)
            .count()
            * TENT_CAPACITY
    }

    /// Chebyshev distance from a tile's center to the furnace center.
    pub fn dist_to_furnace(x: u8, y: u8) -> f32 {
        let (fx, fy) = Self::furnace_center();
        let dx = (x as f32 + 0.5 - fx).abs();
        let dy = (y as f32 + 0.5 - fy).abs();
        dx.max(dy)
    }

    /// Full placement validation, shared by client preview and server authority.
    pub fn can_place(&self, kind: BuildingKind, x: u8, y: u8) -> Result<(), &'static str> {
        if !kind.buildable() {
            return Err("That cannot be built");
        }
        let (w, h) = kind.size();
        if x as usize + w as usize > MAP_W || y as usize + h as usize > MAP_H {
            return Err("Out of bounds");
        }
        for dy in 0..h {
            for dx in 0..w {
                let (tx, ty) = (x + dx, y + dy);
                if self.building_at(tx, ty).is_some() {
                    return Err("Space is occupied");
                }
                let tile = self.tile(tx, ty);
                match kind {
                    BuildingKind::CoalMine => {
                        if tile.terrain != Terrain::Coal || tile.deposit == 0 {
                            return Err("Needs a coal deposit");
                        }
                    }
                    _ => {
                        if tile.terrain != Terrain::Snow {
                            return Err("Ground must be clear");
                        }
                    }
                }
            }
        }
        if self.stock.wood < kind.cost_wood() as f32 {
            return Err("Not enough wood");
        }
        Ok(())
    }

    /// How many forest units are harvestable within `r` tiles of (x, y) —
    /// used by the client to warn about badly placed sawmills.
    pub fn forest_near(&self, x: u8, y: u8, r: i32) -> u32 {
        let mut total = 0u32;
        for dy in -r..=r {
            for dx in -r..=r {
                let (tx, ty) = (x as i32 + dx, y as i32 + dy);
                if in_bounds(tx, ty) {
                    let t = &self.tiles[tile_index(tx as u8, ty as u8)];
                    if t.terrain == Terrain::Forest {
                        total += t.deposit as u32;
                    }
                }
            }
        }
        total
    }
}
