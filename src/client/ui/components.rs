use bevy::prelude::*;

use frozen_city::game::types::{BuildingKind, TradeGood};

#[derive(Component, Clone, Copy, PartialEq)]
pub enum HudField {
    Wood,
    Coal,
    Food,
    /// V0.11: colony water stockpile — mirrors `Food`'s chip exactly, since
    /// thirst is now as core a survival need as hunger.
    Water,
    /// Raw hides from hunting, spent by a staffed Tailor Shop. Held in the
    /// stockpile since V0.10 but never surfaced in the HUD until the card
    /// redesign gave the top bar room for the full set.
    Fur,
    /// Tailored from `fur`; grants the colony a passive warmth bonus.
    Cloth,
    /// V0.13: gold earned/spent trading through the Tunnel — see
    /// [`CaravanBtn`].
    Gold,
    Pop,
    Clock,
    Temp,
    Furnace,
    Events,
    /// V0.7: colony morale, banded to match `GameState::morale_multiplier`'s
    /// four tiers, plus a short "Mourning -15%" indicator while the city
    /// mourns a dead leader (`GameState::mourning_active`).
    Morale,
    /// V0.17: compact "Kasal N   Holdan toygan M" alert (`GameState::
    /// sick_count`/`exhausted_count`) — empty text, so effectively invisible,
    /// while both counts are zero.
    Health,
}

#[derive(Component)]
pub struct TooltipText;

#[derive(Component)]
pub struct BuildBtn(pub BuildingKind);

#[derive(Component)]
pub struct FurnaceLvlBtn(pub u8);

/// V0.13: one quick trade-caravan action — dispatches `QUICK_TRADE_AMOUNT` of
/// `good`, selling if `selling` else buying. Row shown only while the Tunnel
/// is selected and unlocked (see `selection_panel_update`).
#[derive(Component)]
pub struct CaravanBtn {
    pub good: TradeGood,
    pub selling: bool,
}

// --- Build modal (corner button + centered modal) ---

/// The Build modal's scrim root (despawn + UI-blocker + visibility
/// target — see `build_panel_visibility`).
#[derive(Component)]
pub struct BuildPanelRoot;

/// The bottom-right corner's single always-visible button (brass medallion
/// with a hammer glyph) — toggles the Build/Manage modal.
#[derive(Component)]
pub struct BuildPanelToggleBtn;

/// Close button inside the Build dock's own header. The corner medallion above
/// used to be the only way out, but once the menu was docked along the bottom
/// of the screen it covers that corner — so the panel needs a dismiss control
/// of its own, not just the `Esc`/`B` keys (which mobile has no access to).
#[derive(Component)]
pub struct BuildPanelCloseBtn;

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
    /// Survivors tab so the player can see headroom without checking the
    /// top bar (mirrors the "N available" column in the reference layout).
    Avail,
    /// V0.8: "Qurilmoqda: 64% ustalar 2/3" while a construction site is
    /// unfinished; empty once the building is standing (the header's own
    /// "{Name} Lv. N" already says the level — see `panel_header`).
    Level,
    /// V0.8: the header's Upgrade button label — "Yangilash → L4 (50
    /// yog'och)", the `furnish_first_btn` explainer, or the max-level text.
    Upgrade,
    /// V0.21: Furniture tab tile `slot`'s corner level badge ("L2", or "-"
    /// while that fitting hasn't been bought yet) — an index into the
    /// selected building's `kind.furnishings()`.
    TileBadge(u8),
    /// V0.21: Furniture tab tile `slot`'s one/two-letter fitting glyph
    /// (`furnishing_glyph`, not localized — a visual mark, not a word, same
    /// convention as `BuildingKind::letter()`).
    TileGlyph(u8),
    /// V0.21: detail card header — the SELECTED fitting's name + level
    /// status (`i18n_furnishing::furnishing_header`).
    FurnName,
    /// V0.21: detail card's one-line description of the selected fitting.
    FurnDesc,
    /// V0.21: detail card's buy/upgrade button label (cost over current
    /// stock, or the max-level text).
    FurnUpgrade,
    /// V0.21: stats grid cell — the selected fitting's production cycle
    /// output, or a dash when it isn't the room's producer.
    FurnStatProduction,
    /// V0.21: stats grid cell — resources the cycle spends per run (only the
    /// Tailor Shop's Workbench has one today; a dash everywhere else).
    FurnStatConsumption,
    /// V0.21: stats grid cell — the fitting's own per-level effect (e.g. a
    /// Heater's fatigue reduction, Shelving's XP bonus).
    FurnStatStats,
    /// V0.21: stats grid cell — the production cycle's duration, with the
    /// next level's delta ("7.8s -0.2s") when there is a next level.
    FurnStatTime,
    /// V0.21: Survivors tab portrait slot `slot`'s initial-letter label,
    /// empty while that slot is unfilled or past capacity.
    SurvivorInitial(u8),
}

