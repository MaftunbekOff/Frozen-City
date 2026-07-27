//! V0.16: the Clash-of-Clans-style placement confirm bar. Once a building is
//! dropped onto a tile (`PendingPlace`, set by `input::build_input`), a small
//! world-anchored row of three buttons floats over it — ✓ confirm, ✗ cancel,
//! ⟳ rotate — instead of the old "left-click places instantly, build mode
//! stays armed" flow. Confirming sends the one `Place` command (with the
//! chosen `facing`) and disarms build mode; nothing is committed before that.
//!
//! The bar is screen-space UI re-projected every frame onto the pending tile
//! via `Camera::world_to_viewport` — the same trick the selection panel
//! (`selection::sync_selection_panel_position`) and chat bubbles use.

use bevy::prelude::*;

use frozen_city::game::types::PlayerCommand;
use frozen_city::net::protocol::ClientMsg;

use super::super::theme::{self, BaseColor, BTN_DANGER, BTN_SUCCESS, TEXT_PRIMARY};
use super::super::*;

/// Root row of the three confirm-bar buttons. Hidden (`Display::None`) unless a
/// `PendingPlace` exists.
#[derive(Component)]
pub struct PlacementBar;

#[derive(Component)]
pub struct ConfirmBtn;

#[derive(Component)]
pub struct CancelBtn;

#[derive(Component)]
pub struct RotateBtn;

const BTN_W: f32 = 52.0;
const BTN_H: f32 = 42.0;
/// Kept in sync with the row's real width (3 buttons + 2 gaps) so the anchor
/// can centre it over the tile.
const BAR_W: f32 = BTN_W * 3.0 + theme::SP_XS * 2.0;
const GAP: f32 = 30.0;

/// Spawn the (initially hidden) confirm bar once, on entering the game.
pub fn spawn_placement_controls(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(theme::SP_XS),
                ..default()
            },
            PlacementBar,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|row| {
            spawn_btn(row, ConfirmBtn, "✓", BTN_SUCCESS);
            spawn_btn(row, RotateBtn, "⟳", theme::BTN);
            spawn_btn(row, CancelBtn, "✗", BTN_DANGER);
        });
}

fn spawn_btn(row: &mut ChildSpawnerCommands, marker: impl Component, glyph: &str, bg: Color) {
    row.spawn((
        Button,
        Node {
            width: Val::Px(BTN_W),
            height: Val::Px(BTN_H),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(1.5)),
            border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
            ..default()
        },
        BackgroundColor(bg),
        BaseColor(bg),
        BorderColor::all(theme::BRASS),
        BoxShadow::new(
            Color::srgba(0.0, 0.0, 0.0, 0.45),
            Val::Px(0.0),
            Val::Px(3.0),
            Val::Px(1.0),
            Val::Px(9.0),
        ),
        marker,
    ))
    .with_children(|b| {
        b.spawn(theme::text(glyph, theme::FS_SECTION, TEXT_PRIMARY));
    });
}

/// Show/hide the bar and re-project it over the pending tile every frame.
pub fn sync_placement_controls(
    view: Res<GameView>,
    pending: Res<PendingPlace>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut bar: Query<&mut Node, With<PlacementBar>>,
) {
    let Ok(mut node) = bar.single_mut() else { return };
    let Some(p) = pending.0.filter(|_| view.ready().is_some()) else {
        if node.display != Display::None {
            node.display = Display::None;
        }
        return;
    };
    let (Ok((cam, cam_gt)), Ok(window)) = (camera.single(), windows.single()) else { return };

    // Float above the tile's centre, like the selection panel.
    let anchor = tile_center_world(p.tx, p.ty) + Vec3::Y * 1.2;
    let Ok(screen) = cam.world_to_viewport(cam_gt, anchor) else {
        // Behind the camera / off-projection: hide rather than snap to a corner.
        if node.display != Display::None {
            node.display = Display::None;
        }
        return;
    };

    let max_left = (window.width() - BAR_W).max(0.0);
    let max_top = (window.height() - BTN_H).max(0.0);
    let above = screen.y - BTN_H - GAP;
    let top = if above >= 0.0 { above } else { screen.y + GAP };

    node.display = Display::Flex;
    node.left = Val::Px((screen.x - BAR_W / 2.0).clamp(0.0, max_left));
    node.top = Val::Px(top.clamp(0.0, max_top));
}

/// Handle the three buttons: ✓ commits the `Place`, ✗ backs out, ⟳ spins the
/// pending building a quarter-turn.
#[allow(clippy::too_many_arguments)]
pub fn placement_buttons(
    net: Res<NetConn>,
    mut pending: ResMut<PendingPlace>,
    mut build: ResMut<BuildMode>,
    mut relocate: ResMut<RelocateMode>,
    confirm: Query<&Interaction, (Changed<Interaction>, With<ConfirmBtn>)>,
    cancel: Query<&Interaction, (Changed<Interaction>, With<CancelBtn>)>,
    rotate: Query<&Interaction, (Changed<Interaction>, With<RotateBtn>)>,
) {
    let Some(p) = pending.0 else { return };
    if confirm.iter().any(|i| *i == Interaction::Pressed) {
        // V0.18: the same bar commits both flows — a brand-new building with
        // `Place`, an existing one being moved with `RelocateFacing` (one
        // command, since relocating re-enters construction and `can_rotate`
        // would then refuse a follow-up turn).
        let cmd = match p.relocating {
            Some(building) => PlayerCommand::RelocateFacing {
                building,
                x: p.tx,
                y: p.ty,
                facing: p.facing,
            },
            None => PlayerCommand::Place {
                kind: p.kind,
                x: p.tx,
                y: p.ty,
                facing: p.facing,
            },
        };
        net.send(ClientMsg::Cmd(cmd));
        // CoC-style: one placement per confirm — disarm build mode instead of
        // leaving it hot (the old "still selected after building" bug). A
        // relocation is one-shot by nature: it targets one specific building,
        // not a repeatable kind.
        pending.0 = None;
        build.0 = None;
        relocate.0 = None;
    } else if cancel.iter().any(|i| *i == Interaction::Pressed) {
        // Full cancel: drop the pending building and disarm both modes. For a
        // relocation this leaves the building exactly where it was — nothing
        // was ever sent.
        pending.0 = None;
        build.0 = None;
        relocate.0 = None;
    } else if rotate.iter().any(|i| *i == Interaction::Pressed) {
        pending.0 = Some(PendingPlaceData { facing: (p.facing + 1) % 4, ..p });
    }
}
