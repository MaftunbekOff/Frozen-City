//! 3D world rendering: vertex-colored terrain, low-poly trees and rocks,
//! block-out buildings, day/night sun + fog, furnace light, snowfall and
//! co-op player cursors. All geometry is procedural — no asset files.
//!
//! The camera looks down at a tilt (2.5D feel) but the scene is true 3D;
//! the rig in `input.rs` allows rotation and zoom.

use std::collections::HashMap;

use bevy::anti_alias::fxaa::Fxaa;
use bevy::asset::RenderAssetUsages;
use bevy::camera::Hdr;
use bevy::light::{CascadeShadowConfigBuilder, DirectionalLightShadowMap};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::mesh::PrimitiveTopology;

use frozen_city::game::rng::Rng;
use frozen_city::game::types::{tile_index, BuildingKind};

use super::*;

// ---------------------------------------------------------------- resources

#[derive(Resource)]
pub struct GameAssets {
    pub cube: Handle<Mesh>,
    pub cylinder: Handle<Mesh>,
    pub cone: Handle<Mesh>,
    pub capsule: Handle<Mesh>,
    pub tent: Handle<Mesh>,
    pub ring: Handle<Mesh>,
    /// Shared vertex-color material for the merged terrain meshes.
    pub terrain_mat: Handle<StandardMaterial>,
    pub snow_mat: Handle<StandardMaterial>,
    /// Health tiers (healthy -> critical); shared so survivors batch.
    pub survivor_mats: [Handle<StandardMaterial>; 4],
    /// One shared material for every building window; its emissive is
    /// animated with the time of day (warm at night, dead by day).
    pub window_mat: Handle<StandardMaterial>,
    pub smoke_mat: Handle<StandardMaterial>,
    /// One emissive material per player-palette color, shared by all map
    /// pings so transient pings never leak fresh materials.
    pub ping_mats: [Handle<StandardMaterial>; 8],
}

#[derive(Resource, Default)]
pub struct TerrainViz {
    pub ground: Option<Entity>,
    pub trees: Option<Entity>,
    pub rocks: Option<Entity>,
    pub cache: Vec<Tile>,
    pub seen_tiles_version: u64,
}

#[derive(Resource, Default)]
pub struct BuildingViz(pub HashMap<u32, Entity>);

#[derive(Resource, Default)]
pub struct SurvivorViz(pub HashMap<u32, Entity>);

/// Remote player cursors: world marker + screen-space name label.
#[derive(Resource, Default)]
pub struct CursorViz(pub HashMap<u64, (Entity, Entity)>);

/// Live map pings, keyed by (player id, creation tick, x-bits, y-bits) so two
/// pings a player drops within the same tick stay distinct.
#[derive(Resource, Default)]
pub struct PingViz(pub HashMap<(u64, u64, u32, u32), Entity>);

// --------------------------------------------------------------- components

#[derive(Component)]
pub struct SunLight;

#[derive(Component)]
pub struct BuildingMarker;

/// Small cube on a building's roof; visible while `index < workers`.
#[derive(Component)]
pub struct WorkerCube {
    pub building: u32,
    pub index: u8,
}

