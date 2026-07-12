//! HUD, build bar, selection panel and the game-over overlay.

use bevy::prelude::*;

use frozen_city::game::types::{
    BuildingKind, GamePhase, FURNACE_COAL_PER_DAY_PER_LEVEL,
};
use frozen_city::net::protocol::ClientMsg;
use frozen_city::game::types::PlayerCommand;

use super::*;

const PANEL_BG: Color = Color::srgba(0.045, 0.075, 0.125, 0.90);
const BTN_BG: Color = Color::srgb(0.16, 0.20, 0.28);
const BTN_ACTIVE: Color = Color::srgb(0.82, 0.50, 0.18);
const BTN_DIM: Color = Color::srgba(0.10, 0.12, 0.16, 0.9);
const BTN_HOVER: Color = Color::srgb(0.27, 0.32, 0.42);
const TEXT_MAIN: Color = Color::srgb(0.90, 0.93, 0.97);
const TEXT_DIM: Color = Color::srgb(0.62, 0.68, 0.78);
const COL_WOOD: Color = Color::srgb(0.85, 0.68, 0.42);
const COL_COAL: Color = Color::srgb(0.62, 0.66, 0.75);
const COL_FOOD: Color = Color::srgb(0.55, 0.82, 0.48);

const DEFAULT_HINT: &str =
    "LMB place/select   RMB cancel   1-8 build   WASD pan   Q/E rotate   wheel zoom   R research   P roster   Enter chat   Alt+click ping";

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

/// Resting color for generically hover-styled buttons.
#[derive(Component)]
pub struct BaseColor(pub Color);

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

fn text(t: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(t.into()),
        TextFont::from_font_size(size),
        TextColor(color),
    )
}

