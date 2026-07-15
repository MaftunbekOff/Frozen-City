//! Personal-world progression: missions, the Tunnel megaproject, and the
//! cooperative technology tree.

use serde::{Deserialize, Serialize};

// --- V0.3: missions & the Tunnel (personal-world progression) ---

/// Each variant carries its completion target. Progress is derived from the
/// live [`super::GameState`] (see [`super::GameState::mission_current`]), so
/// missions never need per-tick counters and stay deterministic.
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
/// `InvestTunnel` commands until `stage` reaches [`super::TUNNEL_STAGES`].
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
            Tech::Rationing => "The city eats and drinks more carefully.",
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