/// On the furnace root: handle to the animated fire material.
#[derive(Component)]
pub struct FurnaceGlow {
    pub fire_mat: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct FurnaceLight;

#[derive(Component)]
pub struct SurvivorDot {
    pub id: u32,
}

#[derive(Component)]
pub struct Wander {
    pub home: Vec3,
    pub target: Vec3,
    pub speed: f32,
}

#[derive(Component)]
pub struct CursorMarker {
    pub player: u64,
}

/// Tags the root of a co-op map ping (identity is tracked in [`PingViz`]).
#[derive(Component)]
pub struct PingMarker;

/// UI text node that follows a remote player's cursor on screen.
#[derive(Component)]
pub struct CursorLabel {
    pub player: u64,
}

#[derive(Component)]
pub struct HeatRing {
    pub mat: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct SelectionRing;

#[derive(Component)]
pub struct GhostMarker {
    pub mat: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct Snowflake {
    pub fall: f32,
    pub drift: f32,
    pub phase: f32,
}

/// A looping puff rising from the furnace chimney while it burns.
#[derive(Component)]
pub struct Smoke {
    pub phase: f32,
}

// ------------------------------------------------------------------- setup

pub fn setup_camera_and_assets(
    mut commands: Commands,
    quality: Res<Quality>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let camera = commands
        .spawn((
            Camera3d::default(),
            Transform::from_xyz(14.0, 20.0, 14.0).looking_at(Vec3::ZERO, Vec3::Y),
            DistanceFog {
                color: Color::srgb(0.30, 0.38, 0.50),
                falloff: FogFalloff::Linear {
                    start: 60.0,
                    end: 140.0,
                },
                ..default()
            },
            AmbientLight {
                color: Color::srgb(0.7, 0.8, 1.0),
                brightness: 220.0,
                ..default()
            },
        ))
        .id();
    // Post-processing per quality tier: the furnace and windows glow
    // through HDR bloom on Medium/High; phones skip it for fill rate.
    match *quality {
        Quality::High => {
            commands
                .entity(camera)
                .insert((Msaa::Sample4, Hdr, Bloom::NATURAL));
        }
        Quality::Medium => {
            commands
                .entity(camera)
                .insert((Msaa::Off, Hdr, Bloom::NATURAL, Fxaa::default()));
        }
        Quality::Low => {
            commands.entity(camera).insert(Msaa::Off);
        }
    }

    commands.insert_resource(DirectionalLightShadowMap {
        size: if *quality == Quality::High { 2048 } else { 1024 },
    });
    commands.spawn((
        DirectionalLight {
            illuminance: 9_000.0,
            shadow_maps_enabled: *quality != Quality::Low,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: 2,
            first_cascade_far_bound: 20.0,
            maximum_distance: 70.0,
            ..default()
        }
        .build(),
        Transform::default().looking_to(Vec3::new(-0.4, -0.8, -0.35), Vec3::Y),
        SunLight,
    ));

    let assets = GameAssets {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        cylinder: meshes.add(Cylinder::new(0.5, 1.0)),
        cone: meshes.add(Cone {
            radius: 0.5,
            height: 1.0,
        }),
        capsule: meshes.add(Capsule3d::new(0.11, 0.22)),
        tent: meshes.add(tent_mesh()),
        ring: meshes.add(Annulus::new(0.93, 1.0)),
        terrain_mat: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.96,
            ..default()
        }),
        // Opaque so all flakes batch into a handful of instanced draws.
        snow_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.96, 0.97, 1.0),
            unlit: true,
            ..default()
        }),
        survivor_mats: std::array::from_fn(|i| {
            let sick = i as f32 / 3.0;
            materials.add(StandardMaterial {
                base_color: Color::srgb(
                    0.30 + 0.60 * sick,
                    0.38 - 0.20 * sick,
                    0.55 - 0.43 * sick,
                ),
                ..default()
            })
        }),
        window_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.35, 0.28, 0.16),
            emissive: LinearRgba::rgb(0.02, 0.015, 0.005),
            ..default()
        }),
        smoke_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.52, 0.54, 0.58),
            unlit: true,
            ..default()
        }),
        ping_mats: std::array::from_fn(|i| {
            let c = player_color(i as u8);
            materials.add(StandardMaterial {
                base_color: c,
                emissive: c.to_linear() * 3.0,
                ..default()
            })
        }),
    };
    commands.insert_resource(assets);
}

/// A 1x1x1 tent: triangular prism with closed ends, flat-shaded.
fn tent_mesh() -> Mesh {
    let mut buf = MeshBuf::default();
    let c = [1.0, 1.0, 1.0, 1.0];
    let (a, b) = (-0.5, 0.5);
    let ridge_f = Vec3::new(0.0, 1.0, a);
    let ridge_b = Vec3::new(0.0, 1.0, b);
    let fl = Vec3::new(a, 0.0, a);
    let fr = Vec3::new(b, 0.0, a);
    let bl = Vec3::new(a, 0.0, b);
    let br = Vec3::new(b, 0.0, b);
    // Sloped sides.
    buf.quad(fl, ridge_f, ridge_b, bl, c);
    buf.quad(fr, br, ridge_b, ridge_f, c);
    // Gable ends.
    buf.tri(fl, fr, ridge_f, c);
    buf.tri(br, bl, ridge_b, c);
    buf.into_mesh()
}

// --------------------------------------------------------------- enter game

pub fn enter_game(
    mut commands: Commands,
    assets: Res<GameAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut rig: ResMut<super::input::CamRig>,
) {
    *rig = super::input::CamRig::default();

    // Furnace heat radius: flat ring on the ground, scaled by the radius.
    let heat_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.55, 0.18, 0.35),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    commands.spawn((
        Mesh3d(assets.ring.clone()),
        MeshMaterial3d(heat_mat.clone()),
        Transform::from_xyz(0.0, 0.03, 0.0)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        Visibility::Hidden,
        HeatRing { mat: heat_mat },
        DespawnOnExit(Screen::Game),
    ));

    // Selection highlight ring.
    let sel_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.5),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    commands.spawn((
        Mesh3d(assets.ring.clone()),
        MeshMaterial3d(sel_mat),
        Transform::from_xyz(0.0, 0.04, 0.0)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        Visibility::Hidden,
        SelectionRing,
        DespawnOnExit(Screen::Game),
    ));

    // Build placement ghost.
    let ghost_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.3, 0.9, 0.4, 0.45),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    commands.spawn((
        Mesh3d(assets.cube.clone()),
        MeshMaterial3d(ghost_mat.clone()),
        Transform::from_xyz(0.0, 0.25, 0.0).with_scale(Vec3::new(0.95, 0.5, 0.95)),
        Visibility::Hidden,
        GhostMarker { mat: ghost_mat },
        DespawnOnExit(Screen::Game),
    ));

    // Snowfall volume around the camera focus.
    let mut rng = Rng::new(0x5005_7EA1);
    for _ in 0..240 {
        let x = rng.range(-24, 24) as f32 + rng.below(100) as f32 * 0.01;
        let z = rng.range(-24, 24) as f32 + rng.below(100) as f32 * 0.01;
        let y = rng.below(140) as f32 * 0.1;
        let s = 0.03 + rng.below(30) as f32 * 0.002;
        commands.spawn((
            Mesh3d(assets.cube.clone()),
            MeshMaterial3d(assets.snow_mat.clone()),
            Transform::from_xyz(x, y, z).with_scale(Vec3::splat(s)),
            Snowflake {
                fall: 1.4 + rng.below(22) as f32 * 0.1,
                drift: 0.2 + rng.below(10) as f32 * 0.06,
                phase: rng.below(628) as f32 * 0.01,
            },
            DespawnOnExit(Screen::Game),
        ));
    }
}

