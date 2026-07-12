//! HUD, build bar, selection panel and the game-over overlay.

use bevy::prelude::*;

use frozen_city::game::types::{
    BuildingKind, GamePhase, FURNACE_COAL_PER_DAY_PER_LEVEL,
};
use frozen_city::net::protocol::ClientMsg;
use frozen_city::game::types::PlayerCommand;

use super::i18n::Lang;
use super::i18n_hud;
use super::i18n_names;
use super::theme::{
    self, BG_PANEL, BORDER, BTN, BTN_ACTIVE, BTN_DANGER, BTN_DIM, BTN_HOVER, FS_BODY, FS_MICRO,
    FS_SMALL, FS_TITLE, RES_COAL, RES_FOOD, RES_WOOD, SP_MD, SP_SM, SP_XS, TEXT_MUTED,
    TEXT_PRIMARY,
};
use super::*;

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
pub use super::theme::BaseColor;

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

/// Convenience wrapper around [`theme::button`] for the common case of a
/// fixed pixel width, so call sites below don't all spell `Val::Px(..)`.
fn btn_px(w: f32, h: f32, bg: Color) -> impl Bundle {
    theme::button(Val::Px(w), h, bg)
}

pub fn spawn_hud(mut commands: Commands, ff: Res<theme::FormFactor>, lang: Res<Lang>) {
    let ff = *ff;
    let lang = *lang;

    // --- Top bar: one compact row on Desktop/Tablet; two rows on Mobile so
    // nothing gets clipped at phone widths (resources up top, status +
    // Menu below). Chosen once at spawn time, like every other `FormFactor`
    // layout in this codebase (see `theme::modal_panel`) — a live resize
    // mid-session just waits for the next HUD (re)spawn to pick up the
    // other layout, same tradeoff `theme::FormFactor`'s doc comment states.
    if ff.compact() {
        spawn_top_bar_mobile(&mut commands, lang);
    } else {
        spawn_top_bar_desktop(&mut commands, lang);
    }

    // --- Bottom build bar ---
    let build_bar_height = if ff.compact() { 96.0 } else { 88.0 };
    let mut build_bar = commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            bottom: Val::Px(0.0),
            height: Val::Px(build_bar_height),
            padding: UiRect::axes(Val::Px(SP_MD), Val::Px(SP_SM)),
            align_items: AlignItems::Center,
            column_gap: Val::Px(SP_SM),
            // Mobile: the bar no longer shrinks to fit every button (see
            // `touch::fit_ui_scale`'s widened pivot) — instead it scrolls
            // horizontally so all 8 buildings + 4 furnace levels stay at a
            // comfortably tappable size.
            overflow: if ff.compact() {
                Overflow::scroll_x()
            } else {
                Overflow::clip()
            },
            border: UiRect::top(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(BG_PANEL),
        BorderColor::all(BORDER),
        Interaction::default(),
        UiBlocker,
        DespawnOnExit(Screen::Game),
    ));
    if ff.compact() {
        build_bar.insert(ScrollPosition::default());
    }
    build_bar.with_children(|p| {
        let btn_h = if ff.compact() { 46.0_f32.max(ff.btn_h()) } else { 62.0 };
        for (i, kind) in BuildingKind::BUILDABLE.into_iter().enumerate() {
            p.spawn((
                Button,
                Node {
                    width: Val::Px(92.0),
                    height: Val::Px(btn_h),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(3.0),
                    flex_shrink: 0.0,
                    ..default()
                },
                BackgroundColor(BTN),
                BaseColor(BTN),
                BuildBtn(kind),
            ))
            .with_children(|b| {
                b.spawn(theme::text(i18n_names::building_name(kind, lang), FS_MICRO + 0.5, TEXT_PRIMARY));
                // Hotkey hint only makes sense where a physical keyboard
                // exists — hidden on Mobile.
                let hotkey = if ff.compact() { None } else { Some(i + 1) };
                b.spawn(theme::text(
                    i18n_hud::build_cost_badge(kind.cost_wood(), hotkey, lang),
                    FS_MICRO - 1.0,
                    TEXT_MUTED,
                ));
            });
        }
        p.spawn(Node {
            flex_grow: 1.0,
            min_width: Val::Px(if ff.compact() { SP_SM } else { 0.0 }),
            ..default()
        });
        p.spawn((
            theme::text(i18n_hud::furnace_level_label(lang), FS_SMALL, TEXT_MUTED),
            Node {
                flex_shrink: 0.0,
                ..default()
            },
        ));
        for lvl in 0u8..=3 {
            // Level 0 ("Off"/"O'chiq"/"Выкл") is a word, not a single digit —
            // a fixed 42px box clips/overflows it into the neighboring "1"
            // button, so it gets auto-width (like the other text-buttons in
            // this design system) with the same horizontal padding; the
            // numeric 1-3 buttons stay fixed-width squares since a single
            // digit always fits.
            let node = if lvl == 0 {
                Node {
                    width: Val::Auto,
                    height: Val::Px(btn_h.min(48.0).max(if ff.compact() { ff.btn_h() } else { 40.0 })),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    padding: UiRect::horizontal(Val::Px(SP_SM)),
                    flex_shrink: 0.0,
                    ..default()
                }
            } else {
                Node {
                    width: Val::Px(42.0),
                    height: Val::Px(btn_h.min(48.0).max(if ff.compact() { ff.btn_h() } else { 40.0 })),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    flex_shrink: 0.0,
                    ..default()
                }
            };
            p.spawn((Button, node, BackgroundColor(BTN), BaseColor(BTN), FurnaceLvlBtn(lvl)))
                .with_children(|b| {
                    let label = if lvl == 0 {
                        i18n_hud::furnace_off_label(lang).to_string()
                    } else {
                        lvl.to_string()
                    };
                    b.spawn(theme::text(label, FS_BODY - 1.0, TEXT_PRIMARY));
                });
        }
    });

    // --- Tooltip / hint line just above the build bar ---
    let hint = if ff.compact() {
        i18n_hud::default_hint_mobile(lang)
    } else {
        i18n_hud::default_hint_desktop(lang)
    };
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(14.0),
            bottom: Val::Px(build_bar_height + 4.0),
            ..default()
        },
        theme::text(hint, FS_SMALL, TEXT_MUTED),
        TooltipText,
        DespawnOnExit(Screen::Game),
    ));

    // --- World-switch transition banner: a big, brief, center-screen line
    // ("Entering the Global World...") that fades out once the new world has
    // actually loaded (see `TransitionMsg`). Deliberately not part of the
    // top bar's fixed-position rows so it can't collide with any of them.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(160.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        theme::text("", FS_TITLE + 6.0, TEXT_PRIMARY),
        TransitionText,
        DespawnOnExit(Screen::Game),
    ));

    // --- FPS readout (below the top bar) ---
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(14.0),
            top: Val::Px(54.0),
            ..default()
        },
        theme::text("", FS_SMALL, TEXT_MUTED),
        FpsText,
        DespawnOnExit(Screen::Game),
    ));

    // --- Event feed (right side) --- (capped narrower on Mobile, with a
    // smaller font, so it can't collide with the minimap on the opposite
    // side of a phone-width screen)
    let events_font = if ff.compact() { FS_MICRO } else { FS_SMALL };
    let mut events_node = Node {
        position_type: PositionType::Absolute,
        right: Val::Px(12.0),
        top: Val::Px(54.0),
        padding: UiRect::all(Val::Px(SP_SM)),
        border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
        ..default()
    };
    if ff.compact() {
        events_node.max_width = Val::Percent(52.0);
    } else {
        events_node.width = Val::Px(340.0);
    }
    commands
        .spawn((
            events_node,
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.25)),
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn((theme::text("", events_font, Color::srgba(0.85, 0.90, 1.0, 0.9)), HudField::Events));
        });

    // --- Selection panel --- (kept clear of the build bar on Mobile, where
    // the bar is taller and the panel would otherwise overlap it)
    let sel_bottom = if ff.compact() { build_bar_height + 12.0 } else { 100.0 };
    commands
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                right: Val::Px(12.0),
                bottom: Val::Px(sel_bottom),
                width: Val::Px(260.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(SP_SM),
                padding: UiRect::all(Val::Px(SP_MD)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(theme::RAD_PANEL)),
                ..default()
            },
            BackgroundColor(BG_PANEL),
            BorderColor::all(BORDER),
            Interaction::default(),
            UiBlocker,
            SelPanelRoot,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn((theme::section("Building"), SelText::Title));
            p.spawn((theme::text("", FS_MICRO + 1.0, TEXT_MUTED), SelText::Info));
            p.spawn((
                Node {
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(10.0),
                    ..default()
                },
                WorkerRow,
            ))
            .with_children(|row| {
                row.spawn((btn_px(34.0, 30.0, BTN), WorkerMinus))
                    .with_children(|b| {
                        b.spawn(theme::text("-", FS_BODY + 1.0, TEXT_PRIMARY));
                    });
                row.spawn((theme::text("0/0", FS_BODY, TEXT_PRIMARY), SelText::Count));
                row.spawn((btn_px(34.0, 30.0, BTN), WorkerPlus))
                    .with_children(|b| {
                        b.spawn(theme::text("+", FS_BODY + 1.0, TEXT_PRIMARY));
                    });
            });
            p.spawn((
                Button,
                Node {
                    display: Display::None,
                    width: Val::Px(236.0),
                    height: Val::Px(30.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(BTN),
                BaseColor(BTN),
                AssignHereBtn,
            ))
            .with_children(|b| {
                b.spawn((theme::text("", FS_MICRO + 1.0, TEXT_PRIMARY), AssignHereLabel));
            });
            p.spawn((
                btn_px(220.0, 30.0, BTN_DANGER),
                DemolishBtn,
            ))
            .with_children(|b| {
                b.spawn(theme::text(i18n_hud::demolish_label(lang), FS_SMALL, TEXT_PRIMARY));
            });
        });

    // --- Game over overlay --- (starts as `theme::scrim`'s default `Flex`
    // for a single `OnEnter`->`Update` transition, but `game_over_ui` runs
    // later in that same frame and immediately corrects it to `None` since
    // the phase always starts `Running` — nothing is ever actually presented
    // in between, so no flash).
    commands
        .spawn((
            theme::scrim(ff),
            GameOverRoot,
            UiBlocker,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn(theme::modal_panel(ff)).with_children(|panel| {
                panel.spawn((
                    theme::text("", FS_TITLE + 8.0, TEXT_PRIMARY),
                    Node {
                        align_self: AlignSelf::Center,
                        ..default()
                    },
                    GoText::Title,
                ));
                panel.spawn((theme::text("", FS_BODY + 1.0, TEXT_MUTED), GoText::Info));
                let central_h = ff.btn_h().max(46.0);
                panel
                    .spawn((
                        theme::button(Val::Percent(100.0), central_h, Color::srgb(0.13, 0.35, 0.45)),
                        EnterCentralBtn,
                    ))
                    .with_children(|b| {
                        b.spawn(theme::text(i18n_hud::enter_global_world_btn(lang), FS_BODY, TEXT_PRIMARY));
                    });
                panel
                    .spawn((theme::button(Val::Percent(100.0), central_h, BTN), GameOverBack))
                    .with_children(|b| {
                        b.spawn(theme::text(i18n_hud::return_to_menu_btn(lang), FS_BODY, TEXT_PRIMARY));
                    });
            });
        });
}

