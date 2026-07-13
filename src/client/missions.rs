//! Missions & Tunnel panel: the personal-world progression toward the Global
//! World. Shows each mission's progress and, once every mission is done, the
//! Tunnel excavation controls. Reads the authoritative snapshot and sends
//! `InvestTunnel` commands; all logic is server-side.

use bevy::prelude::*;

use frozen_city::game::types::{
    GamePhase, PlayerCommand, TUNNEL_INVEST_COAL, TUNNEL_INVEST_WOOD, TUNNEL_STAGES,
};
use frozen_city::net::protocol::ClientMsg;

use super::i18n::Lang;
use super::i18n_names;
use super::i18n_panels;
use super::theme::{self, FormFactor};
use super::ui::UiBlocker;
use super::{GameView, NetConn, Screen};

/// Fixed mission rows (there are 5 missions; a little headroom).
const ROWS: usize = 6;

const DONE: Color = Color::srgb(0.550, 0.880, 0.500);
const TUNNEL_COL: Color = Color::srgb(0.700, 0.820, 1.000);

#[derive(Component)]
struct TitleText;

#[derive(Component)]
struct MissionRow(usize);

#[derive(Component)]
struct TunnelSection;

#[derive(Component)]
struct TunnelText;

#[derive(Component)]
struct TunnelInvestBtn;

#[derive(Component)]
struct TunnelInvestLabel;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Game), spawn_panel).add_systems(
        Update,
        (update_panel, invest_button).run_if(in_state(Screen::Game)),
    );
}

fn spawn_panel(mut commands: Commands, ff: Res<FormFactor>) {
    let ff = *ff;
    // Mobile: cap the panel to roughly 46% of the window width and use the
    // micro font size, per the task brief (no collapse-toggle — out of scope).
    let width = if ff.compact() { Val::Percent(46.0) } else { Val::Px(320.0) };
    let row_font = if ff.compact() { theme::FS_MICRO } else { theme::FS_SMALL };
    // Mobile: the events feed sits right above this panel (also right-
    // anchored, `ui::spawn_hud`'s "Event feed" at `top:54`) and can grow to
    // several lines — 232px (tuned for Desktop's feed height) isn't enough
    // clearance on a phone screen, so start well below it instead.
    let top = if ff.compact() { 204.0 } else { 232.0 };
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(12.0),
                top: Val::Px(top),
                width,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(theme::SP_XS),
                padding: UiRect::all(Val::Px(theme::SP_SM)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(theme::RAD_PANEL)),
                ..default()
            },
            BackgroundColor(theme::BG_PANEL),
            BorderColor::all(theme::BORDER),
            BoxShadow::new(
                Color::srgba(0.0, 0.0, 0.0, 0.35),
                Val::Px(0.0),
                Val::Px(4.0),
                Val::Px(1.0),
                Val::Px(12.0),
            ),
            Interaction::default(),
            UiBlocker,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            // Maqsad-tracker sarlavhasi: mis rang + mis ostki chiziq —
            // skrinshotdagi vazifa kartasi uslubi (dizayn-tizim BRASS urg'usi).
            p.spawn((theme::text("", theme::FS_SMALL, theme::BRASS), TitleText));
            p.spawn(theme::brass_divider());
            for i in 0..ROWS {
                p.spawn((theme::text("", row_font, theme::TEXT_MUTED), MissionRow(i)));
            }

            // Tunnel section, hidden until every mission is complete.
            p.spawn((
                Node {
                    display: Display::None,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(theme::SP_XS),
                    margin: UiRect::top(Val::Px(theme::SP_SM)),
                    ..default()
                },
                TunnelSection,
            ))
            .with_children(|t| {
                t.spawn((theme::text("", row_font, TUNNEL_COL), TunnelText));
                t.spawn((
                    Button,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(ff.btn_h()),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                        ..default()
                    },
                    // No BaseColor: this button's color is owned entirely by
                    // update_panel (affordability dim/bright), so the generic
                    // hover system must not fight it over BackgroundColor.
                    BackgroundColor(theme::BTN_SUCCESS),
                    TunnelInvestBtn,
                ))
                .with_children(|b| {
                    b.spawn((theme::text("", row_font, theme::TEXT_PRIMARY), TunnelInvestLabel));
                });
            });
        });
}

#[allow(clippy::too_many_arguments)]
fn update_panel(
    view: Res<GameView>,
    lang: Res<Lang>,
    mut title: Query<&mut Text, (With<TitleText>, Without<MissionRow>, Without<TunnelText>)>,
    mut rows: Query<(&MissionRow, &mut Text, &mut TextColor), Without<TunnelText>>,
    mut section: Query<&mut Node, With<TunnelSection>>,
    mut tunnel_text: Query<&mut Text, (With<TunnelText>, Without<MissionRow>, Without<TitleText>)>,
    mut invest_bg: Query<&mut BackgroundColor, With<TunnelInvestBtn>>,
    mut invest_label: Query<&mut Text, (With<TunnelInvestLabel>, Without<MissionRow>, Without<TunnelText>, Without<TitleText>)>,
) {
    let lang = *lang;
    if let Ok(mut t) = title.single_mut() {
        if t.0 != i18n_panels::missions_title(lang) {
            t.0 = i18n_panels::missions_title(lang).to_string();
        }
    }
    if let Ok(mut t) = invest_label.single_mut() {
        let new = i18n_panels::tunnel_invest_btn(TUNNEL_INVEST_WOOD, TUNNEL_INVEST_COAL, lang);
        if t.0 != new {
            t.0 = new;
        }
    }

    let Some(state) = view.state.as_ref() else { return };

    for (row, mut t, mut c) in &mut rows {
        let (s, col) = match state.missions.get(row.0) {
            Some(m) => {
                let cur = state.mission_current(m.kind).min(m.kind.target());
                let label = i18n_names::mission_label(m.kind, lang);
                if m.done {
                    (i18n_panels::mission_done_row(label, m.kind.target()), DONE)
                } else {
                    (
                        i18n_panels::mission_pending_row(label, m.kind.target(), cur),
                        theme::TEXT_MUTED,
                    )
                }
            }
            None => (String::new(), theme::TEXT_MUTED),
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
            let new = i18n_panels::tunnel_status(state.tunnel.stage, TUNNEL_STAGES, pct, lang);
            if tt.0 != new {
                tt.0 = new;
            }
        }
        let affordable = state.stock.wood >= TUNNEL_INVEST_WOOD && state.stock.coal >= TUNNEL_INVEST_COAL;
        if let Ok(mut bg) = invest_bg.single_mut() {
            let want = if affordable { theme::BTN_SUCCESS } else { theme::BTN_DIM };
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
