use bevy::anti_alias::fxaa::Fxaa;
use bevy::camera::Hdr;
use bevy::gltf::GltfAssetLabel;
use bevy::light::{CascadeShadowConfigBuilder, DirectionalLightShadowMap};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;

use frozen_city::game::types::{BuildingKind, Profession};

use super::*;
use crate::client::*;

// ---------------------------------------------------------------- resources

/// Fixed `BuildingKind` order backing `GameAssets::building_mats` — index
/// `i` corresponds to `ALL_KINDS[i]`.
///
/// V0.22: MUST list every variant — `building_mat`'s lookup panics on a
/// miss. `SnowCrew` (V0.19) was missing here until this pass, which meant
/// placing one paniced the client; caught while wiring furnished rooms into
/// `spawn_room` (every room-kind arm calls `building_mat`, including this
/// one now that it's no longer its own dedicated match arm).
const ALL_KINDS: [BuildingKind; 16] = [
    BuildingKind::Furnace,
    BuildingKind::Tent,
    BuildingKind::Sawmill,
    BuildingKind::CoalMine,
    BuildingKind::HunterHut,
    BuildingKind::Greenhouse,
    BuildingKind::Hospital,
    BuildingKind::Kitchen,
    BuildingKind::Warehouse,
    BuildingKind::TailorShop,
    BuildingKind::Wall,
    BuildingKind::Gate,
    BuildingKind::Well,
    BuildingKind::Farmhouse,
    BuildingKind::SnowCrew,
    BuildingKind::Tunnel,
];