/// Desktop/Tablet top bar: a single row (unchanged layout from before the
/// design-system pass, just theme colors + localized text).
fn spawn_top_bar_desktop(commands: &mut Commands, lang: Lang) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                height: Val::Px(46.0),
                padding: UiRect::horizontal(Val::Px(14.0)),
                align_items: AlignItems::Center,
                column_gap: Val::Px(20.0),
                border: UiRect::bottom(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(BG_PANEL),
            BorderColor::all(BORDER),
            Interaction::default(),
            UiBlocker,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn((theme::text(i18n_hud::hud_wood(0, lang), FS_BODY, RES_WOOD), HudField::Wood));
            p.spawn((theme::text(i18n_hud::hud_coal(0, lang), FS_BODY, RES_COAL), HudField::Coal));
            p.spawn((theme::text(i18n_hud::hud_food(0, lang), FS_BODY, RES_FOOD), HudField::Food));
            p.spawn((theme::text(i18n_hud::hud_pop(0, 0, lang), FS_BODY, TEXT_PRIMARY), HudField::Pop));
            p.spawn(Node {
                flex_grow: 1.0,
                ..default()
            });
            p.spawn((theme::text(i18n_hud::hud_clock(1, 1, 6, 0, lang), FS_BODY, TEXT_PRIMARY), HudField::Clock));
            p.spawn((theme::text("-0 C", FS_BODY, Color::srgb(0.55, 0.80, 0.95)), HudField::Temp));
            p.spawn(Node {
                flex_grow: 1.0,
                ..default()
            });
            p.spawn((theme::text("Furnace L1", FS_BODY, TEXT_PRIMARY), HudField::Furnace));
            p.spawn((theme::text("Morale --", FS_BODY, TEXT_PRIMARY), HudField::Morale));
            p.spawn((
                Button,
                Node {
                    width: Val::Px(110.0),
                    height: Val::Px(30.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    // Hidden until `world_switch_button` decides this session
                    // may switch worlds at all.
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.13, 0.30, 0.40)),
                BaseColor(Color::srgb(0.13, 0.30, 0.40)),
                WorldSwitchBtn,
            ))
            .with_children(|b| {
                b.spawn((theme::text(i18n_hud::world_switch_global(lang), FS_SMALL, TEXT_PRIMARY), WorldSwitchLabel));
            });
            p.spawn((btn_px(70.0, 30.0, BTN), QuitToMenuBtn))
                .with_children(|b| {
                    b.spawn(theme::text(i18n_hud::menu_button(lang), FS_SMALL, TEXT_PRIMARY));
                });
        });
}