/// V0.21: which half of the selection panel's Furniture/Survivors tab strip
/// is showing — the reference design's two-tab layout. A component on
/// `SelPanelRoot`'s own entity (not a `Resource`: adding one means
/// registering it in `ClientPlugin::build`, `src/client/mod.rs`, which this
/// panel does not own) so a pick survives frame to frame. `TabBtn`/`TabRoot`
/// tag the matching button/content-root entities with the same value so
/// `selection_panel_update`/`selection_panel_buttons` can compare by it.
#[derive(Component, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelTab {
    #[default]
    Furniture,
    Survivors,
}

/// One Furniture/Survivors tab button. `selection_panel_buttons` writes `.0`
/// into the `SelTab` component on `SelPanelRoot` when clicked;
/// `selection_panel_update` colors it active/inactive and hides it entirely
/// when the selected building has nothing for that tab to show.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct TabBtn(pub SelTab);

/// The Furniture/Survivors tab's own content area — visible while the
/// `SelTab` on `SelPanelRoot` equals `.0` AND that tab applies to the
/// selected building.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct TabRoot(pub SelTab);

/// V0.21: which Furniture-tab tile's detail card is showing — an index into
/// the selected building's `kind.furnishings()`. A component on
/// `SelPanelRoot`'s entity, same reasoning as `SelTab`.
#[derive(Component, Clone, Copy, PartialEq, Eq, Default)]
pub struct SelFurnSlot(pub u8);

/// V0.21: one piece of the Furniture tab's content. `Tile(slot)` is the
/// selectable strip entry (visible while `slot` exists on this building,
/// highlighted while it equals `SelFurnSlot`'s current pick); `UpgradeBtn`
/// is the detail card's own buy/upgrade button (visible while the panel's
/// Furniture tab applies at all, colored by afford/maxed exactly like the
/// header's `UpgradeBtn`). Bundles both roles under one component type —
/// `selection_panel_update`'s `&mut Node` `ParamSet` is at Bevy's 8-tuple
/// cap (see that fn's doc comment), so a marker per role would not fit.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum FurnitureCard {
    Tile(u8),
    UpgradeBtn,
}

/// V0.21: one portrait slot in the Survivors tab's roster strip. Up to 3 are
/// pre-spawned — the highest of any `BuildingKind::max_workers()`/
/// `CONSTRUCTION_CREW_MAX`. `Root(slot)` is the slot's own circle (visible
/// while `slot` is within the building's current worker capacity, tinted by
/// the assigned survivor's profession color while filled); `Lock(slot)` is
/// the padlock glyph drawn inside it (visible only while that slot is within
/// capacity but UNFILLED). Bundled for the same `ParamSet`-cap reason as
/// `FurnitureCard`.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum SurvivorSlot {
    Root(u8),
    Lock(u8),
}

/// V0.8: raise the selected building one level (`UpgradeBuilding`). Lives in
/// the panel's header, next to the title. Shown only for finished,
/// buildable, below-max buildings; its color is owned by
/// `selection_panel_update` (affordability), so no `BaseColor` hover.
#[derive(Component)]
pub struct UpgradeBtn;

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

/// V0.21: the panel's per-kind extra controls that live OUTSIDE the two tabs
/// — Furnace burn-level buttons, Tunnel caravan quick-trade, Relocate,
/// Demolish, and the Survivors tab's anonymous −/count/+ (+ None/Max) block
/// (hidden in the central world, where only named `AssignSurvivor` works —
/// see `AssignHereBtn`). Each is a plain Node-visibility toggle with no
/// color logic of its own here (the buttons INSIDE `FurnaceControls`/
/// `CaravanControls` style themselves — `furnace_buttons`/`caravan_buttons`
/// in `buildbar.rs`), so bundling all five under one component/query is a
/// pure win against the `ParamSet` cap, not a compromise.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum PanelAction {
    Demolish,
    Relocate,
    FurnaceControls,
    CaravanControls,
    WorkerAdjustControls,
}

// V0.16's `RotateBuildingBtn` (a "Rotate" button in the selection panel,
// sending `PlayerCommand::RotateBuilding`) was removed in V0.18: turning a
// building is not a separate decision from placing it, and having both
// buttons meant the panel asked "where?" and "which way?" in two places. Both
// now live in the relocate flow's confirm bar (`placement::RotateBtn`), which
// commits them together as one `RelocateFacing`. The COMMAND is still there
// and still tested — only this button is gone.

/// Assigns the survivor currently selected in the roster panel (`roster.rs`)
/// to the building selected here. Only visible/enabled when both a building
/// and a roster survivor are selected — logic lives in `roster.rs` since it
/// needs `SurvivorSelection`, but the button is part of this panel's layout
/// (nested inside the Survivors tab, so it only shows alongside the portrait
/// strip it fills).
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
