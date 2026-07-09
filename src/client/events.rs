//! Dynamic-event UI: a non-blocking caravan choice popup (Accept / Decline)
//! and a status line for active disease / blizzard. Reads the authoritative
//! snapshot and sends `RespondEvent`; the sim owns all logic and timers.

use bevy::prelude::*;

use frozen_city::game::types::{GamePhase, PlayerCommand};
use frozen_city::net::protocol::ClientMsg;

use super::ui::{BaseColor, UiBlocker};
use super::{GameView, NetConn, Screen};

const PANEL_BG: Color = Color::srgba(0.06, 0.09, 0.14, 0.96);
const ACCEPT_BG: Color = Color::srgb(0.20, 0.40, 0.26);
const DECLINE_BG: Color = Color::srgb(0.42, 0.20, 0.18);
const TEXT_MAIN: Color = Color::srgb(0.92, 0.95, 1.0);
const SICK_COL: Color = Color::srgb(0.72, 0.90, 0.45);
const COLD_COL: Color = Color::srgb(0.60, 0.82, 1.0);

#[derive(Component)]
struct CaravanRoot;

#[derive(Component)]
struct CaravanText;

#[derive(Component)]
struct CaravanAcceptBtn;

#[derive(Component)]
struct CaravanDeclineBtn;

#[derive(Component)]
struct EventStatusText;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Game), spawn_events_ui)
        .add_systems(
            Update,
            (update_events, caravan_buttons).run_if(in_state(Screen::Game)),
        );
}

fn text(t: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(t.into()), TextFont::from_font_size(size), TextColor(color))
}

fn btn(label: &str, bg: Color) -> impl Bundle {
    (
        Button,
        Node {
            width: Val::Px(120.0),
            height: Val::Px(30.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(bg),
        BaseColor(bg),
        Name::new(label.to_string()),
    )
}

fn spawn_events_ui(mut commands: Commands) {
    // Active-event status line, centered just below the top bar.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(50.0),
                justify_content: JustifyContent::Center,
                column_gap: Val::Px(16.0),
                ..default()
            },
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn((text("", 15.0, SICK_COL), EventStatusText));
        });

    // Caravan choice popup (non-blocking), centered near the top.
    commands
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(96.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            CaravanRoot,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: Val::Px(440.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(10.0),
                    padding: UiRect::all(Val::Px(14.0)),
                    ..default()
                },
                BackgroundColor(PANEL_BG),
                Interaction::default(),
                UiBlocker,
            ))
            .with_children(|panel| {
                panel.spawn((text("", 14.0, TEXT_MAIN), CaravanText));
                panel
                    .spawn(Node {
                        column_gap: Val::Px(12.0),
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((btn("Accept", ACCEPT_BG), CaravanAcceptBtn))
                            .with_children(|b| {
                                b.spawn(text("Take them in", 13.0, TEXT_MAIN));
                            });
                        row.spawn((btn("Decline", DECLINE_BG), CaravanDeclineBtn))
                            .with_children(|b| {
                                b.spawn(text("Turn away", 13.0, TEXT_MAIN));
                            });
                    });
            });
        });
}

fn update_events(
    view: Res<GameView>,
    mut root: Query<&mut Node, With<CaravanRoot>>,
    mut caravan_text: Query<&mut Text, (With<CaravanText>, Without<EventStatusText>)>,
    mut status: Query<(&mut Text, &mut TextColor), With<EventStatusText>>,
) {
    let Some(state) = view.state.as_ref() else { return };
    // Once the game is over, `tick` stops running so events never clear — hide
    // the popup and status line rather than let them freeze over the game-over
    // overlay (and intercept its clicks).
    let running = state.phase == GamePhase::Running;

    // Caravan popup.
    let display = if running && state.pending_event.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut root {
        if node.display != display {
            node.display = display;
        }
    }
    if let Some(offer) = state.pending_event {
        if let Ok(mut t) = caravan_text.single_mut() {
            let new = format!(
                "A caravan of {} refugees asks for shelter.\nTake them in for {} food?",
                offer.count, offer.food_cost
            );
            if t.0 != new {
                t.0 = new;
            }
        }
    }

    // Disease / blizzard status line.
    if let Ok((mut t, mut c)) = status.single_mut() {
        let (new, col) = match (running && state.disease_active(), running && state.blizzard_active()) {
            (true, true) => ("SICKNESS + BLIZZARD".to_string(), COLD_COL),
            (true, false) => ("SICKNESS spreading".to_string(), SICK_COL),
            (false, true) => ("BLIZZARD".to_string(), COLD_COL),
            (false, false) => (String::new(), SICK_COL),
        };
        if t.0 != new {
            t.0 = new;
        }
        if c.0 != col {
            c.0 = col;
        }
    }
}

fn caravan_buttons(
    net: Res<NetConn>,
    accept: Query<&Interaction, (Changed<Interaction>, With<CaravanAcceptBtn>)>,
    decline: Query<&Interaction, (Changed<Interaction>, With<CaravanDeclineBtn>)>,
) {
    if accept.iter().any(|i| *i == Interaction::Pressed) {
        net.send(ClientMsg::Cmd(PlayerCommand::RespondEvent { accept: true }));
    }
    if decline.iter().any(|i| *i == Interaction::Pressed) {
        net.send(ClientMsg::Cmd(PlayerCommand::RespondEvent { accept: false }));
    }
}