// ------------------------------------------------------------ merged meshes

/// Flat-shaded triangle soup with per-vertex colors.
#[derive(Default)]
struct MeshBuf {
    pos: Vec<[f32; 3]>,
    nor: Vec<[f32; 3]>,
    col: Vec<[f32; 4]>,
}

impl MeshBuf {
    fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, color: [f32; 4]) {
        let n = (b - a).cross(c - a).normalize_or_zero().to_array();
        for p in [a, b, c] {
            self.pos.push(p.to_array());
            self.nor.push(n);
            self.col.push(color);
        }
    }

    fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, color: [f32; 4]) {
        self.tri(a, b, c, color);
        self.tri(a, c, d, color);
    }

    /// Quad with explicit per-vertex normals (smooth-shaded terrain).
    fn quad_smooth(&mut self, v: [(Vec3, [f32; 3]); 4], color: [f32; 4]) {
        for i in [0, 1, 2, 0, 2, 3] {
            self.pos.push(v[i].0.to_array());
            self.nor.push(v[i].1);
            self.col.push(color);
        }
    }

    /// Axis-aligned box between `min` and `max` (top, sides — no bottom).
    fn boxx(&mut self, min: Vec3, max: Vec3, color: [f32; 4]) {
        let (a, b) = (min, max);
        let p = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
        // Top.
        self.quad(p(a.x, b.y, a.z), p(a.x, b.y, b.z), p(b.x, b.y, b.z), p(b.x, b.y, a.z), color);
        // Sides.
        self.quad(p(a.x, a.y, a.z), p(b.x, a.y, a.z), p(b.x, b.y, a.z), p(a.x, b.y, a.z), color);
        self.quad(p(b.x, a.y, b.z), p(a.x, a.y, b.z), p(a.x, b.y, b.z), p(b.x, b.y, b.z), color);
        self.quad(p(a.x, a.y, b.z), p(a.x, a.y, a.z), p(a.x, b.y, a.z), p(a.x, b.y, b.z), color);
        self.quad(p(b.x, a.y, a.z), p(b.x, a.y, b.z), p(b.x, b.y, b.z), p(b.x, b.y, a.z), color);
    }

    /// Open cone (no base) with `seg` sides.
    fn cone(&mut self, center: Vec3, radius: f32, height: f32, seg: u32, color: [f32; 4]) {
        let apex = center + Vec3::Y * height;
        for i in 0..seg {
            let t0 = i as f32 / seg as f32 * std::f32::consts::TAU;
            let t1 = (i + 1) as f32 / seg as f32 * std::f32::consts::TAU;
            let p0 = center + Vec3::new(t0.cos() * radius, 0.0, t0.sin() * radius);
            let p1 = center + Vec3::new(t1.cos() * radius, 0.0, t1.sin() * radius);
            self.tri(p0, apex, p1, color);
        }
    }

    fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.nor);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.col);
        mesh
    }
}

fn linear(c: Color) -> [f32; 4] {
    c.to_linear().to_f32_array()
}

/// Deterministic per-grid-point hash for snow-drift heights and tree jitter.
fn hash2(x: u32, y: u32) -> u32 {
    let mut h = x.wrapping_mul(0x9E37_79B9) ^ y.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 13;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^ (h >> 16)
}

fn corner_height(gx: u32, gz: u32) -> f32 {
    // Keep the drift subtle and flatten toward the furnace so buildings sit flush.
    let d = GameState::dist_to_furnace(
        (gx.min(MAP_W as u32 - 1)) as u8,
        (gz.min(MAP_H as u32 - 1)) as u8,
    );
    let k = ((d - 3.0) / 10.0).clamp(0.0, 1.0);
    (hash2(gx, gz) % 100) as f32 * 0.0011 * k
}

