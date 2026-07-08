//! Camera control, build placement and selection.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use frozen_city::game::types::{BuildingKind, GamePhase, PlayerCommand};
use frozen_city::net::protocol::ClientMsg;

use super::render::GhostMarker;
use super::*;

const PAN_SPEED: f32 = 520.0;
const MIN_ZOOM: f32 = 0.4;
const MAX_ZOOM: f32 = 3.0;

pub fn camera_control(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    ui_hover: Res<UiHover>,
    mut cam: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    let Ok((mut transform, mut projection)) = cam.single_mut() else {
        return;
    };
    let Projection::Orthographic(ortho) = &mut *projection else {
        return;
    };

    // Zoom (mouse wheel), unless the cursor is parked on the UI.
    if !ui_hover.0 && scroll.delta.y.abs() > 0.0 {
        let lines = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y,
            MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
        };
        ortho.scale = (ortho.scale * 0.9f32.powf(lines)).clamp(MIN_ZOOM, MAX_ZOOM);
    }

    // Keyboard pan.
    let mut dir = Vec2::ZERO;
    if keys.any_pressed([KeyCode::KeyW, KeyCode::ArrowUp]) {
        dir.y += 1.0;
    }
    if keys.any_pressed([KeyCode::KeyS, KeyCode::ArrowDown]) {
        dir.y -= 1.0;
    }
    if keys.any_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]) {
        dir.x -= 1.0;
    }
    if keys.any_pressed([KeyCode::KeyD, KeyCode::ArrowRight]) {
        dir.x += 1.0;
    }
    if dir != Vec2::ZERO {
        let delta = dir.normalize() * PAN_SPEED * ortho.scale * time.delta_secs();
        transform.translation.x += delta.x;
        transform.translation.y += delta.y;
    }

    // Middle-mouse drag pan.
    if buttons.pressed(MouseButton::Middle) && motion.delta != Vec2::ZERO {
        transform.translation.x -= motion.delta.x * ortho.scale;
        transform.translation.y += motion.delta.y * ortho.scale;
    }

    // Keep the camera near the map.
    let limit = MAP_W as f32 / 2.0 * TILE + 6.0 * TILE;
    transform.translation.x = transform.translation.x.clamp(-limit, limit);
    transform.translation.y = transform.translation.y.clamp(-limit, limit);
}

/// Cursor position in world coordinates, if it is over the window.
pub fn cursor_world(
    window: &Window,
    camera: &Camera,
    cam_transform: &GlobalTransform,
) -> Option<Vec2> {
    let cursor = window.cursor_position()?;
    camera.viewport_to_world_2d(cam_transform, cursor).ok()
}

pub fn build_input(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    ui_hover: Res<UiHover>,
    view: Res<GameView>,
    net: Res<NetConn>,
    mut build: ResMut<BuildMode>,
    mut selection: ResMut<Selection>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut ghost: Query<(&mut Transform, &mut Sprite, &mut Visibility), With<GhostMarker>>,
) {
    // Quick-build hotkeys.
    let hotkeys = [
        (KeyCode::Digit1, BuildingKind::Tent),
        (KeyCode::Digit2, BuildingKind::Sawmill),
        (KeyCode::Digit3, BuildingKind::CoalMine),
        (KeyCode::Digit4, BuildingKind::HunterHut),
    ];
    for (key, kind) in hotkeys {
        if keys.just_pressed(key) {
            build.0 = if build.0 == Some(kind) { None } else { Some(kind) };
        }
    }
    if keys.just_pressed(KeyCode::Escape) {
        build.0 = None;
        selection.0 = None;
    }
    if buttons.just_pressed(MouseButton::Right) {
        if build.0.is_some() {
            build.0 = None;
        } else {
            selection.0 = None;
        }
    }

    let Some(state) = view.ready() else {
        hide_ghost(&mut ghost);
        return;
    };
    let cursor = window
        .iter()
        .next()
        .zip(camera.iter().next())
        .and_then(|(w, (c, gt))| cursor_world(w, c, gt));

    // Placement ghost.
    let mut ghost_tile: Option<(u8, u8)> = None;
    if let (Some(kind), Some(world), false) = (build.0, cursor, ui_hover.0) {
        if let Some((tx, ty)) = world_to_tile(world) {
            ghost_tile = Some((tx, ty));
            if let Ok((mut t, mut sprite, mut vis)) = ghost.single_mut() {
                *vis = Visibility::Visible;
                let pos = tile_center_world(tx, ty);
                t.translation.x = pos.x;
                t.translation.y = pos.y;
                sprite.color = match state.can_place(kind, tx, ty) {
                    Ok(()) => {
                        if kind == BuildingKind::Sawmill && state.forest_near(tx, ty, 4) == 0 {
                            Color::srgba(0.95, 0.85, 0.30, 0.5) // valid but pointless
                        } else {
                            Color::srgba(0.30, 0.90, 0.40, 0.5)
                        }
                    }
                    Err(_) => Color::srgba(0.95, 0.25, 0.25, 0.5),
                };
            }
        } else {
            hide_ghost(&mut ghost);
        }
    } else {
        hide_ghost(&mut ghost);
    }

    // Clicks.
    if buttons.just_pressed(MouseButton::Left) && !ui_hover.0 && state.phase == GamePhase::Running
    {
        if let Some(kind) = build.0 {
            if let Some((tx, ty)) = ghost_tile {
                if state.can_place(kind, tx, ty).is_ok() {
                    net.send(ClientMsg::Cmd(PlayerCommand::Place {
                        kind,
                        x: tx,
                        y: ty,
                    }));
                }
            }
        } else if let Some(world) = cursor {
            selection.0 = world_to_tile(world)
                .and_then(|(tx, ty)| state.building_at(tx, ty))
                .map(|b| b.id);
        }
    }
}

fn hide_ghost(
    ghost: &mut Query<(&mut Transform, &mut Sprite, &mut Visibility), With<GhostMarker>>,
) {
    if let Ok((_, _, mut vis)) = ghost.single_mut() {
        *vis = Visibility::Hidden;
    }
}

/// Share our cursor with the other players a few times per second.
pub fn send_cursor(
    time: Res<Time>,
    net: Res<NetConn>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut accum: Local<f32>,
    mut last_sent: Local<Option<(f32, f32)>>,
) {
    *accum += time.delta_secs();
    if *accum < 0.12 {
        return;
    }
    let Some(world) = window
        .iter()
        .next()
        .zip(camera.iter().next())
        .and_then(|(w, (c, gt))| cursor_world(w, c, gt))
    else {
        return;
    };
    let tf = world_to_tilef(world);
    let moved = last_sent
        .map(|(lx, ly)| (lx - tf.0).abs() + (ly - tf.1).abs() > 0.05)
        .unwrap_or(true);
    if moved {
        net.send(ClientMsg::Cursor { x: tf.0, y: tf.1 });
        *last_sent = Some(tf);
    }
    *accum = 0.0;
}
