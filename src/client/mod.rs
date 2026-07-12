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

pub mod audio;
pub mod chat;
pub mod events;
pub mod i18n;
pub mod input;
#[cfg(target_arch = "wasm32")]
pub mod local_server;
pub mod menu;
pub mod menu_fx;
pub mod minimap;
pub mod missions;
pub mod net_sync;
pub mod render;
pub mod research;
pub mod roles;
pub mod roster;
pub mod social;
pub mod theme;
pub mod touch;
pub mod ui;

// i18n matn kataloglari (har UI sohasi o'z faylida — parallel tahrirga qulay).
pub mod i18n_hud;
pub mod i18n_menu;
pub mod i18n_names;
pub mod i18n_panels;

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

/// Player-controlled graphics quality preference, saved via `i18n::pref_set`
/// under the `"quality"` key. `Auto` (the default) defers entirely to the
/// existing per-platform detection (native: `Quality::High`; web:
/// `web_quality()`); the other three tiers override it once the app starts.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum QualityPref {
    #[default]
    Auto,
    Low,
    Medium,
    High,
}

impl QualityPref {
    pub fn code(self) -> &'static str {
        match self {
            QualityPref::Auto => "auto",
            QualityPref::Low => "low",
            QualityPref::Medium => "medium",
            QualityPref::High => "high",
        }
    }

    pub fn from_code(s: &str) -> Option<QualityPref> {
        match s {
            "auto" => Some(QualityPref::Auto),
            "low" => Some(QualityPref::Low),
            "medium" => Some(QualityPref::Medium),
            "high" => Some(QualityPref::High),
            _ => None,
        }
    }

    /// The concrete `Quality` this preference forces, or `None` for `Auto`
    /// (meaning: keep whatever the platform-detected default already is).
    pub fn resolve(self) -> Option<Quality> {
        match self {
            QualityPref::Auto => None,
            QualityPref::Low => Some(Quality::Low),
            QualityPref::Medium => Some(Quality::Medium),
            QualityPref::High => Some(Quality::High),
        }
    }
}

/// Whether sound effects and the blizzard wind bed play at all. Saved via
/// `i18n::pref_set` under the `"audio"` key. Every audio spawn/volume site
/// lives in `audio.rs` and checks this before making any sound.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub struct AudioSettings {
    pub enabled: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        AudioSettings { enabled: true }
    }
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
    /// Session token from the last `Welcome`, replayed on reconnect.
    pub token: Option<u64>,
    /// Bumped on every received snapshot.
    pub version: u64,
    /// Bumped whenever the tile grid is refreshed.
    pub tiles_version: u64,
    pub disconnected: bool,
    pub error: Option<String>,
    /// Seconds until this persistent game-over world auto-resets (from
    /// `ServerMsg::ResetCountdown`); `None` while the game runs, or on a
    /// server that never resets (singleplayer sends no countdown).
    pub reset_countdown: Option<u32>,
}

/// Credentials for an account sign-in, kept in memory only for the life of
/// the session so a dropped connection can be redialed with `Login` instead
/// of falling back to a guest `Hello`.
#[derive(Clone)]
pub struct AccountAuth {
    pub login: String,
    pub password: String,
}

/// What is needed to transparently re-join the same world after a dropped
/// connection. Only meaningful for the `Join` path (a remote server we can
/// dial again); host/singleplayer worlds live in-process and cannot reconnect.
#[derive(Resource, Default)]
pub struct Session {
    pub join_addr: String,
    pub name: String,
    /// `Some` when this session signed in with an account instead of as a
    /// guest — reconnects replay `Login` with these instead of `Hello`.
    pub auth: Option<AccountAuth>,
    /// Latest session token (updated on every `Welcome`).
    pub token: Option<u64>,
    /// True only for a `Join` game — enables auto-reconnect on disconnect.
    pub reconnectable: bool,
    /// Reconnect attempts already spent for the current outage.
    pub attempts: u32,
    /// True while this session is in the CENTRAL world (entered through the
    /// Tunnel) — reconnects must replay `EnterCentral`, not `Login`, or a
    /// blip would silently drop the player back into their personal world.
    pub central: bool,
    /// `Some(host account)` while this session is visiting a friend's
    /// personal world — reconnects must replay `VisitFriend` for the same
    /// host (the invite outlives a blip).
    pub visiting: Option<i64>,
}

/// Which world a requested in-game switch should land in.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WorldTarget {
    /// The account's own personal world (a plain `Login`).
    Personal,
    /// The shared Global World through the Tunnel (`EnterCentral`).
    Central,
    /// A friend's personal world, by their account id (`VisitFriend` — needs
    /// a standing invite, see `SocialState::invite`).
    Visit(i64),
}

/// One transient nearby-chat line received from the server, waiting for the
/// social UI to display it (an inbox drained by `social.rs`, never a log).
#[derive(Clone)]
pub struct BubbleEvent {
    pub player_id: u64,
    pub name: String,
    pub color: u8,
    pub text: String,
}

