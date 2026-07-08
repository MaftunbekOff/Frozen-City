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
pub const MAX_POPULATION: i32 = 60;

pub const FOOD_PER_SURVIVOR_DAY: f32 = 1.2;
pub const FURNACE_COAL_PER_DAY_PER_LEVEL: f32 = 12.0;
/// Wood burns less efficiently than coal.
pub const WOOD_FUEL_PENALTY: f32 = 1.5;
pub const DEMOLISH_REFUND: f32 = 0.4;
pub const TENT_CAPACITY: usize = 4;

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
}

impl BuildingKind {
    pub const BUILDABLE: [BuildingKind; 4] = [
        BuildingKind::Tent,
        BuildingKind::Sawmill,
        BuildingKind::CoalMine,
        BuildingKind::HunterHut,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BuildingKind::Furnace => "Furnace",
            BuildingKind::Tent => "Tent",
            BuildingKind::Sawmill => "Sawmill",
            BuildingKind::CoalMine => "Coal Mine",
            BuildingKind::HunterHut => "Hunter's Hut",
        }
    }

    pub fn letter(self) -> &'static str {
        match self {
            BuildingKind::Furnace => "F",
            BuildingKind::Tent => "T",
            BuildingKind::Sawmill => "S",
            BuildingKind::CoalMine => "C",
            BuildingKind::HunterHut => "H",
        }
    }

    pub fn cost_wood(self) -> u32 {
        match self {
            BuildingKind::Furnace => 0,
            BuildingKind::Tent => 15,
            BuildingKind::Sawmill => 25,
            BuildingKind::CoalMine => 30,
            BuildingKind::HunterHut => 25,
        }
    }

    pub fn max_workers(self) -> u8 {
        match self {
            BuildingKind::Furnace | BuildingKind::Tent => 0,
            BuildingKind::Sawmill => 2,
            BuildingKind::CoalMine => 3,
            BuildingKind::HunterHut => 2,
        }
    }

    /// Units produced per worker per in-game day.
    pub fn production_per_worker_day(self) -> f32 {
        match self {
            BuildingKind::Sawmill => 12.0,
            BuildingKind::CoalMine => 15.0,
            BuildingKind::HunterHut => 10.0,
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
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Survivor {
    pub id: u32,
    pub name: String,
    /// 0..=100; death at 0.
    pub hp: f32,
    /// 0..=120; starvation damage above 80.
    pub hunger: f32,
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
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GamePhase {
    Running,
    Won,
    Lost,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GameEvent {
    pub day: u32,
    pub text: String,
}

/// Commands a player may issue; the server validates every one of them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum PlayerCommand {
    Place { kind: BuildingKind, x: u8, y: u8 },
    Demolish { building: u32 },
    AdjustWorkers { building: u32, delta: i8 },
    SetFurnaceLevel { level: u8 },
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
    pub next_id: u32,
    /// SplitMix64 RNG state.
    pub rng: u64,
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
        base + diurnal + snap
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
