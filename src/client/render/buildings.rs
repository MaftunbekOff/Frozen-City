use bevy::prelude::*;

use frozen_city::game::types::{BuildingKind, FurnishingKind, CONSTRUCTION_CREW_MAX};

use super::*;
use crate::client::*;

// ---------------------------------------------------------------- buildings

// V0.22: `sync_buildings` already sat near Bevy's ~16-`SystemParam` cap
// before this pass (see `teardown_game`'s doc comment in `mod.rs` for the
// same limit hitting a different system) — six more `Query`s for the roof
// toggle and the scaffold bars would blow through it. Bundled into one
// `ParamSet` instead (same trick `ui::selection::selection_panel_update`
// uses): a `ParamSet` counts as a single `SystemParam`, and Rust's own
// borrow checker enforces the "only one sub-query active at a time" rule a
// `Without<>` filter would otherwise have to prove statically — which also
// means none of these six need mutual `Without<>`s against each other.
#[allow(clippy::type_complexity)]
pub fn sync_buildings(
    mut commands: Commands,
    view: Res<GameView>,
    assets: Res<GameAssets>,
    quality: Res<Quality>,
    selection: Res<Selection>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut viz: ResMut<BuildingViz>,
    mut scaffolds: Local<ScaffoldViz>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut queries: ParamSet<(
        Query<(&WorkerCube, &mut Visibility)>,
        Query<(&BuildingRoof, &mut Visibility)>,
        Query<(&BuildingInterior, &mut Visibility)>,
        Query<(&ScaffoldBar, &mut Node)>,
        Query<&mut Node, With<ScaffoldFill>>,
        Query<&mut Text, With<ScaffoldCountdown>>,
    )>,
    mut seen: Local<u64>,
) {
    let Some(state) = view.ready() else { return };

    if *seen != view.version {
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

        // Floating construction-status bars: reconciled from scratch against
        // the CURRENT snapshot rather than patched incrementally (see
        // `ScaffoldViz`'s doc comment for why — building ids aren't unique
        // across worlds, and this Local outlives a world switch).
        let wanted: std::collections::HashSet<u32> = state
            .buildings
            .iter()
            .filter(|b| b.under_construction())
            .map(|b| b.id)
            .collect();
        let stale: Vec<u32> = scaffolds.0.keys().filter(|id| !wanted.contains(id)).copied().collect();
        for id in stale {
            if let Some(e) = scaffolds.0.remove(&id) {
                commands.entity(e).despawn();
            }
        }
        for &id in &wanted {
            scaffolds.0.entry(id).or_insert_with(|| spawn_scaffold_bar(&mut commands));
        }

        // Worker indicators: one small cube per assigned worker.
        for (cube, mut vis) in &mut queries.p0() {
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

    // Everything below runs EVERY frame, gated snapshot or not: the camera
    // and `Selection` both move independently of when a new snapshot
    // arrives, so tying them to `seen` would make the roof toggle and the
    // scaffold bars visibly lag behind the mouse instead of tracking it.

    // Roof vs. interior: the core "select a building to see inside it"
    // toggle. `Visibility`, never despawn/respawn — this flips constantly
    // as the player clicks around (see `BuildingRoof`/`BuildingInterior`).
    for (roof, mut vis) in &mut queries.p1() {
        let want = if selection.0 == Some(roof.building) {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *vis != want {
            *vis = want;
        }
    }
    for (interior, mut vis) in &mut queries.p2() {
        let want = if selection.0 == Some(interior.building) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != want {
            *vis = want;
        }
    }

    // Scaffold bars: re-project onto their building every frame — the same
    // `world_to_viewport` trick `ui::placement::sync_placement_controls`
    // uses for the placement confirm bar — and refresh progress/countdown.
    if !scaffolds.0.is_empty() {
        if let (Ok((cam, cam_gt)), Ok(window)) = (camera.single(), windows.single()) {
            for (&id, &root) in scaffolds.0.iter() {
                let Some(b) = state.find_building(id) else { continue };
                // Scoped to end `p3()`'s borrow of `queries` before `p4()`/
                // `p5()` are taken below — a `ParamSet` only ever lets one
                // sub-query be active at a time (see the type alias's doc
                // comment), so `fill`/`countdown` are copied out as plain
                // `Entity`s rather than kept as references into this query.
                let child = {
                    let mut bar_roots = queries.p3();
                    let Ok((bar, mut node)) = bar_roots.get_mut(root) else { continue };
                    let anchor = building_center_world(b) + Vec3::Y * SCAFFOLD_BAR_Y;
                    match cam.world_to_viewport(cam_gt, anchor) {
                        Ok(screen) => {
                            let max_left = (window.width() - SCAFFOLD_BAR_W).max(0.0);
                            let max_top = (window.height() - SCAFFOLD_BAR_H * 4.0).max(0.0);
                            node.display = Display::Flex;
                            node.left = Val::Px((screen.x - SCAFFOLD_BAR_W / 2.0).clamp(0.0, max_left));
                            node.top = Val::Px((screen.y - SCAFFOLD_BAR_H * 3.0).clamp(0.0, max_top));
                        }
                        Err(_) => node.display = Display::None,
                    }
                    (bar.fill, bar.countdown)
                };
                let (fill_e, countdown_e) = child;
                if let Ok(mut fill) = queries.p4().get_mut(fill_e) {
                    fill.width = Val::Percent((b.build_progress() * 100.0).clamp(0.0, 100.0));
                }
                if let Ok(mut text) = queries.p5().get_mut(countdown_e) {
                    text.0 = match b.build_eta_secs() {
                        Some(secs) => format!("\u{2692} {}", format_countdown(secs)),
                        // Nobody working the site: show the bar without a
                        // countdown rather than a time that will never arrive
                        // (`Building::build_eta_secs`'s own doc comment).
                        None => String::new(),
                    };
                }
            }
        }
    }
}

/// Width of the floating construction status bar (kept in sync with the
/// track width passed to `theme::stat_bar_track` in `spawn_scaffold_bar`).
const SCAFFOLD_BAR_W: f32 = 84.0;
const SCAFFOLD_BAR_H: f32 = 8.0;
/// World-space height above the building's centre the status bar anchors
/// to — clear of even a fully-grown 3x3 scaffold's corner posts.
const SCAFFOLD_BAR_Y: f32 = 1.5;

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
    // V0.16: the player-chosen orientation turns the whole building around its
    // centre — every child (shape, props, a room's fittings, ...) rides
    // along. `facing` is 0 for the fixtures (Furnace/Tunnel) and any
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
        // V0.22: this kind's footprint, in tiles — 3x3 for every room kind
        // (`BuildingKind::size`'s doc comment), 2x2 for the Furnace/Tunnel
        // fixtures and the Tent, 1x1 for Wall/Gate. Used both by the
        // scaffold below and by the level-decoration flag further down, so
        // every buildable kind scales correctly regardless of footprint.
        let (bw, bh) = b.kind.size();
        let (w, h) = (bw as f32, bh as f32);
        let (hw, hh) = (w / 2.0, h / 2.0);
        if show_scaffold {
            // Qurilish maydonchasi: yog'och poydevor + 4 burchak ustun +
            // ustki to'sinlar — bino bitmaguncha o'z shakli ko'rinmaydi
            // (Frostpunk'dagi karkas bosqichi uslubida). V0.22: sized from
            // `(w, h)` above rather than a hardcoded 1x1 — a 3x3 room's
            // scaffold now genuinely spans its plot instead of sitting
            // small in one corner of it.
            let post_h = 0.66;
            let planks = assets.warehouse_plank_mat.clone();
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(planks.clone()),
                Transform::from_xyz(0.0, 0.05, 0.0).with_scale(Vec3::new(w - 0.1, 0.1, h - 0.1)),
            ));
            for (dx, dz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(planks.clone()),
                    Transform::from_xyz(dx * (hw - 0.09), post_h * 0.58, dz * (hh - 0.09))
                        .with_scale(Vec3::new(0.07, post_h, 0.07)),
                ));
            }
            // Top rail connecting every corner post into a full rectangle —
            // the old version only ran two beams along one axis, fine for a
            // 1x1 shed but far too sparse for a 3x3 room's much wider frame.
            for dz in [-hh + 0.09, hh - 0.09] {
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(planks.clone()),
                    Transform::from_xyz(0.0, post_h, dz).with_scale(Vec3::new(w - 0.14, 0.06, 0.06)),
                ));
            }
            for dx in [-hw + 0.09, hw - 0.09] {
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(planks.clone()),
                    Transform::from_xyz(dx, post_h, 0.0).with_scale(Vec3::new(0.06, 0.06, h - 0.14)),
                ));
            }
            roof_y = post_h + 0.16;
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
                // V0.22: Tent's footprint grew from 1x1 to 2x2
                // (`BuildingKind::size`) so a colony can fit two bunks per
                // shelter — but it's still not a room (no fittings, see
                // `furnishings()`), so it keeps its old single-solid-mesh
                // look, just scaled up to actually fill the bigger plot
                // instead of sitting small in one corner of it.
                let body = building_mat(assets, b.kind);
                p.spawn((
                    Mesh3d(assets.tent.clone()),
                    MeshMaterial3d(body.clone()),
                    Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::new(1.55, 0.68, 1.55)),
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
                for (dx, dz, hgt) in [(0.62, -0.54, 0.16), (0.86, -0.50, 0.20)] {
                    p.spawn((
                        Mesh3d(assets.cube.clone()),
                        MeshMaterial3d(planks.clone()),
                        Transform::from_xyz(dx, hgt * 0.5, dz).with_scale(Vec3::splat(hgt)),
                    ));
                }
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(assets.tent_pelt_mat.clone()),
                    Transform::from_xyz(-0.68, 0.015, 0.36).with_scale(Vec3::new(0.56, 0.03, 0.80)),
                ));
                roof_y = 0.82;
            }
            BuildingKind::Sawmill
            | BuildingKind::CoalMine
            | BuildingKind::HunterHut
            | BuildingKind::Greenhouse
            | BuildingKind::Hospital
            | BuildingKind::Kitchen
            | BuildingKind::Warehouse
            | BuildingKind::TailorShop
            | BuildingKind::Well
            | BuildingKind::Farmhouse
            | BuildingKind::SnowCrew => {
                // V0.22: every kind with an interior (`kind.furnishings()`
                // non-empty — see `BuildingKind::size`'s doc for the full
                // list and why they're all `ROOM_SIZE` square) shares one
                // room shell: a closed roofed look while unselected, an open
                // furnished one while selected. See `spawn_room`.
                roof_y = spawn_room(p, assets, b, low);
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
        } // qurilish maydonchasi / bitgan bino tarmoqlari

        // A small window that glows warm at night (shared animated material) —
        // only the Tent still needs one spawned unconditionally here: every
        // room kind now gets its own (plural, on the roofed exterior) inside
        // `spawn_room`, and the fixture-only Furnace/Tunnel and the thin
        // Wall/Gate slabs never had one.
        if b.kind == BuildingKind::Tent {
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.window_mat.clone()),
                Transform::from_xyz(0.0, 0.22, 0.68).with_scale(Vec3::new(0.22, 0.16, 0.05)),
            ));
        }

        // V0.8 daraja bezaklari: poydevor atrofidagi metall tasma + burchak
        // bayrog'i (L2-4 bronza, L5-7 kumush, L8-10 oltin) — daraja uzoqdan
        // ham o'qiladi. V0.22: sized/anchored from `(w, h)`/`(hw, hh)` above
        // instead of a hardcoded 1x1, so a 3x3 room's flag sits at its own
        // back corner rather than floating in the middle of the plot.
        if b.kind.buildable() && b.level >= 2 {
            let tier = (((b.level - 2) / 3) as usize).min(2);
            let mat = assets.tier_flag_mats[tier].clone();
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(mat.clone()),
                Transform::from_xyz(0.0, 0.035, 0.0).with_scale(Vec3::new(w - 0.12, 0.07, h - 0.12)),
            ));
            let pole_x = hw * 0.8;
            let pole_z = -hh * 0.8;
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(assets.warehouse_plank_mat.clone()),
                Transform::from_xyz(pole_x, 0.45, pole_z).with_scale(Vec3::new(0.05, 0.9, 0.05)),
            ));
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(mat),
                Transform::from_xyz(pole_x - 0.09, 0.84, pole_z).with_scale(Vec3::new(0.16, 0.10, 0.03)),
            ));
        }

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

