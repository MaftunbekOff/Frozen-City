//! Bevy client: rendering, input and UI. Everything here consumes the
//! authoritative `GameState` snapshots mirrored into [`GameView`].

use std::sync::Mutex;

use bevy::prelude::*;

use frozen_city::game::types::{
    Building, BuildingKind, GameState, Terrain, Tile, MAP_H, MAP_W,
};
use frozen_city::net::client::ClientConn;
use frozen_city::net::protocol::ClientMsg;
#[cfg(not(target_arch = "wasm32"))]
use frozen_city::net::server::ServerHandle;

pub mod input;
#[cfg(target_arch = "wasm32")]
pub mod local_server;
pub mod menu;
pub mod net_sync;
pub mod render;
pub mod touch;
pub mod ui;

/// One grid tile = one 3D world unit. The map lies on the XZ plane
/// (sim grid y -> world Z), +Y is up, the furnace center is the origin.
pub const TILE: f32 = 1.0;
pub const DEFAULT_PORT: u16 = 4595;

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Screen {
    #[default]
    Menu,
    Game,
}

/// Graphics quality tier, decided once at startup for the platform.
/// The scene content is identical — tiers only trade post-processing,
/// shadows and pixel density so every device hits a high frame rate.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Quality {
    /// Phones: no shadows/bloom, capped pixel ratio.
    Low,
    /// Desktop browsers: HDR bloom, shadows, FXAA. (Only constructed on wasm.)
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Medium,
    /// Native desktop: HDR bloom, high-res shadows, MSAA 4x.
    High,
}

