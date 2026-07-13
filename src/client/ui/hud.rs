use bevy::prelude::*;

use frozen_city::game::types::{BuildingKind, FURNACE_COAL_PER_DAY_PER_LEVEL};

use super::super::i18n::Lang;
use super::super::i18n_hud;
use super::super::i18n_names;
use super::super::theme::{
    self, BG_PANEL, BORDER, BTN, BTN_DANGER, FS_BODY, FS_MICRO, FS_SMALL, FS_TITLE, RES_COAL,
    RES_FOOD, RES_WOOD, SP_MD, SP_SM, SP_XS, TEXT_MUTED, TEXT_PRIMARY,
};
use super::super::*;
use super::*;

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
