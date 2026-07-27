//! V0.19: the road-drawing tool — draw/erase modes armed from the build bar
//! (`ui::buildbar`'s Infrastructure category), a drag-to-paint gesture (the
//! actual mouse/touch tracking lives in `input::road_drag_input` /
//! `touch::touch_control` — this module owns the mode itself, the live
//! preview and the confirm bar), and the batch commit.
//!
//! Modelled on `ui::placement`'s V0.16 CoC confirm bar (drop, then ✓/⟳/✗ —
//! nothing reaches the server before ✓), with two differences: there is no
//! single "drop point" here — a drag paints a whole BATCH of tiles at once,
//! so the bar floats over the last-painted tile instead of a fixed spot —
//! and rotation is meaningless for a road, so only ✓/✗ exist.
//!
//! Unlike `BuildMode` (which the CoC flow disarms on every confirm — "one
//! placement per confirm"), `RoadMode` stays armed after a confirm: a road is
//! a network laid a segment at a time, not a single discrete choice, so
//! forcing the player back to the build bar after every stroke would make
//! laying "dozens of tiles" (see `PlayerCommand::BuildRoad`'s doc) a chore.
//! The tool only disarms on Escape, re-clicking its own button, or picking a
//! different tool (`ui::buildbar`).

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use frozen_city::game::types::{
    GameState, PlayerCommand, MAX_ROAD_TILES_PER_COMMAND, ROAD_COST_WOOD, ROAD_REFUND,
};
use frozen_city::net::protocol::ClientMsg;

use super::i18n::Lang;
use super::i18n_roads;
use super::render::GameAssets;
use super::theme::{self, BTN_DANGER, BTN_SUCCESS, TEXT_PRIMARY};
use super::{tile_center_world, GameView, NetConn, Screen};

/// Which road tool (if any) is armed from the build bar. Mutually exclusive
/// with `BuildMode`/`RelocateMode` — selecting one clears the others (see
/// `ui::buildbar`'s `road_tool_buttons`/`build_buttons`).
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RoadMode {
    #[default]
    Off,
    Draw,
    Erase,
}

impl RoadMode {
    pub fn active(self) -> bool {
        self != RoadMode::Off
    }
}

/// The batch a drag is painting (or just finished painting, awaiting
/// confirm). `tiles` is append-only and deduped while a stroke is live (see
/// `input::road_drag_input`) — order matters and is preserved: the server
/// lays/tears tiles in list order and a `BuildRoad` stops wherever the wood
/// runs out (see that command's doc comment), so the FIRST tiles drawn are
/// the ones a low-wood drag is guaranteed to still get.
#[derive(Resource, Default)]
pub struct RoadPaint {
    pub tiles: Vec<(u8, u8)>,
    /// True from the first frame of a stroke until the button/finger lifts.
    /// The confirm bar's ✓/✗ row only appears once this drops back to
    /// `false` — "a drag should never send anything to the server by
    /// itself" applies to even SHOWING the commit button, not just to
    /// sending the command.
    pub dragging: bool,
    /// `true` for `RemoveRoad`, `false` for `BuildRoad` — fixed for the whole
    /// batch, latched from `RoadMode` when a stroke starts.
    pub erasing: bool,
}

impl RoadPaint {
    pub fn reset(&mut self) {
        self.tiles.clear();
        self.dragging = false;
    }
}

/// Bundles the two road resources for systems that need both, keeping them
/// to ONE parameter slot — `input::build_input`/`touch::touch_control` are
/// already near Bevy's per-system parameter cap (see their own doc
/// comments), same reasoning as `input::PlacementModes`.
#[derive(bevy::ecs::system::SystemParam)]
pub struct RoadModes<'w> {
    pub mode: ResMut<'w, RoadMode>,
    pub paint: ResMut<'w, RoadPaint>,
}

