use bevy::prelude::*;

use frozen_city::game::types::{PlayerCommand, FATIGUE_EXHAUSTED, FATIGUE_TIRED};
use frozen_city::net::protocol::ClientMsg;

use super::super::chat::ChatState;
use super::super::i18n::Lang;
use super::super::i18n_names;
use super::super::i18n_panels;
use super::super::theme::{self, BaseColor, FormFactor};
use super::super::ui::{AssignHereBtn, AssignHereLabel, UiBlocker};
use super::super::{GameView, NetConn, Screen, Selection};
use super::*;

/// Visible rows at once. Population can reach `MAX_POPULATION` (60), which
/// doesn't fit on screen — idle survivors sort first (most actionable), the
/// rest are summarized by a trailing "+N more" line rather than scrolling
/// further (the modal itself scrolls via `theme::modal_panel`).
const ROSTER_ROWS: usize = 20;

pub(crate) fn spawn_roster(mut commands: Commands, ff: Res<FormFactor>) {
    let ff = *ff;
    commands
        .spawn((theme::scrim(ff), UiBlocker, RosterRoot, DespawnOnExit(Screen::Game)))
        .with_children(|p| {
            p.spawn(theme::modal_panel(ff)).with_children(|panel| {
                panel.spawn((theme::title(""), RosterTitle));
                panel.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_MUTED), RosterSubtitle));
                for i in 0..ROSTER_ROWS {
                    panel
                        .spawn((
                            Node {
                                display: Display::None,
                                align_items: AlignItems::Center,
                                column_gap: Val::Px(theme::SP_SM),
                                ..default()
                            },
                            RosterRow(i),
                        ))
                        .with_children(|row| {
                            // Name (left, shrinkable) + profession/status
                            // (middle, fixed but non-shrinking so a long
                            // "Profession L2 — Leader, Building" line doesn't
                            // get squeezed into wrapping onto two lines,
                            // which would otherwise sit visually above this
                            // row's vertical center) + unassign (right) — all
                            // three in one row, centered on the cross axis.
                            row.spawn((
                                Button,
                                Node {
                                    flex_grow: 1.0,
                                    flex_shrink: 1.0,
                                    min_width: Val::Px(0.0),
                                    align_items: AlignItems::Center,
                                    padding: UiRect::axes(Val::Px(theme::SP_SM), Val::Px(theme::SP_XS)),
                                    border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                                    ..default()
                                },
                                BackgroundColor(theme::BG_SECTION),
                                BaseColor(theme::BG_SECTION),
                                RosterRowBtn { row: i, survivor: None },
                            ))
                            .with_children(|btn| {
                                btn.spawn((
                                    theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY),
                                    RosterName(i),
                                ));
                            });
                            row.spawn((
                                theme::text("", theme::FS_MICRO, theme::TEXT_MUTED),
                                RosterStatus(i),
                                Node {
                                    width: Val::Px(170.0),
                                    flex_shrink: 0.0,
                                    ..default()
                                },
                            ));
                            row.spawn((
                                Button,
                                Node {
                                    width: Val::Px(ff.btn_h() * 0.55),
                                    height: Val::Px(ff.btn_h() * 0.5),
                                    flex_shrink: 0.0,
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                                    ..default()
                                },
                                BackgroundColor(theme::BTN_DANGER),
                                BaseColor(theme::BTN_DANGER),
                                UnassignBtn { row: i, survivor: None },
                            ))
                            .with_children(|b| {
                                b.spawn(theme::text("x", theme::FS_MICRO, theme::TEXT_PRIMARY));
                            });
                        });
                }
                panel.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_MUTED), MoreText));
            });
        });
}

