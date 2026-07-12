//! Camera rig, build placement and selection in the 3D world.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use frozen_city::game::types::{BuildingKind, GamePhase, GameState, PlayerCommand};
use frozen_city::net::protocol::ClientMsg;

use super::chat::ChatState;
use super::render::GhostMarker;
use super::roster::SurvivorSelection;
use super::*;

/// A world-space click closer than this to a survivor's sim position selects
/// them instead of whatever building (or empty ground) is under the cursor —
/// "aholiga bosish ... bino tanlashdan ustunroq bo'lsin agar survivor yaqin
/// bo'lsa" from the brief. Generous enough to forgive imprecise taps on
/// mobile, small enough that tightly packed buildings still pick cleanly.
pub const SURVIVOR_PICK_RADIUS: f32 = 0.6;

/// Nearest survivor (by sim position, world-space distance) to `world`, if
/// any is within `SURVIVOR_PICK_RADIUS` — the "raycast" for survivor
/// selection is really just a closest-point pick against each survivor's
/// authoritative tile position, which is all a flat-ground scene needs.
pub fn pick_survivor(state: &GameState, world: Vec3) -> Option<u32> {
    state
        .survivors
        .iter()
        .map(|s| (s.id, super::tilef_to_world((s.x, s.y)).distance(world)))
        .filter(|(_, d)| *d <= SURVIVOR_PICK_RADIUS)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// Shared click-resolution logic for desktop (`build_input`) and touch
/// (`touch::touch_control`): a click/tap on a survivor selects them (only
/// your own settlers in the central world, mirroring `can_issue`); with a
/// survivor already selected, a click on empty ground (no survivor, no
/// building) issues `MoveSurvivor` there and queues the confirmation ping;
/// otherwise it falls back to ordinary building selection/deselection.
pub fn resolve_world_click(
    state: &GameState,
    me: Option<u64>,
    world: Vec3,
    net: &NetConn,
    selection: &mut Selection,
    survivor_sel: &mut SurvivorSelection,
    move_queue: &mut MoveOrderQueue,
) {
    let my_account = me
        .and_then(|pid| state.players.iter().find(|p| p.id == pid))
        .and_then(|p| p.account);
    let commandable = |id: u32| -> bool {
        if !state.central {
            return true;
        }
        my_account.is_some()
            && state.survivors.iter().find(|s| s.id == id).is_some_and(|s| s.owner == my_account)
    };

    if let Some(id) = pick_survivor(state, world).filter(|&id| commandable(id)) {
        survivor_sel.0 = if survivor_sel.0 == Some(id) { None } else { Some(id) };
        return;
    }

    let tile = world_to_tile(world);
    let building = tile.and_then(|(tx, ty)| state.building_at(tx, ty));

    // A survivor is selected and the click landed on open ground (no
    // building): issue the walk order there — "aholi tanlangan holatda
    // bo'sh yerga bosish" from the brief. A click on a BUILDING instead
    // falls through to ordinary building selection below, closing the
    // survivor card exactly like the explicit X button would ("boshqa
    // joyga bosish ... yopadi").
    if let (Some(id), Some((tx, ty)), None) = (survivor_sel.0, tile, building) {
        net.send(ClientMsg::Cmd(PlayerCommand::MoveSurvivor { survivor: id, x: tx, y: ty }));
        move_queue.0.push((tx, ty));
        return;
    }

    survivor_sel.0 = None;
    selection.0 = building.map(|b| b.id);
}

pub const MIN_DIST: f32 = 7.0;
pub const MAX_DIST: f32 = 60.0;
pub const MIN_PITCH: f32 = 0.35;
pub const MAX_PITCH: f32 = 1.35;

/// Orbit camera: looks at `focus` on the ground from `dist` away, tilted by
/// `pitch`. The default is a classic 2.5D three-quarter view; the player can
/// rotate (Q/E or middle-drag), tilt and zoom freely.
#[derive(Resource)]
pub struct CamRig {
    pub focus: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
}

impl Default for CamRig {
    fn default() -> Self {
        Self {
            focus: Vec3::ZERO,
            yaw: std::f32::consts::FRAC_PI_4,
            pitch: 0.95,
            dist: 26.0,
        }
    }
}

impl CamRig {
    pub fn eye(&self) -> Vec3 {
        let dir = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        );
        self.focus + dir * self.dist
    }
}