/// Smooth terrain normal from the height field (central differences).
fn corner_normal(gx: u32, gz: u32) -> [f32; 3] {
    let h = |x: i64, z: i64| {
        corner_height(
            x.clamp(0, MAP_W as i64) as u32,
            z.clamp(0, MAP_H as i64) as u32,
        )
    };
    let dx = h(gx as i64 + 1, gz as i64) - h(gx as i64 - 1, gz as i64);
    let dz = h(gx as i64, gz as i64 + 1) - h(gx as i64, gz as i64 - 1);
    Vec3::new(-dx, 2.0, -dz).normalize().to_array()
}

fn ground_mesh(tiles: &[Tile]) -> Mesh {
    let mut buf = MeshBuf::default();
    let half_w = MAP_W as f32 / 2.0;
    let half_h = MAP_H as f32 / 2.0;
    for ty in 0..MAP_H as u32 {
        for tx in 0..MAP_W as u32 {
            let tile = &tiles[tile_index(tx as u8, ty as u8)];
            let color = linear(terrain_color(tile, tx as u8, ty as u8));
            let p = |gx: u32, gz: u32| {
                (
                    Vec3::new(
                        gx as f32 - half_w,
                        corner_height(gx, gz),
                        gz as f32 - half_h,
                    ),
                    corner_normal(gx, gz),
                )
            };
            buf.quad_smooth(
                [p(tx, ty), p(tx, ty + 1), p(tx + 1, ty + 1), p(tx + 1, ty)],
                color,
            );
        }
    }
    buf.into_mesh()
}

fn trees_mesh(tiles: &[Tile]) -> Mesh {
    let mut buf = MeshBuf::default();
    let trunk = linear(Color::srgb(0.32, 0.22, 0.14));
    for ty in 0..MAP_H as u8 {
        for tx in 0..MAP_W as u8 {
            let tile = &tiles[tile_index(tx, ty)];
            if tile.terrain != Terrain::Forest || tile.deposit == 0 {
                continue;
            }
            let base = tile_center_world(tx, ty);
            let n = 1 + (tile.deposit / 40).min(2) as u32;
            for i in 0..n {
                let h = hash2(tx as u32 * 31 + i * 7, ty as u32 * 17 + i * 13);
                let ox = ((h % 60) as f32 - 30.0) * 0.011;
                let oz = (((h >> 8) % 60) as f32 - 30.0) * 0.011;
                let scale = 0.65 + ((h >> 16) % 40) as f32 * 0.012;
                let c = base + Vec3::new(ox, corner_height(tx as u32, ty as u32), oz);
                let green = 0.30 + ((h >> 5) % 20) as f32 * 0.006;
                let canopy = linear(Color::srgb(0.10, green, 0.16));
                let snowy = linear(Color::srgb(0.55, 0.62 + green * 0.3, 0.62));
                buf.boxx(
                    c - Vec3::new(0.035, 0.0, 0.035),
                    c + Vec3::new(0.035, 0.16 * scale, 0.035),
                    trunk,
                );
                buf.cone(c + Vec3::Y * 0.12 * scale, 0.26 * scale, 0.42 * scale, 7, canopy);
                buf.cone(c + Vec3::Y * 0.38 * scale, 0.18 * scale, 0.34 * scale, 7, snowy);
            }
        }
    }
    buf.into_mesh()
}

fn rocks_mesh(tiles: &[Tile]) -> Mesh {
    let mut buf = MeshBuf::default();
    for ty in 0..MAP_H as u8 {
        for tx in 0..MAP_W as u8 {
            let tile = &tiles[tile_index(tx, ty)];
            if tile.terrain != Terrain::Coal || tile.deposit == 0 {
                continue;
            }
            let base = tile_center_world(tx, ty);
            let richness = (tile.deposit as f32 / 500.0).clamp(0.2, 1.0);
            for i in 0..2u32 {
                let h = hash2(tx as u32 * 13 + i * 29, ty as u32 * 7 + i * 41);
                let ox = ((h % 50) as f32 - 25.0) * 0.012;
                let oz = (((h >> 6) % 50) as f32 - 25.0) * 0.012;
                let s = 0.10 + ((h >> 12) % 30) as f32 * 0.004 + richness * 0.10;
                let dark = 0.13 + ((h >> 18) % 12) as f32 * 0.006;
                let color = linear(Color::srgb(dark, dark + 0.012, dark + 0.03));
                let c = base + Vec3::new(ox, 0.0, oz);
                buf.boxx(
                    c - Vec3::new(s, 0.0, s * 0.8),
                    c + Vec3::new(s, s * 1.5, s * 0.8),
                    color,
                );
            }
        }
    }
    buf.into_mesh()
}

pub fn sync_terrain(
    mut commands: Commands,
    view: Res<GameView>,
    assets: Res<GameAssets>,
    mut viz: ResMut<TerrainViz>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if view.tiles.is_empty() {
        return;
    }
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
    viz.trees = Some(spawn(&mut commands, &mut meshes, trees_mesh(&view.tiles)));
    viz.rocks = Some(spawn(&mut commands, &mut meshes, rocks_mesh(&view.tiles)));
    viz.cache = view.tiles.clone();
    viz.seen_tiles_version = view.tiles_version;
}

