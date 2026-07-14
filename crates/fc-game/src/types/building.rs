//! Buildings: the kinds players can place and a placed building instance.

use serde::{Deserialize, Serialize};

use super::{
    BUILDING_MAX_LEVEL, BUILD_WORKDAYS_PER_WOOD, LEVEL_PRODUCTION_BONUS, TENT_CAPACITY,
    TENT_CAPACITY_PER_LEVEL,
};

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
    /// V0.9: the Tunnel to the Global World — a single always-present,
    /// non-buildable fixture (same convention as `Furnace`), placed once by
    /// `mapgen`. Its excavation state lives in `GameState.tunnel`
    /// (`TunnelState`), not on this `Building` (`level`/`build_left` are
    /// unused/inert for it — see `render/buildings.rs`).
    Tunnel,
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
            BuildingKind::Tunnel => "Tunnel",
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
            BuildingKind::Tunnel => "U",
        }
    }

    pub fn cost_wood(self) -> u32 {
        match self {
            // Never actually spent on `Place` (neither kind is `buildable`),
            // but the Furnace's value here still matters: it's the base
            // `upgrade_cost_wood`/`upgrade_workdays` scale for its (V0.9)
            // level 1-10 upgrade path (see `upgradeable`). The Tunnel has no
            // such path, so it stays 0.
            BuildingKind::Furnace => 50,
            BuildingKind::Tunnel => 0,
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
            BuildingKind::Furnace | BuildingKind::Tent | BuildingKind::Tunnel => 0,
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
            BuildingKind::Furnace | BuildingKind::Tunnel => (2, 2),
            _ => (1, 1),
        }
    }

    pub fn buildable(self) -> bool {
        !matches!(self, BuildingKind::Furnace | BuildingKind::Tunnel)
    }

    /// V0.9: whether `UpgradeBuilding` accepts this kind — distinct from
    /// `buildable` (which gates `Place`/`Demolish`). The Furnace is the one
    /// exception: never placeable or demolishable (a single permanent
    /// fixture), but still growable — level 1-6 stays a modest "gulxan"
    /// (campfire), 7-10 becomes an established "Pech" (see
    /// `render/buildings.rs`'s two-tier model). The Tunnel has no such path.
    pub fn upgradeable(self) -> bool {
        self.buildable() || self == BuildingKind::Furnace
    }

    /// V0.8: worker-days of construction to erect this building at level 1.
    pub fn build_workdays(self) -> f32 {
        self.cost_wood() as f32 * BUILD_WORKDAYS_PER_WOOD
    }

    /// V0.8: wood price of upgrading TO `next_level` (2..=BUILDING_MAX_LEVEL).
    /// Scales with the target level so late levels are a real wood sink.
    pub fn upgrade_cost_wood(self, next_level: u8) -> u32 {
        (self.cost_wood() * next_level as u32).div_ceil(2)
    }

    /// V0.8: worker-days of construction an upgrade TO `next_level` takes.
    pub fn upgrade_workdays(self, next_level: u8) -> f32 {
        self.build_workdays() * next_level as f32 * 0.5
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
            BuildingKind::Tunnel => "The way to the Global World. Travelers pass through once it's open.",
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
    /// V0.8: building level, 1..=`BUILDING_MAX_LEVEL`. Upgrades go one level
    /// at a time (`UpgradeBuilding`); migration default for old saves is 1.
    pub level: u8,
    /// V0.8: remaining construction work in worker-days; > 0 while this is an
    /// unfinished site (a fresh placement or an in-progress upgrade). The
    /// assigned crew works it down each tick and the building produces
    /// nothing / houses nobody until it reaches 0.
    pub build_left: f32,
}

impl Building {
    /// True while this is an unfinished construction site (new or upgrading).
    pub fn under_construction(&self) -> bool {
        self.build_left > 0.0
    }

    /// Level-scaled output/effect multiplier: 1.0 at level 1,
    /// +`LEVEL_PRODUCTION_BONUS` per level above it.
    pub fn level_factor(&self) -> f32 {
        1.0 + LEVEL_PRODUCTION_BONUS * self.level.saturating_sub(1) as f32
    }

    /// Housing slots this building contributes (Tents only; a site under
    /// construction shelters nobody). Grows with level.
    pub fn housing_slots(&self) -> usize {
        if self.kind != BuildingKind::Tent || self.under_construction() {
            return 0;
        }
        TENT_CAPACITY + TENT_CAPACITY_PER_LEVEL * self.level.saturating_sub(1) as usize
    }

    /// Sanity guard used by upgrade validation.
    pub fn at_max_level(&self) -> bool {
        self.level >= BUILDING_MAX_LEVEL
    }
}
