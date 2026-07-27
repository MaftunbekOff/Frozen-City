//! V0.20: building interiors — the fittings inside a workplace.
//!
//! Before this, a building was a single number: `Building::level`, bought
//! outright with wood. That made "improving the Kitchen" one anonymous click
//! with no texture to it. Interiors break that one number into the things a
//! room actually contains — a workbench, seating, a heater, shelving — each
//! bought and improved separately, each wired to a DIFFERENT system, so
//! furnishing a room is a series of small, distinguishable decisions rather
//! than a resource sink with a cosmetic result.
//!
//! Deliberately a SHARED, small vocabulary rather than a bespoke list per
//! building: four kinds, drawn from by every staffed building
//! ([`super::BuildingKind::furnishings`]). A Kitchen's seating and a
//! Hospital's seating are the same idea and should not be two enum variants,
//! two names and two translations apiece — the interesting variety is in
//! WHICH fittings a room takes, not in inventing forty nouns.

use serde::{Deserialize, Serialize};

use super::{BuildingKind, TICKS_PER_DAY};

/// V0.21: one fitting's production cycle — what a single run of it yields,
/// what it costs to run, and how long a run takes. This is the shape the
/// building panel reads directly ("Production 1 / Consumption 6 / Time
/// 7.8s"), so the number on screen and the number the sim applies are the
/// same value rather than two descriptions of one rule.
///
/// Which RESOURCE a cycle yields is not stored here: that stays a property of
/// the building (`BuildingKind::production_per_worker_day`'s resource, applied
/// in `sim::tick`), because the deposit-capping around it — a Sawmill eating
/// nearby forest, a Coal Mine its tile, a Hunter's Hut the wildlife — belongs
/// to the building too. A fitting sets the RATE, not the trade.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FurnishingCycle {
    /// Units produced by one completed cycle.
    pub output: f32,
    /// Ticks one cycle takes. Falls as the fitting is upgraded — which is
    /// exactly the "Time 7.8s −0.2s" the panel shows on an upgrade button.
    pub ticks: f32,
}

impl FurnishingCycle {
    /// Throughput in units per in-game day, per worker — the form
    /// `sim::tick`'s production loop actually wants.
    pub fn per_day(&self) -> f32 {
        if self.ticks <= 0.0 {
            return 0.0;
        }
        self.output * TICKS_PER_DAY as f32 / self.ticks
    }

    /// Seconds of real time per cycle, for display (`TICK_MS` is 200 ms).
    pub fn seconds(&self) -> f32 {
        self.ticks * super::TICK_MS as f32 / 1000.0
    }
}

/// One kind of fitting. Bincode serializes this enum positionally, so new
/// variants must be APPENDED at the end.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FurnishingKind {
    /// Proper tools and a surface to use them on: the building simply does
    /// its job better (production, or a staffed effect's strength).
    Workbench,
    /// Tables and chairs — somewhere to sit down. Lifts colony morale a
    /// little every day the building is staffed.
    Seating,
    /// A stove in the corner. The people working HERE tire more slowly.
    Heater,
    /// Shelves, records, a place to keep the craft: workers assigned here
    /// learn their trade faster (XP).
    Shelving,
}

impl FurnishingKind {
    pub const ALL: [FurnishingKind; 4] = [
        FurnishingKind::Workbench,
        FurnishingKind::Seating,
        FurnishingKind::Heater,
        FurnishingKind::Shelving,
    ];

    pub fn name(self) -> &'static str {
        match self {
            FurnishingKind::Workbench => "Workbench",
            FurnishingKind::Seating => "Table and chairs",
            FurnishingKind::Heater => "Stove",
            FurnishingKind::Shelving => "Shelving",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            FurnishingKind::Workbench => "Proper tools. The work goes better.",
            FurnishingKind::Seating => "Somewhere to sit. The city's spirits lift a little.",
            FurnishingKind::Heater => "Warmth at the workplace. People here tire slower.",
            FurnishingKind::Shelving => "A place for the craft. Workers here learn faster.",
        }
    }

    /// Wood to bring this fitting from `level - 1` to `level` (1..=
    /// [`super::FURNISHING_MAX_LEVEL`]). Rises with the level so a fully
    /// fitted room is a real investment, but the first of each is cheap —
    /// the point is that furnishing a new building is something you can start
    /// immediately, not a late-game project.
    pub fn cost_wood(self, level: u8) -> f32 {
        let base = match self {
            FurnishingKind::Workbench => 10.0,
            FurnishingKind::Seating => 6.0,
            FurnishingKind::Heater => 8.0,
            FurnishingKind::Shelving => 7.0,
        };
        base * level.max(1) as f32
    }

    /// V0.21: this fitting's production cycle inside `building` at `level`,
    /// or `None` when it isn't the thing that produces there.
    ///
    /// Only the `Workbench` produces, and only in a building that yields a
    /// resource at all — it IS the stove, the saw bench, the pit head. That
    /// makes a bare room genuinely idle: a Sawmill with no workbench has
    /// nobody's tools to cut with and yields nothing, which is the whole
    /// point of furnishing a workplace rather than it being a bonus on top.
    ///
    /// The rate is derived from `production_per_worker_day` so a level-1
    /// workbench reproduces exactly the throughput that building always had,
    /// and each level shortens the cycle by `per_level()` — the same +8%
    /// per level as before, now expressed as time rather than as a hidden
    /// multiplier. Nothing about the balance moved; only its shape did.
    pub fn cycle(self, building: BuildingKind, level: u8) -> Option<FurnishingCycle> {
        if self != FurnishingKind::Workbench || level == 0 {
            return None;
        }
        let per_day = building.production_per_worker_day();
        if per_day <= 0.0 {
            return None;
        }
        // One whole unit per cycle: the panel reads "Production 1", and the
        // cycle time carries all the variation between trades.
        let output = 1.0;
        let base_ticks = output * TICKS_PER_DAY as f32 / per_day;
        // Level 1 restores exactly the throughput the building always had —
        // buying the first workbench is what makes the trade work at all, not
        // what makes it better than normal. Each level above that is the
        // familiar +8%.
        let speed = 1.0 + self.per_level() * (level.saturating_sub(1)) as f32;
        Some(FurnishingCycle { output, ticks: base_ticks / speed })
    }

    /// Per-level strength of this fitting's effect. Each kind reads its own
    /// number against a different system, so these are not comparable with
    /// each other — see the call sites in `sim::tick`.
    pub fn per_level(self) -> f32 {
        match self {
            // +8% to this building's output/effect per level (max +24%).
            FurnishingKind::Workbench => 0.08,
            // +0.4 morale/day per level while the building is staffed.
            FurnishingKind::Seating => 0.4,
            // -7% fatigue accrual per level for survivors assigned here.
            FurnishingKind::Heater => 0.07,
            // +15% XP per level for survivors assigned here.
            FurnishingKind::Shelving => 0.15,
        }
    }
}
