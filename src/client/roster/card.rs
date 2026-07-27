use bevy::prelude::*;

use frozen_city::game::types::{LifeStage, PlayerCommand, FATIGUE_EXHAUSTED, FATIGUE_TIRED, TICKS_PER_DAY};
use frozen_city::net::protocol::ClientMsg;

use super::super::i18n::Lang;
use super::super::i18n_names;
use super::super::i18n_panels;
use super::super::theme::{self, BaseColor};
use super::super::ui::UiBlocker;
use super::super::{GameView, NetConn, Screen};
use super::*;

/// Survivor detail card: shown whenever `SurvivorSelection` is set, whether
/// from a roster row click or a world click on a survivor
/// (`input::resolve_world_click`). Sits below the players panel (`roles.rs`,
/// which ends around y=536) in the same left-side column.
pub(crate) fn spawn_card(mut commands: Commands) {
    commands
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                top: Val::Px(546.0),
                width: Val::Px(236.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(theme::SP_SM),
                padding: UiRect::all(Val::Px(theme::SP_SM)),
                border_radius: BorderRadius::all(Val::Px(theme::RAD_PANEL)),
                ..default()
            },
            BackgroundColor(theme::BG_PANEL),
            Interaction::default(),
            UiBlocker,
            CardRoot,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn(Node {
                align_items: AlignItems::Center,
                column_gap: Val::Px(theme::SP_SM),
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    theme::text("", theme::FS_BODY, theme::TEXT_PRIMARY),
                    CardText::Name,
                    Node {
                        flex_grow: 1.0,
                        ..default()
                    },
                ));
                row.spawn((
                    Button,
                    Node {
                        width: Val::Px(22.0),
                        height: Val::Px(22.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                        ..default()
                    },
                    BackgroundColor(theme::BTN),
                    BaseColor(theme::BTN),
                    CardCloseBtn,
                ))
                .with_children(|b| {
                    b.spawn(theme::text("x", theme::FS_MICRO, theme::TEXT_PRIMARY));
                });
            });
            p.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_MUTED), CardText::Stats));
            // V0.17: fatigue bar — same `stat_bar_track` widget the bottom-
            // center morale bar uses (`ui::hud::spawn_morale_bar`), just
            // sized to the card's own width; `update_card` drives the
            // fill's percent width and tier color.
            p.spawn(theme::stat_bar_track(Val::Px(216.0), 8.0))
                .with_children(|track| {
                    track.spawn((
                        Node {
                            width: Val::Percent(0.0),
                            height: Val::Percent(100.0),
                            border_radius: BorderRadius::MAX,
                            ..default()
                        },
                        BackgroundColor(theme::SUCCESS),
                        CardFatigueFill,
                    ));
                });
            p.spawn((
                Button,
                Node {
                    display: Display::None,
                    width: Val::Px(216.0),
                    height: Val::Px(28.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                    ..default()
                },
                BackgroundColor(theme::BTN_ACTIVE),
                BaseColor(theme::BTN_ACTIVE),
                CardLeaderBtn,
            ))
            .with_children(|b| {
                b.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY), CardLeaderLabel));
            });
            p.spawn((
                Button,
                Node {
                    display: Display::None,
                    width: Val::Px(216.0),
                    height: Val::Px(28.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                    ..default()
                },
                BackgroundColor(theme::BTN_DANGER),
                BaseColor(theme::BTN_DANGER),
                CardUnassignBtn,
            ))
            .with_children(|b| {
                b.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY), CardUnassignLabel));
            });
        });
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_card(
    view: Res<GameView>,
    lang: Res<Lang>,
    roster_open: Res<RosterOpen>,
    mut sel: ResMut<SurvivorSelection>,
    mut root: Query<&mut Node, With<CardRoot>>,
    #[allow(clippy::type_complexity)]
    mut texts: Query<(&mut Text, &CardText), (Without<CardCloseBtn>, Without<CardLeaderBtn>)>,
    mut leader_btn: Query<
        &mut Node,
        (With<CardLeaderBtn>, Without<CardRoot>, Without<CardUnassignBtn>),
    >,
    mut leader_label: Query<
        &mut Text,
        (With<CardLeaderLabel>, Without<CardText>, Without<CardUnassignLabel>),
    >,
    mut unassign_btn: Query<
        &mut Node,
        (With<CardUnassignBtn>, Without<CardRoot>, Without<CardLeaderBtn>),
    >,
    mut unassign_label: Query<
        &mut Text,
        (With<CardUnassignLabel>, Without<CardText>, Without<CardLeaderLabel>),
    >,
    mut fatigue_fill: Query<
        (&mut Node, &mut BackgroundColor),
        (With<CardFatigueFill>, Without<CardRoot>, Without<CardLeaderBtn>, Without<CardUnassignBtn>),
    >,
) {
    let lang = *lang;
    let Some(state) = view.state.as_ref() else { return };

    // Drop the selection if that survivor no longer exists (died).
    if let Some(id) = sel.0 {
        if !state.survivors.iter().any(|s| s.id == id) {
            sel.0 = None;
        }
    }

    let survivor = sel.0.and_then(|id| state.survivors.iter().find(|s| s.id == id));
    // Hidden while the roster modal is open: it's spawned after (so drawn
    // over) this card, and its full-screen backdrop would otherwise leave the
    // card visually stacked underneath a translucent overlay.
    let show = survivor.is_some() && !roster_open.0;
    let d = if show { Display::Flex } else { Display::None };
    for mut node in &mut root {
        if node.display != d {
            node.display = d;
        }
    }
    let Some(s) = survivor else { return };

    // V0.17: fatigue tier — same three-way banding for the card's text tag
    // and its bar's fill color, computed once here so both stay in sync.
    let (fatigue_tag, fatigue_color) = if s.fatigue >= FATIGUE_EXHAUSTED {
        (i18n_panels::fatigue_tier_exhausted(lang), theme::DANGER)
    } else if s.fatigue >= FATIGUE_TIRED {
        (i18n_panels::fatigue_tier_tired(lang), theme::ACCENT_WARM)
    } else {
        (i18n_panels::fatigue_tier_rested(lang), theme::SUCCESS)
    };

    for (mut t, kind) in &mut texts {
        let new = match kind {
            CardText::Name => s.name.clone(),
            CardText::Stats => {
                let workplace = match s.assigned_building {
                    Some(b_id) => state
                        .find_building(b_id)
                        .map(|b| i18n_names::building_name(b.kind, lang).to_string())
                        .unwrap_or_else(|| i18n_panels::status_idle(lang).to_string()),
                    None if s.move_target.is_some() => i18n_panels::status_moving(lang).to_string(),
                    None => i18n_panels::status_idle(lang).to_string(),
                };
                let leader_tag = if state.leader == Some(s.id) { i18n_panels::leader_tag(lang) } else { "" };
                // V0.18: life stage — Adult is the common case and stays
                // untagged (same "only show when it's not the default"
                // convention `health_tag` in `panel.rs` uses); a Child or
                // Elder gets a short suffix next to the profession tag.
                // `i18n_panels` has no life-stage catalog entry yet (V0.18
                // landed after V1.0's UI catalogs were built out, and that
                // file isn't ours to extend) — unlocalized name-only text
                // (`LifeStage::name()`) is the documented fallback until it
                // does.
                let stage_tag = match s.stage() {
                    LifeStage::Adult => String::new(),
                    other => format!(" · {}", other.name()),
                };
                // Sick line only appears while `is_sick()` — `sick_left`
                // (ticks) converted to in-game days for the player, same unit
                // the rest of the UI reasons in (`GameState::day`, TICKS_PER_DAY).
                let sick_line = if s.is_sick() {
                    let days_left = s.sick_left / TICKS_PER_DAY as f32;
                    format!("\n{}", i18n_panels::card_sick_line(days_left, lang))
                } else {
                    String::new()
                };
                // V0.18: who this survivor is partnered with, if anyone —
                // name only (a proper noun needs no translation), same
                // unlocalized-fallback reasoning as `stage_tag` above.
                let partner_line = match s.partner.and_then(|pid| state.survivors.iter().find(|o| o.id == pid)) {
                    Some(partner) => format!("\n♥ {}", partner.name),
                    None => String::new(),
                };
                format!(
                    "{}{stage_tag}{leader_tag}\n{}\n{}{sick_line}\n{}{partner_line}",
                    profession_level_tag(s, lang),
                    i18n_panels::card_stats_line(s.hp, s.hunger, lang),
                    i18n_panels::card_fatigue_line(s.fatigue, fatigue_tag, lang),
                    i18n_panels::card_working(&workplace, lang),
                )
            }
        };
        if t.0 != new {
            t.0 = new;
        }
    }

    if let Ok((mut node, mut bg)) = fatigue_fill.single_mut() {
        let w = Val::Percent(s.fatigue.clamp(0.0, 100.0));
        if node.width != w {
            node.width = w;
        }
        if bg.0 != fatigue_color {
            bg.0 = fatigue_color;
        }
    }

    if let Ok(mut t) = leader_label.single_mut() {
        if t.0 != i18n_panels::make_leader(lang) {
            t.0 = i18n_panels::make_leader(lang).to_string();
        }
    }
    if let Ok(mut t) = unassign_label.single_mut() {
        if t.0 != i18n_panels::unassign(lang) {
            t.0 = i18n_panels::unassign(lang).to_string();
        }
    }

    // "Make Leader": owner-gated, and never offered in the central world
    // (`can_issue` refuses `SetLeader` there outright — see
    // `GameState::can_issue`'s central-world branch). Hidden entirely rather
    // than shown-disabled, matching how `AssignHereBtn` hides when inapplicable.
    let me = view.player_id.unwrap_or(0);
    let can_lead = !state.central
        && state.can_issue(me, &PlayerCommand::SetLeader { survivor: s.id })
        && state.leader != Some(s.id);
    if let Ok(mut node) = leader_btn.single_mut() {
        let want = if can_lead { Display::Flex } else { Display::None };
        if node.display != want {
            node.display = want;
        }
    }

    let can_unassign = s.assigned_building.is_some();
    if let Ok(mut node) = unassign_btn.single_mut() {
        let want = if can_unassign { Display::Flex } else { Display::None };
        if node.display != want {
            node.display = want;
        }
    }
}

pub(crate) fn card_buttons(
    net: Res<NetConn>,
    mut sel: ResMut<SurvivorSelection>,
    close: Query<&Interaction, (Changed<Interaction>, With<CardCloseBtn>)>,
    leader: Query<&Interaction, (Changed<Interaction>, With<CardLeaderBtn>)>,
    unassign: Query<&Interaction, (Changed<Interaction>, With<CardUnassignBtn>)>,
) {
    if close.iter().any(|i| *i == Interaction::Pressed) {
        sel.0 = None;
        return;
    }
    let Some(survivor) = sel.0 else { return };
    if leader.iter().any(|i| *i == Interaction::Pressed) {
        net.send(ClientMsg::Cmd(PlayerCommand::SetLeader { survivor }));
    }
    if unassign.iter().any(|i| *i == Interaction::Pressed) {
        net.send(ClientMsg::Cmd(PlayerCommand::AssignSurvivor { survivor, building: None }));
        sel.0 = None;
    }
}