#[derive(Resource, Clone)]
pub struct Settings {
    pub name: String,
    pub join_addr: String,
    /// Unused in the browser: pages cannot listen for connections.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub host_port: u16,
    pub seed: Option<u64>,
    pub win_days: u32,
    pub smoke: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AutoAction {
    Single,
    Host,
    Join,
}

/// One-shot action taken from the CLI (`--host`, `--join`, `--smoke`).
#[derive(Resource, Default)]
pub struct AutoStart(pub Option<AutoAction>);

/// The live connection to the (possibly in-process) server.
/// `Receiver` is not `Sync`, hence the mutex.
#[derive(Resource, Default)]
pub struct NetConn(pub Option<Mutex<ClientConn>>);

impl NetConn {
    pub fn send(&self, msg: ClientMsg) {
        if let Some(m) = &self.0 {
            if let Ok(c) = m.lock() {
                c.send(msg);
            }
        }
    }
}

/// The locally owned authoritative world, when this client is also the host:
/// a handle to the server threads on native, the inline sim on wasm.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource, Default)]
pub struct ServerRes(pub Option<ServerHandle>);

#[cfg(target_arch = "wasm32")]
#[derive(Resource, Default)]
pub struct ServerRes(pub Option<local_server::LocalServer>);

/// Client-side mirror of the latest authoritative snapshot.
#[derive(Resource, Default)]
pub struct GameView {
    pub state: Option<GameState>,
    /// Last tile grid received (tiles only ride along periodically).
    pub tiles: Vec<Tile>,
    pub player_id: Option<u64>,
    /// Bumped on every received snapshot.
    pub version: u64,
    /// Bumped whenever the tile grid is refreshed.
    pub tiles_version: u64,
    pub disconnected: bool,
    pub error: Option<String>,
}

impl GameView {
    /// State with the cached tile grid guaranteed to be present.
    pub fn ready(&self) -> Option<&GameState> {
        self.state.as_ref().filter(|_| !self.tiles.is_empty())
    }
}

#[derive(Resource, Default)]
pub struct BuildMode(pub Option<BuildingKind>);

#[derive(Resource, Default)]
pub struct Selection(pub Option<u32>);

/// True while the mouse is over any interactive UI region.
#[derive(Resource, Default)]
pub struct UiHover(pub bool);

/// Center of a tile on the ground plane.
pub fn tile_center_world(x: u8, y: u8) -> Vec3 {
    Vec3::new(
        (x as f32 + 0.5 - MAP_W as f32 / 2.0) * TILE,
        0.0,
        (y as f32 + 0.5 - MAP_H as f32 / 2.0) * TILE,
    )
}

pub fn building_center_world(b: &Building) -> Vec3 {
    let (w, h) = b.kind.size();
    Vec3::new(
        (b.x as f32 + w as f32 / 2.0 - MAP_W as f32 / 2.0) * TILE,
        0.0,
        (b.y as f32 + h as f32 / 2.0 - MAP_H as f32 / 2.0) * TILE,
    )
}

pub fn world_to_tile(p: Vec3) -> Option<(u8, u8)> {
    let tx = (p.x / TILE + MAP_W as f32 / 2.0).floor();
    let ty = (p.z / TILE + MAP_H as f32 / 2.0).floor();
    if tx >= 0.0 && ty >= 0.0 && tx < MAP_W as f32 && ty < MAP_H as f32 {
        Some((tx as u8, ty as u8))
    } else {
        None
    }
}

/// Ground position -> fractional tile coordinates (for cursor sharing —
/// the wire format is unchanged from the 2D renderer).
pub fn world_to_tilef(p: Vec3) -> (f32, f32) {
    (p.x / TILE + MAP_W as f32 / 2.0, p.z / TILE + MAP_H as f32 / 2.0)
}

pub fn tilef_to_world(t: (f32, f32)) -> Vec3 {
    Vec3::new(
        (t.0 - MAP_W as f32 / 2.0) * TILE,
        0.0,
        (t.1 - MAP_H as f32 / 2.0) * TILE,
    )
}

pub fn kind_color(k: BuildingKind) -> Color {
    match k {
        BuildingKind::Furnace => Color::srgb(0.85, 0.38, 0.14),
        BuildingKind::Tent => Color::srgb(0.76, 0.62, 0.38),
        BuildingKind::Sawmill => Color::srgb(0.55, 0.38, 0.20),
        BuildingKind::CoalMine => Color::srgb(0.36, 0.37, 0.44),
        BuildingKind::HunterHut => Color::srgb(0.34, 0.51, 0.30),
    }
}

pub fn terrain_color(t: &Tile, x: u8, y: u8) -> Color {
    match t.terrain {
        Terrain::Snow => {
            let v = ((x as u32 * 7 + y as u32 * 13) % 5) as f32 * 0.012;
            Color::srgb(0.80 + v, 0.85 + v, 0.92 + v * 0.5)
        }
        Terrain::Forest => {
            let d = (t.deposit as f32 / 80.0).clamp(0.25, 1.0);
            Color::srgb(0.13, 0.16 + 0.24 * d, 0.14 + 0.14 * d)
        }
        Terrain::Coal => {
            if t.deposit > 0 {
                Color::srgb(0.21, 0.22, 0.26)
            } else {
                Color::srgb(0.48, 0.49, 0.53)
            }
        }
    }
}

const PLAYER_PALETTE: [(f32, f32, f32); 8] = [
    (0.95, 0.65, 0.20),
    (0.30, 0.75, 0.95),
    (0.55, 0.90, 0.40),
    (0.95, 0.40, 0.55),
    (0.75, 0.55, 0.95),
    (0.95, 0.90, 0.35),
    (0.40, 0.90, 0.80),
    (0.95, 0.55, 0.30),
];

pub fn player_color(idx: u8) -> Color {
    let (r, g, b) = PLAYER_PALETTE[idx as usize % PLAYER_PALETTE.len()];
    Color::srgb(r, g, b)
}

pub struct ClientPlugin;

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<Screen>()
            .init_resource::<input::CamRig>()
            .init_resource::<touch::TouchCtl>()
            .init_resource::<NetConn>()
            .init_resource::<ServerRes>()
            .init_resource::<GameView>()
            .init_resource::<BuildMode>()
            .init_resource::<Selection>()
            .init_resource::<UiHover>()
            .init_resource::<render::TerrainViz>()
            .init_resource::<render::BuildingViz>()
            .init_resource::<render::SurvivorViz>()
            .init_resource::<render::CursorViz>()
            .add_systems(Startup, render::setup_camera_and_assets)
            // Menu.
            .add_systems(OnEnter(Screen::Menu), menu::spawn_menu)
            .add_systems(
                Update,
                (
                    menu::autostart,
                    menu::menu_buttons,
                    ui::generic_button_hover,
                )
                    .run_if(in_state(Screen::Menu)),
            )
            // Game lifecycle.
            .add_systems(OnEnter(Screen::Game), (render::enter_game, ui::spawn_hud))
            .add_systems(OnExit(Screen::Game), teardown_game)
            // Snapshot intake + world sync (ordered).
            .add_systems(
                Update,
                (
                    net_sync::pump_net,
                    render::sync_terrain,
                    render::sync_buildings,
                    render::sync_survivors,
                )
                    .chain()
                    .run_if(in_state(Screen::Game)),
            )
            // Continuous effects & presence.
            .add_systems(
                Update,
                (
                    render::animate_environment,
                    render::animate_effects,
                    render::animate_smoke,
                    render::animate_survivors,
                    render::sync_player_cursors,
                    render::update_cursor_labels,
                    render::snow_fall,
                )
                    .run_if(in_state(Screen::Game)),
            )
            // Input (UI hover must be computed first).
            .add_systems(
                Update,
                (
                    ui::track_ui_hover,
                    touch::touch_control,
                    input::camera_control,
                    input::build_input,
                    input::send_cursor,
                )
                    .chain()
                    .run_if(in_state(Screen::Game)),
            )
            // Phone-sized windows get a smaller UI scale.
            .add_systems(Update, touch::fit_ui_scale)
            // HUD & panels.
            .add_systems(
                Update,
                (
                    ui::hud_update,
                    ui::fps_update,
                    ui::build_buttons,
                    ui::furnace_buttons,
                    ui::selection_panel_update,
                    ui::selection_panel_buttons,
                    ui::game_over_ui,
                    ui::generic_button_hover,
                    net_sync::watch_disconnect,
                )
                    .run_if(in_state(Screen::Game)),
            )
            .add_systems(Update, smoke_exit);
        #[cfg(target_arch = "wasm32")]
        app.add_plugins(local_server::plugin);
    }
}