// ---------------------------------------------------------------- buildings

pub fn sync_buildings(
    mut commands: Commands,
    view: Res<GameView>,
    assets: Res<GameAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut viz: ResMut<BuildingViz>,
    mut worker_cubes: Query<(&WorkerCube, &mut Visibility)>,
    mut seen: Local<u64>,
) {
    let Some(state) = view.ready() else { return };
    if *seen == view.version {
        return;
    }
    *seen = view.version;

    for b in &state.buildings {
        if viz.0.contains_key(&b.id) {
            continue;
        }
        let e = spawn_building(&mut commands, &assets, &mut materials, b);
        viz.0.insert(b.id, e);
    }

    // Demolished / removed buildings.
    let gone: Vec<u32> = viz
        .0
        .keys()
        .filter(|id| state.find_building(**id).is_none())
        .copied()
        .collect();
    for id in gone {
        if let Some(e) = viz.0.remove(&id) {
            commands.entity(e).despawn();
        }
    }

    // Worker indicators: one small cube per assigned worker.
    for (cube, mut vis) in &mut worker_cubes {
        let workers = state
            .find_building(cube.building)
            .map(|b| b.workers)
            .unwrap_or(0);
        *vis = if cube.index < workers {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

fn spawn_building(
    commands: &mut Commands,
    assets: &GameAssets,
    materials: &mut Assets<StandardMaterial>,
    b: &frozen_city::game::types::Building,
) -> Entity {
    let center = building_center_world(b);
    let color = kind_color(b.kind);
    let body = materials.add(StandardMaterial {
        base_color: color,
        perceptual_roughness: 0.9,
        ..default()
    });
    // Created up front so the pulse system's handle can live on the root.
    let fire_mat = (b.kind == BuildingKind::Furnace).then(|| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.6, 0.2),
            emissive: LinearRgba::rgb(6.0, 2.2, 0.5),
            ..default()
        })
    });
    let root = commands
        .spawn((
            Transform::from_translation(center),
            Visibility::Inherited,
            BuildingMarker,
            DespawnOnExit(Screen::Game),
        ))
        .id();

    let mut roof_y = 0.6;
    commands.entity(root).with_children(|p| {
        match b.kind {
            BuildingKind::Furnace => {
                let stone = materials.add(StandardMaterial {
                    base_color: Color::srgb(0.34, 0.30, 0.29),
                    perceptual_roughness: 0.95,
                    ..default()
                });
                // Base, glowing core, chimney.
                p.spawn((
                    Mesh3d(assets.cylinder.clone()),
                    MeshMaterial3d(stone.clone()),
                    Transform::from_xyz(0.0, 0.3, 0.0).with_scale(Vec3::new(1.9, 0.6, 1.9)),
                ));
                p.spawn((
                    Mesh3d(assets.cylinder.clone()),
                    MeshMaterial3d(fire_mat.clone().expect("furnace fire material")),
                    Transform::from_xyz(0.0, 0.68, 0.0).with_scale(Vec3::new(1.35, 0.25, 1.35)),
                ));
                p.spawn((
                    Mesh3d(assets.cylinder.clone()),
                    MeshMaterial3d(stone),
                    Transform::from_xyz(0.0, 1.35, 0.0).with_scale(Vec3::new(0.85, 1.3, 0.85)),
                ));
                p.spawn((
                    PointLight {
                        color: Color::srgb(1.0, 0.62, 0.25),
                        intensity: 2_400_000.0,
                        range: 26.0,
                        ..default()
                    },
                    Transform::from_xyz(0.0, 2.6, 0.0),
                    FurnaceLight,
                ));
                // Chimney smoke: looping puffs, hidden when the fire is out.
                for i in 0..10u32 {
                    p.spawn((
                        Mesh3d(assets.cube.clone()),
                        MeshMaterial3d(assets.smoke_mat.clone()),
                        Transform::from_xyz(0.0, 2.2, 0.0).with_scale(Vec3::splat(0.1)),
                        Visibility::Hidden,
                        Smoke {
                            phase: i as f32 / 10.0,
                        },
                    ));
                }
            }
            BuildingKind::Tent => {
                p.spawn((
                    Mesh3d(assets.tent.clone()),
                    MeshMaterial3d(body.clone()),
                    Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::new(0.85, 0.6, 0.85)),
                ));
                roof_y = 0.7;
            }
            BuildingKind::Sawmill => {
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(body.clone()),
                    Transform::from_xyz(0.0, 0.25, 0.0).with_scale(Vec3::new(0.8, 0.5, 0.8)),
                ));
                let roof = materials.add(StandardMaterial {
                    base_color: Color::srgb(0.42, 0.28, 0.16),
                    ..default()
                });
                p.spawn((
                    Mesh3d(assets.tent.clone()),
                    MeshMaterial3d(roof),
                    Transform::from_xyz(0.0, 0.5, 0.0).with_scale(Vec3::new(0.9, 0.3, 0.9)),
                ));
                // Saw blade.
                let blade = materials.add(StandardMaterial {
                    base_color: Color::srgb(0.75, 0.78, 0.82),
                    metallic: 0.8,
                    perceptual_roughness: 0.35,
                    ..default()
                });
                p.spawn((
                    Mesh3d(assets.cylinder.clone()),
                    MeshMaterial3d(blade),
                    Transform::from_xyz(0.47, 0.3, 0.0)
                        .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
                        .with_scale(Vec3::new(0.5, 0.06, 0.5)),
                ));
                roof_y = 0.9;
            }
            BuildingKind::CoalMine => {
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(body.clone()),
                    Transform::from_xyz(0.0, 0.2, 0.0).with_scale(Vec3::new(0.8, 0.4, 0.8)),
                ));
                // Headframe tower.
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(body.clone()),
                    Transform::from_xyz(0.0, 0.7, 0.0).with_scale(Vec3::new(0.28, 0.7, 0.28)),
                ));
                roof_y = 1.15;
            }
            BuildingKind::HunterHut => {
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(body.clone()),
                    Transform::from_xyz(0.0, 0.25, 0.0).with_scale(Vec3::new(0.75, 0.5, 0.75)),
                ));
                let roof = materials.add(StandardMaterial {
                    base_color: Color::srgb(0.24, 0.35, 0.22),
                    ..default()
                });
                p.spawn((
                    Mesh3d(assets.cone.clone()),
                    MeshMaterial3d(roof),
                    Transform::from_xyz(0.0, 0.75, 0.0).with_scale(Vec3::new(1.1, 0.5, 1.1)),
                ));
                roof_y = 1.05;
            }
        }

        // A small window that glows warm at night (shared animated material).
        if b.kind != BuildingKind::Furnace {
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.window_mat.clone()),
                Transform::from_xyz(0.0, 0.22, 0.40).with_scale(Vec3::new(0.22, 0.16, 0.05)),
            ));
        }

        // Worker cubes above the roof.
        let max = b.kind.max_workers();
        if max > 0 {
            let w_mat = materials.add(StandardMaterial {
                base_color: Color::srgb(0.95, 0.97, 1.0),
                emissive: LinearRgba::rgb(0.6, 0.65, 0.75),
                ..default()
            });
            for i in 0..max {
                let off = (i as f32 - (max as f32 - 1.0) / 2.0) * 0.24;
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(w_mat.clone()),
                    Transform::from_xyz(off, roof_y + 0.12, 0.0).with_scale(Vec3::splat(0.13)),
                    Visibility::Hidden,
                    WorkerCube {
                        building: b.id,
                        index: i,
                    },
                ));
            }
        }
    });

    if let Some(fire_mat) = fire_mat {
        commands.entity(root).insert(FurnaceGlow { fire_mat });
    }
    root
}