#[derive(Resource)]
pub struct GameAssets {
    pub cube: Handle<Mesh>,
    pub cylinder: Handle<Mesh>,
    pub cone: Handle<Mesh>,
    pub capsule: Handle<Mesh>,
    /// Survivor heads — a plain unit sphere, scaled down like every other
    /// shared primitive.
    pub sphere: Handle<Mesh>,
    pub tent: Handle<Mesh>,
    pub ring: Handle<Mesh>,
    /// Shared vertex-color material for the merged terrain meshes.
    pub terrain_mat: Handle<StandardMaterial>,
    pub snow_mat: Handle<StandardMaterial>,
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
    /// One winter-coat material per trade (`Profession::ALL` order, see
    /// [`crate::client::profession_coat_color`]) — shared so survivors of the
    /// same trade batch into one draw call, same trick as `avatar_mats`.
    pub survivor_coat_mats: [Handle<StandardMaterial>; 7],
    /// One hood-tint material per trade, deliberately distinct from the coat
    /// color so a trade still reads even though the skinned model's hood
    /// shape itself no longer varies by profession (that used to be a
    /// separate cone/cylinder/toque prop under the old procedural rig — see
    /// `render::survivors::fixup_survivor_materials`, which applies this to
    /// the model's `fc_hood` primitive).
    pub survivor_head_mats: [Handle<StandardMaterial>; 7],
    /// Coat override for whoever currently holds `GameState.leader` — a
    /// royal color no trade uses, so leading reads as visually distinct
    /// (crown + this) rather than looking stuck in their day job.
    pub leader_coat_mat: Handle<StandardMaterial>,
    /// The appointed leader's crown — spawned as a child directly inside
    /// `spawn_survivor_body` (not a separately tracked/reparented entity),
    /// so it lives and dies with the rest of that survivor's body instead
    /// of risking a stale handle across a leadership hand-off.
    pub leader_crown_mat: Handle<StandardMaterial>,
    /// Shared skin tone for every survivor's head — the headwear above it is
    /// what carries the per-trade distinction.
    /// Four skin tones, picked per survivor from their id
    /// (`render::survivors::survivor_look`). A fixed small set rather than a
    /// per-entity material: a colony is up to 60 people and same-tone heads
    /// still batch into one draw call, exactly like the coats do.
    pub survivor_skin_mats: [Handle<StandardMaterial>; 4],
    /// Scarf colors, chosen the same way. The models author one shared red
    /// scarf; varying it is the cheapest way to stop a trade's workers
    /// reading as identical copies, since the coat and hat are spoken for by
    /// the profession and can't carry individual variation.
    pub survivor_scarf_mats: [Handle<StandardMaterial>; 4],
    /// V0.17: pale/sickly override for `survivor_skin_mat`, swapped onto a
    /// survivor's head while `Survivor::is_sick()`
    /// (`render::survivors::sync_survivors`'s `SurvivorHead` loop) — shared,
    /// so every sick survivor still batches into one draw call, same trick
    /// `leader_coat_mat` uses for leadership.
    pub survivor_skin_sick_mat: Handle<StandardMaterial>,
    /// One body material per `BuildingKind` (see `ALL_KINDS`), shared so
    /// every building of the same kind batches into one draw call.
    pub building_mats: [Handle<StandardMaterial>; 16],
    /// Furnace base/chimney stone — identical for every furnace.
    pub furnace_stone_mat: Handle<StandardMaterial>,
    /// The Tunnel's dark mouth/opening, once unlocked.
    pub tunnel_mouth_mat: Handle<StandardMaterial>,
    pub sawmill_roof_mat: Handle<StandardMaterial>,
    pub sawmill_blade_mat: Handle<StandardMaterial>,
    pub hunter_roof_mat: Handle<StandardMaterial>,
    pub greenhouse_glass_mat: Handle<StandardMaterial>,
    pub hospital_cross_mat: Handle<StandardMaterial>,
    /// Generic stone gray — despite the name (kept from V0.16, when this was
    /// Kitchen-exclusive), V0.22 reuses it for every room's roof chimney
    /// AND every bought `Heater` fitting's stove ring (`render::buildings`'s
    /// `spawn_room`/`spawn_furnishing`) — one shared "stone" look colony-wide.
    pub kitchen_stone_mat: Handle<StandardMaterial>,
    pub warehouse_plank_mat: Handle<StandardMaterial>,
    /// V0.16: muted fur/pelt rug laid beside a Tent — decorative only, no
    /// gameplay meaning (see `render::buildings`'s `BuildingKind::Tent` arm).
    pub tent_pelt_mat: Handle<StandardMaterial>,
    /// Shared "dyed wool" material for the Tailor Shop's cloth prop and the
    /// Tailor survivor's spool prop — echoes `kind_color(TailorShop)`, same
    /// tool-echoes-workplace convention every other trade's material follows.
    pub tailor_cloth_mat: Handle<StandardMaterial>,
    /// Roof worker-indicator cube; identical for every building.
    pub worker_mat: Handle<StandardMaterial>,
    /// V0.8 bino daraja-bayroqlari: [bronza (L2-4), kumush (L5-7),
    /// oltin (L8-10)] — barcha binolar bo'ylab umumiy, batching saqlanadi.
    pub tier_flag_mats: [Handle<StandardMaterial>; 3],
    /// XP-daraja anjomlari: peshona tasmasi (L1) va qalpoq (L2); L3 nishoni
    /// oltin `tier_flag_mats[2]`ni qayta ishlatadi.
    pub gear_band_mat: Handle<StandardMaterial>,
    pub gear_cap_mat: Handle<StandardMaterial>,
    /// V0.11: a fallen survivor's shroud, lying flat until buried or decayed
    /// into a `Grave` — see `render::sync_corpses_and_graves`.
    pub corpse_mat: Handle<StandardMaterial>,
    /// V0.11: the wooden cross marking a `Grave`.
    pub grave_cross_mat: Handle<StandardMaterial>,
    /// V0.22: a room's wood floor — `spawn_room`'s interior. A slightly
    /// different tone from `warehouse_plank_mat` (the wall frame/furniture
    /// timber) so the floor reads as its own surface, not more framing.
    pub room_floor_mat: Handle<StandardMaterial>,
    /// V0.22: the snow-covered pitched roof every room-kind building shows
    /// while NOT selected (`spawn_room`'s `BuildingRoof` group). A few kinds
    /// swap this for a themed handle instead (Greenhouse's glass, HunterHut's
    /// and TailorShop's own roof tones) — see `spawn_room`.
    pub roof_snow_mat: Handle<StandardMaterial>,
    /// V0.22: shared flame material for every bought `Heater` fitting's small
    /// stove (`spawn_furnishing`) — one handle for every room's hearth,
    /// animated in lockstep by `animate_meal_fire` (see `MealFireGlow`'s doc
    /// comment for why that system's name predates this broader use).
    pub heater_fire_mat: Handle<StandardMaterial>,
}

/// One gender's skinned survivor model: the glTF scene plus an animation
/// graph wired with its three clips (`Idle`/`Walk`/`Work` — see
/// `render::survivors::{setup_survivor_animations, drive_survivor_animations}`).
pub struct SurvivorGenderModel {
    pub scene: Handle<WorldAsset>,
    pub graph: Handle<AnimationGraph>,
    pub idle: AnimationNodeIndex,
    pub walk: AnimationNodeIndex,
    pub work: AnimationNodeIndex,
}

