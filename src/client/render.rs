//! World rendering: terrain grid, buildings, survivors, heat glow, day/night
//! tint, snowfall and co-op player cursors. All visuals are procedural — no
//! asset files.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use frozen_city::game::rng::Rng;
use frozen_city::game::types::BuildingKind;

use super::*;

#[derive(Resource)]
pub struct GameAssets {
    pub glow: Handle<Image>,
    pub disc: Handle<Image>,
}

#[derive(Resource, Default)]
pub struct TerrainViz {
    pub entities: Vec<Entity>,
    pub cache: Vec<Tile>,
    pub seen_tiles_version: u64,
}

#[derive(Resource, Default)]
pub struct BuildingViz(pub HashMap<u32, Entity>);

#[derive(Resource, Default)]
pub struct SurvivorViz(pub HashMap<u32, Entity>);

#[derive(Resource, Default)]
pub struct CursorViz(pub HashMap<u64, Entity>);

#[derive(Component)]
pub struct BuildingMarker;

#[derive(Component)]
pub struct WorkerLabel(pub u32);

/// On the furnace root sprite: pulses orange while lit.
#[derive(Component)]
pub struct FurnacePulse;

#[derive(Component)]
pub struct SurvivorDot {
    pub id: u32,
}

#[derive(Component)]
pub struct Wander {
    pub home: Vec2,
    pub target: Vec2,
    pub speed: f32,
}

#[derive(Component)]
pub struct CursorMarker {
    pub player: u64,
}

#[derive(Component)]
pub struct NightOverlay;

#[derive(Component)]
pub struct HeatGlow;

#[derive(Component)]
pub struct FurnaceFire;

#[derive(Component)]
pub struct SelectionRing;

#[derive(Component)]
pub struct GhostMarker;

#[derive(Component)]
pub struct Snowflake {
    pub fall: f32,
    pub drift: f32,
    pub phase: f32,
}

pub fn setup_camera_and_assets(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.spawn(Camera2d);
    let glow = images.add(radial_image(96, |d| (1.0 - d).max(0.0).powf(2.0)));
    let disc = images.add(radial_image(24, |d| ((0.95 - d) / 0.15).clamp(0.0, 1.0)));
    commands.insert_resource(GameAssets { glow, disc });
}

fn radial_image(size: u32, alpha: impl Fn(f32) -> f32) -> Image {
    let half = size as f32 / 2.0;
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 + 0.5 - half) / half;
            let dy = (y as f32 + 0.5 - half) / half;
            let d = (dx * dx + dy * dy).sqrt().min(1.0);
            let a = (alpha(d) * 255.0).round().clamp(0.0, 255.0) as u8;
            data.extend_from_slice(&[255, 255, 255, a]);
        }
    }
    Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