/// Client-side mirror of the account's social life: the friends list the
/// server last sent, a pending world-visit invite (if any), and freshly
/// arrived nearby-chat bubbles.
#[derive(Resource, Default)]
pub struct SocialState {
    pub friends: Vec<frozen_city::net::protocol::FriendInfo>,
    /// `(host account, host display name)` of the newest unanswered invite.
    pub invite: Option<(i64, String)>,
    /// Inbox of bubbles not yet rendered; the social UI drains this.
    pub bubbles: Vec<BubbleEvent>,
    /// Friends' personal-world stats (`ServerMsg::Showcase`), refreshed
    /// alongside the friends list; rows match `friends` by `account`.
    pub showcase: Vec<frozen_city::net::protocol::ShowcaseEntry>,
    /// This account's own offline-guest setting (`ServerMsg::VisitPolicy`);
    /// `None` until the server reports it, i.e. for guest sessions.
    pub visit_policy: Option<bool>,
}

/// A world switch requested from inside the game (the Tunnel buttons in
/// `ui.rs`). Routed through a one-frame trip to `Screen::Menu` so the whole
/// game scene tears down and rebuilds for the new world — entity ids from the
/// old world mean nothing in the new one — then consumed by
/// `menu::pending_switch`, which dials and re-enters `Screen::Game`.
#[derive(Resource, Default)]
pub struct PendingSwitch(pub Option<WorldTarget>);

/// A short human-readable line describing the world switch in flight, set by
/// whichever button requests one alongside `PendingSwitch` (the two are set
/// together everywhere `PendingSwitch` is written — see `ui.rs`'s Tunnel/
/// world-switch buttons and `social.rs`'s Visit/Accept/Home buttons).
/// Rendered by `ui::transition_overlay` for a couple of seconds after the
/// new world loads — the actual dial+rebuild is a same-frame trip through
/// `Screen::Menu` (see `PendingSwitch`'s doc comment), too fast to read
/// anything shown only during it, so the message is carried forward and
/// faded out once the game scene is back up rather than shown during the
/// blink-and-you-miss-it menu frame itself.
#[derive(Resource, Default)]
pub struct TransitionMsg {
    pub text: Option<String>,
    /// Seconds since `Screen::Game` was (re-)entered with a message pending;
    /// reset to 0 in `render::enter_game` and counted up in
    /// `ui::transition_overlay`, which clears `text` once it expires.
    pub age: f32,
}

