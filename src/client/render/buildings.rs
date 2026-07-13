use bevy::prelude::*;

use frozen_city::game::types::BuildingKind;

use super::*;
use crate::client::*;

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
