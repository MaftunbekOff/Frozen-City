//! Survivor roster: a modal panel (toggled with `P`) listing every named
//! `Survivor`, idle ones first, letting the player select one and assign
//! them to a specific building (`AssignSurvivor`) — distinct from the
//! anonymous `AdjustWorkers` +/- already on the building selection panel.
//! Structured like `research.rs`'s modal; the "Assign here" button itself
//! lives on `ui.rs`'s `SelPanelRoot` (the building panel) since it needs a
//! building to be selected there too, but its visibility/label/click logic
//! lives here since it also needs `SurvivorSelection`.
//!
//! Also owns the survivor detail card (V0.7): a small always-available panel
//! (distinct from the roster modal, which is toggled with P) shown whenever
//! `SurvivorSelection` is set — from a roster row click OR a world click on a
//! survivor (`input::resolve_world_click`) — with the survivor's stats and
//! Make Leader / Unassign actions.

use bevy::prelude::*;

use frozen_city::game::types::{PlayerCommand, Survivor, XP_DAYS_LEVEL_1, XP_DAYS_LEVEL_2, XP_DAYS_LEVEL_3};
use frozen_city::net::protocol::ClientMsg;

use super::chat::ChatState;
use super::i18n::Lang;
use super::i18n_names;
use super::i18n_panels;
use super::theme::{self, BaseColor, FormFactor};
use super::ui::{AssignHereBtn, AssignHereLabel, UiBlocker};
use super::{GameView, NetConn, Screen, Selection};

/// Client-side mirror of `fc_game::sim::xp_level` (private to the sim crate):
/// XP level (0..=3) from accrued in-game work-days, thresholded by the same
/// public `XP_DAYS_LEVEL_*` cumulative-total constants the sim uses.
pub fn xp_level(xp: f32) -> u8 {
    if xp >= XP_DAYS_LEVEL_3 {
        3
    } else if xp >= XP_DAYS_LEVEL_2 {
        2
    } else if xp >= XP_DAYS_LEVEL_1 {
        1
    } else {
        0
    }
}

/// Short "Profession Lx" tag used by both the roster rows and the detail
/// card, e.g. "Miner L2".
pub fn profession_level_tag(s: &Survivor, l: Lang) -> String {
    format!("{} L{}", i18n_names::profession_name(s.profession, l), xp_level(s.xp))
}

/// Visible rows at once. Population can reach `MAX_POPULATION` (60), which
/// doesn't fit on screen — idle survivors sort first (most actionable), the
/// rest are summarized by a trailing "+N more" line rather than scrolling
/// further (the modal itself scrolls via `theme::modal_panel`).
const ROSTER_ROWS: usize = 20;

/// Whether the roster modal is open (also gates world/camera input).
#[derive(Resource, Default)]
pub struct RosterOpen(pub bool);

/// The survivor currently selected in the roster, if any. Distinct from
/// `Selection` (building-only, shared by desktop input and touch) — a
/// building and a survivor can be selected at the same time, e.g. while
/// assigning one to the other.
#[derive(Resource, Default)]
pub struct SurvivorSelection(pub Option<u32>);

#[derive(Component)]
struct RosterRoot;

#[derive(Component)]
struct RosterTitle;

#[derive(Component)]
struct RosterSubtitle;

#[derive(Component)]
struct RosterRow(usize);

#[derive(Component)]
struct RosterName(usize);

#[derive(Component)]
struct RosterStatus(usize);

/// Clicking selects `survivor` (or deselects if already selected). Kept in
/// sync with the current sorted roster order every frame in `update_roster`,
/// so the click handler never has to reconstruct the sort itself.
#[derive(Component)]
struct RosterRowBtn {
    row: usize,
    survivor: Option<u32>,
}

#[derive(Component)]
struct UnassignBtn {
    row: usize,
    survivor: Option<u32>,
}

#[derive(Component)]
struct MoreText;

// ------------------------------------------------------------ detail card

#[derive(Component)]
struct CardRoot;

#[derive(Component, Clone, Copy, PartialEq)]
enum CardText {
    Name,
    Stats,
}

#[derive(Component)]
struct CardCloseBtn;

#[derive(Component)]
struct CardLeaderBtn;

#[derive(Component)]
struct CardLeaderLabel;

#[derive(Component)]
struct CardUnassignBtn;

#[derive(Component)]
struct CardUnassignLabel;

pub fn plugin(app: &mut App) {
    app.init_resource::<RosterOpen>()
        .init_resource::<SurvivorSelection>()
        .add_systems(OnEnter(Screen::Game), (spawn_roster, spawn_card))
        .add_systems(
            Update,
            (
                toggle_roster,
                update_roster,
                roster_row_buttons,
                unassign_buttons,
                update_assign_here,
                assign_here_button,
                update_card,
                card_buttons,
            )
                .run_if(in_state(Screen::Game)),
        );
}

fn spawn_roster(mut commands: Commands, ff: Res<FormFactor>) {
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

fn toggle_roster(
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
fn update_roster(
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
                // Leader gets its own status word ahead of the workplace —
                // the settlement's one leader is always worth flagging in the
                // list, not just on the detail card.
                if state.leader == Some(s.id) {
                    format!("{} — {}, {workplace}", profession_level_tag(s, lang), i18n_panels::status_leader(lang))
                } else {
                    format!("{} — {workplace}", profession_level_tag(s, lang))
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

fn roster_row_buttons(
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

fn unassign_buttons(
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

fn update_assign_here(
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

    let show = matches!(
        (target, survivor),
        (Some(b), Some(_)) if b.kind.max_workers() > 0
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

fn assign_here_button(
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

/// Survivor detail card: shown whenever `SurvivorSelection` is set, whether
/// from a roster row click or a world click on a survivor
/// (`input::resolve_world_click`). Sits below the players panel (`roles.rs`,
/// which ends around y=536) in the same left-side column.
fn spawn_card(mut commands: Commands) {
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
fn update_card(
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
                format!(
                    "{}{leader_tag}\n{}\n{}",
                    profession_level_tag(s, lang),
                    i18n_panels::card_stats_line(s.hp, s.hunger, lang),
                    i18n_panels::card_working(&workplace, lang),
                )
            }
        };
        if t.0 != new {
            t.0 = new;
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

fn card_buttons(
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