/// The two survivor body models (`client::SURVIVOR_MODELS`, embedded glTF —
/// custom-made for this game: skinned mesh + `Idle`/`Walk`/`Work` clips,
/// materials named `fc_coat`/`fc_hood`/`fc_skin`/`fc_leather`/`fc_scarf`).
/// Each survivor spawns its own `WorldAssetRoot` instance of whichever
/// gender `SurvivorRig::gender` picked; both share these two graphs.
#[derive(Resource)]
pub struct SurvivorModels {
    pub male: SurvivorGenderModel,
    pub female: SurvivorGenderModel,
}

/// Loads one gender's scene + builds its `Idle`/`Walk`/`Work` animation
/// graph — the embedded glb's animations are in that fixed order (verified
/// against both source files at authoring time), so plain clip indices 0..2
/// are used rather than a name lookup.
fn load_survivor_model(
    asset_server: &AssetServer,
    graphs: &mut Assets<AnimationGraph>,
    path: &str,
) -> SurvivorGenderModel {
    let embedded = format!("embedded://{path}");
    let clip =
        |i: usize| asset_server.load(GltfAssetLabel::Animation(i).from_asset(embedded.clone()));
    let (graph, nodes) = AnimationGraph::from_clips([clip(0), clip(1), clip(2)]);
    SurvivorGenderModel {
        scene: asset_server.load(GltfAssetLabel::Scene(0).from_asset(embedded)),
        graph: graphs.add(graph),
        idle: nodes[0],
        walk: nodes[1],
        work: nodes[2],
    }
}

/// Shared body material handle for a building kind — keeps same-kind
/// buildings batched into a single draw call instead of each getting its
/// own `StandardMaterial`.
pub(crate) fn building_mat(assets: &GameAssets, kind: BuildingKind) -> Handle<StandardMaterial> {
    let i = ALL_KINDS
        .iter()
        .position(|&k| k == kind)
        .expect("kind is present in ALL_KINDS");
    assets.building_mats[i].clone()
}

// ------------------------------------------------------------------- setup

pub fn setup_camera_and_assets(
    mut commands: Commands,
    quality: Res<Quality>,
    asset_server: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(SurvivorModels {
        male: load_survivor_model(&asset_server, &mut graphs, "models/survivor_male.glb"),
        female: load_survivor_model(&asset_server, &mut graphs, "models/survivor_female.glb"),
    });
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
            // Raised well above Bevy's defaults (0.02 / 1.8). This scene is
            // one big flat plane covered in small objects, which is the worst
            // case for shadow acne: a nearly-parallel surface samples the
            // depth map at grazing incidence and self-shadows in stripes. The
            // sun's own elevation floor (`effects.rs`) does the heavy lifting;
            // this is the second belt, for the remaining low-angle hours.
            shadow_depth_bias: 0.06,
            shadow_normal_bias: 3.0,
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
        sphere: meshes.add(Sphere::new(0.5)),
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
        survivor_coat_mats: Profession::ALL.map(|p| {
            materials.add(StandardMaterial {
                base_color: profession_coat_color(p),
                perceptual_roughness: 0.92,
                ..default()
            })
        }),
        survivor_head_mats: Profession::ALL.map(|p| {
            materials.add(StandardMaterial {
                base_color: profession_head_color(p),
                perceptual_roughness: 0.85,
                ..default()
            })
        }),
        survivor_skin_mats: [
            Color::srgb(0.87, 0.70, 0.57),
            Color::srgb(0.78, 0.60, 0.46),
            Color::srgb(0.62, 0.45, 0.33),
            Color::srgb(0.45, 0.31, 0.23),
        ]
        .map(|c| {
            materials.add(StandardMaterial {
                base_color: c,
                perceptual_roughness: 0.8,
                ..default()
            })
        }),
        survivor_scarf_mats: [
            Color::srgb(0.55, 0.22, 0.20), // the models' own authored red
            Color::srgb(0.22, 0.40, 0.44), // weathered teal
            Color::srgb(0.60, 0.46, 0.18), // mustard
            Color::srgb(0.38, 0.28, 0.42), // dull plum
        ]
        .map(|c| {
            materials.add(StandardMaterial {
                base_color: c,
                perceptual_roughness: 0.95,
                ..default()
            })
        }),
        survivor_skin_sick_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.64, 0.70, 0.58),
            perceptual_roughness: 0.8,
            ..default()
        }),
        leader_coat_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.38, 0.20, 0.50),
            perceptual_roughness: 0.75,
            ..default()
        }),
        leader_crown_mat: materials.add(StandardMaterial {
            // Barely emissive on purpose. The crown is head-sized geometry
            // (~0.12 units) and Bloom::NATURAL smears anything brighter into
            // a glowing ball twice the width of the skull — at this camera
            // distance that read as "leader has no head" rather than as a
            // crown. Metallic gold carries it instead.
            base_color: Color::srgb(1.0, 0.82, 0.20),
            emissive: LinearRgba::rgb(0.05, 0.04, 0.01),
            metallic: 0.6,
            perceptual_roughness: 0.25,
            ..default()
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
        tunnel_mouth_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.04, 0.035, 0.05),
            perceptual_roughness: 1.0,
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
        tent_pelt_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.42, 0.34, 0.28),
            perceptual_roughness: 0.95,
            ..default()
        }),
        tailor_cloth_mat: materials.add(StandardMaterial {
            base_color: kind_color(BuildingKind::TailorShop),
            perceptual_roughness: 0.9,
            ..default()
        }),
        worker_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.95, 0.97, 1.0),
            emissive: LinearRgba::rgb(0.6, 0.65, 0.75),
            ..default()
        }),
        tier_flag_mats: {
            let tiers = [
                Color::srgb(0.72, 0.48, 0.28), // bronza
                Color::srgb(0.80, 0.83, 0.88), // kumush
                Color::srgb(1.00, 0.82, 0.25), // oltin
            ];
            std::array::from_fn(|i| {
                materials.add(StandardMaterial {
                    base_color: tiers[i],
                    emissive: tiers[i].to_linear() * 0.15,
                    metallic: 0.6,
                    perceptual_roughness: 0.4,
                    ..default()
                })
            })
        },
        gear_band_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.92, 0.93, 0.96),
            ..default()
        }),
        gear_cap_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.30, 0.24, 0.18),
            perceptual_roughness: 0.9,
            ..default()
        }),
        corpse_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.72, 0.72, 0.78),
            perceptual_roughness: 0.95,
            ..default()
        }),
        grave_cross_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.36, 0.26, 0.16),
            perceptual_roughness: 0.85,
            ..default()
        }),
        room_floor_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.58, 0.42, 0.26),
            perceptual_roughness: 0.92,
            ..default()
        }),
        roof_snow_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.90, 0.93, 0.98),
            perceptual_roughness: 0.85,
            ..default()
        }),
        heater_fire_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.55, 0.15),
            emissive: LinearRgba::rgb(4.0, 1.4, 0.3),
            ..default()
        }),
    };
    commands.insert_resource(assets);
}

