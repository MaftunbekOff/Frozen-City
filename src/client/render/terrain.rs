use bevy::prelude::*;

use super::*;
use crate::client::*;

pub fn sync_terrain(
    mut commands: Commands,
    view: Res<GameView>,
    assets: Res<GameAssets>,
    quality: Res<Quality>,
    mut viz: ResMut<TerrainViz>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if view.tiles.is_empty() {
        return;
    }
    // Phones get sparser scenery — less vertex work and less overdraw.
    let dense = *quality != Quality::Low;
    let first = viz.ground.is_none();
    if !first && viz.seen_tiles_version == view.tiles_version {
        return;
    }
    if !first && viz.cache == view.tiles {
        viz.seen_tiles_version = view.tiles_version;
        return;
    }

    for e in [viz.ground.take(), viz.trees.take(), viz.rocks.take()]
        .into_iter()
        .flatten()
    {
        commands.entity(e).despawn();
    }
    let spawn = |commands: &mut Commands, meshes: &mut Assets<Mesh>, mesh: Mesh| {
        commands
            .spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(assets.terrain_mat.clone()),
                Transform::IDENTITY,
                DespawnOnExit(Screen::Game),
            ))
            .id()
    };
    viz.ground = Some(spawn(&mut commands, &mut meshes, ground_mesh(&view.tiles)));
    viz.trees = Some(spawn(&mut commands, &mut meshes, trees_mesh(&view.tiles, dense)));
    viz.rocks = Some(spawn(&mut commands, &mut meshes, rocks_mesh(&view.tiles, dense)));
    viz.cache = view.tiles.clone();
    viz.seen_tiles_version = view.tiles_version;
}
