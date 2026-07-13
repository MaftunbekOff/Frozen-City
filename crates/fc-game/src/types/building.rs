//! Buildings: the kinds players can place and a placed building instance.

use serde::{Deserialize, Serialize};

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
