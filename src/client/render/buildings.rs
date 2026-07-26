use bevy::prelude::*;

use frozen_city::game::types::{BuildingKind, CONSTRUCTION_CREW_MAX};

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
        // V0.8: bino ko'rinishi (daraja, qurilish-holati)ga bog'liq — juftlik
        // o'zgargan zahoti eski entity buziladi va yangi shakl quriladi
        // (SpawnGrow bitish/yangilashda kichik "o'sish" animatsiyasini beradi).
        let uc = b.under_construction();
        // Only the Furnace's shape depends on `furnace_level` (its
        // log-teepee grows a tier with it) — comparing it for every other
        // kind would be harmless (always 0 == 0) but gating on `is_furnace`
        // makes that intent explicit.
        let is_furnace = b.kind == BuildingKind::Furnace;
        let furnace_level = if is_furnace { state.furnace_level } else { 0 };
        let is_tunnel = b.kind == BuildingKind::Tunnel;
        let tunnel_stage = if is_tunnel {
            if state.tunnel.unlocked { state.tunnel.stage + 1 } else { 0 }
        } else {
            0
        };
        let fresh = match viz.0.get(&b.id) {
            Some(v) => {
                v.level != b.level
                    || v.under_construction != uc
                    || (is_furnace && v.furnace_level != furnace_level)
                    || (is_tunnel && v.tunnel_stage != tunnel_stage)
            }
            None => true,
        };
        if !fresh {
            continue;
        }
        if let Some(v) = viz.0.remove(&b.id) {
            commands.entity(v.entity).despawn();
        }
        let e = spawn_building(
            &mut commands,
            &assets,
            &mut materials,
            b,
            *quality == Quality::Low,
            furnace_level,
            tunnel_stage,
        );
        viz.0.insert(
            b.id,
            BuildingVizEntry {
                entity: e,
                level: b.level,
                under_construction: uc,
                furnace_level,
                tunnel_stage,
            },
        );
    }

    // Demolished / removed buildings.
    let gone: Vec<u32> = viz
        .0
        .keys()
        .filter(|id| state.find_building(**id).is_none())
        .copied()
        .collect();
    for id in gone {
        if let Some(v) = viz.0.remove(&id) {
            commands.entity(v.entity).despawn();
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
    furnace_level: u8,
    tunnel_stage: u8,
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
    // V0.16: the Kitchen's own always-lit campfire — a distinct handle from
    // `fire_mat` above (created the same "up front, outside `with_children`"
    // way, for the same reason) so its flicker is independent of the
    // player's Furnace on/off state (`animate_meal_fire` in effects.rs, not
    // `animate_effects`).
    let meal_fire_mat = (b.kind == BuildingKind::Kitchen).then(|| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.55, 0.15),
            emissive: LinearRgba::rgb(4.0, 1.4, 0.3),
            ..default()
        })
    });
    // V0.16: the player-chosen orientation turns the whole building around its
    // centre — every child (shape, props, the Kitchen's dining cluster, ...)
    // rides along. `facing` is 0 for the fixtures (Furnace/Tunnel) and any
    // pre-V0.16 save, so those keep their original south-facing pose.
    let root = commands
        .spawn((
            Transform::from_translation(center)
                .with_rotation(Quat::from_rotation_y(b.facing as f32 * std::f32::consts::FRAC_PI_2)),
            Visibility::Inherited,
            BuildingMarker,
            SpawnGrow { age: 0.0 },
            DespawnOnExit(Screen::Game),
        ))
        .id();

    // V0.8: daraja o'sgan sari bino biroz kattalashadi (L10 ≈ +18%) — shakl
    // alohida o'rama-entity ostida, SpawnGrow'ning root-masshtab animatsiyasi
    // bilan to'qnashmaydi.
    let size = 1.0 + 0.02 * b.level.saturating_sub(1) as f32;
    let mut roof_y = 0.6;
    // A V0.9 Furnace level-upgrade (`b.level` 1-10, `BuildingKind::upgradeable`)
    // re-sets `build_left` too, same field the generic scaffold below keys
    // off — but by then it's already lit at least once (`furnace_level > 0`,
    // passed in from `sync_buildings`), so it keeps showing its own
    // campfire/Pech model (sized to the new level) instead of reverting to
    // the bare-scaffold "not built yet" look every other kind gets.
    let show_scaffold = b.under_construction() && !(b.kind == BuildingKind::Furnace && furnace_level > 0);
    commands.entity(root).with_children(|top| {
        top.spawn((Transform::from_scale(Vec3::splat(size)), Visibility::Inherited))
            .with_children(|p| {
        if show_scaffold {
            // Qurilish maydonchasi: yog'och poydevor + 4 burchak ustun +
            // ustki to'sinlar — bino bitmaguncha o'z shakli ko'rinmaydi
            // (Frostpunk'dagi karkas bosqichi uslubida).
            let planks = assets.warehouse_plank_mat.clone();
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(planks.clone()),
                Transform::from_xyz(0.0, 0.05, 0.0).with_scale(Vec3::new(0.9, 0.1, 0.9)),
            ));
            for (dx, dz) in [(-0.38, -0.38), (0.38, -0.38), (-0.38, 0.38), (0.38, 0.38)] {
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(planks.clone()),
                    Transform::from_xyz(dx, 0.38, dz).with_scale(Vec3::new(0.07, 0.66, 0.07)),
                ));
            }
            for dz in [-0.38f32, 0.38] {
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(planks.clone()),
                    Transform::from_xyz(0.0, 0.69, dz).with_scale(Vec3::new(0.84, 0.06, 0.06)),
                ));
            }
            roof_y = 0.82;
        } else {
        match b.kind {
            BuildingKind::Furnace => {
                // Two structural tiers keyed on `b.level` (V0.9, the
                // Furnace's own 1-10 upgrade path — see
                // `BuildingKind::upgradeable`), distinct from
                // `furnace_level` (0-3, the player's burn-intensity setting,
                // which still only sizes the fire/smoke/light below): L1-6
                // is a rough "gulxan" log-teepee that grows gradually; L7-10
                // rebuilds it into an actual stone "Pech" — a solid
                // masonry body + chimney — echoing that only a well-
                // established furnace deserves the name. Both tiers morph
                // continuously with level, not just a single jump at 7.
                let apex_y;
                if b.level <= 6 {
                    let t = b.level.saturating_sub(1) as f32; // 0..=5
                    let base_r = 0.13 + 0.03 * t;
                    apex_y = 0.32 + 0.07 * t;
                    let log_thick = 0.04 + 0.006 * t;
                    let log_count = 5 + t as u32 / 2;
                    let band_count = ((t as u32).saturating_sub(1) / 2).min(2); // 0,0,0,1,1,2

                    p.spawn((
                        Mesh3d(assets.cylinder.clone()),
                        MeshMaterial3d(assets.furnace_stone_mat.clone()),
                        Transform::from_xyz(0.0, 0.02 + 0.004 * t, 0.0)
                            .with_scale(Vec3::new(0.16 + 0.035 * t, 0.03 + 0.004 * t, 0.16 + 0.035 * t)),
                    ));
                    let log_mat = assets.sawmill_roof_mat.clone();
                    for i in 0..log_count {
                        let theta = i as f32 * std::f32::consts::TAU / log_count as f32;
                        let base = Vec3::new(theta.cos() * base_r, 0.05, theta.sin() * base_r);
                        let apex = Vec3::new(0.0, apex_y, 0.0);
                        let dir = apex - base;
                        p.spawn((
                            Mesh3d(assets.cylinder.clone()),
                            MeshMaterial3d(log_mat.clone()),
                            Transform::from_translation((base + apex) / 2.0)
                                .with_rotation(Quat::from_rotation_arc(Vec3::Y, dir.normalize()))
                                .with_scale(Vec3::new(log_thick, dir.length(), log_thick)),
                        ));
                    }
                    for band in 0..band_count {
                        let frac = 0.3 + 0.3 * band as f32;
                        let band_r = base_r * (1.0 - frac) * 1.05;
                        p.spawn((
                            Mesh3d(assets.ring.clone()),
                            MeshMaterial3d(log_mat.clone()),
                            Transform::from_xyz(0.0, apex_y * frac, 0.0)
                                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                                .with_scale(Vec3::splat(band_r)),
                        ));
                    }
                } else {
                    let u = (b.level - 7) as f32; // 0..=3
                    let body_r = 0.34 + 0.05 * u;
                    let body_h = 0.55 + 0.10 * u;
                    let chimney_h = 0.35 + 0.08 * u;
                    apex_y = body_h + chimney_h;
                    let band_count = 1 + u as u32; // 1..=4 reinforcement bands

                    let stone = assets.furnace_stone_mat.clone();
                    p.spawn((
                        Mesh3d(assets.cylinder.clone()),
                        MeshMaterial3d(stone.clone()),
                        Transform::from_xyz(0.0, body_h * 0.5, 0.0)
                            .with_scale(Vec3::new(body_r, body_h, body_r)),
                    ));
                    p.spawn((
                        Mesh3d(assets.cylinder.clone()),
                        MeshMaterial3d(stone.clone()),
                        Transform::from_xyz(0.0, body_h + chimney_h * 0.5, 0.0)
                            .with_scale(Vec3::new(body_r * 0.4, chimney_h, body_r * 0.4)),
                    ));
                    // The mouth: a dark arched opening low on the body, with
                    // the fire glowing through it.
                    p.spawn((
                        Mesh3d(assets.cube.clone()),
                        MeshMaterial3d(assets.tunnel_mouth_mat.clone()),
                        Transform::from_xyz(0.0, body_h * 0.32, body_r * 0.85)
                            .with_scale(Vec3::new(body_r * 0.7, body_h * 0.5, 0.10)),
                    ));
                    // Metal reinforcement bands, more of them at higher
                    // level — an established Pech, not a rough campfire.
                    let band_mat = assets.sawmill_blade_mat.clone();
                    for band in 0..band_count {
                        let y = body_h * (0.25 + 0.6 * band as f32 / band_count.max(1) as f32);
                        p.spawn((
                            Mesh3d(assets.ring.clone()),
                            MeshMaterial3d(band_mat.clone()),
                            Transform::from_xyz(0.0, y, 0.0)
                                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                                .with_scale(Vec3::splat(body_r * 1.02)),
                        ));
                    }
                }
                // Fire core, sized by the player's burn-intensity setting
                // (0-3) — independent of the structural tier above.
                let lvl = furnace_level as f32;
                p.spawn((
                    Mesh3d(assets.cone.clone()),
                    MeshMaterial3d(fire_mat.clone().expect("furnace fire material")),
                    Transform::from_xyz(0.0, 0.10, 0.0)
                        .with_scale(Vec3::splat(0.13 + 0.03 * lvl)),
                ));
                // Per-fragment point lighting is costly on mobile WebGL2, so
                // phones skip it — the emissive fire still reads as a glow.
                // These starting values are overwritten every frame by
                // `animate_effects`; kept in sync with it just so the very
                // first rendered frame (before that system first runs)
                // doesn't flash brighter than the steady-state look.
                if !low {
                    p.spawn((
                        PointLight {
                            color: Color::srgb(1.0, 0.62, 0.25),
                            intensity: 150_000.0,
                            range: 8.0,
                            ..default()
                        },
                        Transform::from_xyz(0.0, apex_y + 0.4, 0.0),
                        FurnaceLight,
                    ));
                }
                // Smoke: looping puffs rising from the structure's peak
                // (teepee apex or Pech chimney), hidden when the fire is out.
                for i in 0..10u32 {
                    p.spawn((
                        Mesh3d(assets.cube.clone()),
                        MeshMaterial3d(assets.smoke_mat.clone()),
                        Transform::from_xyz(0.0, apex_y + 0.2, 0.0).with_scale(Vec3::splat(0.1)),
                        Visibility::Hidden,
                        Smoke {
                            phase: i as f32 / 10.0,
                        },
                    ));
                }
                roof_y = apex_y + 0.3;
            }
            BuildingKind::Tent => {
                let body = building_mat(assets, b.kind);
                p.spawn((
                    Mesh3d(assets.tent.clone()),
                    MeshMaterial3d(body.clone()),
                    Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::new(0.85, 0.6, 0.85)),
                ));
                // V0.16: a small stacked-crate pair (same idiom as the
                // Warehouse's, below) and a flat fur pelt beside the tent —
                // purely decorative "cozy home with storage" dressing.
                // Tent's mesh is a closed solid prism with no interior, so
                // nothing can go inside it — stays outside, same convention
                // `sim::gather_points`'s offset already establishes for why
                // survivors themselves wait just past the door rather than
                // on the tile center.
                let planks = assets.warehouse_plank_mat.clone();
                for (dx, dz, h) in [(0.34, -0.30, 0.14), (0.48, -0.28, 0.18)] {
                    p.spawn((
                        Mesh3d(assets.cube.clone()),
                        MeshMaterial3d(planks.clone()),
                        Transform::from_xyz(dx, h * 0.5, dz).with_scale(Vec3::splat(h)),
                    ));
                }
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(assets.tent_pelt_mat.clone()),
                    Transform::from_xyz(-0.40, 0.015, 0.20).with_scale(Vec3::new(0.34, 0.03, 0.48)),
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
                    MeshMaterial3d(stone.clone()),
                    Transform::from_xyz(0.22, 0.58, 0.0).with_scale(Vec3::new(0.16, 0.5, 0.16)),
                ));
                roof_y = 0.9;

                // V0.16: dining/campfire cluster, at the same "just outside
                // the door" spot `gather_points` parks meal-time survivors
                // at (`by + 1.15` in tile-fractional world coords — here,
                // relative to this 1x1 building's own centered local space,
                // that's local Z ≈ +0.65; see `sim::gather_points`'s doc
                // comment for why not the tile center). Makes the V0.15
                // daily routine actually visible instead of people just
                // standing at a bare point.
                let planks = assets.warehouse_plank_mat.clone();
                let meal_fire_mat = meal_fire_mat.clone().expect("kitchen meal-fire material");
                let low_quality = low;
                // The whole cluster hangs off a wrapper that cancels the
                // root's `facing` rotation: `sim::gather_points` (and so the
                // seat offsets in `render::survivors`) always parks diners at
                // the building's SOUTH side, whatever way the player turned
                // the Kitchen. Without this counter-rotation the stools would
                // swing around the building while the survivors kept sitting
                // in mid-air to the south.
                p.spawn((
                    Transform::from_rotation(Quat::from_rotation_y(
                        -(b.facing as f32) * std::f32::consts::FRAC_PI_2,
                    )),
                    Visibility::Inherited,
                ))
                .with_children(|c| {
                    c.spawn((
                        Mesh3d(assets.cube.clone()),
                        MeshMaterial3d(planks.clone()),
                        Transform::from_xyz(0.0, 0.16, 0.75).with_scale(Vec3::new(0.52, 0.05, 0.30)),
                    ));
                    for (dx, dz) in [(-0.26, 0.62), (0.26, 0.62), (0.0, 0.94)] {
                        c.spawn((
                            Mesh3d(assets.cylinder.clone()),
                            MeshMaterial3d(planks.clone()),
                            Transform::from_xyz(dx, 0.09, dz).with_scale(Vec3::new(0.10, 0.18, 0.10)),
                        ));
                    }
                    // A low stone ring holding a small flame — always lit (the
                    // Kitchen is always "cooking"), independent of the player's
                    // Furnace on/off state; see `meal_fire_mat` above and
                    // `animate_meal_fire` in effects.rs.
                    c.spawn((
                        Mesh3d(assets.ring.clone()),
                        MeshMaterial3d(stone),
                        Transform::from_xyz(0.0, 0.01, 0.42)
                            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                            .with_scale(Vec3::splat(0.14)),
                    ));
                    c.spawn((
                        Mesh3d(assets.cone.clone()),
                        MeshMaterial3d(meal_fire_mat.clone()),
                        Transform::from_xyz(0.0, 0.07, 0.42).with_scale(Vec3::new(0.10, 0.14, 0.10)),
                        MealFireGlow { fire_mat: meal_fire_mat.clone() },
                    ));
                    if !low_quality {
                        c.spawn((
                            PointLight {
                                color: Color::srgb(1.0, 0.6, 0.25),
                                intensity: 60_000.0,
                                range: 2.5,
                                ..default()
                            },
                            Transform::from_xyz(0.0, 0.25, 0.42),
                            MealFireLight,
                        ));
                    }
                });
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
            BuildingKind::TailorShop => {
                let body = building_mat(assets, b.kind);
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(body),
                    Transform::from_xyz(0.0, 0.24, 0.0).with_scale(Vec3::new(0.78, 0.48, 0.78)),
                ));
                // A hung bolt of cloth on the front face — the "tailor" tell
                // at a glance, reusing the same material the Tailor
                // survivor's spool prop uses.
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(assets.tailor_cloth_mat.clone()),
                    Transform::from_xyz(0.0, 0.30, 0.40).with_scale(Vec3::new(0.30, 0.36, 0.04)),
                ));
                roof_y = 0.5;
            }
            BuildingKind::Wall => {
                // A single thin, wide slab spanning the tile — purely
                // decorative (this game has no collision/threat mechanic to
                // actually block), just a boundary marker.
                let body = building_mat(assets, b.kind);
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(body),
                    Transform::from_xyz(0.0, 0.22, 0.0).with_scale(Vec3::new(0.94, 0.44, 0.14)),
                ));
                roof_y = 0.0; // flat-topped, no roof/worker-cube perch
            }
            BuildingKind::Gate => {
                // Two posts with a lintel bar — an "opening" silhouette,
                // distinct from Wall's solid slab, same decorative role.
                let body = building_mat(assets, b.kind);
                for dx in [-0.34f32, 0.34] {
                    p.spawn((
                        Mesh3d(assets.cube.clone()),
                        MeshMaterial3d(body.clone()),
                        Transform::from_xyz(dx, 0.24, 0.0).with_scale(Vec3::new(0.16, 0.48, 0.16)),
                    ));
                }
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(body),
                    Transform::from_xyz(0.0, 0.52, 0.0).with_scale(Vec3::new(0.94, 0.10, 0.16)),
                ));
                roof_y = 0.0;
            }
            BuildingKind::Well => {
                // A low stone rim (kind_color's water-blue) around a darker
                // water-surface disc, with two posts and a crossbeam roof —
                // the classic well silhouette.
                let rim = building_mat(assets, b.kind);
                p.spawn((
                    Mesh3d(assets.cylinder.clone()),
                    MeshMaterial3d(rim),
                    Transform::from_xyz(0.0, 0.14, 0.0).with_scale(Vec3::new(0.46, 0.28, 0.46)),
                ));
                p.spawn((
                    Mesh3d(assets.cylinder.clone()),
                    MeshMaterial3d(assets.tunnel_mouth_mat.clone()),
                    Transform::from_xyz(0.0, 0.29, 0.0).with_scale(Vec3::new(0.36, 0.02, 0.36)),
                ));
                let posts = assets.warehouse_plank_mat.clone();
                for dx in [-0.36f32, 0.36] {
                    p.spawn((
                        Mesh3d(assets.cylinder.clone()),
                        MeshMaterial3d(posts.clone()),
                        Transform::from_xyz(dx, 0.5, 0.0).with_scale(Vec3::new(0.05, 0.44, 0.05)),
                    ));
                }
                p.spawn((
                    Mesh3d(assets.cylinder.clone()),
                    MeshMaterial3d(posts),
                    Transform::from_xyz(0.0, 0.74, 0.0)
                        .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
                        .with_scale(Vec3::new(0.04, 0.82, 0.04)),
                ));
                roof_y = 0.74;
            }
            BuildingKind::Farmhouse => {
                // A barn: body in `kind_color`'s barn-red, plank-toned
                // gabled roof (reusing the `tent` mesh the same way
                // Greenhouse reuses it for its glass roof) — the classic
                // farm silhouette, distinct from Greenhouse's flatter glass
                // canopy.
                let body = building_mat(assets, b.kind);
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(body),
                    Transform::from_xyz(0.0, 0.22, 0.0).with_scale(Vec3::new(0.8, 0.44, 0.8)),
                ));
                p.spawn((
                    Mesh3d(assets.tent.clone()),
                    MeshMaterial3d(assets.warehouse_plank_mat.clone()),
                    Transform::from_xyz(0.0, 0.44, 0.0).with_scale(Vec3::new(0.92, 0.34, 0.92)),
                ));
                roof_y = 0.78;
            }
            BuildingKind::Tunnel => {
                if tunnel_stage == 0 {
                    // Sealed: an unremarkable snow-dusted rock mound —
                    // nothing yet hints at what's underneath.
                    let stone = assets.furnace_stone_mat.clone();
                    p.spawn((
                        Mesh3d(assets.sphere.clone()),
                        MeshMaterial3d(stone.clone()),
                        Transform::from_xyz(0.0, 0.16, 0.0).with_scale(Vec3::new(0.95, 0.34, 0.80)),
                    ));
                    for (dx, dz, s) in [(-0.30, 0.18, 0.22), (0.32, -0.12, 0.18), (0.05, -0.30, 0.16)] {
                        p.spawn((
                            Mesh3d(assets.sphere.clone()),
                            MeshMaterial3d(stone.clone()),
                            Transform::from_xyz(dx, s * 0.4, dz).with_scale(Vec3::splat(s)),
                        ));
                    }
                    roof_y = 0.4;
                } else {
                    // Unlocked: a stone archway framing a dark mouth,
                    // widening a little each excavation stage (`tunnel_stage`
                    // is 1..=4, i.e. `TunnelState.stage` 0..=3 shifted by one
                    // so 0 stays reserved for "not yet unlocked").
                    let stage = (tunnel_stage - 1) as f32;
                    let half_w = 0.30 + 0.06 * stage;
                    let h = 0.62 + 0.10 * stage;
                    let stone = assets.furnace_stone_mat.clone();
                    for dx in [-1.0f32, 1.0] {
                        p.spawn((
                            Mesh3d(assets.cube.clone()),
                            MeshMaterial3d(stone.clone()),
                            Transform::from_xyz(dx * (half_w + 0.08), h * 0.5, 0.0)
                                .with_scale(Vec3::new(0.16, h, 0.18)),
                        ));
                    }
                    p.spawn((
                        Mesh3d(assets.cube.clone()),
                        MeshMaterial3d(stone),
                        Transform::from_xyz(0.0, h + 0.08, 0.0)
                            .with_scale(Vec3::new(2.0 * half_w + 0.32, 0.16, 0.20)),
                    ));
                    p.spawn((
                        Mesh3d(assets.cube.clone()),
                        MeshMaterial3d(assets.tunnel_mouth_mat.clone()),
                        Transform::from_xyz(0.0, h * 0.5, 0.02).with_scale(Vec3::new(half_w * 2.0, h, 0.05)),
                    ));
                    roof_y = h + 0.18;
                }
            }
        }

        // A small window that glows warm at night (shared animated material) —
        // skipped for the thin, non-housing Wall/Gate slabs, same as the
        // fixture-only Furnace/Tunnel.
        if !matches!(
            b.kind,
            BuildingKind::Furnace | BuildingKind::Tunnel | BuildingKind::Wall | BuildingKind::Gate
        ) {
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.window_mat.clone()),
                Transform::from_xyz(0.0, 0.22, 0.40).with_scale(Vec3::new(0.22, 0.16, 0.05)),
            ));
        }

        // V0.8 daraja bezaklari: poydevor atrofidagi metall tasma + burchak
        // bayrog'i (L2-4 bronza, L5-7 kumush, L8-10 oltin) — daraja uzoqdan
        // ham o'qiladi.
        if b.kind.buildable() && b.level >= 2 {
            let tier = (((b.level - 2) / 3) as usize).min(2);
            let mat = assets.tier_flag_mats[tier].clone();
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(mat.clone()),
                Transform::from_xyz(0.0, 0.035, 0.0).with_scale(Vec3::new(0.88, 0.07, 0.88)),
            ));
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(assets.warehouse_plank_mat.clone()),
                Transform::from_xyz(0.40, 0.45, -0.40).with_scale(Vec3::new(0.05, 0.9, 0.05)),
            ));
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(mat),
                Transform::from_xyz(0.31, 0.84, -0.40).with_scale(Vec3::new(0.16, 0.10, 0.03)),
            ));
        }
        } // qurilish maydonchasi / bitgan bino tarmoqlari

        // Worker cubes above the roof — qurilishda usta-brigada o'rinlari,
        // bitganda binoning o'z kasb o'rinlari.
        let max = if b.under_construction() {
            CONSTRUCTION_CREW_MAX
        } else {
            b.kind.max_workers()
        };
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
    });

    if let Some(fire_mat) = fire_mat {
        commands.entity(root).insert(FurnaceGlow { fire_mat });
    }
    root
}