pub fn camera_control(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    ui_hover: Res<UiHover>,
    chat: Res<ChatState>,
    research: Res<super::research::ResearchOpen>,
    social: Res<super::social::SocialOpen>,
    mut rig: ResMut<CamRig>,
    mut cam: Query<&mut Transform, With<Camera3d>>,
) {
    // While typing in chat or browsing the research/social modal, the world
    // ignores input.
    if chat.active || research.0 || social.0 {
        return;
    }
    // Zoom (mouse wheel), unless the cursor is parked on the UI.
    if !ui_hover.0 && scroll.delta.y.abs() > 0.0 {
        let lines = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y,
            MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
        };
        rig.dist = (rig.dist * 0.9f32.powf(lines)).clamp(MIN_DIST, MAX_DIST);
    }

    // Keyboard pan, relative to the camera's yaw.
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
        let dir = dir.normalize();
        let forward = Vec3::new(-rig.yaw.sin(), 0.0, -rig.yaw.cos());
        let right = Vec3::new(-rig.yaw.cos(), 0.0, rig.yaw.sin());
        let speed = 0.55 * rig.dist * time.delta_secs();
        rig.focus += (forward * dir.y + right * dir.x) * speed;
    }

    // Rotate: Q/E keys or middle-mouse drag; drag also tilts.
    let rot_keys = (keys.pressed(KeyCode::KeyQ) as i32 - keys.pressed(KeyCode::KeyE) as i32) as f32;
    if rot_keys != 0.0 {
        rig.yaw += rot_keys * 1.8 * time.delta_secs();
    }
    if buttons.pressed(MouseButton::Middle) && motion.delta != Vec2::ZERO {
        rig.yaw -= motion.delta.x * 0.006;
        rig.pitch = (rig.pitch + motion.delta.y * 0.004).clamp(MIN_PITCH, MAX_PITCH);
    }

    // Keep the focus on the map.
    let limit = MAP_W as f32 / 2.0 * TILE + 4.0 * TILE;
    rig.focus.x = rig.focus.x.clamp(-limit, limit);
    rig.focus.z = rig.focus.z.clamp(-limit, limit);
    rig.focus.y = 0.0;

    if let Ok(mut transform) = cam.single_mut() {
        *transform = Transform::from_translation(rig.eye()).looking_at(rig.focus, Vec3::Y);
    }
}

/// A screen-space position projected onto the ground plane (y = 0).
pub fn ground_from_screen(
    camera: &Camera,
    cam_transform: &GlobalTransform,
    screen: Vec2,
) -> Option<Vec3> {
    let ray = camera.viewport_to_world(cam_transform, screen).ok()?;
    let dir_y = ray.direction.y;
    if dir_y.abs() < 1e-5 {
        return None;
    }
    let t = -ray.origin.y / dir_y;
    if t < 0.0 {
        return None;
    }
    Some(ray.origin + *ray.direction * t)
}

/// Cursor position projected onto the ground plane (y = 0).
pub fn cursor_ground(
    window: &Window,
    camera: &Camera,
    cam_transform: &GlobalTransform,
) -> Option<Vec3> {
    ground_from_screen(camera, cam_transform, window.cursor_position()?)
}

