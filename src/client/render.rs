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

/// Fixed `BuildingKind` order backing `GameAssets::building_mats` — index
/// `i` corresponds to `ALL_KINDS[i]`.
const ALL_KINDS: [BuildingKind; 9] = [
    BuildingKind::Furnace,
    BuildingKind::Tent,
    BuildingKind::Sawmill,
    BuildingKind::CoalMine,
    BuildingKind::HunterHut,
    BuildingKind::Greenhouse,
    BuildingKind::Hospital,
    BuildingKind::Kitchen,
    BuildingKind::Warehouse,
];

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
    /// One material per player-palette color for remote-player cursor
    /// markers (dimmer emissive than `ping_mats`), shared so cursors batch.
    pub cursor_mats: [Handle<StandardMaterial>; 8],
    /// One body material per player-palette color for the central-world
    /// light avatars (see `AvatarViz`) — a plain (non-emissive) tint so
    /// avatars read as people, not markers; shared so avatars of the same
    /// color batch into one draw call, same trick as `survivor_mats`.
    pub avatar_mats: [Handle<StandardMaterial>; 8],
    /// One body material per `BuildingKind` (see `ALL_KINDS`), shared so
    /// every building of the same kind batches into one draw call.
    pub building_mats: [Handle<StandardMaterial>; 9],
    /// Furnace base/chimney stone — identical for every furnace.
    pub furnace_stone_mat: Handle<StandardMaterial>,
    pub sawmill_roof_mat: Handle<StandardMaterial>,
    pub sawmill_blade_mat: Handle<StandardMaterial>,
    pub hunter_roof_mat: Handle<StandardMaterial>,
    pub greenhouse_glass_mat: Handle<StandardMaterial>,
    pub hospital_cross_mat: Handle<StandardMaterial>,
    pub kitchen_stone_mat: Handle<StandardMaterial>,
    pub warehouse_plank_mat: Handle<StandardMaterial>,
    /// Roof worker-indicator cube; identical for every building.
    pub worker_mat: Handle<StandardMaterial>,
}

