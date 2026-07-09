//! Missions & Tunnel panel: the personal-world progression toward the Global
//! World. Shows each mission's progress and, once every mission is done, the
//! Tunnel excavation controls. Reads the authoritative snapshot and sends
//! `InvestTunnel` commands; all logic is server-side.

use bevy::prelude::*;

use frozen_city::game::types::{
    GamePhase, PlayerCommand, TUNNEL_INVEST_COAL, TUNNEL_INVEST_WOOD, TUNNEL_STAGES,
};
use frozen_city::net::protocol::ClientMsg;

use super::ui::UiBlocker;
use super::{GameView, NetConn, Screen};

/// Fixed mission rows (there are 5 missions; a little headroom).
const ROWS: usize = 6;

const PANEL_BG: Color = Color::srgba(0.045, 0.075, 0.125, 0.9);
const BTN_BG: Color = Color::srgb(0.20, 0.36, 0.30);
const BTN_DIM: Color = Color::srgba(0.10, 0.12, 0.16, 0.9);
const TEXT_MAIN: Color = Color::srgb(0.90, 0.93, 0.97);
const TEXT_DIM: Color = Color::srgb(0.60, 0.66, 0.76);
const DONE: Color = Color::srgb(0.55, 0.88, 0.50);
const TUNNEL_COL: Color = Color::srgb(0.70, 0.82, 1.0);

#[derive(Component)]
struct MissionRow(usize);

#[derive(Component)]
struct TunnelSection;

#[derive(Component)]
struct TunnelText;

#[derive(Component)]
struct TunnelInvestBtn;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Game), spawn_panel).add_systems(
        Update,
        (update_panel, invest_button).run_if(in_state(Screen::Game)),
    );
}

fn text(t: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(t.into()), TextFont::from_font_size(size), TextColor(color))
}

fn spawn_panel(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(12.0),
                top: Val::Px(232.0),
                width: Val::Px(320.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                padding: UiRect::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
            Interaction::default(),
            UiBlocker,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn(text("Missions", 14.0, TEXT_MAIN));
            for i in 0..ROWS {
                p.spawn((text("", 12.5, TEXT_DIM), MissionRow(i)));
            }

            // Tunnel section, hidden until every mission is complete.
            p.spawn((
                Node {
                    display: Display::None,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    margin: UiRect::top(Val::Px(6.0)),
                    ..default()
                },
                TunnelSection,
            ))
            .with_children(|t| {
                t.spawn((text("", 13.0, TUNNEL_COL), TunnelText));
                t.spawn((
                    Button,
                    Node {
                        width: Val::Px(240.0),
                        height: Val::Px(30.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    // No BaseColor: this button's color is owned entirely by
                    // update_panel (affordability dim/bright), so the generic
                    // hover system must not fight it over BackgroundColor.
                    BackgroundColor(BTN_BG),
                    TunnelInvestBtn,
                ))
                .with_children(|b| {
                    b.spawn(text(
                        format!(
                            "Excavate  ({:.0} wood, {:.0} coal)",
                            TUNNEL_INVEST_WOOD, TUNNEL_INVEST_COAL
                        ),
                        12.0,
                        TEXT_MAIN,
                    ));
                });
            });
        });
}

fn update_panel(
    view: Res<GameView>,
    mut rows: Query<(&MissionRow, &mut Text, &mut TextColor), Without<TunnelText>>,
    mut section: Query<&mut Node, With<TunnelSection>>,
    mut tunnel_text: Query<&mut Text, With<TunnelText>>,
    mut invest_bg: Query<&mut BackgroundColor, With<TunnelInvestBtn>>,
) {
    let Some(state) = view.state.as_ref() else { return };

    for (row, mut t, mut c) in &mut rows {
        let (s, col) = match state.missions.get(row.0) {
            Some(m) => {
                let cur = state.mission_current(m.kind).min(m.kind.target());
                if m.done {
                    (
                        format!("[x] {} {}", m.kind.label(), m.kind.target()),
                        DONE,
                    )
                } else {
                    (
                        format!("[ ] {} {}  ({}/{})", m.kind.label(), m.kind.target(), cur, m.kind.target()),
                        TEXT_DIM,
                    )
                }
            }
            None => (String::new(), TEXT_DIM),
        };
        if t.0 != s {
            t.0 = s;
        }
        if c.0 != col {
            c.0 = col;
        }
    }

    // Tunnel section visibility + status.
    let show = state.tunnel.unlocked && state.phase == GamePhase::Running;
    for mut node in &mut section {
        let d = if show { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
        }
    }
    if show {
        if let Ok(mut tt) = tunnel_text.single_mut() {
            let pct = (state.tunnel.progress * 100.0) as u32;
            let new = format!(
                "TUNNEL  —  stage {}/{}  ({}%)",
                state.tunnel.stage, TUNNEL_STAGES, pct
            );
            if tt.0 != new {
                tt.0 = new;
            }
        }
        let affordable = state.stock.wood >= TUNNEL_INVEST_WOOD && state.stock.coal >= TUNNEL_INVEST_COAL;
        if let Ok(mut bg) = invest_bg.single_mut() {
            let want = if affordable { BTN_BG } else { BTN_DIM };
            if bg.0 != want {
                bg.0 = want;
            }
        }
    }
}

fn invest_button(
    net: Res<NetConn>,
    clicked: Query<&Interaction, (Changed<Interaction>, With<TunnelInvestBtn>)>,
) {
    if clicked.iter().any(|i| *i == Interaction::Pressed) {
        net.send(ClientMsg::Cmd(PlayerCommand::InvestTunnel));
    }
}
