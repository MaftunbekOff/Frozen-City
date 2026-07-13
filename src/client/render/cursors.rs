use std::collections::HashMap;

use bevy::prelude::*;

use super::*;
use crate::client::*;

// ------------------------------------------------------------------ cursors

pub fn sync_player_cursors(
    mut commands: Commands,
    time: Res<Time>,
    view: Res<GameView>,
    assets: Res<GameAssets>,
    mut viz: ResMut<CursorViz>,
    mut q: Query<(&CursorMarker, &mut Transform)>,
) {
    let Some(state) = view.state.as_ref() else { return };
    let me = view.player_id.unwrap_or(0);

    // The central world shows light avatars instead (`sync_avatars`) — every
    // other world keeps this marker-cursor behavior exactly as before. Tear
    // down any leftover cursor markers so switching into the central world
    // (a live `GameState.central` flip, not just a fresh connection) doesn't
    // leave stale cones floating from the last non-central snapshot.
    if state.central {
        for (m, l) in viz.0.drain().map(|(_, v)| v) {
            commands.entity(m).despawn();
            commands.entity(l).despawn();
        }
        return;
    }

    let mut targets: HashMap<u64, Vec3> = HashMap::new();
    for p in &state.players {
        if p.id == me {
            continue;
        }
        if let Some(c) = p.cursor {
            targets.insert(p.id, tilef_to_world(c));
        }
    }

    for p in &state.players {
        if p.id == me || !targets.contains_key(&p.id) || viz.0.contains_key(&p.id) {
            continue;
        }
        let color = player_color(p.color);
        let mat = assets.cursor_mats[(p.color as usize) % assets.cursor_mats.len()].clone();
        // Downward cone floating over the ground.
        let marker = commands
            .spawn((
                Mesh3d(assets.cone.clone()),
                MeshMaterial3d(mat),
                Transform::from_translation(targets[&p.id] + Vec3::Y * 0.9)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::PI))
                    .with_scale(Vec3::new(0.3, 0.5, 0.3)),
                CursorMarker { player: p.id },
                DespawnOnExit(Screen::Game),
            ))
            .id();
        let label = commands
            .spawn((
                Text::new(p.name.clone()),
                TextFont::from_font_size(12.0),
                TextColor(color),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(-1000.0),
                    top: Val::Px(-1000.0),
                    ..default()
                },
                CursorLabel { player: p.id },
                DespawnOnExit(Screen::Game),
            ))
            .id();
        viz.0.insert(p.id, (marker, label));
    }

    let gone: Vec<u64> = viz
        .0
        .keys()
        .filter(|id| !targets.contains_key(id))
        .copied()
        .collect();
    for id in gone {
        if let Some((m, l)) = viz.0.remove(&id) {
            commands.entity(m).despawn();
            commands.entity(l).despawn();
        }
    }

    let blend = 1.0 - (-12.0 * time.delta_secs()).exp();
    let bob = (time.elapsed_secs() * 3.0).sin() * 0.06;
    for (marker, mut tr) in &mut q {
        if let Some(target) = targets.get(&marker.player) {
            let goal = *target + Vec3::Y * (0.9 + bob);
            tr.translation = tr.translation.lerp(goal, blend);
        }
    }
}

/// Project remote cursor names into screen space.
pub fn update_cursor_labels(
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    markers: Query<(&CursorMarker, &Transform)>,
    mut labels: Query<(&CursorLabel, &mut Node, &mut Visibility)>,
) {
    let Ok((cam, cam_gt)) = camera.single() else { return };
    let mut screen: HashMap<u64, Option<Vec2>> = HashMap::new();
    for (m, tr) in &markers {
        screen.insert(
            m.player,
            cam.world_to_viewport(cam_gt, tr.translation + Vec3::Y * 0.35)
                .ok(),
        );
    }
    for (label, mut node, mut vis) in &mut labels {
        match screen.get(&label.player).copied().flatten() {
            Some(p) => {
                *vis = Visibility::Visible;
                node.left = Val::Px(p.x - 20.0);
                node.top = Val::Px(p.y - 34.0);
            }
            None => *vis = Visibility::Hidden,
        }
    }
}

// ------------------------------------------------------------------ avatars