/// A single-line HUD text node: `flex_shrink: 0.0` so a tight mobile row
/// never compresses it below its natural content width (which is what forces
/// a wrap mid-word, e.g. "Yog'och" and its number landing on separate lines).
fn hud_text_mobile(t: impl Into<String>, color: Color) -> impl Bundle {
    (
        theme::text(t, FS_MICRO, color),
        // NoWrap ham shart: flex_shrink 0 taffy'ga tegishli, lekin matn
        // o'lchagichi baribir mavjud enga qarab ichki o'rashga ruxsat beradi
        // ("Yog'och" va soni alohida qatorlarga tushib qolardi).
        TextLayout::new(Justify::Left, LineBreak::NoWrap),
        Node {
            flex_shrink: 0.0,
            ..default()
        },
    )
}

/// Mobile top bar: two compact rows instead of one wide one, so nothing
/// spills off-screen at phone widths. Row 1: the three resources + pop.
/// Row 2: clock, temperature, furnace, morale and the Menu button
/// (world-switch stays inside row 2 too, right next to Menu, since it's
/// rarely both visible and it's still just a single extra slot). Every field
/// is `FS_MICRO` + non-shrinking (see `hud_text_mobile`) and each row wraps
/// (`FlexWrap::Wrap`) as a last resort, so at a 390px-wide phone (UiScale
/// 0.8) both rows fit without any single field wrapping mid-text or the two
/// rows overlapping each other.
fn spawn_top_bar_mobile(commands: &mut Commands, lang: Lang) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                height: Val::Auto,
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(Val::Px(SP_SM), Val::Px(SP_XS)),
                row_gap: Val::Px(SP_XS),
                border: UiRect::bottom(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(BG_PANEL),
            BorderColor::all(BORDER),
            Interaction::default(),
            UiBlocker,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            // Row 1: resources + population.
            p.spawn(Node {
                height: Val::Auto,
                align_items: AlignItems::Center,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(SP_SM),
                row_gap: Val::Px(SP_XS),
                ..default()
            })
            .with_children(|row| {
                row.spawn((hud_text_mobile(i18n_hud::hud_wood(0, lang), RES_WOOD), HudField::Wood));
                row.spawn((hud_text_mobile(i18n_hud::hud_coal(0, lang), RES_COAL), HudField::Coal));
                row.spawn((hud_text_mobile(i18n_hud::hud_food(0, lang), RES_FOOD), HudField::Food));
                row.spawn(Node {
                    flex_grow: 1.0,
                    ..default()
                });
                row.spawn((hud_text_mobile(i18n_hud::hud_pop(0, 0, lang), TEXT_PRIMARY), HudField::Pop));
            });
            // Row 2: clock, temp, furnace, morale, world-switch, Menu.
            p.spawn(Node {
                height: Val::Auto,
                align_items: AlignItems::Center,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(SP_SM),
                row_gap: Val::Px(SP_XS),
                ..default()
            })
            .with_children(|row| {
                row.spawn((hud_text_mobile(i18n_hud::hud_clock(1, 1, 6, 0, lang), TEXT_PRIMARY), HudField::Clock));
                row.spawn((hud_text_mobile("-0 C", Color::srgb(0.55, 0.80, 0.95)), HudField::Temp));
                row.spawn(Node {
                    flex_grow: 1.0,
                    ..default()
                });
                row.spawn((hud_text_mobile("Furnace L1", TEXT_PRIMARY), HudField::Furnace));
                row.spawn((hud_text_mobile("Morale --", TEXT_PRIMARY), HudField::Morale));
                row.spawn((
                    Button,
                    Node {
                        width: Val::Px(96.0),
                        height: Val::Px(30.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        flex_shrink: 0.0,
                        display: Display::None,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.13, 0.30, 0.40)),
                    BaseColor(Color::srgb(0.13, 0.30, 0.40)),
                    WorldSwitchBtn,
                ))
                .with_children(|b| {
                    b.spawn((theme::text(i18n_hud::world_switch_global(lang), FS_MICRO, TEXT_PRIMARY), WorldSwitchLabel));
                });
                row.spawn((
                    Button,
                    Node {
                        width: Val::Px(60.0),
                        height: Val::Px(30.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        flex_shrink: 0.0,
                        ..default()
                    },
                    BackgroundColor(BTN),
                    BaseColor(BTN),
                    QuitToMenuBtn,
                ))
                .with_children(|b| {
                    b.spawn(theme::text(i18n_hud::menu_button(lang), FS_MICRO, TEXT_PRIMARY));
                });
            });
        });
}