/// Every tile crossed by a straight line between two grid cells, inclusive of
/// both ends. A fast mouse/finger drag can jump several tiles between two
/// frames; painting only the sampled endpoints would leave gaps in the road.
/// Plain integer Bresenham — the map has no diagonal-cost distinction
/// (`Tile::move_cost` doesn't care how a tile was reached), so there's no
/// reason for anything fancier.
pub fn line_tiles(a: (u8, u8), b: (u8, u8)) -> Vec<(u8, u8)> {
    let (mut x0, mut y0) = (a.0 as i32, a.1 as i32);
    let (x1, y1) = (b.0 as i32, b.1 as i32);
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut out = Vec::new();
    loop {
        out.push((x0 as u8, y0 as u8));
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
    out
}

/// Whether painting `(x, y)` in the current batch would actually do
/// something — green in the preview, and what the confirm bar's cost/refund
/// counts. Shared by both so they can never disagree, and mirrors exactly
/// what the server itself checks (`GameState::can_lay_road` for a draw,
/// `Tile::road` directly for an erase — see `sim::command`'s `RemoveRoad` arm,
/// which has no dedicated `can_` function of its own to mirror).
fn tile_valid(state: &GameState, erasing: bool, x: u8, y: u8) -> bool {
    if erasing {
        state.tile(x, y).is_some_and(|t| t.road)
    } else {
        state.can_lay_road(x, y).is_ok()
    }
}

// ------------------------------------------------------------ live preview

/// Translucent flat tiles marking the current batch, green/red by
/// `tile_valid` — the 3D-world equivalent of `input::build_input`'s ghost,
/// just one quad per painted tile instead of one footprint box.
#[derive(Resource, Default)]
struct RoadPreviewAssets {
    valid_mat: Handle<StandardMaterial>,
    invalid_mat: Handle<StandardMaterial>,
}

/// Height the preview quads float at — clear of `render::meshes`' corner-
/// height drift bump (maxes out around 0.11) without floating so high they
/// read as detached from the ground.
const PREVIEW_Y: f32 = 0.16;

#[derive(Resource, Default)]
pub struct RoadPreviewViz {
    entities: Vec<Entity>,
    /// Mirrors `RoadPaint.tiles` exactly once this system catches up —
    /// append-only just like it, so a plain length/prefix check (see
    /// `sync_road_preview`) is enough to tell "just grew" from "was reset"
    /// without re-spawning tiles that are already shown.
    cache: Vec<(u8, u8)>,
    erasing: bool,
}

fn setup_road_preview(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(RoadPreviewAssets {
        valid_mat: materials.add(StandardMaterial {
            base_color: Color::srgba(0.30, 0.90, 0.40, 0.55),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
        invalid_mat: materials.add(StandardMaterial {
            base_color: Color::srgba(0.95, 0.25, 0.25, 0.55),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
    });
}

/// Grows the preview incrementally (spawns entities for newly-painted tiles
/// only) rather than despawning and rebuilding the whole batch every frame —
/// a full stroke can reach `MAX_ROAD_TILES_PER_COMMAND`, and re-spawning the
/// whole thing on every single new tile would be O(n^2) over the drag. Only a
/// genuine reset (the batch got shorter, i.e. cleared/confirmed/cancelled, or
/// draw/erase flipped) pays for a full rebuild.
fn sync_road_preview(
    mut commands: Commands,
    view: Res<GameView>,
    assets: Res<RoadPreviewAssets>,
    game_assets: Res<GameAssets>,
    paint: Res<RoadPaint>,
    mut viz: ResMut<RoadPreviewViz>,
) {
    let incremental = viz.erasing == paint.erasing
        && paint.tiles.len() >= viz.cache.len()
        && paint.tiles[..viz.cache.len()] == viz.cache[..];
    if !incremental {
        for e in viz.entities.drain(..) {
            commands.entity(e).despawn();
        }
        viz.cache.clear();
    }
    if viz.cache.len() == paint.tiles.len() {
        return;
    }
    let Some(state) = view.ready() else { return };
    viz.erasing = paint.erasing;
    for &(x, y) in &paint.tiles[viz.cache.len()..] {
        let mat = if tile_valid(state, paint.erasing, x, y) {
            assets.valid_mat.clone()
        } else {
            assets.invalid_mat.clone()
        };
        let pos = tile_center_world(x, y) + Vec3::Y * PREVIEW_Y;
        let id = commands
            .spawn((
                Mesh3d(game_assets.cube.clone()),
                MeshMaterial3d(mat),
                Transform::from_translation(pos).with_scale(Vec3::new(0.92, 0.03, 0.92)),
                DespawnOnExit(Screen::Game),
            ))
            .id();
        viz.entities.push(id);
    }
    viz.cache = paint.tiles.clone();
}

// ------------------------------------------------------------- confirm bar

#[derive(Component)]
struct RoadBar;

#[derive(Component)]
struct RoadBtnRow;

#[derive(Component)]
struct RoadCostText;

#[derive(Component)]
struct RoadConfirmBtn;

#[derive(Component)]
struct RoadCancelBtn;

const BTN_W: f32 = 52.0;
const BTN_H: f32 = 42.0;

/// Spawn the (initially hidden) confirm bar once, on entering the game —
/// same convention as `ui::placement::spawn_placement_controls`.
fn spawn_road_bar(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                display: Display::None,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(theme::SP_XS),
                ..default()
            },
            RoadBar,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|col| {
            col.spawn(theme::chip()).with_children(|c| {
                c.spawn((theme::text("", theme::FS_SMALL, TEXT_PRIMARY), RoadCostText));
            });
            col.spawn((
                Node {
                    display: Display::None,
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(theme::SP_XS),
                    ..default()
                },
                RoadBtnRow,
            ))
            .with_children(|row| {
                spawn_btn(row, RoadConfirmBtn, "✓", BTN_SUCCESS);
                spawn_btn(row, RoadCancelBtn, "✗", BTN_DANGER);
            });
        });
}

fn spawn_btn(row: &mut ChildSpawnerCommands, marker: impl Component, glyph: &str, bg: Color) {
    row.spawn((
        Button,
        Node {
            width: Val::Px(BTN_W),
            height: Val::Px(BTN_H),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(Val::Px(1.5)),
            border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
            ..default()
        },
        BackgroundColor(bg),
        theme::BaseColor(bg),
        BorderColor::all(theme::BRASS),
        BoxShadow::new(
            Color::srgba(0.0, 0.0, 0.0, 0.45),
            Val::Px(0.0),
            Val::Px(3.0),
            Val::Px(1.0),
            Val::Px(9.0),
        ),
        marker,
    ))
    .with_children(|b| {
        b.spawn(theme::text(glyph, theme::FS_SECTION, TEXT_PRIMARY));
    });
}

/// Shows/hides the bar, re-projects it over the last-painted tile, and keeps
/// the cost/refund readout live — updates continuously while painting (the
/// "running wood cost the player can see before committing"), while the ✓/✗
/// row itself only appears once the stroke has stopped (`!paint.dragging`) —
/// a drag never even shows a way to send anything, let alone sends it.
#[allow(clippy::too_many_arguments)]
fn sync_road_bar(
    view: Res<GameView>,
    paint: Res<RoadPaint>,
    lang: Res<Lang>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut bar: Query<&mut Node, (With<RoadBar>, Without<RoadBtnRow>)>,
    mut btn_row: Query<&mut Node, (With<RoadBtnRow>, Without<RoadBar>)>,
    mut cost_text: Query<(&mut Text, &mut TextColor), With<RoadCostText>>,
) {
    let Ok(mut node) = bar.single_mut() else { return };
    let hide = |node: &mut Node| {
        if node.display != Display::None {
            node.display = Display::None;
        }
    };
    let (Some(state), Some(&last)) = (view.ready(), paint.tiles.last()) else {
        hide(&mut node);
        return;
    };

    let count = paint
        .tiles
        .iter()
        .filter(|&&(x, y)| tile_valid(state, paint.erasing, x, y))
        .count() as f32;
    if let Ok((mut t, mut color)) = cost_text.single_mut() {
        let (label, over_budget) = if paint.erasing {
            (i18n_roads::road_erase_refund(count * ROAD_COST_WOOD * ROAD_REFUND, *lang), false)
        } else {
            let cost = count * ROAD_COST_WOOD;
            (i18n_roads::road_draw_cost(cost, *lang), cost > state.stock.wood)
        };
        if t.0 != label {
            t.0 = label;
        }
        let want = if over_budget { theme::DANGER } else { TEXT_PRIMARY };
        if color.0 != want {
            color.0 = want;
        }
    }
    if let Ok(mut row) = btn_row.single_mut() {
        let want = if paint.dragging { Display::None } else { Display::Flex };
        if row.display != want {
            row.display = want;
        }
    }

    let (Ok((cam, cam_gt)), Ok(window)) = (camera.single(), windows.single()) else {
        hide(&mut node);
        return;
    };
    let anchor = tile_center_world(last.0, last.1) + Vec3::Y * 1.2;
    let Ok(screen) = cam.world_to_viewport(cam_gt, anchor) else {
        // Behind the camera / off-projection: hide rather than snap to a
        // corner — mirrors `ui::placement::sync_placement_controls`.
        hide(&mut node);
        return;
    };
    // Rough half-width guess (the row's real width varies with the localized
    // cost text) — good enough for a floating hint, unlike `ui::placement`'s
    // fixed-width bar which centers exactly.
    let half_w = 70.0;
    let max_left = (window.width() - half_w * 2.0).max(0.0);
    let max_top = (window.height() - BTN_H * 2.0).max(0.0);
    node.display = Display::Flex;
    node.left = Val::Px((screen.x - half_w).clamp(0.0, max_left));
    node.top = Val::Px((screen.y - BTN_H * 2.0).clamp(0.0, max_top));
}

/// ✓ sends the batch (`BuildRoad`/`RemoveRoad`, capped defensively — painting
/// already stops at the cap) and clears the stroke; ✗ just clears it. Neither
/// touches `RoadMode`: the tool stays armed so the next stroke can start
/// immediately (see the module doc comment for why that differs from the
/// building-placement flow this is modelled on).
fn road_confirm_buttons(
    net: Res<NetConn>,
    mut paint: ResMut<RoadPaint>,
    confirm: Query<&Interaction, (Changed<Interaction>, With<RoadConfirmBtn>)>,
    cancel: Query<&Interaction, (Changed<Interaction>, With<RoadCancelBtn>)>,
) {
    if paint.dragging || paint.tiles.is_empty() {
        return;
    }
    if confirm.iter().any(|i| *i == Interaction::Pressed) {
        let tiles: Vec<(u8, u8)> =
            paint.tiles.iter().copied().take(MAX_ROAD_TILES_PER_COMMAND).collect();
        let cmd = if paint.erasing {
            PlayerCommand::RemoveRoad { tiles }
        } else {
            PlayerCommand::BuildRoad { tiles }
        };
        net.send(ClientMsg::Cmd(cmd));
        paint.reset();
    } else if cancel.iter().any(|i| *i == Interaction::Pressed) {
        paint.reset();
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<RoadMode>()
        .init_resource::<RoadPaint>()
        .init_resource::<RoadPreviewAssets>()
        .init_resource::<RoadPreviewViz>()
        .add_systems(OnEnter(Screen::Game), (setup_road_preview, spawn_road_bar))
        .add_systems(
            Update,
            (sync_road_preview, sync_road_bar, road_confirm_buttons)
                .run_if(in_state(Screen::Game)),
        );
}
