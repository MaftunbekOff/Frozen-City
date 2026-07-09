//! Corner minimap: a north-up overview of the whole 64x64 world, baked into a
//! small CPU texture each frame. Shows terrain, buildings (furnace highlighted),
//! other players' cursors, and the current camera view box. Purely a HUD read-out
//! — it consumes the same authoritative snapshot everything else does.

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageSampler};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;

use frozen_city::game::types::{tile_index, BuildingKind, MAP_H, MAP_W};

use super::input::CamRig;
use super::ui::UiBlocker;
use super::{kind_color, player_color, terrain_color, tilef_to_world, GameView, Screen, TILE};

/// On-screen size of the (square) minimap in logical pixels.
const MINIMAP_PX: f32 = 184.0;

/// Handle to the minimap's CPU-updated texture (created once at startup).
#[derive(Resource)]
pub struct MinimapTex(pub Handle<Image>);

/// Cached terrain layer, rebuilt only when the tile grid changes; the dynamic
/// overlay (buildings/cursors/camera) is drawn on top of a clone each frame.
#[derive(Resource, Default)]
struct MinimapCache {
    terrain: Vec<u8>,
    tiles_version: u64,
    has_terrain: bool,
}

#[derive(Component)]
struct MinimapNode;

pub fn plugin(app: &mut App) {
    app.init_resource::<MinimapCache>()
        .add_systems(Startup, setup_texture)
        .add_systems(OnEnter(Screen::Game), spawn_minimap)
        .add_systems(
            Update,
            (update_minimap, minimap_click).run_if(in_state(Screen::Game)),
        );
}

fn setup_texture(mut images: ResMut<Assets<Image>>, mut commands: Commands) {
    let mut image = Image::new_fill(
        Extent3d {
            width: MAP_W as u32,
            height: MAP_H as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[10, 14, 22, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    // Nearest sampling keeps the tiles crisp when upscaled onto the HUD.
    image.sampler = ImageSampler::nearest();
    let handle = images.add(image);
    commands.insert_resource(MinimapTex(handle));
}

fn spawn_minimap(mut commands: Commands, tex: Res<MinimapTex>) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                top: Val::Px(78.0),
                width: Val::Px(MINIMAP_PX),
                height: Val::Px(MINIMAP_PX),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.06, 0.10, 0.85)),
            BorderColor::all(Color::srgba(0.55, 0.68, 0.85, 0.6)),
            // Register as a UI region so world clicks/zoom are suppressed here.
            Interaction::default(),
            UiBlocker,
            MinimapNode,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn((
                ImageNode::new(tex.0.clone()),
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
            ));
        });
}

#[inline]
fn put(buf: &mut [u8], x: i32, y: i32, rgba: [u8; 4]) {
    if x < 0 || y < 0 || x >= MAP_W as i32 || y >= MAP_H as i32 {
        return;
    }
    let i = (y as usize * MAP_W + x as usize) * 4;
    buf[i..i + 4].copy_from_slice(&rgba);
}

fn bytes(color: Color) -> [u8; 4] {
    let c = color.to_srgba();
    [
        (c.red.clamp(0.0, 1.0) * 255.0) as u8,
        (c.green.clamp(0.0, 1.0) * 255.0) as u8,
        (c.blue.clamp(0.0, 1.0) * 255.0) as u8,
        255,
    ]
}

