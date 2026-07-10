//! Survivor roster: a modal panel (toggled with `P`) listing every named
//! `Survivor`, idle ones first, letting the player select one and assign
//! them to a specific building (`AssignSurvivor`) — distinct from the
//! anonymous `AdjustWorkers` +/- already on the building selection panel.
//! Structured like `research.rs`'s modal; the "Assign here" button itself
//! lives on `ui.rs`'s `SelPanelRoot` (the building panel) since it needs a
//! building to be selected there too, but its visibility/label/click logic
//! lives here since it also needs `SurvivorSelection`.

use bevy::prelude::*;

use frozen_city::game::types::PlayerCommand;
use frozen_city::net::protocol::ClientMsg;

use super::chat::ChatState;
use super::ui::{AssignHereBtn, AssignHereLabel, BaseColor, UiBlocker};
use super::{GameView, NetConn, Screen, Selection};

/// Visible rows at once. Population can reach `MAX_POPULATION` (60), which
/// doesn't fit on screen — idle survivors sort first (most actionable), the
/// rest are summarized by a trailing "+N more" line rather than scrolling
/// (no panel in this codebase scrolls yet).
const ROSTER_ROWS: usize = 20;

const PANEL_BG: Color = Color::srgba(0.05, 0.08, 0.13, 0.98);
const ROW_BG: Color = Color::srgb(0.16, 0.20, 0.28);
const ROW_SELECTED: Color = Color::srgb(0.36, 0.30, 0.14);
const UNASSIGN_BG: Color = Color::srgb(0.45, 0.16, 0.14);
const TEXT_MAIN: Color = Color::srgb(0.90, 0.93, 0.97);
const TEXT_DIM: Color = Color::srgb(0.62, 0.68, 0.78);

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

pub fn plugin(app: &mut App) {
    app.init_resource::<RosterOpen>()
        .init_resource::<SurvivorSelection>()
        .add_systems(OnEnter(Screen::Game), spawn_roster)
        .add_systems(
            Update,
            (
                toggle_roster,
                update_roster,
                roster_row_buttons,
                unassign_buttons,
                update_assign_here,
                assign_here_button,
            )
                .run_if(in_state(Screen::Game)),
        );
}

fn text(t: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(t.into()), TextFont::from_font_size(size), TextColor(color))
}

fn spawn_roster(mut commands: Commands) {
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
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            Interaction::default(),
            UiBlocker,
            RosterRoot,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: Val::Px(360.0),
                    max_height: Val::Px(520.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    padding: UiRect::all(Val::Px(16.0)),
                    ..default()
                },
                BackgroundColor(PANEL_BG),
            ))
            .with_children(|panel| {
                panel.spawn(text("Survivors   (P or Esc to close)", 18.0, TEXT_MAIN));
                panel.spawn(text(
                    "Pick a survivor, then select a building to assign them.",
                    12.0,
                    TEXT_DIM,
                ));
                for i in 0..ROSTER_ROWS {
                    panel
                        .spawn((
                            Node {
                                display: Display::None,
                                align_items: AlignItems::Center,
                                column_gap: Val::Px(6.0),
                                ..default()
                            },
                            RosterRow(i),
                        ))
                        .with_children(|row| {
                            row.spawn((
                                Button,
                                Node {
                                    flex_grow: 1.0,
                                    padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                                    ..default()
                                },
                                BackgroundColor(ROW_BG),
                                BaseColor(ROW_BG),
                                RosterRowBtn { row: i, survivor: None },
                            ))
                            .with_children(|btn| {
                                btn.spawn((text("", 13.0, TEXT_MAIN), RosterName(i)));
                            });
                            row.spawn((
                                text("", 11.5, TEXT_DIM),
                                RosterStatus(i),
                                Node { width: Val::Px(88.0), ..default() },
                            ));
                            row.spawn((
                                Button,
                                Node {
                                    width: Val::Px(24.0),
                                    height: Val::Px(22.0),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                                BackgroundColor(UNASSIGN_BG),
                                BaseColor(UNASSIGN_BG),
                                UnassignBtn { row: i, survivor: None },
                            ))
                            .with_children(|b| {
                                b.spawn(text("x", 11.0, TEXT_MAIN));
                            });
                        });
                }
                panel.spawn((text("", 12.0, TEXT_DIM), MoreText));
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
    mut sel: ResMut<SurvivorSelection>,
    mut root: Query<&mut Node, With<RosterRoot>>,
    mut rows: Query<(&RosterRow, &mut Node), Without<RosterRoot>>,
    mut names: Query<(&RosterName, &mut Text), Without<RosterStatus>>,
    mut statuses: Query<(&RosterStatus, &mut Text), Without<RosterName>>,
    mut row_btns: Query<(&mut RosterRowBtn, &mut BackgroundColor)>,
    mut unassign_btns: Query<&mut UnassignBtn>,
    mut more: Query<(&mut Text, &mut Node), (With<MoreText>, Without<RosterRow>)>,
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

    // Drop the selection if that survivor no longer exists (died).
    if let Some(id) = sel.0 {
        if !state.survivors.iter().any(|s| s.id == id) {
            sel.0 = None;
        }
    }

    // Idle survivors first (most actionable), stable by id otherwise.
    let mut sorted: Vec<&frozen_city::game::types::Survivor> = state.survivors.iter().collect();
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
            .map(|s| match s.assigned_building {
                Some(b_id) => state
                    .find_building(b_id)
                    .map(|b| b.kind.name().to_string())
                    .unwrap_or_else(|| "Idle".to_string()),
                None => "Idle".to_string(),
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
        let want = if survivor.is_some() && survivor == sel.0 { ROW_SELECTED } else { ROW_BG };
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
            let label = format!("+{n} more (not shown)");
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
    let label = format!("Assign {} here", survivor.unwrap().name);
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