/// Shared body material handle for a building kind — keeps same-kind
/// buildings batched into a single draw call instead of each getting its
/// own `StandardMaterial`.
fn building_mat(assets: &GameAssets, kind: BuildingKind) -> Handle<StandardMaterial> {
    let i = ALL_KINDS
        .iter()
        .position(|&k| k == kind)
        .expect("kind is present in ALL_KINDS");
    assets.building_mats[i].clone()
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

/// Central-world light avatars: one body + one screen-space name label per
/// connected player (including the local player — everyone sees everyone,
/// unlike `CursorViz` which skips "me"). Only populated while
/// `GameState.central` is true; `sync_player_cursors` keeps its normal
/// marker-cursor behavior in every other world unchanged.
#[derive(Resource, Default)]
pub struct AvatarViz(pub HashMap<u64, (Entity, Entity)>);

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

/// Drives a survivor entity toward the sim-authoritative position
/// (`Survivor.x/y`, see `types.rs`'s V0.7 doc comment), smoothed with an
/// exponential lerp — the same pattern `sync_player_cursors`/`sync_avatars`
/// use for remote cursors/avatars. Once within `shuffle_target` of that sim
/// goal (i.e. the survivor is standing still, not actively walking), a tiny
/// ±0.3-tile shuffle keeps them from looking frozen — purely cosmetic, never
/// fights the authoritative position.
#[derive(Component)]
pub struct Wander {
    /// Latest sim position (world space), refreshed every snapshot.
    pub sim_pos: Vec3,
    /// Small cosmetic shuffle offset from `sim_pos`, active only once the
    /// entity has visually caught up to `sim_pos`.
    pub shuffle_target: Vec3,
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

/// Central-world light-avatar body: walks (lerps) toward the owning
/// player's synced cursor tile. Distinct from `CursorMarker` (which floats a
/// marker cone above the cursor in every other world) and from `Wander`
/// (survivor idle wandering around a fixed home) — an avatar has neither a
/// fixed home nor autonomous wandering, it only ever chases the live cursor.
#[derive(Component)]
pub struct AvatarWalk {
    pub player: u64,
    pub target: Vec3,
}

/// UI text node that follows a central-world avatar's name on screen —
/// same projection idea as `CursorLabel`, kept as a separate type so the two
/// systems' queries stay disjoint (Bevy 0.19 ECS safety: no query ever needs
/// `Without<CursorLabel>` just to also see `AvatarLabel`, they're unrelated).
#[derive(Component)]
pub struct AvatarLabel {
    pub player: u64,
}

#[derive(Component)]
pub struct HeatRing {
    pub mat: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct SelectionRing;

/// Ring hovering under the selected survivor (`roster::SurvivorSelection`),
/// distinct from `SelectionRing` (buildings) so the two never fight over the
/// same component in a query — see the module's "one query conflict per new
/// system" rule.
#[derive(Component)]
pub struct SurvivorSelectionRing;

/// Small low-poly crown worn by the appointed leader (`GameState.leader`).
/// One instance is spawned as a child of the leader's `SurvivorDot` entity
/// and moved to whichever survivor is currently leader on change.
#[derive(Component)]
pub struct LeaderCrown;

/// A brief expanding ring dropped at a `MoveSurvivor` destination as visual
/// confirmation the command was sent — purely cosmetic, self-despawns after
/// `MOVE_PING_LIFETIME` seconds.
#[derive(Component)]
pub struct MoveOrderPing {
    pub age: f32,
}

pub const MOVE_PING_LIFETIME: f32 = 0.6;

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

/// Newly-spawned buildings scale up from nothing for tactile placement feedback.
#[derive(Component)]
pub struct SpawnGrow {
    pub age: f32,
}

/// Full-screen cold haze that fades in during a blizzard.
#[derive(Component)]
pub struct BlizzardOverlay;

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
        cursor_mats: std::array::from_fn(|i| {
            let c = player_color(i as u8);
            materials.add(StandardMaterial {
                base_color: c,
                emissive: c.to_linear() * 0.6,
                ..default()
            })
        }),
        avatar_mats: std::array::from_fn(|i| {
            let c = player_color(i as u8);
            materials.add(StandardMaterial {
                base_color: c,
                perceptual_roughness: 0.85,
                ..default()
            })
        }),
        building_mats: std::array::from_fn(|i| {
            materials.add(StandardMaterial {
                base_color: kind_color(ALL_KINDS[i]),
                perceptual_roughness: 0.9,
                ..default()
            })
        }),
        furnace_stone_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.34, 0.30, 0.29),
            perceptual_roughness: 0.95,
            ..default()
        }),
        sawmill_roof_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.42, 0.28, 0.16),
            ..default()
        }),
        sawmill_blade_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.75, 0.78, 0.82),
            metallic: 0.8,
            perceptual_roughness: 0.35,
            ..default()
        }),
        hunter_roof_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.24, 0.35, 0.22),
            ..default()
        }),
        greenhouse_glass_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.62, 0.88, 0.72),
            ..default()
        }),
        hospital_cross_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.85, 0.20, 0.20),
            emissive: LinearRgba::rgb(0.35, 0.04, 0.04),
            ..default()
        }),
        kitchen_stone_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.40, 0.36, 0.34),
            ..default()
        }),
        warehouse_plank_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.50, 0.38, 0.24),
            perceptual_roughness: 0.85,
            ..default()
        }),
        worker_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.95, 0.97, 1.0),
            emissive: LinearRgba::rgb(0.6, 0.65, 0.75),
            ..default()
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
    quality: Res<Quality>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut rig: ResMut<super::input::CamRig>,
    mut transition: ResMut<super::TransitionMsg>,
) {
    *rig = super::input::CamRig::default();
    // A pending transition message (set by whichever button requested this
    // world switch, just before the menu-frame trip) starts its on-screen
    // countdown now that the new world has actually loaded — see
    // `TransitionMsg`'s doc comment for why it isn't shown during the
    // (single-frame, too-fast-to-read) menu trip itself.
    transition.age = 0.0;

    // Full-screen cold haze for blizzards. `GlobalZIndex(-1)` keeps it behind
    // the HUD (so panels stay clear) and it has no `Interaction`, so it never
    // captures clicks — it only tints the visible world.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.80, 0.86, 0.95, 0.0)),
        GlobalZIndex(-1),
        BlizzardOverlay,
        DespawnOnExit(Screen::Game),
    ));

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

    // Survivor selection highlight ring — a distinct color from the building
    // ring above so the two read as different kinds of selection at a glance.
    let survivor_sel_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.45, 0.85, 1.0, 0.65),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    commands.spawn((
        Mesh3d(assets.ring.clone()),
        MeshMaterial3d(survivor_sel_mat),
        Transform::from_xyz(0.0, 0.05, 0.0)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::splat(0.55)),
        Visibility::Hidden,
        SurvivorSelectionRing,
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

    // Snowfall volume around the camera focus. Phones get far fewer flakes —
    // each is a translucent draw and mobile GPUs are fill-rate bound.
    let flake_count = if *quality == Quality::Low { 60 } else { 240 };
    let mut rng = Rng::new(0x5005_7EA1);
    for _ in 0..flake_count {
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

fn trees_mesh(tiles: &[Tile], dense: bool) -> Mesh {
    let mut buf = MeshBuf::default();
    let trunk = linear(Color::srgb(0.32, 0.22, 0.14));
    // Phones: one tree per tile, fewer cone facets, no snowy cap cone.
    let seg = if dense { 7 } else { 5 };
    for ty in 0..MAP_H as u8 {
        for tx in 0..MAP_W as u8 {
            let tile = &tiles[tile_index(tx, ty)];
            if tile.terrain != Terrain::Forest || tile.deposit == 0 {
                continue;
            }
            let base = tile_center_world(tx, ty);
            let n = if dense {
                1 + (tile.deposit / 40).min(2) as u32
            } else {
                1
            };
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
                buf.cone(c + Vec3::Y * 0.12 * scale, 0.26 * scale, 0.42 * scale, seg, canopy);
                if dense {
                    buf.cone(c + Vec3::Y * 0.38 * scale, 0.18 * scale, 0.34 * scale, seg, snowy);
                }
            }
        }
    }
    buf.into_mesh()
}

fn rocks_mesh(tiles: &[Tile], dense: bool) -> Mesh {
    let mut buf = MeshBuf::default();
    let rocks_per_tile = if dense { 2u32 } else { 1 };
    for ty in 0..MAP_H as u8 {
        for tx in 0..MAP_W as u8 {
            let tile = &tiles[tile_index(tx, ty)];
            if tile.terrain != Terrain::Coal || tile.deposit == 0 {
                continue;
            }
            let base = tile_center_world(tx, ty);
            let richness = (tile.deposit as f32 / 500.0).clamp(0.2, 1.0);
            for i in 0..rocks_per_tile {
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

// ---------------------------------------------------------------- buildings

pub fn sync_buildings(
    mut commands: Commands,
    view: Res<GameView>,
    assets: Res<GameAssets>,
    quality: Res<Quality>,
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
        let e = spawn_building(&mut commands, &assets, &mut materials, b, *quality == Quality::Low);
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
    low: bool,
) -> Entity {
    let center = building_center_world(b);
    // `body` (the shared per-kind material) is looked up inside each
    // non-furnace arm below — furnaces use `furnace_stone_mat` + a
    // per-entity `fire_mat` instead and never touch it, so the
    // linear-search + clone in `building_mat` is skipped for them.
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
            SpawnGrow { age: 0.0 },
            DespawnOnExit(Screen::Game),
        ))
        .id();

    let mut roof_y = 0.6;
    commands.entity(root).with_children(|p| {
        match b.kind {
            BuildingKind::Furnace => {
                let stone = assets.furnace_stone_mat.clone();
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
                // Per-fragment point lighting is costly on mobile WebGL2, so
                // phones skip it — the emissive fire still reads as a glow.
                if !low {
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
                }
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
                let body = building_mat(assets, b.kind);
                p.spawn((
                    Mesh3d(assets.tent.clone()),
                    MeshMaterial3d(body.clone()),
                    Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::new(0.85, 0.6, 0.85)),
                ));
                roof_y = 0.7;
            }
            BuildingKind::Sawmill => {
                let body = building_mat(assets, b.kind);
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(body.clone()),
                    Transform::from_xyz(0.0, 0.25, 0.0).with_scale(Vec3::new(0.8, 0.5, 0.8)),
                ));
                let roof = assets.sawmill_roof_mat.clone();
                p.spawn((
                    Mesh3d(assets.tent.clone()),
                    MeshMaterial3d(roof),
                    Transform::from_xyz(0.0, 0.5, 0.0).with_scale(Vec3::new(0.9, 0.3, 0.9)),
                ));
                // Saw blade.
                let blade = assets.sawmill_blade_mat.clone();
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
                let body = building_mat(assets, b.kind);
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
                let body = building_mat(assets, b.kind);
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(body.clone()),
                    Transform::from_xyz(0.0, 0.25, 0.0).with_scale(Vec3::new(0.75, 0.5, 0.75)),
                ));
                let roof = assets.hunter_roof_mat.clone();
                p.spawn((
                    Mesh3d(assets.cone.clone()),
                    MeshMaterial3d(roof),
                    Transform::from_xyz(0.0, 0.75, 0.0).with_scale(Vec3::new(1.1, 0.5, 1.1)),
                ));
                roof_y = 1.05;
            }
            BuildingKind::Greenhouse => {
                let body = building_mat(assets, b.kind);
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(body.clone()),
                    Transform::from_xyz(0.0, 0.20, 0.0).with_scale(Vec3::new(0.8, 0.40, 0.8)),
                ));
                // Bright glass roof.
                let glass = assets.greenhouse_glass_mat.clone();
                p.spawn((
                    Mesh3d(assets.tent.clone()),
                    MeshMaterial3d(glass),
                    Transform::from_xyz(0.0, 0.40, 0.0).with_scale(Vec3::new(0.92, 0.38, 0.92)),
                ));
                roof_y = 0.85;
            }
            BuildingKind::Hospital => {
                let body = building_mat(assets, b.kind);
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(body.clone()),
                    Transform::from_xyz(0.0, 0.28, 0.0).with_scale(Vec3::new(0.8, 0.56, 0.8)),
                ));
                // Red cross on the roof (two crossbars).
                let cross = assets.hospital_cross_mat.clone();
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(cross.clone()),
                    Transform::from_xyz(0.0, 0.62, 0.0).with_scale(Vec3::new(0.30, 0.08, 0.10)),
                ));
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(cross),
                    Transform::from_xyz(0.0, 0.62, 0.0).with_scale(Vec3::new(0.10, 0.08, 0.30)),
                ));
                roof_y = 0.92;
            }
            BuildingKind::Kitchen => {
                let body = building_mat(assets, b.kind);
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(body.clone()),
                    Transform::from_xyz(0.0, 0.24, 0.0).with_scale(Vec3::new(0.78, 0.48, 0.78)),
                ));
                // A little chimney.
                let stone = assets.kitchen_stone_mat.clone();
                p.spawn((
                    Mesh3d(assets.cylinder.clone()),
                    MeshMaterial3d(stone),
                    Transform::from_xyz(0.22, 0.58, 0.0).with_scale(Vec3::new(0.16, 0.5, 0.16)),
                ));
                roof_y = 0.9;
            }
            BuildingKind::Warehouse => {
                let body = building_mat(assets, b.kind);
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(body),
                    Transform::from_xyz(0.0, 0.22, 0.0).with_scale(Vec3::new(0.92, 0.40, 0.92)),
                ));
                // Flat plank roof cap, plus a couple of stacked crates by the
                // door — the "storage" tell at a glance.
                let planks = assets.warehouse_plank_mat.clone();
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(planks.clone()),
                    Transform::from_xyz(0.0, 0.44, 0.0).with_scale(Vec3::new(0.98, 0.06, 0.98)),
                ));
                for (dx, dz, h) in [(-0.28, 0.30, 0.16), (-0.10, 0.32, 0.20)] {
                    p.spawn((
                        Mesh3d(assets.cube.clone()),
                        MeshMaterial3d(planks.clone()),
                        Transform::from_xyz(dx, h * 0.5, dz).with_scale(Vec3::splat(h)),
                    ));
                }
                roof_y = 0.48;
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
            let w_mat = assets.worker_mat.clone();
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