/// Central-world "light avatar mode": every connected player (including the
/// local one — this is the hub, everyone should see themselves and each
/// other as people, not just remote cursors) gets a small low-poly humanoid
/// that walks toward their synced cursor tile, plus a name tag. Reuses the
/// survivor capsule mesh and a shared per-color material (`avatar_mats`) so
/// this costs nothing extra to batch. No-ops outside the central world —
/// `sync_player_cursors` keeps ownership of every other world's visuals.
pub fn sync_avatars(
    mut commands: Commands,
    view: Res<GameView>,
    assets: Res<GameAssets>,
    mut viz: ResMut<AvatarViz>,
    mut walkers: Query<&mut AvatarWalk>,
) {
    let Some(state) = view.state.as_ref() else { return };
    if !state.central {
        // Not the central world: drop any leftover avatars (mirrors the
        // cleanup `sync_player_cursors` does in the opposite direction).
        if !viz.0.is_empty() {
            for (body, label) in viz.0.drain().map(|(_, v)| v) {
                commands.entity(body).despawn();
                commands.entity(label).despawn();
            }
        }
        return;
    }

    let mut targets: HashMap<u64, Vec3> = HashMap::new();
    for p in &state.players {
        if let Some(c) = p.cursor {
            targets.insert(p.id, tilef_to_world(c));
        }
    }

    for p in &state.players {
        let Some(&target) = targets.get(&p.id) else { continue };
        if viz.0.contains_key(&p.id) {
            continue;
        }
        let mat = assets.avatar_mats[(p.color as usize) % assets.avatar_mats.len()].clone();
        let body = commands
            .spawn((
                Mesh3d(assets.capsule.clone()),
                MeshMaterial3d(mat),
                Transform::from_translation(target + Vec3::Y * 0.24),
                AvatarWalk { player: p.id, target },
                DespawnOnExit(Screen::Game),
            ))
            .id();
        let label = commands
            .spawn((
                Text::new(p.name.clone()),
                TextFont::from_font_size(12.0),
                TextColor(player_color(p.color)),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(-1000.0),
                    top: Val::Px(-1000.0),
                    ..default()
                },
                AvatarLabel { player: p.id },
                DespawnOnExit(Screen::Game),
            ))
            .id();
        viz.0.insert(p.id, (body, label));
    }

    let gone: Vec<u64> = viz
        .0
        .keys()
        .filter(|id| !targets.contains_key(id))
        .copied()
        .collect();
    for id in gone {
        if let Some((body, label)) = viz.0.remove(&id) {
            commands.entity(body).despawn();
            commands.entity(label).despawn();
        }
    }

    // Refresh each walker's chase target (position itself is smoothed in
    // `animate_avatars`, same split as `sync_player_cursors`/its `q` loop).
    for mut walker in &mut walkers {
        if let Some(&target) = targets.get(&walker.player) {
            walker.target = target;
        }
    }
}

/// Smoothly walks each avatar body toward its current target tile — a
/// straightforward exponential lerp, same shape as the cursor marker's blend
/// in `sync_player_cursors`, plus a gentle walking bob.
pub fn animate_avatars(
    time: Res<Time>,
    mut q: Query<(&AvatarWalk, &mut Transform)>,
) {
    let blend = 1.0 - (-8.0 * time.delta_secs()).exp();
    let bob = (time.elapsed_secs() * 6.0).sin().abs() * 0.03;
    for (walker, mut tr) in &mut q {
        let goal = walker.target + Vec3::Y * (0.24 + bob);
        tr.translation = tr.translation.lerp(goal, blend);
    }
}

/// Project avatar name tags into screen space — identical idea to
/// `update_cursor_labels`, kept as its own system since it reads disjoint
/// component types (`AvatarWalk`/`AvatarLabel` vs `CursorMarker`/
/// `CursorLabel`), so there is no query-conflict risk between the two.
pub fn update_avatar_labels(
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    bodies: Query<(&AvatarWalk, &Transform)>,
    mut labels: Query<(&AvatarLabel, &mut Node, &mut Visibility)>,
) {
    let Ok((cam, cam_gt)) = camera.single() else { return };
    let mut screen: HashMap<u64, Option<Vec2>> = HashMap::new();
    for (w, tr) in &bodies {
        screen.insert(
            w.player,
            cam.world_to_viewport(cam_gt, tr.translation + Vec3::Y * 0.35)
                .ok(),
        );
    }
    for (label, mut node, mut vis) in &mut labels {
        match screen.get(&label.player).copied().flatten() {
            Some(p) => {
                *vis = Visibility::Visible;
                node.left = Val::Px(p.x - 20.0);
                node.top = Val::Px(p.y - 34.0);
            }
            None => *vis = Visibility::Hidden,
        }
    }
}

