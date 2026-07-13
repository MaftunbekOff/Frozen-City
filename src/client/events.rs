//! Dynamic-event UI: a non-blocking caravan choice popup (Accept / Decline)
//! and a status line for active disease / blizzard. Reads the authoritative
//! snapshot and sends `RespondEvent`; the sim owns all logic and timers.
//! The caravan dialog BODY is server-authored text and is never translated
//! here — only the frame and the two client-side button labels are.

use bevy::prelude::*;

use frozen_city::game::types::{GamePhase, PlayerCommand};
use frozen_city::net::protocol::ClientMsg;

use super::i18n::Lang;
use super::i18n_panels;
use super::theme::{self, FormFactor};
use super::ui::UiBlocker;
use super::{GameView, NetConn, Screen};

const SICK_COL: Color = Color::srgb(0.720, 0.900, 0.450);
const COLD_COL: Color = Color::srgb(0.600, 0.820, 1.000);

#[derive(Component)]
struct CaravanRoot;

#[derive(Component)]
struct CaravanText;

#[derive(Component)]
struct CaravanAcceptBtn;

#[derive(Component)]
struct CaravanAcceptLabel;

#[derive(Component)]
struct CaravanDeclineBtn;

#[derive(Component)]
struct CaravanDeclineLabel;

#[derive(Component)]
struct EventStatusText;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Game), spawn_events_ui)
        .add_systems(
            Update,
            (update_events, caravan_buttons).run_if(in_state(Screen::Game)),
        );
}

fn spawn_events_ui(mut commands: Commands, ff: Res<FormFactor>) {
    let ff = *ff;
    // Active-event status line, centered just below the top bar (and, on
    // Desktop, below the central temperature medallion + clock chip, which
    // together hang down to ~130px — see `ui::hud::spawn_center_gauge`).
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(134.0),
                justify_content: JustifyContent::Center,
                column_gap: Val::Px(theme::SP_LG),
                ..default()
            },
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn((theme::text("", theme::FS_BODY, SICK_COL), EventStatusText));
        });

    // Caravan choice popup: same scrim+modal_panel shell as roster/social/
    // research. Visible by default for one frame until `update_events`'
    // very first tick hides it (no pending event yet) — same idiom those
    // modals use for their `*Open` resource's `false` default.
    commands
        .spawn((theme::scrim(ff), UiBlocker, CaravanRoot, DespawnOnExit(Screen::Game)))
        .with_children(|p| {
            p.spawn(theme::modal_panel(ff)).with_children(|panel| {
                panel.spawn((theme::text("", theme::FS_BODY, theme::TEXT_PRIMARY), CaravanText));
                panel
                    .spawn(Node {
                        column_gap: Val::Px(theme::SP_MD),
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            theme::button(Val::Px(120.0), ff.btn_h(), theme::BTN_SUCCESS),
                            CaravanAcceptBtn,
                        ))
                        .with_children(|b| {
                            b.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY), CaravanAcceptLabel));
                        });
                        row.spawn((
                            theme::button(Val::Px(120.0), ff.btn_h(), theme::BTN_DANGER),
                            CaravanDeclineBtn,
                        ))
                        .with_children(|b| {
                            b.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY), CaravanDeclineLabel));
                        });
                    });
            });
        });
}

#[allow(clippy::too_many_arguments)]
fn update_events(
    view: Res<GameView>,
    lang: Res<Lang>,
    mut root: Query<&mut Node, With<CaravanRoot>>,
    mut caravan_text: Query<&mut Text, (With<CaravanText>, Without<EventStatusText>)>,
    mut accept_label: Query<&mut Text, (With<CaravanAcceptLabel>, Without<CaravanText>, Without<EventStatusText>)>,
    mut decline_label: Query<
        &mut Text,
        (With<CaravanDeclineLabel>, Without<CaravanText>, Without<EventStatusText>, Without<CaravanAcceptLabel>),
    >,
    mut status: Query<(&mut Text, &mut TextColor), (With<EventStatusText>, Without<CaravanText>)>,
) {
    let lang = *lang;
    if let Ok(mut t) = accept_label.single_mut() {
        if t.0 != i18n_panels::caravan_accept(lang) {
            t.0 = i18n_panels::caravan_accept(lang).to_string();
        }
    }
    if let Ok(mut t) = decline_label.single_mut() {
        if t.0 != i18n_panels::caravan_decline(lang) {
            t.0 = i18n_panels::caravan_decline(lang).to_string();
        }
    }

    let Some(state) = view.state.as_ref() else { return };
    // Once the game is over, `tick` stops running so events never clear — hide
    // the popup and status line rather than let them freeze over the game-over
    // overlay (and intercept its clicks).
    let running = state.phase == GamePhase::Running;

    // Caravan popup. Dialog body is server-authored (refugee count / food
    // cost come from the snapshot) and intentionally left untranslated.
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
            (true, true) => (i18n_panels::event_sickness_blizzard(lang).to_string(), COLD_COL),
            (true, false) => (i18n_panels::event_sickness(lang).to_string(), SICK_COL),
            (false, true) => (i18n_panels::event_blizzard(lang).to_string(), COLD_COL),
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