// ---------------------------------------------------------------- survivors

pub fn sync_survivors(
    mut commands: Commands,
    view: Res<GameView>,
    assets: Res<GameAssets>,
    mut viz: ResMut<SurvivorViz>,
    mut dots: Query<(
        &SurvivorDot,
        &mut Wander,
        &mut MeshMaterial3d<StandardMaterial>,
    )>,
    mut seen: Local<u64>,
) {
    let Some(state) = view.ready() else { return };
    if *seen == view.version {
        return;
    }
    *seen = view.version;

    // Visual home for each survivor: workers cluster at their buildings (in
    // assignment order), everyone else gathers around the furnace.
    let mut homes: Vec<Vec3> = Vec::with_capacity(state.survivors.len());
    for b in &state.buildings {
        for _ in 0..b.workers {
            homes.push(building_center_world(b));
        }
    }
    while homes.len() < state.survivors.len() {
        homes.push(Vec3::new(0.0, 0.0, 2.0));
    }

    let mut wanted: HashMap<u32, Vec3> = HashMap::new();
    for (i, s) in state.survivors.iter().enumerate() {
        wanted.insert(s.id, homes[i]);
    }

    for s in &state.survivors {
        if viz.0.contains_key(&s.id) {
            continue;
        }
        let home = wanted[&s.id];
        let e = commands
            .spawn((
                Mesh3d(assets.capsule.clone()),
                MeshMaterial3d(assets.survivor_mats[0].clone()),
                Transform::from_translation(home + Vec3::Y * 0.24),
                SurvivorDot { id: s.id },
                Wander {
                    home,
                    target: home,
                    speed: 0.9 + (s.id % 7) as f32 * 0.1,
                },
                DespawnOnExit(Screen::Game),
            ))
            .id();
        viz.0.insert(s.id, e);
    }

    let gone: Vec<u32> = viz
        .0
        .keys()
        .filter(|id| !wanted.contains_key(id))
        .copied()
        .collect();
    for id in gone {
        if let Some(e) = viz.0.remove(&id) {
            commands.entity(e).despawn();
        }
    }

    // Update homes and health tint (a shared material per health tier).
    for (dot, mut wander, mut mat) in &mut dots {
        if let Some(home) = wanted.get(&dot.id) {
            if wander.home.distance(*home) > 0.05 {
                wander.home = *home;
                wander.target = *home;
            }
        }
        if let Some(s) = state.survivors.iter().find(|s| s.id == dot.id) {
            let sick = 1.0 - (s.hp / 100.0).clamp(0.0, 1.0);
            let tier = ((sick * 3.99) as usize).min(3);
            if mat.0 != assets.survivor_mats[tier] {
                mat.0 = assets.survivor_mats[tier].clone();
            }
        }
    }
}