/// V0.22: shared room shell for every building with an interior
/// (`kind.furnishings()` non-empty). Builds two sibling groups, each
/// toggled every frame by `sync_buildings`'s selection check:
///   - `BuildingRoof`: the closed look — walls in the kind's own color, a
///     snow-covered pitched roof, a chimney, lit windows. Shown while this
///     building is NOT selected.
///   - `BuildingInterior`: the open look — a wood floor, a low timber wall
///     frame with corner posts (a doorway gap on the south/`facing = 0`
///     edge, matching every other south-facing convention in this file —
///     the ghost's front arrow, `sim::gather_points`), and every fitting
///     the player has actually bought, each standing at its
///     `Building::station` — the exact point the sim walks a worker to
///     (`Building::worker_station`), so the model and the sim's walk target
///     can never visually disagree.
///
/// Returns the roof's peak height, for `spawn_building`'s worker-cube perch
/// (same convention every other kind's arm already follows).
fn spawn_room(p: &mut ChildSpawnerCommands, assets: &GameAssets, b: &frozen_city::game::types::Building, low: bool) -> f32 {
    let (bw, bh) = b.kind.size();
    let (w, h) = (bw as f32, bh as f32);
    let (hw, hh) = (w / 2.0, h / 2.0);
    // Height scales with the footprint. These were fixed at 0.42/0.30 back
    // when a workplace was a 1x1 shed; V0.22 widened every room kind to
    // `ROOM_SIZE` (3) tiles without touching them, which left a 2.9-wide
    // building 0.72 tall — a 4:1 slab barely taller than the 0.62 survivor
    // standing next to it, reading as "the walls didn't render". Tying both
    // to the shortest side keeps any future footprint change proportional.
    let span = w.min(h);
    let body_h = 0.30 * span;
    let roof_h = 0.19 * span;

    // --- closed exterior: solid walls + pitched roof + chimney + windows ---
    p.spawn((Transform::IDENTITY, Visibility::Inherited, BuildingRoof { building: b.id }))
        .with_children(|r| {
            let walls = building_mat(assets, b.kind);
            r.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(walls),
                Transform::from_xyz(0.0, body_h * 0.5, 0.0).with_scale(Vec3::new(w - 0.10, body_h, h - 0.10)),
            ));
            // A few kinds keep their old dedicated roof material instead of
            // the generic snow cap — cheap per-kind character to carry
            // forward now that their old bespoke body shapes are gone.
            let roof_mat = match b.kind {
                BuildingKind::Greenhouse => assets.greenhouse_glass_mat.clone(),
                BuildingKind::HunterHut => assets.hunter_roof_mat.clone(),
                BuildingKind::TailorShop => assets.tailor_cloth_mat.clone(),
                _ => assets.roof_snow_mat.clone(),
            };
            r.spawn((
                Mesh3d(assets.tent.clone()),
                MeshMaterial3d(roof_mat),
                Transform::from_xyz(0.0, body_h, 0.0).with_scale(Vec3::new(w + 0.06, roof_h, h + 0.06)),
            ));
            // Hospital keeps its roof-ridge cross — the one silhouette this
            // genre always reaches for, cheap to carry forward.
            if b.kind == BuildingKind::Hospital {
                let cross = assets.hospital_cross_mat.clone();
                r.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(cross.clone()),
                    Transform::from_xyz(0.0, body_h + roof_h + 0.10, 0.0).with_scale(Vec3::new(0.26, 0.07, 0.09)),
                ));
                r.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(cross),
                    Transform::from_xyz(0.0, body_h + roof_h + 0.10, 0.0).with_scale(Vec3::new(0.09, 0.07, 0.26)),
                ));
            }
            r.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(assets.kitchen_stone_mat.clone()),
                Transform::from_xyz(hw * 0.55, body_h + roof_h * 0.75, hh * 0.55)
                    .with_scale(Vec3::new(0.13, roof_h * 1.7, 0.13)),
            ));
            let window = assets.window_mat.clone();
            for dx in [-hw * 0.4, hw * 0.4] {
                r.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(window.clone()),
                    Transform::from_xyz(dx, body_h * 0.55, hh - 0.06).with_scale(Vec3::new(0.22, 0.16, 0.05)),
                ));
            }
        });

    // --- open interior: floor + wall frame + corner posts + fittings ---
    p.spawn((Transform::IDENTITY, Visibility::Hidden, BuildingInterior { building: b.id }))
        .with_children(|r| {
            r.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.room_floor_mat.clone()),
                Transform::from_xyz(0.0, 0.02, 0.0).with_scale(Vec3::new(w - 0.08, 0.04, h - 0.08)),
            ));
            let frame = assets.warehouse_plank_mat.clone();
            // Deliberately far lower than the exterior's `body_h`: this is the
            // opened-up "look inside" view, so the frame only has to read as
            // walls without hiding the fittings. Scaled off the same `span`
            // so it stays in proportion with them.
            let wall_h = 0.14 * span;
            // North wall + both side walls: solid, full length. The doorway
            // gap is on the south edge only (below).
            r.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(frame.clone()),
                Transform::from_xyz(0.0, wall_h * 0.5, -hh + 0.03).with_scale(Vec3::new(w - 0.06, wall_h, 0.06)),
            ));
            for dx in [-1.0f32, 1.0] {
                r.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(frame.clone()),
                    Transform::from_xyz(dx * (hw - 0.03), wall_h * 0.5, 0.0)
                        .with_scale(Vec3::new(0.06, wall_h, h - 0.06)),
                ));
            }
            // South wall, split around a door-width gap.
            let door = 1.0f32.min(w - 0.6);
            let seg = (w - door) / 2.0 - 0.03;
            if seg > 0.05 {
                for side in [-1.0f32, 1.0] {
                    let cx = side * (door / 2.0 + seg / 2.0);
                    r.spawn((
                        Mesh3d(assets.cube.clone()),
                        MeshMaterial3d(frame.clone()),
                        Transform::from_xyz(cx, wall_h * 0.5, hh - 0.03).with_scale(Vec3::new(seg, wall_h, 0.06)),
                    ));
                }
            }
            // Corner posts, taller than the frame — the detail that reads
            // as "timber room" at a glance even from an isometric angle.
            let post_h = 0.22 * span;
            for (dx, dz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                r.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(frame.clone()),
                    Transform::from_xyz(dx * (hw - 0.05), post_h * 0.5, dz * (hh - 0.05))
                        .with_scale(Vec3::new(0.09, post_h, 0.09)),
                ));
            }

            // Fittings, each at the sim's own `Building::station(slot)` — the
            // same absolute tile point the worker assigned there walks to
            // (`Building::worker_station`). Wrapped in a counter-rotation so
            // the player's chosen `facing` (which spins the whole root)
            // doesn't ALSO spin these: `station` is computed in absolute
            // tile space and knows nothing about `facing` (same trick the
            // old V0.16 Kitchen dining cluster used for the same reason).
            let center = (b.x as f32 + hw, b.y as f32 + hh);
            r.spawn((
                Transform::from_rotation(Quat::from_rotation_y(-(b.facing as f32) * std::f32::consts::FRAC_PI_2)),
                Visibility::Inherited,
            ))
            .with_children(|f| {
                for (slot, kind) in b.kind.furnishings().iter().enumerate() {
                    let level = b.furnishing_level(slot);
                    if level == 0 {
                        continue;
                    }
                    let Some((sx, sy)) = b.station(slot) else { continue };
                    let at = Vec3::new(sx - center.0, 0.0, sy - center.1);
                    spawn_furnishing(f, assets, *kind, level, at, low);
                }
            });
        });

    body_h + roof_h + 0.55
}

