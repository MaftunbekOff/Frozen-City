use bevy::prelude::*;

/// Whether the roster modal is open (also gates world/camera input).
#[derive(Resource, Default)]
pub struct RosterOpen(pub bool);

/// The survivor currently selected in the roster, if any. Distinct from
/// `Selection` (building-only, shared by desktop input and touch) — a
/// building and a survivor can be selected at the same time, e.g. while
/// assigning one to the other.
#[derive(Resource, Default)]
pub struct SurvivorSelection(pub Option<u32>);

#[derive(Component)]
pub struct RosterRoot;

#[derive(Component)]
pub struct RosterTitle;

#[derive(Component)]
pub struct RosterSubtitle;

#[derive(Component)]
pub struct RosterRow(pub usize);

#[derive(Component)]
pub struct RosterName(pub usize);

#[derive(Component)]
pub struct RosterStatus(pub usize);

/// Clicking selects `survivor` (or deselects if already selected). Kept in
/// sync with the current sorted roster order every frame in `update_roster`,
/// so the click handler never has to reconstruct the sort itself.
#[derive(Component)]
pub struct RosterRowBtn {
    pub row: usize,
    pub survivor: Option<u32>,
}

#[derive(Component)]
pub struct UnassignBtn {
    pub row: usize,
    pub survivor: Option<u32>,
}

#[derive(Component)]
pub struct MoreText;

// ------------------------------------------------------------ detail card

#[derive(Component)]
pub struct CardRoot;

#[derive(Component, Clone, Copy, PartialEq)]
pub enum CardText {
    Name,
    Stats,
}

#[derive(Component)]
pub struct CardCloseBtn;

#[derive(Component)]
pub struct CardLeaderBtn;

#[derive(Component)]
pub struct CardLeaderLabel;

#[derive(Component)]
pub struct CardUnassignBtn;

#[derive(Component)]
pub struct CardUnassignLabel;

/// V0.17: fatigue-bar fill inside the detail card — width driven as a
/// percent of `Survivor::fatigue`, color banded by the same three tiers the
/// card's fatigue text line uses. Mirrors `ui::MoraleBarFill`.
#[derive(Component)]
pub struct CardFatigueFill;