pub fn enter_game(
    mut commands: Commands,
    assets: Res<GameAssets>,
    mut cam: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
) {
    if let Ok((mut t, mut proj)) = cam.single_mut() {
        t.translation = Vec3::ZERO;
        if let Projection::Orthographic(o) = &mut *proj {
            o.scale = 1.0;
        }
    }

    // Day/night tint that follows the camera.
    commands.spawn((
        Sprite::from_color(Color::srgba(0.02, 0.04, 0.12, 0.0), Vec2::splat(30000.0)),
        Transform::from_xyz(0.0, 0.0, Z_NIGHT),
        NightOverlay,
        DespawnOnExit(Screen::Game),
    ));
    // Furnace heat radius glow (world center = furnace center).
    commands.spawn((
        Sprite {
            image: assets.glow.clone(),
            color: Color::srgba(1.0, 0.55, 0.18, 0.20),
            custom_size: Some(Vec2::splat(1.0)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, Z_HEAT),
        Visibility::Hidden,
        HeatGlow,
        DespawnOnExit(Screen::Game),
    ));
    // Flame glow right on the furnace.
    commands.spawn((
        Sprite {
            image: assets.glow.clone(),
            color: Color::srgba(1.0, 0.72, 0.30, 0.55),
            custom_size: Some(Vec2::splat(110.0)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, Z_FIRE),
        Visibility::Hidden,
        FurnaceFire,
        DespawnOnExit(Screen::Game),
    ));
    // Selection highlight ring.
    commands.spawn((
        Sprite::from_color(Color::srgba(1.0, 1.0, 1.0, 0.25), Vec2::splat(TILE * 1.4)),
        Transform::from_xyz(0.0, 0.0, Z_RING),
        Visibility::Hidden,
        SelectionRing,
        DespawnOnExit(Screen::Game),
    ));
    // Build placement ghost.
    commands.spawn((
        Sprite::from_color(Color::srgba(0.3, 0.9, 0.4, 0.45), Vec2::splat(TILE)),
        Transform::from_xyz(0.0, 0.0, Z_GHOST),
        Visibility::Hidden,
        GhostMarker,
        DespawnOnExit(Screen::Game),
    ));

    // Snowfall.
    let mut rng = Rng::new(0x5005_7EA1);
    for _ in 0..220 {
        let x = rng.range(-1400, 1400) as f32;
        let y = rng.range(-900, 900) as f32;
        let s = 1.5 + rng.below(20) as f32 * 0.1;
        commands.spawn((
            Sprite::from_color(
                Color::srgba(1.0, 1.0, 1.0, 0.35 + rng.below(40) as f32 * 0.01),
                Vec2::splat(s),
            ),
            Transform::from_xyz(x, y, Z_SNOW),
            Snowflake {
                fall: 35.0 + rng.below(55) as f32,
                drift: 6.0 + rng.below(18) as f32,
                phase: rng.below(628) as f32 * 0.01,
            },
            DespawnOnExit(Screen::Game),
        ));
    }
}

pub fn sync_terrain(
    mut commands: Commands,
    view: Res<GameView>,
    mut viz: ResMut<TerrainViz>,
    mut sprites: Query<&mut Sprite>,
) {
    if view.tiles.is_empty() {
        return;
    }
    if viz.entities.is_empty() {
        viz.entities.reserve(MAP_W * MAP_H);
        for y in 0..MAP_H as u8 {
            for x in 0..MAP_W as u8 {
                let tile = &view.tiles[frozen_city::game::types::tile_index(x, y)];
                let e = commands
                    .spawn((
                        Sprite::from_color(terrain_color(tile, x, y), Vec2::splat(TILE - 1.0)),
                        Transform::from_translation(tile_center_world(x, y).extend(Z_TERRAIN)),
                        DespawnOnExit(Screen::Game),
                    ))
                    .id();
                viz.entities.push(e);
            }
        }
        viz.cache = view.tiles.clone();
        viz.seen_tiles_version = view.tiles_version;
        return;
    }
    if viz.seen_tiles_version == view.tiles_version {
        return;
    }
    viz.seen_tiles_version = view.tiles_version;
    for idx in 0..viz.cache.len().min(view.tiles.len()) {
        if viz.cache[idx] != view.tiles[idx] {
            let (x, y) = ((idx % MAP_W) as u8, (idx / MAP_W) as u8);
            if let Ok(mut s) = sprites.get_mut(viz.entities[idx]) {
                s.color = terrain_color(&view.tiles[idx], x, y);
            }
        }
    }
    viz.cache = view.tiles.clone();
}

pub fn sync_buildings(
    mut commands: Commands,
    view: Res<GameView>,
    mut viz: ResMut<BuildingViz>,
    mut labels: Query<(&WorkerLabel, &mut Text2d)>,
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
        let (w, h) = b.kind.size();
        let size = Vec2::new(w as f32 * TILE - 6.0, h as f32 * TILE - 6.0);
        let is_furnace = b.kind == BuildingKind::Furnace;
        let root = commands
            .spawn((
                Sprite::from_color(kind_color(b.kind), size),
                Transform::from_translation(building_center_world(b).extend(Z_BUILDING)),
                BuildingMarker,
                DespawnOnExit(Screen::Game),
            ))
            .with_children(|p| {
                let has_workers = b.kind.max_workers() > 0;
                p.spawn((
                    Text2d::new(b.kind.letter()),
                    TextFont::from_font_size(if is_furnace { 30.0 } else { 15.0 }),
                    TextColor(Color::srgba(1.0, 1.0, 1.0, 0.92)),
                    Transform::from_xyz(0.0, if has_workers { 4.0 } else { 0.0 }, 0.2),
                ));
                if has_workers {
                    p.spawn((
                        Text2d::new(""),
                        TextFont::from_font_size(10.0),
                        TextColor(Color::srgba(0.95, 0.97, 1.0, 0.9)),
                        Transform::from_xyz(0.0, -9.0, 0.2),
                        WorkerLabel(b.id),
                    ));
                }
            })
            .id();
        if is_furnace {
            commands.entity(root).insert(FurnacePulse);
        }
        viz.0.insert(b.id, root);
    }

    // Demolished / removed buildings.
    let mut gone: Vec<u32> = Vec::new();
    for id in viz.0.keys() {
        if state.find_building(*id).is_none() {
            gone.push(*id);
        }
    }
    for id in gone {
        if let Some(e) = viz.0.remove(&id) {
            commands.entity(e).despawn();
        }
    }

    // Worker count labels.
    for (label, mut text) in &mut labels {
        if let Some(b) = state.find_building(label.0) {
            let s = format!("{}/{}", b.workers, b.kind.max_workers());
            if text.0 != s {
                text.0 = s;
            }
        }
    }
}

fn mix(a: (f32, f32, f32), b: (f32, f32, f32), t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::srgb(
        a.0 + (b.0 - a.0) * t,
        a.1 + (b.1 - a.1) * t,
        a.2 + (b.2 - a.2) * t,
    )
}

pub fn sync_survivors(
    mut commands: Commands,
    view: Res<GameView>,
    assets: Res<GameAssets>,
    mut viz: ResMut<SurvivorViz>,
    mut dots: Query<(&SurvivorDot, &mut Wander, &mut Sprite)>,
    mut seen: Local<u64>,
) {
    let Some(state) = view.ready() else { return };
    if *seen == view.version {
        return;
    }
    *seen = view.version;

    // Visual home for each survivor: workers cluster at their buildings (in
    // assignment order), everyone else gathers around the furnace.
    let mut homes: Vec<Vec2> = Vec::with_capacity(state.survivors.len());
    for b in &state.buildings {
        for _ in 0..b.workers {
            homes.push(building_center_world(b));
        }
    }
    while homes.len() < state.survivors.len() {
        homes.push(Vec2::new(0.0, -TILE * 2.0));
    }

    let mut wanted: HashMap<u32, Vec2> = HashMap::new();
    for (i, s) in state.survivors.iter().enumerate() {
        wanted.insert(s.id, homes[i]);
    }

    // Spawn newcomers.
    for s in &state.survivors {
        if viz.0.contains_key(&s.id) {
            continue;
        }
        let home = wanted[&s.id];
        let e = commands
            .spawn((
                Sprite {
                    image: assets.disc.clone(),
                    color: Color::srgb(0.25, 0.30, 0.45),
                    custom_size: Some(Vec2::splat(9.0)),
                    ..default()
                },
                Transform::from_translation(home.extend(Z_SURVIVOR)),
                SurvivorDot { id: s.id },
                Wander {
                    home,
                    target: home,
                    speed: 28.0 + (s.id % 7) as f32 * 3.0,
                },
                DespawnOnExit(Screen::Game),
            ))
            .id();
        viz.0.insert(s.id, e);
    }

    // Remove the dead.
    let mut gone: Vec<u32> = Vec::new();
    for id in viz.0.keys() {
        if !wanted.contains_key(id) {
            gone.push(*id);
        }
    }
    for id in gone {
        if let Some(e) = viz.0.remove(&id) {
            commands.entity(e).despawn();
        }
    }

    // Update homes and health tint.
    for (dot, mut wander, mut sprite) in &mut dots {
        if let Some(home) = wanted.get(&dot.id) {
            if wander.home.distance(*home) > 1.0 {
                wander.home = *home;
                wander.target = *home;
            }
        }
        if let Some(s) = state.survivors.iter().find(|s| s.id == dot.id) {
            let sick = 1.0 - (s.hp / 100.0).clamp(0.0, 1.0);
            sprite.color = mix((0.30, 0.38, 0.55), (0.90, 0.18, 0.12), sick);
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
        let pos = t.translation.truncate();
        let to = w.target - pos;
        let dist = to.length();
        if dist < 2.0 {
            let off = Vec2::new(
                rng.range(-16, 16) as f32,
                rng.range(-16, 16) as f32,
            );
            w.target = w.home + off;
        } else {
            let step = (w.speed * dt).min(dist);
            let np = pos + to / dist * step;
            t.translation.x = np.x;
            t.translation.y = np.y;
        }
    }
}

/// Heat glow, flame, day/night tint, furnace pulse and the selection ring.
pub fn animate_effects(
    time: Res<Time>,
    view: Res<GameView>,
    selection: Res<Selection>,
    cam: Query<
        &Transform,
        (
            With<Camera2d>,
            Without<NightOverlay>,
            Without<SelectionRing>,
        ),
    >,
    mut set: ParamSet<(
        Query<(&mut Sprite, &mut Transform), With<NightOverlay>>,
        Query<(&mut Sprite, &mut Visibility), With<HeatGlow>>,
        Query<(&mut Sprite, &mut Visibility), With<FurnaceFire>>,
        Query<(&mut Transform, &mut Sprite, &mut Visibility), With<SelectionRing>>,
        Query<&mut Sprite, With<FurnacePulse>>,
    )>,
    mut glow_px: Local<f32>,
) {
    let Some(state) = view.state.as_ref() else { return };
    let elapsed = time.elapsed_secs();

    // Day/night overlay follows the camera.
    let cam_pos = cam.iter().next().map(|t| t.translation).unwrap_or(Vec3::ZERO);
    let t = state.time_of_day();
    let daylight = (1.0 - (std::f32::consts::TAU * t).cos()) / 2.0;
    let alpha = (1.0 - daylight).powf(1.4) * 0.45;
    for (mut sprite, mut tr) in &mut set.p0() {
        sprite.color = Color::srgba(0.02, 0.04, 0.12, alpha);
        tr.translation.x = cam_pos.x;
        tr.translation.y = cam_pos.y;
    }

    // Heat radius glow.
    let target = state.heat_radius() * TILE * 1.12;
    *glow_px += (target - *glow_px) * (4.0 * time.delta_secs()).min(1.0);
    for (mut sprite, mut vis) in &mut set.p1() {
        if *glow_px < 6.0 {
            *vis = Visibility::Hidden;
        } else {
            *vis = Visibility::Visible;
            sprite.custom_size = Some(Vec2::splat(*glow_px * 2.0));
            sprite.color = Color::srgba(
                1.0,
                0.55,
                0.18,
                0.14 + 0.04 * state.furnace_level as f32,
            );
        }
    }

    // Flame on the furnace.
    let pulse = (elapsed * 6.0).sin();
    for (mut sprite, mut vis) in &mut set.p2() {
        if state.furnace_lit {
            *vis = Visibility::Visible;
            let base = 95.0 + 22.0 * state.furnace_level as f32;
            sprite.custom_size = Some(Vec2::splat(base + 7.0 * pulse));
            sprite.color = Color::srgba(1.0, 0.72, 0.30, 0.45 + 0.12 * pulse.abs());
        } else {
            *vis = Visibility::Hidden;
        }
    }

    // Selection ring.
    for (mut tr, mut sprite, mut vis) in &mut set.p3() {
        let sel = selection.0.and_then(|id| state.find_building(id));
        if let Some(b) = sel {
            let (w, h) = b.kind.size();
            *vis = Visibility::Visible;
            sprite.custom_size = Some(Vec2::new(
                w as f32 * TILE * 1.22,
                h as f32 * TILE * 1.22,
            ));
            let pos = building_center_world(b);
            tr.translation.x = pos.x;
            tr.translation.y = pos.y;
        } else {
            *vis = Visibility::Hidden;
        }
    }

    // Furnace body tint.
    for mut sprite in &mut set.p4() {
        sprite.color = if state.furnace_lit {
            let k = 0.92 + 0.08 * pulse;
            Color::srgb(0.85 * k, 0.38 * k, 0.14)
        } else {
            Color::srgb(0.38, 0.32, 0.30)
        };
    }
}

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

    let mut targets: HashMap<u64, Vec2> = HashMap::new();
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
        let e = commands
            .spawn((
                Sprite {
                    image: assets.disc.clone(),
                    color,
                    custom_size: Some(Vec2::splat(12.0)),
                    ..default()
                },
                Transform::from_translation(targets[&p.id].extend(Z_CURSOR)),
                CursorMarker { player: p.id },
                DespawnOnExit(Screen::Game),
            ))
            .with_children(|parent| {
                parent.spawn((
                    Text2d::new(p.name.clone()),
                    TextFont::from_font_size(11.0),
                    TextColor(color),
                    Transform::from_xyz(0.0, -16.0, 0.1),
                ));
            })
            .id();
        viz.0.insert(p.id, e);
    }

    let mut gone: Vec<u64> = Vec::new();
    for id in viz.0.keys() {
        if !targets.contains_key(id) {
            gone.push(*id);
        }
    }
    for id in gone {
        if let Some(e) = viz.0.remove(&id) {
            commands.entity(e).despawn();
        }
    }

    let blend = 1.0 - (-12.0 * time.delta_secs()).exp();
    for (marker, mut tr) in &mut q {
        if let Some(target) = targets.get(&marker.player) {
            let pos = tr.translation.truncate().lerp(*target, blend);
            tr.translation.x = pos.x;
            tr.translation.y = pos.y;
        }
    }
}