pub fn track_ui_hover(mut hover: ResMut<UiHover>, q: Query<&Interaction>) {
    hover.0 = q.iter().any(|i| *i != Interaction::None);
}

#[derive(Component)]
pub struct FpsText;

pub fn fps_update(
    time: Res<Time>,
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
    quality: Res<Quality>,
    adapter: Option<Res<bevy::render::renderer::RenderAdapterInfo>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut q: Query<&mut Text, With<FpsText>>,
    mut accum: Local<f32>,
) {
    // Throttle to ~4Hz: the diagnostic itself is smoothed, so redoing this
    // format! every single frame just churns the allocator for no visible
    // benefit.
    *accum += time.delta_secs();
    if *accum < 0.25 {
        return;
    }
    *accum = 0.0;

    let Ok(mut t) = q.single_mut() else { return };
    let fps = diagnostics
        .get(&bevy::diagnostic::FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    // Also surface the graphics tier, the actual GPU backend in use (so
    // players/devs can see WebGPU vs the WebGL2 fallback at a glance) and the
    // backing resolution, so performance problems (esp. mobile-web fill
    // rate) can be diagnosed on-device.
    let tier = match *quality {
        Quality::Low => "Low",
        Quality::Medium => "Med",
        Quality::High => "High",
    };
    // wgpu's `Backend::to_str()` returns lowercase machine names ("gl",
    // "webgpu", "vulkan", ...); render the ones players will actually see friendlier.
    let backend = adapter.map(|a| match a.0.backend.to_str() {
        "webgpu" => "WebGPU".to_string(),
        "gl" => "WebGL2".to_string(),
        other => other.to_string(),
    });
    let mut s = format!("FPS {fps:.0}  |  {tier}");
    if let Some(b) = backend {
        s.push_str(&format!("  |  {b}"));
    }
    if let Ok(w) = windows.single() {
        s.push_str(&format!(
            "  |  {}x{}",
            w.resolution.physical_width(),
            w.resolution.physical_height()
        ));
    }
    if t.0 != s {
        t.0 = s;
    }
}

/// Last-displayed value per HUD field, so `hud_update` only pays for a
/// `format!` (and, for the events log, the sort/truncate/join) when the
/// underlying number actually changed instead of on every frame. `Lang` rides
/// along in every key (not its own field) so a language switch — which
/// doesn't change any of these numbers — still invalidates every cache entry
/// and refreshes the on-screen text.
#[derive(Default)]
pub(crate) struct HudCache {
    wood: Option<(i64, Lang)>,
    coal: Option<(i64, Lang)>,
    food: Option<(i64, Lang)>,
    pop: Option<(usize, u32, Lang)>,
    clock: Option<(u32, u32, u32, Lang)>,
    temp: Option<(i32, bool, Lang)>,
    furnace: Option<(u8, bool, Lang)>,
    events: Option<u64>,
    morale: Option<(i32, bool, Lang)>,
}

pub fn hud_update(
    view: Res<GameView>,
    lang: Res<Lang>,
    ff: Res<theme::FormFactor>,
    mut cache: Local<HudCache>,
    mut q: Query<(&mut Text, Option<&mut TextColor>, &HudField)>,
) {
    let Some(state) = view.state.as_ref() else { return };
    let lang = *lang;
    for (mut text, color, field) in &mut q {
        match field {
            HudField::Wood => {
                let v = state.stock.wood as i64;
                if cache.wood != Some((v, lang)) {
                    cache.wood = Some((v, lang));
                    text.0 = i18n_hud::hud_wood(v, lang);
                }
            }
            HudField::Coal => {
                let v = state.stock.coal as i64;
                if cache.coal != Some((v, lang)) {
                    cache.coal = Some((v, lang));
                    text.0 = i18n_hud::hud_coal(v, lang);
                }
            }
            HudField::Food => {
                let v = state.stock.food as i64;
                if cache.food != Some((v, lang)) {
                    cache.food = Some((v, lang));
                    text.0 = i18n_hud::hud_food(v, lang);
                }
            }
            HudField::Pop => {
                let (pop, idle) = (state.survivors.len(), state.idle_workers());
                if cache.pop != Some((pop, idle, lang)) {
                    cache.pop = Some((pop, idle, lang));
                    text.0 = i18n_hud::hud_pop(pop, idle, lang);
                }
            }
            HudField::Clock => {
                let mins = (state.time_of_day() * 24.0 * 60.0) as u32;
                let key = (state.day(), state.win_days, mins);
                if cache.clock != Some((key.0, key.1, key.2, lang)) {
                    cache.clock = Some((key.0, key.1, key.2, lang));
                    text.0 = i18n_hud::hud_clock(key.0, key.1, mins / 60, mins % 60, lang);
                }
            }
            HudField::Temp => {
                let temp = state.temperature();
                let key = (temp.round() as i32, state.cold_snap);
                if cache.temp != Some((key.0, key.1, lang)) {
                    cache.temp = Some((key.0, key.1, lang));
                    let snap = if state.cold_snap { i18n_hud::hud_cold_snap(lang) } else { "" };
                    text.0 = i18n_hud::hud_temp(temp, snap, lang);
                }
            }
            HudField::Furnace => {
                let key = (state.furnace_level, state.furnace_lit);
                if cache.furnace != Some((key.0, key.1, lang)) {
                    cache.furnace = Some((key.0, key.1, lang));
                    let out_of_fuel = !state.furnace_lit && state.furnace_level > 0;
                    if let Some(mut c) = color {
                        c.0 = if state.furnace_lit {
                            Color::srgb(0.95, 0.65, 0.30)
                        } else {
                            Color::srgb(0.95, 0.30, 0.25)
                        };
                    }
                    // Mobile: a short single-line form (see
                    // `i18n_hud::hud_furnace_short`'s doc) — the full status
                    // word would wrap at `FS_MICRO` on a phone-width bar and
                    // collide with the row below.
                    text.0 = if ff.compact() {
                        i18n_hud::hud_furnace_short(
                            state.furnace_level,
                            state.furnace_level as f32 * FURNACE_COAL_PER_DAY_PER_LEVEL,
                            out_of_fuel,
                            lang,
                        )
                    } else {
                        let status = if state.furnace_lit {
                            i18n_hud::furnace_status_burning(lang)
                        } else if state.furnace_level == 0 {
                            i18n_hud::furnace_status_off(lang)
                        } else {
                            i18n_hud::furnace_status_out_of_fuel(lang)
                        };
                        i18n_hud::hud_furnace(
                            state.furnace_level,
                            state.furnace_level as f32 * FURNACE_COAL_PER_DAY_PER_LEVEL,
                            status,
                            lang,
                        )
                    };
                }
            }
            HudField::Morale => {
                let mourning = state.mourning_active();
                let key = (state.morale.round() as i32, mourning);
                if cache.morale != Some((key.0, key.1, lang)) {
                    cache.morale = Some((key.0, key.1, lang));
                    // Four-tier band matching `GameState::morale_multiplier`'s
                    // thresholds exactly, so the HUD symbol always agrees with
                    // the actual production multiplier in effect.
                    let (tier, tier_color) = if state.morale < 25.0 {
                        (i18n_hud::morale_tier_critical(lang), Color::srgb(0.90, 0.30, 0.25))
                    } else if state.morale < 50.0 {
                        (i18n_hud::morale_tier_low(lang), Color::srgb(0.92, 0.62, 0.28))
                    } else if state.morale <= 75.0 {
                        (i18n_hud::morale_tier_steady(lang), Color::srgb(0.85, 0.88, 0.60))
                    } else {
                        (i18n_hud::morale_tier_high(lang), Color::srgb(0.55, 0.90, 0.50))
                    };
                    if let Some(mut c) = color {
                        c.0 = if mourning { Color::srgb(0.70, 0.55, 0.85) } else { tier_color };
                    }
                    let mourn_tag = if mourning { i18n_hud::hud_mourning_tag(lang) } else { "" };
                    text.0 = i18n_hud::hud_morale(state.morale, tier, mourn_tag, lang);
                }
            }
            HudField::Events => {
                if cache.events != Some(state.total_events) {
                    cache.events = Some(state.total_events);
                    // Show up to 8 lines, prioritising system events (deaths,
                    // weather, victory) over cosmetic ones so the server's
                    // eviction protection actually reaches the player's eyes;
                    // then display the chosen lines chronologically. Server
                    // event-stream text is NOT localized (see `HudField`
                    // doc) — only the "Day N:" framing lives client-side,
                    // and that's still plain since it's just a number.
                    let mut idx: Vec<usize> = (0..state.events.len()).collect();
                    idx.sort_by_key(|&i| {
                        (
                            std::cmp::Reverse(state.events[i].system),
                            std::cmp::Reverse(i),
                        )
                    });
                    idx.truncate(8);
                    idx.sort_unstable();
                    text.0 = idx
                        .iter()
                        .map(|&i| {
                            let e = &state.events[i];
                            format!("Day {}: {}", e.day, e.text)
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                }
            }
        }
    }
}

pub fn build_buttons(
    view: Res<GameView>,
    lang: Res<Lang>,
    ff: Res<theme::FormFactor>,
    mut build: ResMut<BuildMode>,
    clicked: Query<(&Interaction, &BuildBtn), Changed<Interaction>>,
    mut all: Query<(&Interaction, &BuildBtn, &mut BackgroundColor)>,
    mut tooltip: Query<&mut Text, With<TooltipText>>,
) {
    for (interaction, btn) in &clicked {
        if *interaction == Interaction::Pressed {
            build.0 = if build.0 == Some(btn.0) {
                None
            } else {
                Some(btn.0)
            };
        }
    }

    let wood = view
        .state
        .as_ref()
        .map(|s| s.stock.wood)
        .unwrap_or_default();
    let mut hovered: Option<BuildingKind> = None;
    for (interaction, btn, mut bg) in &mut all {
        let affordable = wood >= btn.0.cost_wood() as f32;
        if *interaction == Interaction::Hovered {
            hovered = Some(btn.0);
        }
        let color = if build.0 == Some(btn.0) {
            BTN_ACTIVE
        } else if *interaction == Interaction::Hovered {
            BTN_HOVER
        } else if !affordable {
            BTN_DIM
        } else {
            BTN
        };
        if bg.0 != color {
            bg.0 = color;
        }
    }

    if let Ok(mut tip) = tooltip.single_mut() {
        let lang = *lang;
        let new = match hovered.or(build.0) {
            Some(k) => i18n_hud::build_tooltip(
                i18n_names::building_name(k, lang),
                k.cost_wood(),
                i18n_names::building_desc(k, lang),
                lang,
            ),
            None if ff.compact() => i18n_hud::default_hint_mobile(lang).to_string(),
            None => i18n_hud::default_hint_desktop(lang).to_string(),
        };
        if tip.0 != new {
            tip.0 = new;
        }
    }
}

pub fn furnace_buttons(
    view: Res<GameView>,
    net: Res<NetConn>,
    clicked: Query<(&Interaction, &FurnaceLvlBtn), Changed<Interaction>>,
    mut all: Query<(&Interaction, &FurnaceLvlBtn, &mut BackgroundColor)>,
) {
    for (interaction, btn) in &clicked {
        if *interaction == Interaction::Pressed {
            net.send(ClientMsg::Cmd(PlayerCommand::SetFurnaceLevel { level: btn.0 }));
        }
    }
    let current = view
        .state
        .as_ref()
        .map(|s| s.furnace_level)
        .unwrap_or(1);
    for (interaction, btn, mut bg) in &mut all {
        let color = if btn.0 == current {
            BTN_ACTIVE
        } else if *interaction == Interaction::Hovered {
            BTN_HOVER
        } else {
            BTN
        };
        if bg.0 != color {
            bg.0 = color;
        }
    }
}

pub fn selection_panel_update(
    view: Res<GameView>,
    lang: Res<Lang>,
    mut selection: ResMut<Selection>,
    mut nodes: ParamSet<(
        Query<&mut Node, With<SelPanelRoot>>,
        Query<&mut Node, With<WorkerRow>>,
        Query<&mut Node, With<DemolishBtn>>,
    )>,
    mut texts: Query<(&mut Text, &SelText)>,
) {
    let lang = *lang;
    let Some(state) = view.ready() else { return };

    // Drop selection if the building disappeared.
    if let Some(id) = selection.0 {
        if state.find_building(id).is_none() {
            selection.0 = None;
        }
    }
    let sel = selection.0.and_then(|id| state.find_building(id)).cloned();

    let display = if sel.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut nodes.p0() {
        if node.display != display {
            node.display = display;
        }
    }
    let Some(b) = sel else { return };

    let has_workers = b.kind.max_workers() > 0;
    let workers_display = if has_workers { Display::Flex } else { Display::None };
    for mut node in &mut nodes.p1() {
        if node.display != workers_display {
            node.display = workers_display;
        }
    }
    let demolish_display = if b.kind.buildable() {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut nodes.p2() {
        if node.display != demolish_display {
            node.display = demolish_display;
        }
    }

    let info = match b.kind {
        BuildingKind::Furnace => i18n_hud::sel_info_furnace(
            state.furnace_level,
            state.furnace_level as f32 * FURNACE_COAL_PER_DAY_PER_LEVEL,
            frozen_city::game::types::WOOD_FUEL_PENALTY,
            state.heat_radius(),
            lang,
        ),
        BuildingKind::Tent => i18n_hud::sel_info_tent(
            state.housing_capacity(),
            state.survivors.len(),
            lang,
        ),
        BuildingKind::Sawmill => i18n_hud::sel_info_sawmill(
            b.kind.production_per_worker_day() * b.kind.max_workers() as f32,
            state.forest_near(b.x, b.y, 4),
            lang,
        ),
        BuildingKind::CoalMine => i18n_hud::sel_info_coal_mine(
            b.kind.production_per_worker_day() * b.kind.max_workers() as f32,
            state.tile(b.x, b.y).map_or(0, |t| t.deposit),
            lang,
        ),
        BuildingKind::HunterHut => i18n_hud::sel_info_hunter_hut(
            b.kind.production_per_worker_day() * b.kind.max_workers() as f32,
            lang,
        ),
        BuildingKind::Greenhouse => i18n_hud::sel_info_greenhouse(
            b.kind.production_per_worker_day() * b.kind.max_workers() as f32,
            lang,
        ),
        BuildingKind::Hospital => i18n_hud::sel_info_hospital(
            frozen_city::game::types::HOSPITAL_CARE_PER_WORKER_DAY,
            b.workers,
            b.workers as f32 * frozen_city::game::types::HOSPITAL_CARE_PER_WORKER_DAY,
            lang,
        ),
        BuildingKind::Kitchen => {
            let cut = (1.0 - frozen_city::game::types::KITCHEN_FOOD_EFFICIENCY) * 100.0;
            if b.workers > 0 {
                i18n_hud::sel_info_kitchen_staffed(cut, lang)
            } else {
                i18n_hud::sel_info_kitchen_unstaffed(cut, lang)
            }
        }
        BuildingKind::Warehouse => {
            let cut = (1.0 - frozen_city::game::types::WAREHOUSE_BUILD_DISCOUNT) * 100.0;
            if b.workers > 0 {
                i18n_hud::sel_info_warehouse_staffed(cut, lang)
            } else {
                i18n_hud::sel_info_warehouse_unstaffed(cut, lang)
            }
        }
    };

    for (mut text, kind) in &mut texts {
        let new = match kind {
            SelText::Title => i18n_names::building_name(b.kind, lang).to_string(),
            SelText::Info => info.clone(),
            SelText::Count => i18n_hud::worker_count(b.workers, b.kind.max_workers(), lang),
        };
        if text.0 != new {
            text.0 = new;
        }
    }
}

pub fn selection_panel_buttons(
    net: Res<NetConn>,
    mut selection: ResMut<Selection>,
    minus: Query<&Interaction, (Changed<Interaction>, With<WorkerMinus>)>,
    plus: Query<&Interaction, (Changed<Interaction>, With<WorkerPlus>)>,
    demolish: Query<&Interaction, (Changed<Interaction>, With<DemolishBtn>)>,
) {
    let Some(id) = selection.0 else { return };
    if minus.iter().any(|i| *i == Interaction::Pressed) {
        net.send(ClientMsg::Cmd(PlayerCommand::AdjustWorkers {
            building: id,
            delta: -1,
        }));
    }
    if plus.iter().any(|i| *i == Interaction::Pressed) {
        net.send(ClientMsg::Cmd(PlayerCommand::AdjustWorkers {
            building: id,
            delta: 1,
        }));
    }
    if demolish.iter().any(|i| *i == Interaction::Pressed) {
        net.send(ClientMsg::Cmd(PlayerCommand::Demolish { building: id }));
        selection.0 = None;
    }
}

pub fn game_over_ui(
    view: Res<GameView>,
    session: Res<Session>,
    lang: Res<super::i18n::Lang>,
    mut pending: ResMut<PendingSwitch>,
    mut transition: ResMut<TransitionMsg>,
    mut next: ResMut<NextState<Screen>>,
    mut root: Query<&mut Node, With<GameOverRoot>>,
    mut texts: Query<(&mut Text, &mut TextColor, &GoText)>,
    back: Query<&Interaction, (Changed<Interaction>, With<GameOverBack>)>,
    quit: Query<&Interaction, (Changed<Interaction>, With<QuitToMenuBtn>)>,
    enter_central: Query<&Interaction, (Changed<Interaction>, With<EnterCentralBtn>)>,
    mut central_btn: Query<&mut Node, (With<EnterCentralBtn>, Without<GameOverRoot>)>,
) {
    if back.iter().any(|i| *i == Interaction::Pressed)
        || quit.iter().any(|i| *i == Interaction::Pressed)
    {
        next.set(Screen::Menu);
        return;
    }
    if enter_central.iter().any(|i| *i == Interaction::Pressed) {
        // The dial happens in `menu::pending_switch` after the game scene
        // tears down — see `PendingSwitch`.
        pending.0 = Some(WorldTarget::Central);
        transition.text = Some(WorldTarget::Central.transition_label(None, *lang));
        transition.age = 0.0;
        next.set(Screen::Menu);
        return;
    }

    let Some(state) = view.state.as_ref() else { return };
    let display = if state.phase == GamePhase::Running {
        Display::None
    } else {
        Display::Flex
    };
    for mut node in &mut root {
        if node.display != display {
            node.display = display;
        }
    }
    if state.phase == GamePhase::Running {
        return;
    }

    // The Tunnel exit is only offered where it can work: a graduated win, an
    // account signed in to own the settlers, and not already in the central
    // world (its overlay never shows anyway — no win/lose there).
    let offer_central = state.graduated && session.auth.is_some() && !state.central;
    for mut node in &mut central_btn {
        let want = if offer_central { Display::Flex } else { Display::None };
        if node.display != want {
            node.display = want;
        }
    }

    // Persistent worlds auto-reset shortly after game-over (the server
    // freezes commands meanwhile); surface its countdown so the pause reads
    // as "new run incoming", not a hang. Singleplayer never sends one.
    let countdown = view
        .reset_countdown
        .map(|s| i18n_hud::go_reset_countdown(s, *lang))
        .unwrap_or_default();
    for (mut text, mut color, kind) in &mut texts {
        let (new, col) = match (kind, state.phase) {
            (GoText::Title, GamePhase::Won) if state.graduated => (
                i18n_hud::go_title_tunnel(*lang).to_string(),
                Color::srgb(0.55, 0.85, 1.00),
            ),
            (GoText::Title, GamePhase::Won) => (
                i18n_hud::go_title_victory(*lang).to_string(),
                Color::srgb(0.55, 0.90, 0.50),
            ),
            (GoText::Title, _) => (
                i18n_hud::go_title_defeat(*lang).to_string(),
                Color::srgb(0.95, 0.35, 0.30),
            ),
            (GoText::Info, GamePhase::Won) if state.graduated => (
                i18n_hud::go_info_graduated(
                    state.day(),
                    state.stock.wood as i64,
                    state.stock.coal as i64,
                    state.stock.food as i64,
                    &countdown,
                    *lang,
                ),
                TEXT_MUTED,
            ),
            (GoText::Info, _) => (
                i18n_hud::go_info_plain(
                    state.day(),
                    state.survivors.len(),
                    state.stock.wood as i64,
                    state.stock.coal as i64,
                    state.stock.food as i64,
                    &countdown,
                    *lang,
                ),
                TEXT_MUTED,
            ),
        };
        if text.0 != new {
            text.0 = new;
        }
        color.0 = col;
    }
}

/// The top-bar world-switch button: visible as "Global World" in a graduated
/// personal world (account sessions only) and as "My City" inside the central
/// world; hidden for guests and ungraduated cities. A press routes through
/// `PendingSwitch` exactly like the game-over button.
pub fn world_switch_button(
    view: Res<GameView>,
    session: Res<Session>,
    lang: Res<super::i18n::Lang>,
    mut pending: ResMut<PendingSwitch>,
    mut transition: ResMut<TransitionMsg>,
    mut next: ResMut<NextState<Screen>>,
    mut btn: Query<(&Interaction, &mut Node), With<WorldSwitchBtn>>,
    mut label: Query<&mut Text, With<WorldSwitchLabel>>,
) {
    let Some(state) = view.state.as_ref() else { return };
    let target = if session.auth.is_none() {
        None
    } else if state.central {
        Some((WorldTarget::Personal, i18n_hud::world_switch_my_city(*lang)))
    } else if state.graduated {
        Some((WorldTarget::Central, i18n_hud::world_switch_global(*lang)))
    } else {
        None
    };
    for (interaction, mut node) in &mut btn {
        let want = if target.is_some() { Display::Flex } else { Display::None };
        if node.display != want {
            node.display = want;
        }
        if let Some((world, _)) = target {
            if *interaction == Interaction::Pressed {
                pending.0 = Some(world);
                transition.text = Some(world.transition_label(None, *lang));
                transition.age = 0.0;
                next.set(Screen::Menu);
                return;
            }
        }
    }
    if let Some((_, wanted)) = target {
        for mut text in &mut label {
            if text.0 != wanted {
                text.0 = wanted.to_string();
            }
        }
    }
}

/// Fades in, holds, then fades out the center-screen transition banner
/// (see `TransitionMsg`). `time.delta_secs()` is used directly rather than a
/// `Local` accumulator since `TransitionMsg::age` is the shared, resettable
/// clock every switch site restarts.
pub fn transition_overlay(
    time: Res<Time>,
    mut transition: ResMut<TransitionMsg>,
    mut q: Query<(&mut Text, &mut TextColor), With<TransitionText>>,
) {
    const HOLD: f32 = 1.6;
    const FADE: f32 = 0.8;
    let Some(msg) = transition.text.clone() else {
        return;
    };
    transition.age += time.delta_secs();
    let age = transition.age;
    let alpha = if age < 0.15 {
        age / 0.15
    } else if age < HOLD {
        1.0
    } else if age < HOLD + FADE {
        1.0 - (age - HOLD) / FADE
    } else {
        transition.text = None;
        0.0
    };
    for (mut text, mut color) in &mut q {
        if text.0 != msg {
            text.0 = msg.clone();
        }
        color.0.set_alpha(alpha.clamp(0.0, 1.0));
    }
}

pub fn generic_button_hover(
    mut q: Query<
        (&Interaction, &mut BackgroundColor, &BaseColor),
        (
            Changed<Interaction>,
            Without<BuildBtn>,
            Without<FurnaceLvlBtn>,
        ),
    >,
) {
    for (interaction, mut bg, base) in &mut q {
        let c = base.0.to_srgba();
        bg.0 = match interaction {
            Interaction::Pressed => Color::srgb(
                c.red + (1.0 - c.red) * 0.30,
                c.green + (1.0 - c.green) * 0.30,
                c.blue + (1.0 - c.blue) * 0.30,
            ),
            Interaction::Hovered => Color::srgb(
                c.red + (1.0 - c.red) * 0.15,
                c.green + (1.0 - c.green) * 0.15,
                c.blue + (1.0 - c.blue) * 0.15,
            ),
            Interaction::None => base.0,
        };
    }
}
