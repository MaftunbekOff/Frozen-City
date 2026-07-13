use bevy::prelude::*;

use frozen_city::game::types::BuildingKind;

#[derive(Component, Clone, Copy, PartialEq)]
pub enum HudField {
    Wood,
    Coal,
    Food,
    Pop,
    Clock,
    Temp,
    Furnace,
    Events,
    /// V0.7: colony morale, banded to match `GameState::morale_multiplier`'s
    /// four tiers, plus a short "Mourning -15%" indicator while the city
    /// mourns a dead leader (`GameState::mourning_active`).
    Morale,
}

#[derive(Component)]
pub struct TooltipText;

#[derive(Component)]
pub struct BuildBtn(pub BuildingKind);

#[derive(Component)]
pub struct FurnaceLvlBtn(pub u8);

/// Marks interactive UI containers so world clicks/zoom are suppressed there.
#[derive(Component)]
pub struct UiBlocker;

/// Resting color for generically hover-styled buttons — now part of the
/// design system; re-exported so existing `ui::BaseColor` paths keep working.
pub use super::super::theme::BaseColor;

#[derive(Component)]
pub struct SelPanelRoot;

#[derive(Component, Clone, Copy, PartialEq)]
pub enum SelText {
    Title,
    Info,
    Count,
}

#[derive(Component)]
pub struct WorkerRow;

#[derive(Component)]
pub struct WorkerMinus;

#[derive(Component)]
pub struct WorkerPlus;

#[derive(Component)]
pub struct DemolishBtn;

/// Assigns the survivor currently selected in the roster panel (`roster.rs`)
/// to the building selected here. Only visible/enabled when both a building
/// and a roster survivor are selected — logic lives in `roster.rs` since it
/// needs `SurvivorSelection`, but the button is part of this panel's layout.
#[derive(Component)]
pub struct AssignHereBtn;

#[derive(Component)]
pub struct AssignHereLabel;

#[derive(Component)]
pub struct GameOverRoot;

#[derive(Component, Clone, Copy, PartialEq)]
pub enum GoText {
    Title,
    Info,
}

#[derive(Component)]
pub struct GameOverBack;

/// Game-over overlay: step through the freshly opened Tunnel into the
/// central world. Shown only on a graduated win, and only for account
/// sessions (a guest has no account to own settlers with).
#[derive(Component)]
pub struct EnterCentralBtn;

#[derive(Component)]
pub struct QuitToMenuBtn;

/// Top-bar world switch: "Global World" in a graduated personal world,
/// "My City" in the central world, hidden otherwise (guests, ungraduated).
#[derive(Component)]
pub struct WorldSwitchBtn;

#[derive(Component)]
pub struct WorldSwitchLabel;

/// The brief center-screen "Entering the Global World..." banner (see
/// `TransitionMsg`).
#[derive(Component)]
pub struct TransitionText;

#[derive(Component)]
pub struct FpsText;