pub fn snow_fall(
    time: Res<Time>,
    cam: Query<(&Transform, &Projection), (With<Camera2d>, Without<Snowflake>)>,
    mut flakes: Query<(&mut Transform, &Snowflake)>,
    mut rng: Local<Rng>,
) {
    let Some((cam_t, proj)) = cam.iter().next() else { return };
    let scale = match proj {
        Projection::Orthographic(o) => o.scale,
        _ => 1.0,
    };
    let half_w = 760.0 * scale.max(1.0);
    let half_h = 480.0 * scale.max(1.0);
    let (cx, cy) = (cam_t.translation.x, cam_t.translation.y);
    let dt = time.delta_secs();
    let elapsed = time.elapsed_secs();

    for (mut t, flake) in &mut flakes {
        t.translation.y -= flake.fall * dt;
        t.translation.x += (elapsed * 0.8 + flake.phase).sin() * flake.drift * dt;
        if t.translation.y < cy - half_h {
            t.translation.y = cy + half_h;
            t.translation.x = cx + rng.range(-(half_w as i32), half_w as i32) as f32;
        }
        if (t.translation.x - cx).abs() > half_w * 1.6 {
            t.translation.x = cx + rng.range(-(half_w as i32), half_w as i32) as f32;
        }
    }
}