/// V0.22: one bought fitting's model, standing at `at` (already converted
/// to local room-space by `spawn_room`'s caller). Four shapes, one per
/// `FurnishingKind`, deliberately distinct silhouettes — a stove is never
/// mistaken for a bookshelf. Grows a little with `level`
/// (1..=`FURNISHING_MAX_LEVEL`): a shared scale factor plus the same
/// bronze/silver/gold tint `spawn_building`'s level flag uses, cheap rather
/// than a different mesh per level.
fn spawn_furnishing(p: &mut ChildSpawnerCommands, assets: &GameAssets, kind: FurnishingKind, level: u8, at: Vec3, low: bool) {
    let grow = 1.0 + 0.08 * level.saturating_sub(1) as f32;
    p.spawn((
        Transform::from_translation(at).with_scale(Vec3::splat(grow)),
        Visibility::Inherited,
    ))
    .with_children(|f| match kind {
        FurnishingKind::Workbench => {
            // A low bench with tools laid on top — the "someone works here" tell.
            let wood = assets.warehouse_plank_mat.clone();
            f.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(wood.clone()),
                Transform::from_xyz(0.0, 0.22, 0.0).with_scale(Vec3::new(0.46, 0.05, 0.30)),
            ));
            for (dx, dz) in [(-0.18, -0.12), (0.18, -0.12), (-0.18, 0.12), (0.18, 0.12)] {
                f.spawn((
                    Mesh3d(assets.cylinder.clone()),
                    MeshMaterial3d(wood.clone()),
                    Transform::from_xyz(dx, 0.10, dz).with_scale(Vec3::new(0.035, 0.20, 0.035)),
                ));
            }
            f.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.sawmill_blade_mat.clone()),
                Transform::from_xyz(0.10, 0.26, 0.0).with_scale(Vec3::new(0.16, 0.03, 0.05)),
            ));
            f.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.tier_flag_mats[(level.max(1) - 1) as usize % 3].clone()),
                Transform::from_xyz(-0.14, 0.26, 0.06).with_scale(Vec3::splat(0.05)),
            ));
        }
        FurnishingKind::Seating => {
            // A table with two/three stools — the old Kitchen dining
            // cluster's idiom, now shared by every room that takes seating.
            let wood = assets.warehouse_plank_mat.clone();
            f.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(wood.clone()),
                Transform::from_xyz(0.0, 0.16, 0.0).with_scale(Vec3::new(0.34, 0.05, 0.34)),
            ));
            for (dx, dz) in [(-0.24, 0.0), (0.24, 0.0), (0.0, 0.24)] {
                f.spawn((
                    Mesh3d(assets.cylinder.clone()),
                    MeshMaterial3d(wood.clone()),
                    Transform::from_xyz(dx, 0.09, dz).with_scale(Vec3::new(0.09, 0.18, 0.09)),
                ));
            }
            f.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.tier_flag_mats[(level.max(1) - 1) as usize % 3].clone()),
                Transform::from_xyz(0.0, 0.20, 0.0).with_scale(Vec3::new(0.10, 0.02, 0.10)),
            ));
        }
        FurnishingKind::Heater => {
            // A stove: a stone ring base with a small flame always
            // flickering above it (`MealFireGlow` — see that component's
            // doc for why the system predates this broader use).
            f.spawn((
                Mesh3d(assets.ring.clone()),
                MeshMaterial3d(assets.kitchen_stone_mat.clone()),
                Transform::from_xyz(0.0, 0.01, 0.0)
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                    .with_scale(Vec3::splat(0.16)),
            ));
            let fire_mat = assets.heater_fire_mat.clone();
            f.spawn((
                Mesh3d(assets.cone.clone()),
                MeshMaterial3d(fire_mat.clone()),
                Transform::from_xyz(0.0, 0.09, 0.0).with_scale(Vec3::new(0.11, 0.16, 0.11)),
                MealFireGlow { fire_mat },
            ));
            if !low {
                f.spawn((
                    PointLight {
                        color: Color::srgb(1.0, 0.6, 0.25),
                        intensity: 35_000.0 + 8_000.0 * level as f32,
                        range: 2.2,
                        ..default()
                    },
                    Transform::from_xyz(0.0, 0.25, 0.0),
                    MealFireLight,
                ));
            }
        }
        FurnishingKind::Shelving => {
            // A tall, narrow shelf unit — back panel + three shelf ledges.
            let wood = assets.warehouse_plank_mat.clone();
            f.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(wood.clone()),
                Transform::from_xyz(0.0, 0.24, -0.05).with_scale(Vec3::new(0.30, 0.48, 0.04)),
            ));
            for i in 0..3u32 {
                let y = 0.10 + i as f32 * 0.15;
                f.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(wood.clone()),
                    Transform::from_xyz(0.0, y, 0.0).with_scale(Vec3::new(0.30, 0.02, 0.16)),
                ));
            }
            f.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.tier_flag_mats[(level.max(1) - 1) as usize % 3].clone()),
                Transform::from_xyz(0.10, 0.32, 0.05).with_scale(Vec3::splat(0.05)),
            ));
        }
    });
}