pub(crate) fn toggle_roster(
    keys: Res<ButtonInput<KeyCode>>,
    chat: Res<ChatState>,
    mut open: ResMut<RosterOpen>,
) {
    if !chat.active && keys.just_pressed(KeyCode::KeyP) {
        open.0 = !open.0;
    }
    if open.0 && keys.just_pressed(KeyCode::Escape) {
        open.0 = false;
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_roster(
    open: Res<RosterOpen>,
    view: Res<GameView>,
    lang: Res<Lang>,
    mut sel: ResMut<SurvivorSelection>,
    mut root: Query<&mut Node, With<RosterRoot>>,
    mut title: Query<&mut Text, (With<RosterTitle>, Without<RosterRow>, Without<RosterName>, Without<RosterStatus>, Without<RosterSubtitle>)>,
    #[allow(clippy::type_complexity)]
    mut subtitle: Query<
        &mut Text,
        (With<RosterSubtitle>, Without<RosterRow>, Without<RosterName>, Without<RosterStatus>, Without<RosterTitle>),
    >,
    mut rows: Query<(&RosterRow, &mut Node), Without<RosterRoot>>,
    mut names: Query<(&RosterName, &mut Text), Without<RosterStatus>>,
    mut statuses: Query<(&RosterStatus, &mut Text), Without<RosterName>>,
    mut row_btns: Query<(&mut RosterRowBtn, &mut BackgroundColor)>,
    mut unassign_btns: Query<&mut UnassignBtn>,
    #[allow(clippy::type_complexity)]
    mut more: Query<
        (&mut Text, &mut Node),
        (
            With<MoreText>,
            Without<RosterRoot>,
            Without<RosterRow>,
            Without<RosterName>,
            Without<RosterStatus>,
            Without<RosterTitle>,
            Without<RosterSubtitle>,
        ),
    >,
) {
    let lang = *lang;
    let display = if open.0 { Display::Flex } else { Display::None };
    for mut node in &mut root {
        if node.display != display {
            node.display = display;
        }
    }
    if let Ok(mut t) = title.single_mut() {
        let new = format!("{}   {}", i18n_panels::roster_title(lang), i18n_panels::roster_hint(lang));
        if t.0 != new {
            t.0 = new;
        }
    }
    if let Ok(mut t) = subtitle.single_mut() {
        if t.0 != i18n_panels::roster_subtitle(lang) {
            t.0 = i18n_panels::roster_subtitle(lang).to_string();
        }
    }
    if !open.0 {
        return;
    }
    let Some(state) = view.state.as_ref() else { return };

    // Drop the selection if that survivor no longer exists (died).
    if let Some(id) = sel.0 {
        if !state.survivors.iter().any(|s| s.id == id) {
            sel.0 = None;
        }
    }

    // In the central world only YOUR settlers are listed — they're the only
    // ones you can command (`can_issue` enforces it server-side), and a
    // shared map's full population would drown the panel in strangers.
    let my_account = view
        .player_id
        .and_then(|pid| state.players.iter().find(|p| p.id == pid))
        .and_then(|p| p.account);
    // Idle survivors first (most actionable), stable by id otherwise.
    let mut sorted: Vec<&frozen_city::game::types::Survivor> = state
        .survivors
        .iter()
        .filter(|s| !state.central || (my_account.is_some() && s.owner == my_account))
        .collect();
    sorted.sort_by_key(|s| (s.assigned_building.is_some(), s.id));

    for (row, mut node) in &mut rows {
        let shown = row.0 < sorted.len().min(ROSTER_ROWS);
        let d = if shown { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
        }
    }
    for (name, mut t) in &mut names {
        let new = sorted.get(name.0).map(|s| s.name.clone()).unwrap_or_default();
        if t.0 != new {
            t.0 = new;
        }
    }
    for (status, mut t) in &mut statuses {
        let new = sorted
            .get(status.0)
            .map(|s| {
                let workplace = match s.assigned_building {
                    Some(b_id) => state
                        .find_building(b_id)
                        .map(|b| i18n_names::building_name(b.kind, lang).to_string())
                        .unwrap_or_else(|| i18n_panels::status_idle(lang).to_string()),
                    None if s.move_target.is_some() => i18n_panels::status_moving(lang).to_string(),
                    None => i18n_panels::status_idle(lang).to_string(),
                };
                // V0.17: compact health suffix — sick outranks fatigue (a
                // more urgent state), rested survivors get no suffix at all
                // so a healthy roster stays uncluttered. Full detail (percent,
                // days left) lives on the detail card (`card.rs`); this row
                // is width-constrained, so just the tag.
                let health_tag = if s.is_sick() {
                    format!(" · {}", i18n_panels::sick_tag(lang))
                } else if s.fatigue >= FATIGUE_EXHAUSTED {
                    format!(" · {}", i18n_panels::fatigue_tier_exhausted(lang))
                } else if s.fatigue >= FATIGUE_TIRED {
                    format!(" · {}", i18n_panels::fatigue_tier_tired(lang))
                } else {
                    String::new()
                };
                // Leader gets its own status word ahead of the workplace —
                // the settlement's one leader is always worth flagging in the
                // list, not just on the detail card.
                if state.leader == Some(s.id) {
                    format!("{} — {}, {workplace}{health_tag}", profession_level_tag(s, lang), i18n_panels::status_leader(lang))
                } else {
                    format!("{} — {workplace}{health_tag}", profession_level_tag(s, lang))
                }
            })
            .unwrap_or_default();
        if t.0 != new {
            t.0 = new;
        }
    }
    for (mut btn, mut bg) in &mut row_btns {
        let survivor = sorted.get(btn.row).map(|s| s.id);
        if btn.survivor != survivor {
            btn.survivor = survivor;
        }
        let want = if survivor.is_some() && survivor == sel.0 { theme::BTN_ACTIVE } else { theme::BTN };
        if bg.0 != want {
            bg.0 = want;
        }
    }
    for mut unassign in &mut unassign_btns {
        unassign.survivor = sorted
            .get(unassign.row)
            .and_then(|s| s.assigned_building.map(|_| s.id));
    }

    if let Ok((mut t, mut node)) = more.single_mut() {
        if sorted.len() > ROSTER_ROWS {
            let n = sorted.len() - ROSTER_ROWS;
            let label = i18n_panels::roster_more(n, lang);
            if t.0 != label {
                t.0 = label;
            }
            if node.display != Display::Flex {
                node.display = Display::Flex;
            }
        } else if node.display != Display::None {
            node.display = Display::None;
        }
    }
}

pub(crate) fn roster_row_buttons(
    mut sel: ResMut<SurvivorSelection>,
    clicked: Query<(&Interaction, &RosterRowBtn), Changed<Interaction>>,
) {
    for (interaction, btn) in &clicked {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Some(id) = btn.survivor else { continue };
        sel.0 = if sel.0 == Some(id) { None } else { Some(id) };
    }
}

pub(crate) fn unassign_buttons(
    net: Res<NetConn>,
    mut sel: ResMut<SurvivorSelection>,
    clicked: Query<(&Interaction, &UnassignBtn), Changed<Interaction>>,
) {
    for (interaction, btn) in &clicked {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Some(survivor) = btn.survivor else { continue };
        net.send(ClientMsg::Cmd(PlayerCommand::AssignSurvivor { survivor, building: None }));
        if sel.0 == Some(survivor) {
            sel.0 = None;
        }
    }
}

pub(crate) fn update_assign_here(
    view: Res<GameView>,
    lang: Res<Lang>,
    selection: Res<Selection>,
    survivor_sel: Res<SurvivorSelection>,
    mut btns: Query<&mut Node, With<AssignHereBtn>>,
    mut labels: Query<&mut Text, With<AssignHereLabel>>,
) {
    let Some(state) = view.state.as_ref() else { return };
    let target = selection.0.and_then(|id| state.find_building(id));
    let survivor = survivor_sel.0.and_then(|id| state.survivors.iter().find(|s| s.id == id));

    // Matches `selection.rs`'s `has_workers`/`input.rs`'s capacity check: a
    // construction site (Furnace/Tent included, both max_workers()==0 once
    // finished) takes a named crew up to CONSTRUCTION_CREW_MAX too, not just
    // kinds with a nonzero finished-state worker cap.
    let show = matches!(
        (target, survivor),
        (Some(b), Some(_)) if b.under_construction() || b.kind.max_workers() > 0
    );
    let d = if show { Display::Flex } else { Display::None };
    for mut node in &mut btns {
        if node.display != d {
            node.display = d;
        }
    }
    if !show {
        return;
    }
    let label = i18n_panels::assign_here(&survivor.unwrap().name, *lang);
    if let Ok(mut t) = labels.single_mut() {
        if t.0 != label {
            t.0 = label;
        }
    }
}

pub(crate) fn assign_here_button(
    net: Res<NetConn>,
    selection: Res<Selection>,
    mut survivor_sel: ResMut<SurvivorSelection>,
    clicked: Query<&Interaction, (Changed<Interaction>, With<AssignHereBtn>)>,
) {
    for interaction in &clicked {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let (Some(building), Some(survivor)) = (selection.0, survivor_sel.0) else { continue };
        net.send(ClientMsg::Cmd(PlayerCommand::AssignSurvivor { survivor, building: Some(building) }));
        survivor_sel.0 = None;
    }
}