/// Survivor world position from the sim's authoritative tile coordinates
/// (`Survivor.x/y` — see `types.rs`'s V0.7 doc comment). The server already
/// walks survivors toward their `move_target` or assigned-building goal every
/// tick, so the client no longer picks its own idle/work position — it just
/// renders where the sim says they are.
fn survivor_sim_world(s: &frozen_city::game::types::Survivor) -> Vec3 {
    tilef_to_world((s.x, s.y))
}

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

    for s in &state.survivors {
        if viz.0.contains_key(&s.id) {
            continue;
        }
        let pos = survivor_sim_world(s);
        let e = commands
            .spawn((
                Mesh3d(assets.capsule.clone()),
                MeshMaterial3d(assets.survivor_mats[0].clone()),
                Transform::from_translation(pos + Vec3::Y * 0.24),
                SurvivorDot { id: s.id },
                Wander {
                    sim_pos: pos,
                    shuffle_target: pos,
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
        .filter(|id| !state.survivors.iter().any(|s| s.id == **id))
        .copied()
        .collect();
    for id in gone {
        if let Some(e) = viz.0.remove(&id) {
            commands.entity(e).despawn();
        }
    }

    // Refresh each entity's sim-position goal and health tint (a shared
    // material per health tier).
    for (dot, mut wander, mut mat) in &mut dots {
        if let Some(s) = state.survivors.iter().find(|s| s.id == dot.id) {
            let pos = survivor_sim_world(s);
            if wander.sim_pos.distance(pos) > 0.001 {
                wander.sim_pos = pos;
            }
            let sick = 1.0 - (s.hp / 100.0).clamp(0.0, 1.0);
            let tier = ((sick * 3.99) as usize).min(3);
            if mat.0 != assets.survivor_mats[tier] {
                mat.0 = assets.survivor_mats[tier].clone();
            }
        }
    }
}

/// Lerp each survivor toward the sim's authoritative position; once caught
/// up, a tiny ±0.3-tile shuffle keeps a stationary survivor from looking
/// frozen. The shuffle is purely cosmetic and re-centers on `sim_pos` — it
/// never accumulates drift away from the authoritative location.
pub fn animate_survivors(
    time: Res<Time>,
    mut q: Query<(&mut Transform, &mut Wander)>,
    mut rng: Local<Rng>,
) {
    const SHUFFLE_RADIUS: f32 = 0.3;
    let dt = time.delta_secs();
    let blend = 1.0 - (-6.0 * dt).exp();
    for (mut t, mut w) in &mut q {
        let pos = Vec3::new(t.translation.x, 0.0, t.translation.z);
        // Caught up to the sim goal (within the shuffle radius): idle-shuffle
        // around it instead of chasing a now-static point exactly.
        if pos.distance(w.sim_pos) < SHUFFLE_RADIUS + 0.05 {
            let to = w.shuffle_target - pos;
            let dist = to.length();
            if dist < 0.08 {
                let off = Vec3::new(
                    rng.range(-100, 100) as f32 * 0.01 * SHUFFLE_RADIUS,
                    0.0,
                    rng.range(-100, 100) as f32 * 0.01 * SHUFFLE_RADIUS,
                );
                w.shuffle_target = w.sim_pos + off;
            } else {
                let step = (w.speed * 0.3 * dt).min(dist);
                let np = pos + to / dist * step;
                t.translation.x = np.x;
                t.translation.z = np.z;
            }
        } else {
            // Actively walking toward the sim goal: reset the shuffle so it
            // doesn't fight the real movement once arrived, and lerp there —
            // same exponential smoothing `sync_player_cursors`/`sync_avatars`
            // use for remote cursors/avatars.
            w.shuffle_target = w.sim_pos;
            let np = pos.lerp(w.sim_pos, blend);
            t.translation.x = np.x;
            t.translation.z = np.z;
        }
        // A tiny walking bob.
        t.translation.y = 0.24 + (time.elapsed_secs() * 7.0 + t.translation.x * 3.0).sin().abs() * 0.02;
    }
}

/// Track the survivor-selection ring under whichever survivor is currently
/// selected in the roster panel (`roster::SurvivorSelection`). Reads
/// `SurvivorDot` transforms to follow the selected survivor as they walk.
pub fn animate_survivor_selection(
    time: Res<Time>,
    survivor_sel: Res<super::roster::SurvivorSelection>,
    dots: Query<(&SurvivorDot, &Transform), Without<SurvivorSelectionRing>>,
    mut ring: Query<(&mut Transform, &mut Visibility), With<SurvivorSelectionRing>>,
) {
    let Ok((mut tr, mut vis)) = ring.single_mut() else { return };
    let Some(id) = survivor_sel.0 else {
        *vis = Visibility::Hidden;
        return;
    };
    let Some((_, dot_tr)) = dots.iter().find(|(d, _)| d.id == id) else {
        *vis = Visibility::Hidden;
        return;
    };
    *vis = Visibility::Visible;
    tr.translation.x = dot_tr.translation.x;
    tr.translation.z = dot_tr.translation.z;
    let pulse = 1.0 + 0.08 * (time.elapsed_secs() * 5.0).sin();
    tr.scale = Vec3::splat(0.55 * pulse);
}

/// Keep one crown mesh as a child of whichever `SurvivorDot` entity is the
/// current `GameState.leader`, (re)parenting it when leadership changes
/// (appointment, succession, or death — `leader` cleared with no replacement
/// just hides it). A single shared crown entity, not one per survivor, since
/// there is at most one leader at a time.
pub fn sync_leader_crown(
    mut commands: Commands,
    view: Res<GameView>,
    assets: Res<GameAssets>,
    viz: Res<SurvivorViz>,
    mut crown: Local<Option<Entity>>,
    mut crown_mat: Local<Option<Handle<StandardMaterial>>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut seen_leader: Local<Option<u32>>,
) {
    let Some(state) = view.state.as_ref() else { return };
    if *seen_leader == state.leader {
        return;
    }
    *seen_leader = state.leader;

    let mat = crown_mat
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.82, 0.20),
                emissive: LinearRgba::rgb(0.35, 0.28, 0.03),
                metallic: 0.4,
                perceptual_roughness: 0.3,
                ..default()
            })
        })
        .clone();

    match (state.leader.and_then(|id| viz.0.get(&id)), *crown) {
        (Some(&parent), Some(existing)) => {
            commands.entity(existing).insert(ChildOf(parent));
        }
        (Some(&parent), None) => {
            let e = commands
                .spawn((
                    Mesh3d(assets.cone.clone()),
                    MeshMaterial3d(mat),
                    // Tip-up (the mesh's default orientation) so it reads as
                    // a crown sitting on the survivor's head, unlike the
                    // downward-pointing cones used for cursor/ping markers.
                    Transform::from_xyz(0.0, 0.30, 0.0).with_scale(Vec3::new(0.16, 0.16, 0.16)),
                    LeaderCrown,
                    ChildOf(parent),
                ))
                .id();
            *crown = Some(e);
        }
        (None, Some(existing)) => {
            commands.entity(existing).despawn();
            *crown = None;
        }
        (None, None) => {}
    }
}

