//! V0.18: the book of laws — standing colony policies, each a genuine
//! trade-off rather than a straight upgrade.
//!
//! Unlike [`super::Tech`] (bought once with resources, pure upside, permanent),
//! a law costs nothing but is always paying for its benefit with a matching
//! cost somewhere else, and can be repealed. Effects are exposed as pure
//! multiplier queries on `GameState` (`law_production_multiplier` and
//! friends) so the sim reads them the same way it reads tech and morale
//! multipliers — there is no per-law special-casing scattered through
//! `sim::tick`.

use serde::{Deserialize, Serialize};

/// A standing policy. Bincode serializes this enum positionally, so new
/// variants must be APPENDED at the end — an inserted variant would silently
/// re-label every enacted law in every persisted world.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Law {
    /// Longer shifts: more output, faster exhaustion.
    LongShifts,
    /// Bigger portions: happier city, more food burned.
    ExtraRations,
    /// Everyone indoors early: better rest, less done.
    Curfew,
    /// One pot for the whole city: thriftier meals, but the cooks are pulled
    /// off everything else.
    CommunalMeals,
    /// Isolate the sick: contagion slows, the city works short-handed.
    Quarantine,
    /// Time to train properly: faster XP, slower output.
    Apprenticeship,
    /// Bury the dead with rites: a death costs the city less spirit, but the
    /// funeral itself costs wood.
    FuneralRites,
}

impl Law {
    pub const ALL: [Law; 7] = [
        Law::LongShifts,
        Law::ExtraRations,
        Law::Curfew,
        Law::CommunalMeals,
        Law::Quarantine,
        Law::Apprenticeship,
        Law::FuneralRites,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Law::LongShifts => "Long Shifts",
            Law::ExtraRations => "Extra Rations",
            Law::Curfew => "Curfew",
            Law::CommunalMeals => "Communal Meals",
            Law::Quarantine => "Quarantine",
            Law::Apprenticeship => "Apprenticeship",
            Law::FuneralRites => "Funeral Rites",
        }
    }

    /// The upside, in the player's words.
    pub fn benefit(self) -> &'static str {
        match self {
            Law::LongShifts => "Everyone works longer. More gets done.",
            Law::ExtraRations => "Fuller plates. The city's spirits lift.",
            Law::Curfew => "Everyone rests properly at night.",
            Law::CommunalMeals => "One pot feeds all. Less food is wasted.",
            Law::Quarantine => "The sick are kept apart. Illness spreads slowly.",
            Law::Apprenticeship => "Workers are taught, not just used. They learn fast.",
            Law::FuneralRites => "The dead are honoured. A loss cuts less deep.",
        }
    }

    /// The price, in the player's words.
    pub fn cost(self) -> &'static str {
        match self {
            Law::LongShifts => "People tire much faster.",
            Law::ExtraRations => "The city eats noticeably more.",
            Law::Curfew => "Less is done during the day.",
            Law::CommunalMeals => "Cooking for all costs some output.",
            Law::Quarantine => "The city works short-handed.",
            Law::Apprenticeship => "Teaching slows the work down.",
            Law::FuneralRites => "Each funeral burns wood.",
        }
    }

    /// Colony-wide production multiplier this law contributes (1.0 = none).
    /// Composed multiplicatively with every other enacted law in
    /// `GameState::law_production_multiplier`.
    pub fn production_factor(self) -> f32 {
        match self {
            Law::LongShifts => 1.15,
            Law::Curfew => 0.93,
            Law::CommunalMeals => 0.96,
            Law::Quarantine => 0.95,
            Law::Apprenticeship => 0.94,
            _ => 1.0,
        }
    }

    /// Food-consumption multiplier (1.0 = none).
    pub fn food_factor(self) -> f32 {
        match self {
            Law::ExtraRations => 1.22,
            Law::CommunalMeals => 0.88,
            _ => 1.0,
        }
    }

    /// Fatigue-accrual multiplier while awake (1.0 = none).
    pub fn fatigue_factor(self) -> f32 {
        match self {
            Law::LongShifts => 1.35,
            _ => 1.0,
        }
    }

    /// Overnight fatigue-recovery multiplier (1.0 = none).
    pub fn rest_factor(self) -> f32 {
        match self {
            Law::Curfew => 1.30,
            _ => 1.0,
        }
    }

    /// Per-day contagion-chance multiplier (1.0 = none).
    pub fn contagion_factor(self) -> f32 {
        match self {
            Law::Quarantine => 0.35,
            _ => 1.0,
        }
    }

    /// XP-accrual multiplier (1.0 = none).
    pub fn xp_factor(self) -> f32 {
        match self {
            Law::Apprenticeship => 1.60,
            _ => 1.0,
        }
    }

    /// Multiplier on the morale hit a death costs the colony (1.0 = none).
    pub fn death_morale_factor(self) -> f32 {
        match self {
            Law::FuneralRites => 0.5,
            _ => 1.0,
        }
    }

    /// Flat per-day morale adjustment while enacted.
    ///
    /// V0.18 balance note: the two positive terms here (`ExtraRations` and
    /// `CommunalMeals`) can both be enacted alongside `LongShifts` at once
    /// (`MAX_ACTIVE_LAWS` is 3) — their sum, 1.5, is deliberately kept BELOW
    /// `LongShifts`' -2.0, so that specific "production law + every morale
    /// law that fits" combo still nets a real, ongoing morale cost instead of
    /// paying for its own downside and then some. Before this was tuned,
    /// `LongShifts` (-1.0) + `ExtraRations` (+2.0) alone already net a
    /// POSITIVE morale swing on top of `LongShifts`' production bonus — i.e.
    /// enacting both bought more output, faster training (add
    /// `Apprenticeship`, itself morale-neutral) AND rising morale, with
    /// nothing but food/fatigue (both absorbable with enough Kitchens/Tents)
    /// standing against it. Any three-law book that includes `LongShifts`
    /// must still cost the colony something on this axis.
    pub fn morale_per_day(self) -> f32 {
        match self {
            Law::ExtraRations => 1.0,
            Law::CommunalMeals => 0.5,
            Law::Curfew => -0.5,
            Law::LongShifts => -2.0,
            _ => 0.0,
        }
    }

    /// Wood burned per funeral under this law (0.0 for every other law).
    pub fn funeral_wood(self) -> f32 {
        match self {
            Law::FuneralRites => 4.0,
            _ => 0.0,
        }
    }
}