fn button(w: f32, h: f32, bg: Color) -> impl Bundle {
    (
        Button,
        Node {
            width: Val::Px(w),
            height: Val::Px(h),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(bg),
        BaseColor(bg),
    )
}

pub fn spawn_hud(mut commands: Commands) {
    // --- Top bar ---
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
                ..default()
            },
            BackgroundColor(PANEL_BG),
            Interaction::default(),
            UiBlocker,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn((text("Wood 0", 15.0, COL_WOOD), HudField::Wood));
            p.spawn((text("Coal 0", 15.0, COL_COAL), HudField::Coal));
            p.spawn((text("Food 0", 15.0, COL_FOOD), HudField::Food));
            p.spawn((text("Pop 0", 15.0, TEXT_MAIN), HudField::Pop));
            p.spawn(Node {
                flex_grow: 1.0,
                ..default()
            });
            p.spawn((text("Day 1  06:00", 15.0, TEXT_MAIN), HudField::Clock));
            p.spawn((text("-0 C", 15.0, Color::srgb(0.55, 0.80, 0.95)), HudField::Temp));
            p.spawn(Node {
                flex_grow: 1.0,
                ..default()
            });
            p.spawn((text("Furnace L1", 15.0, TEXT_MAIN), HudField::Furnace));
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
                b.spawn((text("Global World", 13.0, TEXT_MAIN), WorldSwitchLabel));
            });
            p.spawn((button(70.0, 30.0, BTN_BG), QuitToMenuBtn))
                .with_children(|b| {
                    b.spawn(text("Menu", 13.0, TEXT_MAIN));
                });
        });

    // --- Bottom build bar ---
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(0.0),
                height: Val::Px(88.0),
                padding: UiRect::axes(Val::Px(12.0), Val::Px(10.0)),
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(PANEL_BG),
            Interaction::default(),
            UiBlocker,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            for (i, kind) in BuildingKind::BUILDABLE.into_iter().enumerate() {
                p.spawn((
                    Button,
                    Node {
                        width: Val::Px(92.0),
                        height: Val::Px(62.0),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(3.0),
                        ..default()
                    },
                    BackgroundColor(BTN_BG),
                    BuildBtn(kind),
                ))
                .with_children(|b| {
                    b.spawn(text(kind.name(), 11.5, TEXT_MAIN));
                    b.spawn(text(
                        format!("{}w  [{}]", kind.cost_wood(), i + 1),
                        10.5,
                        TEXT_DIM,
                    ));
                });
            }
            p.spawn(Node {
                flex_grow: 1.0,
                ..default()
            });
            p.spawn(text("Furnace level", 13.0, TEXT_DIM));
            for lvl in 0u8..=3 {
                p.spawn((
                    Button,
                    Node {
                        width: Val::Px(42.0),
                        height: Val::Px(40.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(BTN_BG),
                    FurnaceLvlBtn(lvl),
                ))
                .with_children(|b| {
                    let label = if lvl == 0 {
                        "Off".to_string()
                    } else {
                        lvl.to_string()
                    };
                    b.spawn(text(label, 14.0, TEXT_MAIN));
                });
            }
        });

    // --- Tooltip / hint line just above the build bar ---
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(14.0),
            bottom: Val::Px(92.0),
            ..default()
        },
        text(DEFAULT_HINT, 13.0, TEXT_DIM),
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
        text("", 26.0, TEXT_MAIN),
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
        text("", 13.0, TEXT_DIM),
        FpsText,
        DespawnOnExit(Screen::Game),
    ));

    // --- Event feed (right side) ---
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(12.0),
                top: Val::Px(54.0),
                width: Val::Px(340.0),
                padding: UiRect::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.25)),
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn((text("", 13.0, Color::srgba(0.85, 0.90, 1.0, 0.9)), HudField::Events));
        });

    // --- Selection panel ---
    commands
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                right: Val::Px(12.0),
                bottom: Val::Px(100.0),
                width: Val::Px(260.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                padding: UiRect::all(Val::Px(12.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
            Interaction::default(),
            UiBlocker,
            SelPanelRoot,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn((text("Building", 17.0, TEXT_MAIN), SelText::Title));
            p.spawn((text("", 12.5, TEXT_DIM), SelText::Info));
            p.spawn((
                Node {
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(10.0),
                    ..default()
                },
                WorkerRow,
            ))
            .with_children(|row| {
                row.spawn((button(34.0, 30.0, BTN_BG), WorkerMinus))
                    .with_children(|b| {
                        b.spawn(text("-", 16.0, TEXT_MAIN));
                    });
                row.spawn((text("0/0 workers", 14.0, TEXT_MAIN), SelText::Count));
                row.spawn((button(34.0, 30.0, BTN_BG), WorkerPlus))
                    .with_children(|b| {
                        b.spawn(text("+", 16.0, TEXT_MAIN));
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
                BackgroundColor(BTN_BG),
                BaseColor(BTN_BG),
                AssignHereBtn,
            ))
            .with_children(|b| {
                b.spawn((text("", 12.5, TEXT_MAIN), AssignHereLabel));
            });
            p.spawn((
                button(220.0, 30.0, Color::srgb(0.45, 0.16, 0.14)),
                DemolishBtn,
            ))
            .with_children(|b| {
                b.spawn(text("Demolish (40% refund)", 13.0, TEXT_MAIN));
            });
        });

    // --- Game over overlay ---
    commands
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(16.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            Interaction::default(),
            UiBlocker,
            GameOverRoot,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn((text("", 44.0, TEXT_MAIN), GoText::Title));
            p.spawn((text("", 16.0, TEXT_DIM), GoText::Info));
            p.spawn((
                button(260.0, 46.0, Color::srgb(0.13, 0.35, 0.45)),
                EnterCentralBtn,
            ))
            .with_children(|b| {
                b.spawn(text("Enter the Global World", 15.0, TEXT_MAIN));
            });
            p.spawn((button(220.0, 46.0, BTN_BG), GameOverBack))
                .with_children(|b| {
                    b.spawn(text("Return to Menu", 15.0, TEXT_MAIN));
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
/// underlying number actually changed instead of on every frame.
#[derive(Default)]
pub(crate) struct HudCache {
    wood: Option<i64>,
    coal: Option<i64>,
    food: Option<i64>,
    pop: Option<(usize, u32)>,
    clock: Option<(u32, u32, u32)>,
    temp: Option<(i32, bool)>,
    furnace: Option<(u8, bool)>,
    events: Option<u64>,
}

pub fn hud_update(
    view: Res<GameView>,
    mut cache: Local<HudCache>,
    mut q: Query<(&mut Text, Option<&mut TextColor>, &HudField)>,
) {
    let Some(state) = view.state.as_ref() else { return };
    for (mut text, color, field) in &mut q {
        match field {
            HudField::Wood => {
                let v = state.stock.wood as i64;
                if cache.wood != Some(v) {
                    cache.wood = Some(v);
                    text.0 = format!("Wood {v}");
                }
            }
            HudField::Coal => {
                let v = state.stock.coal as i64;
                if cache.coal != Some(v) {
                    cache.coal = Some(v);
                    text.0 = format!("Coal {v}");
                }
            }
            HudField::Food => {
                let v = state.stock.food as i64;
                if cache.food != Some(v) {
                    cache.food = Some(v);
                    text.0 = format!("Food {v}");
                }
            }
            HudField::Pop => {
                let key = (state.survivors.len(), state.idle_workers());
                if cache.pop != Some(key) {
                    cache.pop = Some(key);
                    text.0 = format!("Pop {}  (idle {})", key.0, key.1);
                }
            }
            HudField::Clock => {
                let mins = (state.time_of_day() * 24.0 * 60.0) as u32;
                let key = (state.day(), state.win_days, mins);
                if cache.clock != Some(key) {
                    cache.clock = Some(key);
                    text.0 = format!(
                        "Day {}/{}   {:02}:{:02}",
                        key.0,
                        key.1,
                        mins / 60,
                        mins % 60
                    );
                }
            }
            HudField::Temp => {
                let temp = state.temperature();
                let key = (temp.round() as i32, state.cold_snap);
                if cache.temp != Some(key) {
                    cache.temp = Some(key);
                    let snap = if state.cold_snap { "   COLD SNAP!" } else { "" };
                    text.0 = format!("{:+.0} C{}", temp, snap);
                }
            }
            HudField::Furnace => {
                let key = (state.furnace_level, state.furnace_lit);
                if cache.furnace != Some(key) {
                    cache.furnace = Some(key);
                    let status = if state.furnace_lit {
                        "burning"
                    } else if state.furnace_level == 0 {
                        "off"
                    } else {
                        "OUT OF FUEL"
                    };
                    if let Some(mut c) = color {
                        c.0 = if state.furnace_lit {
                            Color::srgb(0.95, 0.65, 0.30)
                        } else {
                            Color::srgb(0.95, 0.30, 0.25)
                        };
                    }
                    text.0 = format!(
                        "Furnace L{} ({:.0}/day) {}",
                        state.furnace_level,
                        state.furnace_level as f32 * FURNACE_COAL_PER_DAY_PER_LEVEL,
                        status
                    );
                }
            }
            HudField::Events => {
                if cache.events != Some(state.total_events) {
                    cache.events = Some(state.total_events);
                    // Show up to 8 lines, prioritising system events (deaths,
                    // weather, victory) over cosmetic ones so the server's
                    // eviction protection actually reaches the player's eyes;
                    // then display the chosen lines chronologically.
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
            BTN_BG
        };
        if bg.0 != color {
            bg.0 = color;
        }
    }

    if let Ok(mut tip) = tooltip.single_mut() {
        let new = match hovered.or(build.0) {
            Some(k) => format!("{} — {} wood. {}", k.name(), k.cost_wood(), k.description()),
            None => DEFAULT_HINT.to_string(),
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
            BTN_BG
        };
        if bg.0 != color {
            bg.0 = color;
        }
    }
}

pub fn selection_panel_update(
    view: Res<GameView>,
    mut selection: ResMut<Selection>,
    mut nodes: ParamSet<(
        Query<&mut Node, With<SelPanelRoot>>,
        Query<&mut Node, With<WorkerRow>>,
        Query<&mut Node, With<DemolishBtn>>,
    )>,
    mut texts: Query<(&mut Text, &SelText)>,
) {
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
        BuildingKind::Furnace => format!(
            "Level {} — burns {:.0} coal/day\n(wood x{} when coal runs out)\nHeat radius {:.0} tiles\nSet the level with the buttons below.",
            state.furnace_level,
            state.furnace_level as f32 * FURNACE_COAL_PER_DAY_PER_LEVEL,
            frozen_city::game::types::WOOD_FUEL_PENALTY,
            state.heat_radius(),
        ),
        BuildingKind::Tent => format!(
            "Houses 4 people.\nCity housing: {} for {} people.\nTents inside the heat glow keep\npeople warm at night.",
            state.housing_capacity(),
            state.survivors.len(),
        ),
        BuildingKind::Sawmill => format!(
            "+{:.0} wood/day at full crew.\nForest within reach: {} wood.",
            b.kind.production_per_worker_day() * b.kind.max_workers() as f32,
            state.forest_near(b.x, b.y, 4),
        ),
        BuildingKind::CoalMine => format!(
            "+{:.0} coal/day at full crew.\nDeposit remaining: {}.",
            b.kind.production_per_worker_day() * b.kind.max_workers() as f32,
            state.tile(b.x, b.y).deposit,
        ),
        BuildingKind::HunterHut => format!(
            "+{:.0} food/day at full crew.",
            b.kind.production_per_worker_day() * b.kind.max_workers() as f32,
        ),
        BuildingKind::Greenhouse => format!(
            "+{:.0} food/day at full crew.\nHigh-output indoor farming.",
            b.kind.production_per_worker_day() * b.kind.max_workers() as f32,
        ),
        BuildingKind::Hospital => format!(
            "Staffed: +{:.0} HP/day to survivors\nper worker ({} workers = +{:.0}/day).",
            frozen_city::game::types::HOSPITAL_CARE_PER_WORKER_DAY,
            b.workers,
            b.workers as f32 * frozen_city::game::types::HOSPITAL_CARE_PER_WORKER_DAY,
        ),
        BuildingKind::Kitchen => {
            let cut = (1.0 - frozen_city::game::types::KITCHEN_FOOD_EFFICIENCY) * 100.0;
            if b.workers > 0 {
                format!("Staffed: the city eats {cut:.0}% less food.")
            } else {
                format!("Unstaffed. Staff it to cut food use by {cut:.0}%.")
            }
        }
        BuildingKind::Warehouse => {
            let cut = (1.0 - frozen_city::game::types::WAREHOUSE_BUILD_DISCOUNT) * 100.0;
            if b.workers > 0 {
                format!("Staffed: new buildings cost {cut:.0}% less wood.")
            } else {
                format!("Unstaffed. Staff it to cut build costs by {cut:.0}%.")
            }
        }
    };

    for (mut text, kind) in &mut texts {
        let new = match kind {
            SelText::Title => b.kind.name().to_string(),
            SelText::Info => info.clone(),
            SelText::Count => format!("{}/{} workers", b.workers, b.kind.max_workers()),
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
        transition.text = Some(WorldTarget::Central.transition_label(None));
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

    for (mut text, mut color, kind) in &mut texts {
        let (new, col) = match (kind, state.phase) {
            (GoText::Title, GamePhase::Won) if state.graduated => (
                "THE TUNNEL IS OPEN".to_string(),
                Color::srgb(0.55, 0.85, 1.00),
            ),
            (GoText::Title, GamePhase::Won) => (
                "VICTORY".to_string(),
                Color::srgb(0.55, 0.90, 0.50),
            ),
            (GoText::Title, _) => (
                "THE CITY HAS FALLEN".to_string(),
                Color::srgb(0.95, 0.35, 0.30),
            ),
            (GoText::Info, GamePhase::Won) if state.graduated => (
                format!(
                    "Day {} — the Tunnel broke through. The Global World awaits!\nWood {}   Coal {}   Food {}",
                    state.day(),
                    state.stock.wood as i64,
                    state.stock.coal as i64,
                    state.stock.food as i64
                ),
                TEXT_DIM,
            ),
            (GoText::Info, _) => (
                format!(
                    "Day {} — population {}.\nWood {}   Coal {}   Food {}",
                    state.day(),
                    state.survivors.len(),
                    state.stock.wood as i64,
                    state.stock.coal as i64,
                    state.stock.food as i64
                ),
                TEXT_DIM,
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
        Some((WorldTarget::Personal, "My City"))
    } else if state.graduated {
        Some((WorldTarget::Central, "Global World"))
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
                transition.text = Some(world.transition_label(None));
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