/// Spawn a brief expanding ring at a `MoveSurvivor` destination — visual
/// confirmation the walk command was actually sent, since the survivor
/// itself won't visibly react for a tick or two over the network. Drains
/// `MoveOrderQueue`, a small inbox `input.rs`/`touch.rs` push into right
/// after sending the command (same "resource inbox" shape as
/// `SocialState::bubbles`, chosen over a Bevy `Message` type since nothing
/// else in this client defines a custom one).
pub fn spawn_move_ping(
    mut commands: Commands,
    assets: Res<GameAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut queue: ResMut<super::MoveOrderQueue>,
) {
    for (x, y) in queue.0.drain(..) {
        let mat = materials.add(StandardMaterial {
            base_color: Color::srgba(0.45, 0.85, 1.0, 0.8),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        });
        commands.spawn((
            Mesh3d(assets.ring.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(tilef_to_world((x as f32 + 0.5, y as f32 + 0.5)))
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                .with_scale(Vec3::splat(0.1)),
            MoveOrderPing { age: 0.0 },
            DespawnOnExit(Screen::Game),
        ));
    }
}

pub fn animate_move_pings(
    time: Res<Time>,
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(
        Entity,
        &mut MoveOrderPing,
        &mut Transform,
        &MeshMaterial3d<StandardMaterial>,
    )>,
) {
    for (e, mut ping, mut tr, mat) in &mut q {
        ping.age += time.delta_secs();
        let t = (ping.age / MOVE_PING_LIFETIME).clamp(0.0, 1.0);
        tr.scale = Vec3::splat(0.1 + 0.9 * t);
        if let Some(mut m) = materials.get_mut(&mat.0) {
            m.base_color = m.base_color.with_alpha(0.8 * (1.0 - t));
        }
        if ping.age >= MOVE_PING_LIFETIME {
            commands.entity(e).despawn();
        }
    }
}

// -------------------------------------------------------------- environment

/// Sun, ambient light, fog and sky color track the in-game time of day.
pub fn animate_environment(
    time: Res<Time>,
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
    let blizzard = state.blizzard_active();

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
    let mut sky = sky_night.lerp(sky_day, daylight.powf(1.2));

    // Aurora: a faint green/violet shimmer high in the deep-night sky.
    let night = (1.0 - daylight * 4.0).clamp(0.0, 1.0);
    if night > 0.0 {
        let e = time.elapsed_secs();
        let g = 0.030 * (e * 0.23).sin().max(0.0) * night;
        let v = 0.020 * (e * 0.17 + 1.3).sin().max(0.0) * night;
        sky.x += v * 0.6;
        sky.y += g;
        sky.z += v;
    }
    // Blizzard whiteout: pull the sky toward pale cold gray.
    if blizzard {
        sky = sky.lerp(Vec3::new(0.55, 0.60, 0.68), 0.6);
    }
    clear.0 = Color::srgb(sky.x, sky.y, sky.z);

    // During a blizzard visibility collapses (fog closes right in).
    let vis = if blizzard { 0.45 } else { 1.0 };
    for (mut f, mut ambient) in &mut cam_fx {
        f.color = Color::srgb(sky.x, sky.y, sky.z);
        let start = (24.0 + 46.0 * daylight) * vis;
        f.falloff = FogFalloff::Linear {
            start,
            end: start + (60.0 + 40.0 * daylight) * vis,
        };
        ambient.brightness = 45.0 + 300.0 * daylight;
        ambient.color = if cold || blizzard {
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

/// Grow newly-placed buildings from almost nothing over a short beat.
pub fn animate_spawn(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Transform, &mut SpawnGrow)>,
) {
    for (e, mut tr, mut grow) in &mut q {
        grow.age += time.delta_secs();
        let t = (grow.age / 0.35).clamp(0.0, 1.0);
        // Smoothstep from a tiny seed to full size.
        let s = 0.08 + 0.92 * (t * t * (3.0 - 2.0 * t));
        tr.scale = Vec3::splat(s);
        if t >= 1.0 {
            tr.scale = Vec3::ONE;
            commands.entity(e).remove::<SpawnGrow>();
        }
    }
}

/// Fade the full-screen cold haze in/out with the blizzard, plus a faint pulse.
pub fn animate_blizzard_overlay(
    time: Res<Time>,
    view: Res<GameView>,
    mut q: Query<&mut BackgroundColor, With<BlizzardOverlay>>,
    mut alpha: Local<f32>,
) {
    let active = view
        .state
        .as_ref()
        .map(|s| s.blizzard_active())
        .unwrap_or(false);
    let target = if active { 0.20 } else { 0.0 };
    *alpha += (target - *alpha) * (2.0 * time.delta_secs()).min(1.0);
    let pulse = if active {
        0.03 * (time.elapsed_secs() * 1.7).sin()
    } else {
        0.0
    };
    for mut bg in &mut q {
        bg.0 = Color::srgba(0.80, 0.86, 0.95, (*alpha + pulse).max(0.0));
    }
}

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

// --------------------------------------------------------------------- snow

pub fn snow_fall(
    time: Res<Time>,
    rig: Res<super::input::CamRig>,
    view: Res<GameView>,
    mut flakes: Query<(&mut Transform, &Snowflake)>,
    mut rng: Local<Rng>,
) {
    let half = 24.0;
    let top = 14.0;
    let focus = rig.focus;
    let dt = time.delta_secs();
    let elapsed = time.elapsed_secs();
    // A blizzard drives the snow harder and more sideways.
    let blizzard = view
        .state
        .as_ref()
        .map(|s| s.blizzard_active())
        .unwrap_or(false);
    let fall_k = if blizzard { 2.0 } else { 1.0 };
    let drift_k = if blizzard { 2.6 } else { 1.0 };

    for (mut t, flake) in &mut flakes {
        t.translation.y -= flake.fall * fall_k * dt;
        t.translation.x += (elapsed * 0.8 + flake.phase).sin() * flake.drift * drift_k * dt;
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
