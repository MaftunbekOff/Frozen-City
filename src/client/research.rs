//! Technology tree: a modal research panel toggled with `R`. Lists every
//! [`Tech`], its cost and status, and sends `Research` commands. The server is
//! authoritative; this only reflects `GameState.techs` and affordability.

use bevy::prelude::*;

use frozen_city::game::types::{PlayerCommand, Tech};
use frozen_city::net::protocol::ClientMsg;

use super::chat::ChatState;
use super::ui::UiBlocker;
use super::{GameView, NetConn, Screen};

const PANEL_BG: Color = Color::srgba(0.05, 0.08, 0.13, 0.98);
const BACKDROP: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);
const BTN_BG: Color = Color::srgb(0.20, 0.36, 0.30);
const BTN_DIM: Color = Color::srgba(0.10, 0.12, 0.16, 0.9);
const BTN_DONE: Color = Color::srgb(0.22, 0.44, 0.26);
const TEXT_MAIN: Color = Color::srgb(0.90, 0.93, 0.97);
const TEXT_DIM: Color = Color::srgb(0.62, 0.68, 0.78);
const DONE: Color = Color::srgb(0.55, 0.88, 0.50);

/// Whether the research modal is open (also gates world/camera input).
#[derive(Resource, Default)]
pub struct ResearchOpen(pub bool);

#[derive(Component)]
struct ResearchRoot;

#[derive(Component)]
struct ResearchBtn(Tech);

#[derive(Component)]
struct ResearchBtnLabel(Tech);

pub fn plugin(app: &mut App) {
    app.init_resource::<ResearchOpen>()
        .add_systems(OnEnter(Screen::Game), spawn_research)
        .add_systems(
            Update,
            (toggle_research, update_research, research_buttons)
                .run_if(in_state(Screen::Game)),
        );
}

fn text(t: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(t.into()), TextFont::from_font_size(size), TextColor(color))
}

fn spawn_research(mut commands: Commands) {
    commands
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(BACKDROP),
            Interaction::default(),
            UiBlocker,
            ResearchRoot,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: Val::Px(470.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(8.0),
                    padding: UiRect::all(Val::Px(16.0)),
                    ..default()
                },
                BackgroundColor(PANEL_BG),
            ))
            .with_children(|panel| {
                panel.spawn(text("Research   (R or Esc to close)", 18.0, TEXT_MAIN));
                for tech in Tech::ALL {
                    panel
                        .spawn(Node {
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(10.0),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                Node {
                                    flex_grow: 1.0,
                                    flex_direction: FlexDirection::Column,
                                    ..default()
                                },
                            ))
                            .with_children(|info| {
                                info.spawn(text(tech.name(), 14.0, TEXT_MAIN));
                                info.spawn(text(tech.description(), 12.0, TEXT_DIM));
                            });
                            row.spawn((
                                Button,
                                Node {
                                    width: Val::Px(150.0),
                                    height: Val::Px(34.0),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                                BackgroundColor(BTN_BG),
                                ResearchBtn(tech),
                            ))
                            .with_children(|b| {
                                b.spawn((text("", 12.0, TEXT_MAIN), ResearchBtnLabel(tech)));
                            });
                        });
                }
            });
        });
}

fn toggle_research(
    keys: Res<ButtonInput<KeyCode>>,
    chat: Res<ChatState>,
    mut open: ResMut<ResearchOpen>,
) {
    if !chat.active && keys.just_pressed(KeyCode::KeyR) {
        open.0 = !open.0;
    }
    if open.0 && keys.just_pressed(KeyCode::Escape) {
        open.0 = false;
    }
}

fn update_research(
    open: Res<ResearchOpen>,
    view: Res<GameView>,
    mut root: Query<&mut Node, With<ResearchRoot>>,
    mut btns: Query<(&ResearchBtn, &mut BackgroundColor)>,
    mut labels: Query<(&ResearchBtnLabel, &mut Text, &mut TextColor)>,
) {
    let display = if open.0 { Display::Flex } else { Display::None };
    for mut node in &mut root {
        if node.display != display {
            node.display = display;
        }
    }
    if !open.0 {
        return;
    }
    let Some(state) = view.state.as_ref() else { return };

    for (btn, mut bg) in &mut btns {
        let owned = state.has_tech(btn.0);
        let affordable = state.stock.wood >= btn.0.cost_wood() as f32
            && state.stock.coal >= btn.0.cost_coal() as f32;
        let want = if owned {
            BTN_DONE
        } else if affordable {
            BTN_BG
        } else {
            BTN_DIM
        };
        if bg.0 != want {
            bg.0 = want;
        }
    }
    for (label, mut t, mut c) in &mut labels {
        let owned = state.has_tech(label.0);
        let (s, col) = if owned {
            ("Researched".to_string(), DONE)
        } else {
            (
                format!("{}w {}c", label.0.cost_wood(), label.0.cost_coal()),
                TEXT_MAIN,
            )
        };
        if t.0 != s {
            t.0 = s;
        }
        if c.0 != col {
            c.0 = col;
        }
    }
}

fn research_buttons(
    net: Res<NetConn>,
    view: Res<GameView>,
    clicked: Query<(&Interaction, &ResearchBtn), Changed<Interaction>>,
) {
    let Some(state) = view.state.as_ref() else { return };
    for (interaction, btn) in &clicked {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let owned = state.has_tech(btn.0);
        let affordable = state.stock.wood >= btn.0.cost_wood() as f32
            && state.stock.coal >= btn.0.cost_coal() as f32;
        if !owned && affordable {
            net.send(ClientMsg::Cmd(PlayerCommand::Research { tech: btn.0 }));
        }
    }
}