pub fn animate_survivors(
    time: Res<Time>,
    mut q: Query<(&mut Transform, &mut Wander)>,
    mut rng: Local<Rng>,
) {
    let dt = time.delta_secs();
    for (mut t, mut w) in &mut q {
        let pos = Vec3::new(t.translation.x, 0.0, t.translation.z);
        let to = w.target - pos;
        let dist = to.length();
        if dist < 0.08 {
            let off = Vec3::new(
                rng.range(-50, 50) as f32 * 0.01,
                0.0,
                rng.range(-50, 50) as f32 * 0.01,
            );
            w.target = w.home + off;
        } else {
            let step = (w.speed * dt).min(dist);
            let np = pos + to / dist * step;
            t.translation.x = np.x;
            t.translation.z = np.z;
            // A tiny walking bob.
            t.translation.y = 0.24 + (time.elapsed_secs() * 7.0 + np.x * 3.0).sin().abs() * 0.02;
        }
    }
}

// -------------------------------------------------------------- environment

/// Sun, ambient light, fog and sky color track the in-game time of day.
pub fn animate_environment(
    view: Res<GameView>,
    assets: Res<GameAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut clear: ResMut<ClearColor>,
    mut sun: Query<(&mut DirectionalLight, &mut Transform), With<SunLight>>,
    mut cam_fx: Query<(&mut DistanceFog, &mut AmbientLight)>,
) {
    let Some(state) = view.state.as_ref() else { return };
    let t = state.time_of_day();
    let daylight = (1.0 - (std::f32::consts::TAU * t).cos()) / 2.0;
    let cold = state.cold_snap && state.is_night();

    // Windows glow as the light fades.
    let glow = (1.0 - daylight).powf(2.0);
    if let Some(mut m) = materials.get_mut(&assets.window_mat) {
        m.emissive = LinearRgba::rgb(3.2 * glow + 0.02, 1.9 * glow + 0.015, 0.55 * glow);
    }

    let sky_night = Vec3::new(0.012, 0.022, 0.052);
    let sky_day = if cold {
        Vec3::new(0.30, 0.36, 0.48)
    } else {
        Vec3::new(0.42, 0.50, 0.62)
    };
    let sky = sky_night.lerp(sky_day, daylight.powf(1.2));
    clear.0 = Color::srgb(sky.x, sky.y, sky.z);

    for (mut f, mut ambient) in &mut cam_fx {
        f.color = Color::srgb(sky.x, sky.y, sky.z);
        let start = 24.0 + 46.0 * daylight;
        f.falloff = FogFalloff::Linear {
            start,
            end: start + 60.0 + 40.0 * daylight,
        };
        ambient.brightness = 45.0 + 300.0 * daylight;
        ambient.color = if cold {
            Color::srgb(0.55, 0.68, 1.0)
        } else {
            Color::srgb(0.70, 0.78, 0.95)
        };
    }

    if let Ok((mut light, mut tr)) = sun.single_mut() {
        light.illuminance = 250.0 + 10_500.0 * daylight.powf(1.3);
        // Low sun is warm, midday is neutral, night is moon-blue.
        light.color = if daylight > 0.05 {
            let warm = (1.0 - daylight).powf(1.5);
            Color::srgb(1.0, 0.96 - 0.25 * warm, 0.90 - 0.42 * warm)
        } else {
            Color::srgb(0.65, 0.72, 1.0)
        };
        let elev = 0.18 + 1.15 * daylight;
        let az = 2.35 + (t - 0.5) * 0.9;
        let sun_dir = -Vec3::new(az.cos() * elev.cos(), elev.sin(), az.sin() * elev.cos());
        *tr = Transform::default().looking_to(sun_dir, Vec3::Y);
    }

}

