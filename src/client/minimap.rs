//! Corner minimap: a north-up overview of the whole 64x64 world, baked into a
//! small CPU texture each frame. Shows terrain, buildings (furnace highlighted),
//! other players' cursors, and the current camera view box. Purely a HUD read-out
//! — it consumes the same authoritative snapshot everything else does.

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageSampler};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::RelativeCursorPosition;

use frozen_city::game::types::{tile_index, BuildingKind, MAP_H, MAP_W};

use super::input::CamRig;
use super::ui::UiBlocker;
use super::{kind_color, player_color, terrain_color, tilef_to_world, GameView, Screen, TILE};

/// On-screen size of the (square) minimap in logical pixels, by
/// `FormFactor` — Mobile gets a much smaller footprint so it doesn't dominate
/// a phone-width screen; Tablet/Desktop keep the original size.
const MINIMAP_PX_MOBILE: f32 = 110.0;
const MINIMAP_PX_DEFAULT: f32 = 184.0;

fn minimap_px(ff: super::theme::FormFactor) -> f32 {
    if ff.compact() {
        MINIMAP_PX_MOBILE
    } else {
        MINIMAP_PX_DEFAULT
    }
}

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
    /// Last-baked signature; the overlay is only re-drawn/re-uploaded when the
    /// snapshot or the camera view box actually changed.
    baked: bool,
    last_version: u64,
    /// Last-uploaded view box in integer minimap-pixel space (fx, fy, half).
    /// Panning/zooming the camera moves `rig.focus`/`rig.dist` continuously,
    /// but the on-screen box only needs to be re-baked once it has actually
    /// shifted by a whole pixel.
    last_box: (i32, i32, i32),
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

fn spawn_minimap(mut commands: Commands, tex: Res<MinimapTex>, ff: Res<super::theme::FormFactor>) {
    let px = minimap_px(*ff);
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                top: Val::Px(crate::client::theme::TOPBAR_H + 28.0),
                width: Val::Px(px),
                height: Val::Px(px),
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(super::theme::RAD_BTN)),
                // Clip the square minimap texture to the rounded frame —
                // otherwise its corners would poke past the rounded border.
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(super::theme::BG_PANEL),
            BorderColor::all(super::theme::BORDER_STRONG),
            // Register as a UI region so world clicks/zoom are suppressed here,
            // and track the cursor's position within it for click-to-navigate.
            Interaction::default(),
            RelativeCursorPosition::default(),
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

    // Camera view box in minimap-pixel space (north-up; it doesn't rotate with
    // the 3D camera), computed up front so it can also gate the re-bake below.
    let fx = (rig.focus.x / TILE + MAP_W as f32 / 2.0).round() as i32;
    let fy = (rig.focus.z / TILE + MAP_H as f32 / 2.0).round() as i32;
    let half = (rig.dist * 0.35).clamp(4.0, 40.0) as i32;
    let view_box = (fx, fy, half);

    // Skip the whole clone + overlay + GPU re-upload when nothing that shows on
    // the minimap changed since the last bake: no new snapshot, and the view
    // box hasn't moved by a whole on-screen pixel (panning/zooming changes
    // `rig.focus`/`rig.dist` continuously, but most of that motion doesn't
    // shift the quantized box at all).
    if cache.baked && cache.last_version == view.version && cache.last_box == view_box {
        return;
    }
    cache.baked = true;
    cache.last_version = view.version;
    cache.last_box = view_box;

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
    // zoom distance (fx/fy/half computed above, alongside the re-bake gate).
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
/// `RelativeCursorPosition` hands us the cursor in the node's own [0,1] space,
/// correct across window scale factor and `UiScale` — unlike a world-space
/// `GlobalTransform`, which UI nodes don't even carry in Bevy 0.19.
fn minimap_click(
    buttons: Res<ButtonInput<MouseButton>>,
    minimap: Query<&RelativeCursorPosition, With<MinimapNode>>,
    mut rig: ResMut<CamRig>,
) {
    if !buttons.pressed(MouseButton::Left) {
        return;
    }
    let Ok(rel) = minimap.single() else { return };
    // `normalized` is Some only while the cursor is within the minimap.
    let Some(norm) = rel.normalized else { return };
    if norm.x < 0.0 || norm.x > 1.0 || norm.y < 0.0 || norm.y > 1.0 {
        return;
    }
    let tx = norm.x * MAP_W as f32;
    let ty = norm.y * MAP_H as f32;
    rig.focus = tilef_to_world((tx, ty));
}