/// V0.22: builds the (initially hidden — `Display::None` until the first
/// reprojection places it) floating status overlay for one construction or
/// upgrade site: a progress track + fill, and a `MM:SS` countdown line
/// beneath it. `sync_buildings` re-projects it onto the building every
/// frame — see `ScaffoldBar`'s doc comment.
fn spawn_scaffold_bar(commands: &mut Commands) -> Entity {
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(2.0),
                width: Val::Px(SCAFFOLD_BAR_W),
                ..default()
            },
            DespawnOnExit(Screen::Game),
        ))
        .id();
    let track = commands
        .spawn(theme::stat_bar_track(Val::Px(SCAFFOLD_BAR_W), SCAFFOLD_BAR_H))
        .insert(ChildOf(root))
        .id();
    let fill = commands
        .spawn((
            Node {
                width: Val::Percent(0.0),
                height: Val::Percent(100.0),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(theme::ACCENT_WARM),
            ScaffoldFill,
            ChildOf(track),
        ))
        .id();
    let countdown = commands
        .spawn((
            theme::text(String::new(), theme::FS_MICRO, theme::TEXT_PRIMARY),
            ScaffoldCountdown,
            ChildOf(root),
        ))
        .id();
    commands.entity(root).insert(ScaffoldBar { fill, countdown });
    root
}

/// `MM:SS` for the scaffold countdown — build times in this game never
/// reach an hour (see `Building::build_eta_secs`'s doc comment), so two
/// segments are enough; an unusually long single-worker max-level upgrade
/// still prints fine, just wider than usual (`{:02}` never truncates).
fn format_countdown(secs: f32) -> String {
    let total = secs.max(0.0).round() as u32;
    format!("{:02}:{:02}", total / 60, total % 60)
}