/// Furnace fire, heat ring and the selection ring.
pub fn animate_effects(
    time: Res<Time>,
    view: Res<GameView>,
    selection: Res<Selection>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    furnaces: Query<&FurnaceGlow>,
    mut lights: Query<&mut PointLight, With<FurnaceLight>>,
    mut heat: Query<(&HeatRing, &mut Transform, &mut Visibility), Without<SelectionRing>>,
    mut sel_ring: Query<(&mut Transform, &mut Visibility), With<SelectionRing>>,
    mut glow_r: Local<f32>,
) {
    let Some(state) = view.state.as_ref() else { return };
    let pulse = (time.elapsed_secs() * 6.0).sin();

    // Heat radius ring.
    let target = if state.furnace_lit {
        state.heat_radius() * TILE
    } else {
        0.0
    };
    *glow_r += (target - *glow_r) * (4.0 * time.delta_secs()).min(1.0);
    for (ring, mut tr, mut vis) in &mut heat {
        if *glow_r < 0.5 {
            *vis = Visibility::Hidden;
        } else {
            *vis = Visibility::Visible;
            tr.scale = Vec3::splat(*glow_r);
            if let Some(mut m) = materials.get_mut(&ring.mat) {
                m.base_color = Color::srgba(
                    1.0,
                    0.55,
                    0.18,
                    0.22 + 0.05 * state.furnace_level as f32 + 0.03 * pulse,
                );
            }
        }
    }

    // Fire glow + light pulse.
    for glow in &furnaces {
        if let Some(mut m) = materials.get_mut(&glow.fire_mat) {
            m.emissive = if state.furnace_lit {
                let k = (1.0 + 0.18 * pulse) * (0.7 + 0.3 * state.furnace_level as f32);
                LinearRgba::rgb(6.0 * k, 2.2 * k, 0.5 * k)
            } else {
                LinearRgba::rgb(0.08, 0.05, 0.04)
            };
        }
    }
    for mut light in &mut lights {
        light.intensity = if state.furnace_lit {
            (900_000.0 + 750_000.0 * state.furnace_level as f32) * (1.0 + 0.12 * pulse)
        } else {
            0.0
        };
        light.range = 6.0 + state.heat_radius() * 1.2;
    }

    // Selection ring.
    for (mut tr, mut vis) in &mut sel_ring {
        let sel = selection.0.and_then(|id| state.find_building(id));
        if let Some(b) = sel {
            let (w, h) = b.kind.size();
            *vis = Visibility::Visible;
            let pos = building_center_world(b);
            tr.translation.x = pos.x;
            tr.translation.z = pos.z;
            tr.scale = Vec3::splat(w.max(h) as f32 * 0.8);
        } else {
            *vis = Visibility::Hidden;
        }
    }
}

/// Chimney smoke rises, drifts and grows while the furnace burns.
pub fn animate_smoke(
    time: Res<Time>,
    view: Res<GameView>,
    mut q: Query<(&Smoke, &mut Transform, &mut Visibility)>,
) {
    let lit = view
        .state
        .as_ref()
        .map(|s| s.furnace_lit)
        .unwrap_or(false);
    let elapsed = time.elapsed_secs();
    for (smoke, mut tr, mut vis) in &mut q {
        if !lit {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Inherited;
        let t = (elapsed * 0.22 + smoke.phase).fract();
        let sway = (smoke.phase * 37.0 + elapsed * 0.6).sin();
        tr.translation = Vec3::new(
            sway * 0.35 * t,
            2.15 + t * 3.4,
            (smoke.phase * 53.0 + elapsed * 0.45).cos() * 0.3 * t,
        );
        // Puffs grow as they rise, then pop back to the chimney.
        tr.scale = Vec3::splat(0.08 + t * 0.42);
    }
}

// ------------------------------------------------------------------ cursors

pub fn sync_player_cursors(
    mut commands: Commands,
    time: Res<Time>,
    view: Res<GameView>,
    assets: Res<GameAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut viz: ResMut<CursorViz>,
    mut q: Query<(&CursorMarker, &mut Transform)>,
) {
    let Some(state) = view.state.as_ref() else { return };
    let me = view.player_id.unwrap_or(0);

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
        let mat = materials.add(StandardMaterial {
            base_color: color,
            emissive: LinearRgba::from(color.to_linear()) * 0.6,
            ..default()
        });
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

// --------------------------------------------------------------------- snow

pub fn snow_fall(
    time: Res<Time>,
    rig: Res<super::input::CamRig>,
    mut flakes: Query<(&mut Transform, &Snowflake)>,
    mut rng: Local<Rng>,
) {
    let half = 24.0;
    let top = 14.0;
    let focus = rig.focus;
    let dt = time.delta_secs();
    let elapsed = time.elapsed_secs();

    for (mut t, flake) in &mut flakes {
        t.translation.y -= flake.fall * dt;
        t.translation.x += (elapsed * 0.8 + flake.phase).sin() * flake.drift * dt;
        t.translation.z += (elapsed * 0.63 + flake.phase * 1.7).cos() * flake.drift * 0.6 * dt;
        if t.translation.y < 0.0 {
            t.translation.y = top;
            t.translation.x = focus.x + rng.range(-(half as i32), half as i32) as f32;
            t.translation.z = focus.z + rng.range(-(half as i32), half as i32) as f32;
        }
        if (t.translation.x - focus.x).abs() > half * 1.6
            || (t.translation.z - focus.z).abs() > half * 1.6
        {
            t.translation.x = focus.x + rng.range(-(half as i32), half as i32) as f32;
            t.translation.z = focus.z + rng.range(-(half as i32), half as i32) as f32;
        }
    }
}
