//! V0.11: corpses (`GameState.corpses`) and graves (`GameState.graves`) —
//! the visible half of the population-lifecycle/burial feature. A corpse
//! renders as a flat shroud at the death location until a player buries it
//! (`PlayerCommand::Bury`) or it decays on its own; a grave renders as a
//! small wooden cross and fades away after `GRAVE_FADE_TICKS`. Both are
//! purely cosmetic — the server is the sole authority on when they appear
//! and disappear.

use bevy::prelude::*;

use frozen_city::game::types::{Corpse, Grave};

use super::*;
use crate::client::*;

/// One corpse's entity, keyed by its stable `Corpse.id` (the dead
/// survivor's former id) — mirrors `BuildingViz`'s simpler cousin, since a
/// corpse never changes appearance over its lifetime (no level/stage to
/// react to, unlike a building).
#[derive(Resource, Default)]
pub struct CorpseViz(pub std::collections::HashMap<u32, Entity>);

/// Graves have no stable id of their own (`Grave` is just a position + the
/// tick it was created), so `sync_graves` fully rebuilds the set whenever
/// that snapshot changes instead of diffing by identity — the same
/// trigger-on-change trick `MigrantViz` uses, and just as cheap in practice
/// since a colony only ever has a handful of graves at once.
#[derive(Resource, Default)]
pub struct GraveViz {
    pub entities: Vec<Entity>,
    pub snapshot: Vec<(u32, u32, u64)>,
}

fn corpse_world(c: &Corpse) -> Vec3 {
    tilef_to_world((c.x, c.y))
}

fn grave_world(g: &Grave) -> Vec3 {
    tilef_to_world((g.x, g.y))
}

pub fn sync_corpses(
    mut commands: Commands,
    view: Res<GameView>,
    assets: Res<GameAssets>,
    mut viz: ResMut<CorpseViz>,
    mut seen: Local<u64>,
) {
    let Some(state) = view.ready() else { return };
    if *seen == view.version {
        return;
    }
    *seen = view.version;

    for c in &state.corpses {
        if viz.0.contains_key(&c.id) {
            continue;
        }
        let e = commands
            .spawn((
                Transform::from_translation(corpse_world(c)),
                Visibility::Inherited,
                DespawnOnExit(Screen::Game),
            ))
            .with_children(|p| {
                p.spawn((
                    Mesh3d(assets.capsule.clone()),
                    MeshMaterial3d(assets.corpse_mat.clone()),
                    Transform::from_xyz(0.0, 0.08, 0.0)
                        .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
                        .with_scale(Vec3::new(0.16, 0.42, 0.16)),
                ));
            })
            .id();
        viz.0.insert(c.id, e);
    }

    let gone: Vec<u32> = viz.0.keys().filter(|id| !state.corpses.iter().any(|c| c.id == **id)).copied().collect();
    for id in gone {
        if let Some(e) = viz.0.remove(&id) {
            commands.entity(e).despawn();
        }
    }
}

pub fn sync_graves(
    mut commands: Commands,
    view: Res<GameView>,
    assets: Res<GameAssets>,
    mut viz: ResMut<GraveViz>,
    mut seen: Local<u64>,
) {
    let Some(state) = view.ready() else { return };
    if *seen == view.version {
        return;
    }
    *seen = view.version;

    let snapshot: Vec<(u32, u32, u64)> =
        state.graves.iter().map(|g| (g.x.to_bits(), g.y.to_bits(), g.created_tick)).collect();
    if snapshot == viz.snapshot {
        return;
    }

    for e in viz.entities.drain(..) {
        commands.entity(e).despawn();
    }
    for g in &state.graves {
        let pos = grave_world(g);
        let e = commands
            .spawn((Transform::from_translation(pos), Visibility::Inherited, DespawnOnExit(Screen::Game)))
            .with_children(|p| {
                // A small wooden cross: one upright post, one crossbar.
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(assets.grave_cross_mat.clone()),
                    Transform::from_xyz(0.0, 0.22, 0.0).with_scale(Vec3::new(0.05, 0.44, 0.05)),
                ));
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(assets.grave_cross_mat.clone()),
                    Transform::from_xyz(0.0, 0.30, 0.0).with_scale(Vec3::new(0.24, 0.05, 0.05)),
                ));
            })
            .id();
        viz.entities.push(e);
    }
    viz.snapshot = snapshot;
}
