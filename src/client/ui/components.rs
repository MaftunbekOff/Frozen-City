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

/// Row of furnace-level buttons inside the selection panel; shown only while
/// the Furnace is the selected building (see `selection_panel_update`), so the
/// furnace level is adjusted from the furnace's own click-info.
#[derive(Component)]
pub struct FurnaceRow;

// --- Build modal (corner button + centered modal) ---

/// The Build modal's scrim root (despawn + UI-blocker + visibility
/// target — see `build_panel_visibility`).
#[derive(Component)]
pub struct BuildPanelRoot;

/// The bottom-right corner's single always-visible button (brass medallion
/// with a hammer glyph) — toggles the Build/Manage modal.
#[derive(Component)]
pub struct BuildPanelToggleBtn;

/// Whether the Build modal is open (`build_panel_toggle` writes,
/// `build_panel_visibility` applies; picking a building closes it so the
/// world is visible for placement).
#[derive(Resource, Default)]
pub struct BuildPanelOpen(pub bool);

/// The build menu's deny/feedback line — "not enough wood" when the player
/// clicks a tile they can't afford (`build_buttons` writes it, empty when
/// there's nothing to say).
#[derive(Component)]
pub struct BuildDenyText;

/// A build tile's cost badge text; `build_buttons` turns it danger-red while
/// the building is unaffordable so the reason is visible before clicking.
#[derive(Component)]
pub struct BuildCostBadge(pub BuildingKind);

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
    /// "Bo'sh ishchi: N" — the colony's idle-worker pool, shown inside the
    /// workforce section so the player can see headroom without checking the
    /// top bar (mirrors the "N available" column in the reference layout).
    Avail,
    /// V0.8: "Daraja 3/10" (bitgan bino) yoki "Qurilmoqda: 64% ustalar 2/3"
    /// (maydoncha); pech/buildable-bo'lmaganlarda bo'sh.
    Level,
    /// V0.8: Yangilash tugmasining yorlig'i — "Yangilash → L4 (50 yog'och)"
    /// yoki maksimal darajada mos matn.
    Upgrade,
}

/// V0.8: raise the selected building one level (`UpgradeBuilding`). Shown
/// only for finished, buildable, below-max buildings; its color is owned by
/// `selection_panel_update` (affordability), so no `BaseColor` hover.
#[derive(Component)]
pub struct UpgradeBtn;

#[derive(Component)]
pub struct WorkerRow;

#[derive(Component)]
pub struct WorkerMinus;

#[derive(Component)]
pub struct WorkerPlus;

/// Workforce quick-set: drop the building to zero anonymous workers. The
/// server clamps to the named-assignment floor (`sim::command`'s
/// `AdjustWorkers` arm), so this can never evict named survivors.
#[derive(Component)]
pub struct WorkerNoneBtn;

/// Workforce quick-set: fill the building to `max_workers` (server caps the
/// raise by the idle pool, so over-asking is safe).
#[derive(Component)]
pub struct WorkerMaxBtn;

/// The bottom-center morale bar's fill node — width driven as a percent of
/// `GameState::morale`, color by the same four tiers `hud_update` uses.
#[derive(Component)]
pub struct MoraleBarFill;

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
