//! Survivor roster: a modal panel (toggled with `P`) listing every named
//! `Survivor`, idle ones first, letting the player select one and assign
//! them to a specific building (`AssignSurvivor`) — distinct from the
//! anonymous `AdjustWorkers` +/- already on the building selection panel.
//! Structured like `research.rs`'s modal; the "Assign here" button itself
//! lives on `ui.rs`'s `SelPanelRoot` (the building panel) since it needs a
//! building to be selected there too, but its visibility/label/click logic
//! lives here since it also needs `SurvivorSelection`.
//!
//! Also owns the survivor detail card (V0.7): a small always-available panel
//! (distinct from the roster modal, which is toggled with P) shown whenever
//! `SurvivorSelection` is set — from a roster row click OR a world click on a
//! survivor (`input::resolve_world_click`) — with the survivor's stats and
//! Make Leader / Unassign actions.

mod card;
mod components;
mod panel;

use bevy::prelude::*;

use frozen_city::game::types::{Survivor, XP_DAYS_LEVEL_1, XP_DAYS_LEVEL_2, XP_DAYS_LEVEL_3};

use super::i18n::Lang;
use super::i18n_names;
use super::Screen;

use card::*;
use panel::*;
pub use components::*;

/// Client-side mirror of `fc_game::sim::xp_level` (private to the sim crate):
/// XP level (0..=3) from accrued in-game work-days, thresholded by the same
/// public `XP_DAYS_LEVEL_*` cumulative-total constants the sim uses.
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

/// Short "Profession Lx" tag used by both the roster rows and the detail
/// card, e.g. "Miner L2".
pub fn profession_level_tag(s: &Survivor, l: Lang) -> String {
    format!("{} L{}", i18n_names::profession_name(s.profession, l), xp_level(s.xp))
}

pub fn plugin(app: &mut App) {
    app.init_resource::<RosterOpen>()
        .init_resource::<SurvivorSelection>()
        .add_systems(OnEnter(Screen::Game), (spawn_roster, spawn_card))
        .add_systems(
            Update,
            (
                toggle_roster,
                update_roster,
                roster_row_buttons,
                unassign_buttons,
                update_assign_here,
                assign_here_button,
                update_card,
                card_buttons,
            )
                .run_if(in_state(Screen::Game)),
        );
}