/// Leaving the game: close the connection, stop a locally hosted server and
/// reset per-session resources. Entities die via `DespawnOnExit`.
fn teardown_game(
    mut net: ResMut<NetConn>,
    mut server: ResMut<ServerRes>,
    mut view: ResMut<GameView>,
    mut build: ResMut<BuildMode>,
    mut sel: ResMut<Selection>,
    mut terrain: ResMut<render::TerrainViz>,
    mut buildings: ResMut<render::BuildingViz>,
    mut survivors: ResMut<render::SurvivorViz>,
    mut cursors: ResMut<render::CursorViz>,
) {
    net.0 = None;
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(h) = server.0.take() {
        h.stop();
    }
    #[cfg(target_arch = "wasm32")]
    {
        server.0 = None;
    }
    // Keep the version counters monotonic across sessions so per-system
    // `Local` caches from a previous game can never collide with fresh values.
    let error = view.error.take();
    *view = GameView {
        error,
        version: view.version,
        tiles_version: view.tiles_version,
        ..Default::default()
    };
    build.0 = None;
    sel.0 = None;
    *terrain = Default::default();
    *buildings = Default::default();
    *survivors = Default::default();
    *cursors = Default::default();
}

/// In `--smoke` mode, exit automatically after a few seconds of rendering.
fn smoke_exit(
    settings: Res<Settings>,
    mut frames: Local<u32>,
    mut exit: MessageWriter<AppExit>,
) {
    if !settings.smoke {
        return;
    }
    *frames += 1;
    if *frames == 300 {
        exit.write(AppExit::Success);
    }
}