impl WorldTarget {
    /// The overlay line shown while switching to this target — phrased from
    /// the traveler's point of view, matching the task's examples.
    pub fn transition_label(self, friend_name: Option<&str>, l: i18n::Lang) -> String {
        use i18n::Lang;
        match self {
            WorldTarget::Personal => match l {
                Lang::Uz => "Shahringizga qaytilmoqda...",
                Lang::En => "Returning to your city...",
                Lang::Ru => "Возвращение в ваш город...",
            }
            .to_string(),
            WorldTarget::Central => match l {
                Lang::Uz => "Global Olamga kirilmoqda...",
                Lang::En => "Entering the Global World...",
                Lang::Ru => "Вход в Глобальный мир...",
            }
            .to_string(),
            WorldTarget::Visit(_) => match (friend_name, l) {
                (Some(name), Lang::Uz) => format!("{name}nikiga tashrif..."),
                (Some(name), Lang::En) => format!("Visiting {name}..."),
                (Some(name), Lang::Ru) => format!("Визит к {name}..."),
                (None, Lang::Uz) => "Tashrif...".to_string(),
                (None, Lang::En) => "Visiting...".to_string(),
                (None, Lang::Ru) => "Визит...".to_string(),
            },
        }
    }
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

/// Small inbox of freshly-issued `MoveSurvivor` destinations (tile coords),
/// drained by `render::spawn_move_ping` to spawn the brief confirmation ring.
/// Written by `input::build_input`/`touch::touch_control` right after they
/// send the command — the same "resource inbox" shape `SocialState::bubbles`
/// uses, picked over a Bevy `Message` type since no custom one exists here.
#[derive(Resource, Default)]
pub struct MoveOrderQueue(pub Vec<(u8, u8)>);

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
        BuildingKind::Greenhouse => Color::srgb(0.40, 0.68, 0.45),
        BuildingKind::Hospital => Color::srgb(0.86, 0.88, 0.92),
        BuildingKind::Kitchen => Color::srgb(0.78, 0.52, 0.30),
        BuildingKind::Warehouse => Color::srgb(0.58, 0.46, 0.32),
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
            .init_resource::<Session>()
            .init_resource::<PendingSwitch>()
            .init_resource::<TransitionMsg>()
            .init_resource::<SocialState>()
            .init_resource::<menu::LoginForm>()
            .init_resource::<net_sync::Reconnecting>()
            .init_resource::<BuildMode>()
            .init_resource::<Selection>()
            .init_resource::<UiHover>()
            .init_resource::<MoveOrderQueue>()
            .init_resource::<render::TerrainViz>()
            .init_resource::<render::BuildingViz>()
            .init_resource::<render::SurvivorViz>()
            .init_resource::<render::CursorViz>()
            .init_resource::<render::AvatarViz>()
            .init_resource::<render::PingViz>()
            .init_resource::<theme::FormFactor>()
            // Cyrillic-capable default font — must land before any UI text is
            // spawned, hence first in `Startup` (see `i18n::install_default_font`'s
            // doc comment for why `Startup` is early enough).
            .add_systems(Startup, i18n::install_default_font)
            // PreUpdate: StateTransition'dan (ya'ni barcha OnEnter spawn'lardan)
            // OLDIN ishlashi shart — aks holda avtostart (`?join`) 1-freymda
            // Screen::Game'ga kirganda FormFactor hali Default (Desktop) bo'lib,
            // mobil layout tarmoqlari umuman ishga tushmaydi.
            .add_systems(PreUpdate, theme::update_form_factor)
            .add_systems(Startup, render::setup_camera_and_assets)
            // Menu.
            .add_systems(OnEnter(Screen::Menu), (menu::spawn_menu, menu_fx::spawn_menu_fx))
            .add_systems(
                Update,
                (
                    menu::pending_switch,
                    menu::autostart,
                    menu::menu_buttons,
                    #[cfg(target_arch = "wasm32")]
                    menu::region_buttons,
                    menu::login_field_focus,
                    menu::login_form_keyboard,
                    menu::account_login_button,
                    menu::register_toggle_button,
                    menu::update_login_fields,
                    menu::update_register_toggle,
                    menu::lang_buttons,
                    menu::quality_buttons,
                    menu::audio_toggle_button,
                    menu::update_settings_buttons,
                    ui::generic_button_hover,
                )
                    .run_if(in_state(Screen::Menu)),
            )
            .add_systems(
                Update,
                menu_fx::snow_fall.run_if(in_state(Screen::Menu)),
            )
            // Game lifecycle.
            .add_systems(OnEnter(Screen::Game), (render::enter_game, ui::spawn_hud))
            .add_systems(OnExit(Screen::Game), (teardown_game, teardown_social))
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
                    render::animate_survivor_selection,
                    render::sync_leader_crown,
                    render::spawn_move_ping,
                    render::animate_move_pings,
                    render::animate_spawn,
                    render::animate_blizzard_overlay,
                    render::sync_player_cursors,
                    render::update_cursor_labels,
                    render::sync_avatars,
                    render::animate_avatars,
                    render::update_avatar_labels,
                    render::sync_pings,
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
                    ui::world_switch_button,
                    ui::transition_overlay,
                    ui::generic_button_hover,
                    net_sync::watch_disconnect,
                )
                    .run_if(in_state(Screen::Game)),
            )
            .add_systems(Update, smoke_exit);
        app.add_plugins(chat::plugin);
        app.add_plugins(minimap::plugin);
        app.add_plugins(roles::plugin);
        app.add_plugins(roster::plugin);
        app.add_plugins(missions::plugin);
        app.add_plugins(research::plugin);
        app.add_plugins(events::plugin);
        app.add_plugins(social::plugin);
        app.add_plugins(audio::plugin);
        #[cfg(target_arch = "wasm32")]
        app.add_plugins(local_server::plugin);
    }
}

/// Leaving the game: close the connection, stop a locally hosted server and
/// reset per-session resources. Entities die via `DespawnOnExit`.
#[allow(clippy::too_many_arguments)]
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
    mut pings: ResMut<render::PingViz>,
    mut chat: ResMut<chat::ChatState>,
    mut reconnecting: ResMut<net_sync::Reconnecting>,
    mut research: ResMut<research::ResearchOpen>,
    mut roster_open: ResMut<roster::RosterOpen>,
    mut roster_sel: ResMut<roster::SurvivorSelection>,
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
    *pings = Default::default();
    // Close any half-composed chat box so a new game doesn't start with the
    // input open (which would silently swallow world/camera input).
    *chat = Default::default();
    // Cancel any in-flight reconnect dial so its (stale) result can never be
    // installed into the next game.
    *reconnecting = Default::default();
    // Close the research modal so a new game doesn't start with it stuck open
    // (which would silently swallow world/camera input, same as chat).
    *research = Default::default();
    *roster_open = Default::default();
    *roster_sel = Default::default();
}

/// Split out of `teardown_game` purely because Bevy caps how many
/// `SystemParam`s a single system function may take (~16) and that one was
/// already at the limit — same reset-per-session purpose, just a second
/// system on the same `OnExit(Screen::Game)` schedule.
fn teardown_social(
    mut avatars: ResMut<render::AvatarViz>,
    mut social_open: ResMut<social::SocialOpen>,
) {
    *avatars = Default::default();
    // Close the social modal so a new game doesn't start with it stuck open
    // (which would silently swallow world/camera input, same as chat/research).
    *social_open = Default::default();
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