/// World position to anchor a chat bubble above `player`: their rendered
/// central-world avatar or remote-cursor marker if one currently exists
/// (smoothed, so the bubble tracks their walk/cursor motion), else a direct
/// lookup of their last-known cursor tile from the snapshot itself — the
/// fallback that covers the *local* player (whose own position is never
/// rendered as a marker/avatar-of-self outside the central world, since
/// `sync_player_cursors` intentionally skips "me") and anyone whose
/// marker/avatar entity hasn't spawned yet this frame. Used by `chat.rs`'s
/// `render_bubbles` — "bubbles from yourself also render" per the spec.
pub fn sender_anchor(
    player: u64,
    state: &GameState,
    avatars: &AvatarViz,
    cursors: &CursorViz,
    avatar_q: &Query<&Transform, With<AvatarWalk>>,
    cursor_q: &Query<&Transform, With<CursorMarker>>,
) -> Option<Vec3> {
    if let Some((body, _)) = avatars.0.get(&player) {
        if let Ok(tr) = avatar_q.get(*body) {
            return Some(tr.translation);
        }
    }
    if let Some((marker, _)) = cursors.0.get(&player) {
        if let Ok(tr) = cursor_q.get(*marker) {
            return Some(tr.translation);
        }
    }
    state
        .players
        .iter()
        .find(|p| p.id == player)
        .and_then(|p| p.cursor)
        .map(tilef_to_world)
}

// -------------------------------------------------------------------- pings

/// Co-op map markers: a bright ground ring plus a floating downward cone in
/// the pinging player's color. Pings live in the snapshot and expire in the
/// sim, so this system only mirrors `state.pings` and animates a gentle pulse.
pub fn sync_pings(
    mut commands: Commands,
    time: Res<Time>,
    view: Res<GameView>,
    assets: Res<GameAssets>,
    mut viz: ResMut<PingViz>,
    mut q: Query<&mut Transform, With<PingMarker>>,
) {
    let Some(state) = view.state.as_ref() else { return };

    let mut present: HashMap<(u64, u64, u32, u32), &frozen_city::game::types::Ping> =
        HashMap::new();
    for p in &state.pings {
        present.insert((p.player_id, p.tick, p.x.to_bits(), p.y.to_bits()), p);
    }

    // Spawn markers for newly arrived pings.
    for (&key, p) in &present {
        if viz.0.contains_key(&key) {
            continue;
        }
        let mat = assets.ping_mats[(p.color as usize) % assets.ping_mats.len()].clone();
        let world = tilef_to_world((p.x, p.y));
        let root = commands
            .spawn((
                Transform::from_translation(world),
                Visibility::Visible,
                PingMarker,
                DespawnOnExit(Screen::Game),
            ))
            .id();
        commands.entity(root).with_children(|c| {
            c.spawn((
                Mesh3d(assets.ring.clone()),
                MeshMaterial3d(mat.clone()),
                Transform::from_xyz(0.0, 0.06, 0.0)
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                    .with_scale(Vec3::splat(1.3)),
            ));
            c.spawn((
                Mesh3d(assets.cone.clone()),
                MeshMaterial3d(mat),
                Transform::from_xyz(0.0, 1.7, 0.0)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::PI))
                    .with_scale(Vec3::new(0.4, 0.7, 0.4)),
            ));
        });
        viz.0.insert(key, root);
    }

    // Despawn markers whose ping has expired.
    let gone: Vec<(u64, u64, u32, u32)> = viz
        .0
        .keys()
        .filter(|k| !present.contains_key(k))
        .copied()
        .collect();
    for k in gone {
        if let Some(e) = viz.0.remove(&k) {
            commands.entity(e).despawn();
        }
    }

    // A gentle attention-grabbing pulse.
    let pulse = 1.0 + 0.15 * (time.elapsed_secs() * 6.0).sin();
    for mut tr in &mut q {
        tr.scale = Vec3::splat(pulse);
    }
}