#[allow(clippy::too_many_arguments)]
pub fn build_input(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    ui_hover: Res<UiHover>,
    view: Res<GameView>,
    net: Res<NetConn>,
    mut build: ResMut<BuildMode>,
    mut selection: ResMut<Selection>,
    mut survivor_sel: ResMut<SurvivorSelection>,
    mut move_queue: ResMut<MoveOrderQueue>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut ghost: Query<(&mut Transform, &GhostMarker, &mut Visibility)>,
    chat: Res<ChatState>,
    research: Res<super::research::ResearchOpen>,
    social: Res<super::social::SocialOpen>,
) {
    // While typing in chat or in the research/social modal, keys don't build/cancel.
    if chat.active || research.0 || social.0 {
        hide_ghost(&mut ghost);
        return;
    }
    // Quick-build hotkeys.
    let hotkeys = [
        (KeyCode::Digit1, BuildingKind::Tent),
        (KeyCode::Digit2, BuildingKind::Sawmill),
        (KeyCode::Digit3, BuildingKind::CoalMine),
        (KeyCode::Digit4, BuildingKind::HunterHut),
        (KeyCode::Digit5, BuildingKind::Greenhouse),
        (KeyCode::Digit6, BuildingKind::Hospital),
        (KeyCode::Digit7, BuildingKind::Kitchen),
        (KeyCode::Digit8, BuildingKind::Warehouse),
    ];
    for (key, kind) in hotkeys {
        if keys.just_pressed(key) {
            build.0 = if build.0 == Some(kind) { None } else { Some(kind) };
        }
    }
    if keys.just_pressed(KeyCode::Escape) {
        build.0 = None;
        selection.0 = None;
        survivor_sel.0 = None;
    }
    if buttons.just_pressed(MouseButton::Right) {
        if build.0.is_some() {
            build.0 = None;
        } else {
            selection.0 = None;
            survivor_sel.0 = None;
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
        .and_then(|(w, (c, gt))| cursor_ground(w, c, gt));

    // Alt+click drops a co-op map ping for the whole team to see.
    if buttons.just_pressed(MouseButton::Left)
        && !ui_hover.0
        && keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight])
    {
        if let Some(world) = cursor {
            let (px, py) = world_to_tilef(world);
            net.send(ClientMsg::Ping { x: px, y: py });
        }
        hide_ghost(&mut ghost);
        return;
    }

    // Placement ghost.
    let mut ghost_tile: Option<(u8, u8)> = None;
    if let (Some(kind), Some(world), false) = (build.0, cursor, ui_hover.0) {
        if let Some((tx, ty)) = world_to_tile(world) {
            ghost_tile = Some((tx, ty));
            if let Ok((mut t, marker, mut vis)) = ghost.single_mut() {
                *vis = Visibility::Visible;
                let pos = tile_center_world(tx, ty);
                t.translation.x = pos.x;
                t.translation.z = pos.z;
                let color = match state.can_place(kind, tx, ty) {
                    Ok(()) => {
                        if kind == BuildingKind::Sawmill && state.forest_near(tx, ty, 4) == 0 {
                            Color::srgba(0.95, 0.85, 0.30, 0.45) // valid but pointless
                        } else {
                            Color::srgba(0.30, 0.90, 0.40, 0.45)
                        }
                    }
                    Err(_) => Color::srgba(0.95, 0.25, 0.25, 0.45),
                };
                if let Some(mut m) = materials.get_mut(&marker.mat) {
                    m.base_color = color;
                }
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
            resolve_world_click(
                state,
                view.player_id,
                world,
                &net,
                &mut selection,
                &mut survivor_sel,
                &mut move_queue,
            );
        }
    }
}

fn hide_ghost(ghost: &mut Query<(&mut Transform, &GhostMarker, &mut Visibility)>) {
    if let Ok((_, _, mut vis)) = ghost.single_mut() {
        *vis = Visibility::Hidden;
    }
}

/// Share our cursor with the other players a few times per second.
pub fn send_cursor(
    time: Res<Time>,
    net: Res<NetConn>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
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
        .and_then(|(w, (c, gt))| cursor_ground(w, c, gt))
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