/// A 1x1x1 tent: triangular prism with closed ends, flat-shaded.
///
/// Every face is wound counter-clockwise as seen from OUTSIDE, which is what
/// [`MeshBuf::tri`] assumes: it takes the outward normal to be
/// `(b - a) × (c - a)`, so a reversed winding does not merely light a face
/// wrongly — it points the normal into the tent and back-face culling drops
/// the triangle entirely. All four faces were reversed here, which made the
/// tent body invisible (only its shadow and the crates beside it showed) and,
/// because `GameAssets::tent` is also every room's pitched roof, left every
/// workshop looking like a flat walls-only slab.
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
    // Sloped sides: -x slope faces out to (-1, +1, 0), +x slope to (+1, +1, 0).
    buf.quad(bl, ridge_b, ridge_f, fl, c);
    buf.quad(ridge_f, ridge_b, br, fr, c);
    // Gable ends, facing -z and +z respectively.
    buf.tri(ridge_f, fr, fl, c);
    buf.tri(ridge_b, bl, br, c);
    buf.into_mesh()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every tent face must point away from the inside of the prism. A
    /// reversed winding is invisible rather than merely mis-lit (back-face
    /// culling drops it), and because this one mesh is both the Tent body and
    /// every room's pitched roof, getting it wrong silently deletes a large
    /// share of the colony's geometry — worth pinning down in a test rather
    /// than re-discovering it from a screenshot.
    #[test]
    fn tent_faces_point_outward() {
        let mesh = tent_mesh();
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("tent mesh has no float3 positions")
        };
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(nor)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("tent mesh has no float3 normals")
        };
        assert_eq!(pos.len(), nor.len());
        assert_eq!(pos.len() % 3, 0, "non-indexed triangle list");
        // Comfortably inside the prism (its base spans y = 0..1 at the ridge).
        let inside = Vec3::new(0.0, 0.3, 0.0);
        for (t, tri) in pos.chunks_exact(3).enumerate() {
            let a = Vec3::from_array(tri[0]);
            let b = Vec3::from_array(tri[1]);
            let c = Vec3::from_array(tri[2]);
            let centroid = (a + b + c) / 3.0;
            // Recomputed from the winding, not read back, so this also catches
            // a normal that disagrees with the vertex order.
            let normal = (b - a).cross(c - a).normalize();
            let outward = (centroid - inside).normalize();
            assert!(
                normal.dot(outward) > 0.0,
                "triangle {t} is wound inside-out: normal {normal:?} vs outward {outward:?}",
            );
        }
    }
}