fn update_minimap(
    view: Res<GameView>,
    rig: Res<CamRig>,
    tex: Res<MinimapTex>,
    mut cache: ResMut<MinimapCache>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(state) = view.ready() else { return };

    // Rebuild the cached terrain layer only when the tile grid actually changes.
    if !cache.has_terrain || cache.tiles_version != view.tiles_version {
        let mut terrain = vec![0u8; MAP_W * MAP_H * 4];
        for y in 0..MAP_H {
            for x in 0..MAP_W {
                let t = &view.tiles[tile_index(x as u8, y as u8)];
                let i = (y * MAP_W + x) * 4;
                terrain[i..i + 4].copy_from_slice(&bytes(terrain_color(t, x as u8, y as u8)));
            }
        }
        cache.terrain = terrain;
        cache.tiles_version = view.tiles_version;
        cache.has_terrain = true;
    }

    // Dynamic overlay: start from a clone of the terrain layer.
    let mut buf = cache.terrain.clone();

    // Buildings (furnace and each building's footprint), brightened for contrast.
    for b in &state.buildings {
        let (w, h) = b.kind.size();
        let base = kind_color(b.kind);
        let col = if b.kind == BuildingKind::Furnace {
            bytes(Color::srgb(1.0, 0.55, 0.15))
        } else {
            let s = base.to_srgba();
            bytes(Color::srgb(
                (s.red + 0.15).min(1.0),
                (s.green + 0.15).min(1.0),
                (s.blue + 0.15).min(1.0),
            ))
        };
        for dy in 0..h as i32 {
            for dx in 0..w as i32 {
                put(&mut buf, b.x as i32 + dx, b.y as i32 + dy, col);
            }
        }
    }

    // Other players' cursors as bright dots in their color.
    let me = view.player_id.unwrap_or(0);
    for p in &state.players {
        if p.id == me {
            continue;
        }
        if let Some((cx, cy)) = p.cursor {
            let col = bytes(player_color(p.color));
            put(&mut buf, cx as i32, cy as i32, col);
            put(&mut buf, cx as i32 + 1, cy as i32, col);
            put(&mut buf, cx as i32, cy as i32 + 1, col);
        }
    }

    // Map pings as a bright plus in the pinging player's color.
    for ping in &state.pings {
        let col = bytes(player_color(ping.color));
        let (px, py) = (ping.x as i32, ping.y as i32);
        put(&mut buf, px, py, col);
        put(&mut buf, px + 1, py, col);
        put(&mut buf, px - 1, py, col);
        put(&mut buf, px, py + 1, col);
        put(&mut buf, px, py - 1, col);
    }

    // Camera view box: a hollow rectangle around the camera focus, sized by the
    // zoom distance (north-up; it doesn't rotate with the 3D camera).
    let fx = (rig.focus.x / TILE + MAP_W as f32 / 2.0).round() as i32;
    let fy = (rig.focus.z / TILE + MAP_H as f32 / 2.0).round() as i32;
    let half = (rig.dist * 0.35).clamp(4.0, 40.0) as i32;
    let frame = bytes(Color::srgb(0.98, 0.95, 0.55));
    for x in (fx - half)..=(fx + half) {
        put(&mut buf, x, fy - half, frame);
        put(&mut buf, x, fy + half, frame);
    }
    for y in (fy - half)..=(fy + half) {
        put(&mut buf, fx - half, y, frame);
        put(&mut buf, fx + half, y, frame);
    }

    if let Some(mut image) = images.get_mut(&tex.0) {
        image.data = Some(buf);
    }
}

/// Click (or drag) on the minimap to move the camera focus to that spot.
fn minimap_click(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    minimap: Query<(&Interaction, &GlobalTransform), With<MinimapNode>>,
    mut rig: ResMut<CamRig>,
) {
    if !buttons.pressed(MouseButton::Left) {
        return;
    }
    let Ok((interaction, gt)) = minimap.single() else { return };
    // Only when the pointer is actually over the minimap.
    if *interaction == Interaction::None {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };

    // The node's GlobalTransform sits at its center (logical pixels); map the
    // cursor into the [0,1]^2 minimap box, then into tile coordinates.
    let center = gt.translation().truncate();
    let origin = center - Vec2::splat(MINIMAP_PX / 2.0);
    let local = (cursor - origin) / MINIMAP_PX;
    if local.x < 0.0 || local.x > 1.0 || local.y < 0.0 || local.y > 1.0 {
        return;
    }
    let tx = local.x * MAP_W as f32;
    let ty = local.y * MAP_H as f32;
    rig.focus = tilef_to_world((tx, ty));
}
