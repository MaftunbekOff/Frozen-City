//! V0.18: expeditions — sending a small party off the map for several days.
//!
//! An expedition is the personal world's one *outward* activity: the map
//! itself is a fixed 64x64 valley with no fog of war and nothing left to
//! discover once it's built out, so the only way to add "somewhere else" is
//! to let people leave for a while and come back changed. Party members are
//! REMOVED from `GameState.survivors` for the duration (stored inside the
//! [`Expedition`] itself, the same shape `sim::extract_migrants` uses for the
//! Tunnel) rather than staying in the roster with an "away" flag: every
//! per-tick system — hunger, thirst, warmth, fatigue, meals, movement,
//! housing — would otherwise need its own "unless they're away" branch, and
//! one forgotten branch means a survivor freezing to death in a place the
//! game says they aren't. Out of the vec, out of every loop, by construction.

use serde::{Deserialize, Serialize};

use super::{Survivor, TICKS_PER_DAY};

/// Where a party can be sent. Each site is a different (duration, danger,
/// payout) point rather than a strict upgrade of the previous one, so the
/// choice stays a real trade-off at every stage of a colony's life.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExpeditionSite {
    /// Close, short, safe, modest: the tutorial expedition.
    FrozenWoods,
    /// Coal-heavy, medium risk — the answer to a fuel crisis.
    AbandonedMine,
    /// Long and dangerous, but the only site that can bring PEOPLE back
    /// (survivors found in the ruins) alongside goods.
    RuinedTown,
    /// Longest and coldest; food/fur heavy, and the site most likely to
    /// send people home sick.
    IceFields,
}

impl ExpeditionSite {
    pub const ALL: [ExpeditionSite; 4] = [
        ExpeditionSite::FrozenWoods,
        ExpeditionSite::AbandonedMine,
        ExpeditionSite::RuinedTown,
        ExpeditionSite::IceFields,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ExpeditionSite::FrozenWoods => "Frozen Woods",
            ExpeditionSite::AbandonedMine => "Abandoned Mine",
            ExpeditionSite::RuinedTown => "Ruined Town",
            ExpeditionSite::IceFields => "Ice Fields",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            ExpeditionSite::FrozenWoods => "A short trip. Wood and a little food.",
            ExpeditionSite::AbandonedMine => "Coal, if the tunnels hold.",
            ExpeditionSite::RuinedTown => "Far and dangerous. Survivors still live there.",
            ExpeditionSite::IceFields => "The longest road. Furs and meat, and the cold.",
        }
    }

    /// In-game days the round trip takes.
    pub fn days(self) -> f32 {
        match self {
            ExpeditionSite::FrozenWoods => 1.0,
            ExpeditionSite::AbandonedMine => 2.0,
            ExpeditionSite::RuinedTown => 3.0,
            ExpeditionSite::IceFields => 4.0,
        }
    }

    /// Round-trip duration in ticks, derived from [`Self::days`] so the two
    /// can never drift apart.
    pub fn trip_ticks(self) -> u64 {
        (self.days() * TICKS_PER_DAY as f32) as u64
    }

    /// Per-member, per-day chance something goes badly wrong out there
    /// (injury — see `HAZARD_HP_LOSS` — or illness on the way home).
    pub fn hazard_per_member_day(self) -> f32 {
        match self {
            ExpeditionSite::FrozenWoods => 0.03,
            ExpeditionSite::AbandonedMine => 0.09,
            ExpeditionSite::RuinedTown => 0.14,
            ExpeditionSite::IceFields => 0.17,
        }
    }

    /// Food taken along per member per day — deducted from the stockpile up
    /// front at launch, so an expedition is a real commitment rather than a
    /// free lottery ticket (and a starving colony simply can't launch one).
    pub fn provisions_per_member_day(self) -> f32 {
        // Above `FOOD_PER_SURVIVOR_DAY` (1.2): travelling in the cold with
        // everything on your back costs more than sitting by the furnace.
        2.0
    }

    /// Base haul per member per day, before the party's own skill/level
    /// multipliers (see `sim::expedition`). Fields are wood/coal/food/fur —
    /// the four raw goods a party can plausibly carry home.
    pub fn haul_per_member_day(self) -> ExpeditionHaul {
        match self {
            ExpeditionSite::FrozenWoods => ExpeditionHaul { wood: 9.0, coal: 0.0, food: 2.0, fur: 0.5 },
            ExpeditionSite::AbandonedMine => ExpeditionHaul { wood: 1.0, coal: 11.0, food: 0.0, fur: 0.0 },
            ExpeditionSite::RuinedTown => ExpeditionHaul { wood: 5.0, coal: 4.0, food: 3.0, fur: 1.0 },
            ExpeditionSite::IceFields => ExpeditionHaul { wood: 0.0, coal: 2.0, food: 8.0, fur: 3.5 },
        }
    }

    /// Per-expedition chance the party brings a stranger home with them.
    /// Only the Ruined Town has anyone left to find.
    pub fn rescue_chance(self) -> f32 {
        match self {
            ExpeditionSite::RuinedTown => 0.45,
            _ => 0.0,
        }
    }
}

/// What a party carried home. Also used as the per-member-per-day base rate
/// on [`ExpeditionSite::haul_per_member_day`].
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct ExpeditionHaul {
    pub wood: f32,
    pub coal: f32,
    pub food: f32,
    pub fur: f32,
}

/// A party currently away from the colony. `party` holds the actual
/// [`Survivor`] records (see the module doc for why they leave the roster),
/// so an expedition is self-contained: nothing outside it can accidentally
/// tick, feed, freeze or kill someone who isn't in the valley.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Expedition {
    pub id: u32,
    pub site: ExpeditionSite,
    /// The travellers themselves, lifted out of `GameState.survivors` at
    /// launch and put back on return.
    pub party: Vec<Survivor>,
    pub departed_tick: u64,
    /// Tick the party walks back in. Shortened by a recall (see `recalled`).
    pub return_tick: u64,
    /// Set by `PlayerCommand::RecallExpedition`: the party turns around
    /// early, keeping only the haul earned so far.
    pub recalled: bool,
}

impl Expedition {
    /// Whole in-game days spent out so far, capped at the site's full trip —
    /// what the haul is actually paid on (a recalled party is paid for the
    /// road it walked, not the road it planned).
    pub fn days_travelled(&self, now: u64) -> f32 {
        let elapsed = now.saturating_sub(self.departed_tick) as f32 / TICKS_PER_DAY as f32;
        elapsed.clamp(0.0, self.site.days())
    }

    /// 0.0..=1.0 progress toward the return, for the client's panel.
    pub fn progress(&self, now: u64) -> f32 {
        let total = self.return_tick.saturating_sub(self.departed_tick).max(1) as f32;
        (now.saturating_sub(self.departed_tick) as f32 / total).clamp(0.0, 1.0)
    }

    pub fn is_due(&self, now: u64) -> bool {
        now >= self.return_tick
    }
}
