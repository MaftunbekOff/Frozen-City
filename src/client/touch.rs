//! Touch controls (mobile browsers and, later, native mobile):
//!
//! - one finger drag  -> pan (the ground sticks to the finger)
//! - one finger tap   -> place a building / select / clear selection
//! - two finger pinch -> zoom
//! - two finger twist -> rotate the camera
//! - two finger slide (vertical) -> tilt
//!
//! Build mode is toggled from the build bar (tapping the active button
//! cancels), so every desktop action has a touch equivalent.

use bevy::input::touch::Touches;
use bevy::prelude::*;

use frozen_city::game::types::{GamePhase, PlayerCommand};
use frozen_city::net::protocol::ClientMsg;

use super::input::{ground_from_screen, CamRig, MAX_DIST, MAX_PITCH, MIN_DIST, MIN_PITCH};
use super::*;

/// A tap moves less than this many logical pixels.
const TAP_SLOP: f32 = 14.0;
/// ...and lasts at most this long.
const TAP_TIME: f32 = 0.6;

struct SingleGesture {
    id: u64,
    started: f32,
    /// The touch began over a UI element; the world must ignore it.
    on_ui: bool,
    /// Once true, releasing is never a tap.
    panned: bool,
}

struct PinchGesture {
    a: u64,
    b: u64,
    dist: f32,
    angle: f32,
    mid: Vec2,
}

#[derive(Resource, Default)]
pub struct TouchCtl {
    single: Option<SingleGesture>,
    pinch: Option<PinchGesture>,
}

#[allow(clippy::too_many_arguments)]
pub fn touch_control(
    time: Res<Time>,
    touches: Res<Touches>,
    ui_hover: Res<UiHover>,
    mut ctl: ResMut<TouchCtl>,
    mut rig: ResMut<CamRig>,
    view: Res<GameView>,
    net: Res<NetConn>,
    build: Res<BuildMode>,
    mut selection: ResMut<Selection>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    chat: Res<super::chat::ChatState>,
) {
    // While composing a chat message, touches must not pan/zoom or build,
    // matching how camera_control and build_input gate on chat.active.
    if chat.active {
        ctl.single = None;
        ctl.pinch = None;
        return;
    }
    let active: Vec<_> = touches.iter().collect();

    // A fresh first finger starts a (potential tap / pan) gesture.
    for t in touches.iter_just_pressed() {
        if active.len() == 1 && ctl.single.is_none() {
            ctl.single = Some(SingleGesture {
                id: t.id(),
                started: time.elapsed_secs(),
                on_ui: ui_hover.0,
                panned: false,
            });
        }
    }

    match active.len() {
        // One finger: pan once the finger travels beyond the tap slop.
        1 => {
            ctl.pinch = None;
            let t = active[0];
            if let Some(g) = ctl.single.as_mut().filter(|g| g.id == t.id()) {
                if (t.position() - t.start_position()).length() > TAP_SLOP {
                    g.panned = true;
                }
                if g.panned && !g.on_ui {
                    if let Some((cam, gt)) = camera.iter().next() {
                        let prev = t.position() - t.delta();
                        if let (Some(was), Some(now)) = (
                            ground_from_screen(cam, gt, prev),
                            ground_from_screen(cam, gt, t.position()),
                        ) {
                            // Keep the grabbed ground point under the finger.
                            rig.focus += was - now;
                        }
                    }
                }
            }
        }
        // Two or more: pinch zoom, twist rotate, vertical slide tilt.
        n if n >= 2 => {
            if let Some(g) = ctl.single.as_mut() {
                g.panned = true;
            }
            let (t0, t1) = (active[0], active[1]);
            let (p0, p1) = (t0.position(), t1.position());
            let dist = p0.distance(p1).max(1.0);
            let angle = (p1 - p0).to_angle();
            let mid = (p0 + p1) / 2.0;
            match ctl.pinch.as_mut().filter(|p| p.a == t0.id() && p.b == t1.id()) {
                Some(p) => {
                    rig.dist = (rig.dist * (p.dist / dist)).clamp(MIN_DIST, MAX_DIST);
                    let mut da = angle - p.angle;
                    if da > std::f32::consts::PI {
                        da -= std::f32::consts::TAU;
                    } else if da < -std::f32::consts::PI {
                        da += std::f32::consts::TAU;
                    }
                    rig.yaw += da;
                    rig.pitch = (rig.pitch + (mid.y - p.mid.y) * 0.005)
                        .clamp(MIN_PITCH, MAX_PITCH);
                    p.dist = dist;
                    p.angle = angle;
                    p.mid = mid;
                }
                None => {
                    ctl.pinch = Some(PinchGesture {
                        a: t0.id(),
                        b: t1.id(),
                        dist,
                        angle,
                        mid,
                    });
                }
            }
        }
        _ => ctl.pinch = None,
    }

    // Releases: short, unmoved, non-UI gestures act on the world.
    for t in touches.iter_just_released() {
        let Some(g) = ctl.single.take_if(|g| g.id == t.id()) else {
            continue;
        };
        ctl.pinch = None;
        if g.on_ui || g.panned || time.elapsed_secs() - g.started > TAP_TIME {
            continue;
        }
        let Some(state) = view.ready() else { continue };
        if state.phase != GamePhase::Running {
            continue;
        }
        let Some((cam, gt)) = camera.iter().next() else { continue };
        let Some(ground) = ground_from_screen(cam, gt, t.position()) else {
            continue;
        };
        let Some((tx, ty)) = world_to_tile(ground) else {
            if build.0.is_none() {
                selection.0 = None;
            }
            continue;
        };
        if let Some(kind) = build.0 {
            if state.can_place(kind, tx, ty).is_ok() {
                net.send(ClientMsg::Cmd(PlayerCommand::Place { kind, x: tx, y: ty }));
            }
        } else {
            selection.0 = state.building_at(tx, ty).map(|b| b.id);
        }
    }

    for t in touches.iter_just_canceled() {
        if ctl.single.as_ref().is_some_and(|g| g.id == t.id()) {
            ctl.single = None;
        }
        ctl.pinch = None;
    }
    if active.is_empty() {
        ctl.single = None;
        ctl.pinch = None;
    }
}

/// Keep the fixed-pixel HUD usable on narrow (phone) windows by scaling the
/// whole UI down; 1.0 on desktop-sized windows.
pub fn fit_ui_scale(
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut scale: ResMut<UiScale>,
) {
    let Ok(w) = window.single() else { return };
    // Pivot chosen so the widest fixed row (the 7-building + furnace build bar,
    // ~1000 px) fits: windows at/above it stay crisp at 1.0, narrower ones
    // shrink the whole HUD to fit rather than overflow.
    let target = (w.resolution.width() / 1000.0).clamp(0.5, 1.0);
    if (scale.0 - target).abs() > 0.01 {
        scale.0 = target;
    }
}
