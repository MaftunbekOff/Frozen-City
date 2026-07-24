//! Entry points into a game: launching from the menu buttons or CLI
//! autostart (`start_game`), and completing an in-flight in-game world
//! switch requested from inside the game (the Tunnel, see `PendingSwitch`).

use std::sync::Mutex;

use bevy::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
use frozen_city::net::server::{self, ServerConfig};
use frozen_city::net::protocol::ClientMsg;

use super::super::i18n::Lang;
use super::super::i18n_menu as mtxt;
use super::super::{
    AutoAction, AutoStart, GameView, NetConn, PendingSwitch, Screen, ServerRes, Session, Settings,
    WorldTarget,
};

#[derive(Component, Clone, Copy, PartialEq)]
pub enum MenuAction {
    Single,
    // Host and Quit exist on desktop only, but the enum stays uniform.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    Host,
    Join,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    Quit,
}

#[derive(Component)]
pub struct MenuErrorText;

/// The menu landing's link to the project briefing page
/// (`web/investors.html`, served alongside the game itself) — native opens it
/// in the system browser, wasm navigates the current tab (its own "Back to
/// Frozen City" link returns here). See `buttons::presentation_button`.
#[derive(Component)]
pub struct PresentationButton;

/// Completes an in-game world switch (Tunnel → central world, or back): the
/// game screen has just torn down, so dial the target world and re-enter
/// `Screen::Game` immediately — the player sees one menu frame, not a menu.
/// Runs before the other menu systems so nothing else reacts to that frame.
pub fn pending_switch(
    mut pending: ResMut<PendingSwitch>,
    mut net: ResMut<NetConn>,
    mut view: ResMut<GameView>,
    mut session: ResMut<Session>,
    mut next: ResMut<NextState<Screen>>,
    mut error_text: Query<&mut Text, With<MenuErrorText>>,
    lang: Res<Lang>,
) {
    let Some(target) = pending.0.take() else { return };
    // Only account sessions can switch worlds; a guest has neither a personal
    // world nor Tunnel access. The buttons are hidden for guests, so this is
    // just a stale-state guard.
    let Some(auth) = session.auth.clone() else { return };
    let central = target == WorldTarget::Central;
    let visiting = match target {
        WorldTarget::Visit(host) => Some(host),
        _ => None,
    };
    let first_msg = match target {
        WorldTarget::Central => ClientMsg::EnterCentral {
            login: auth.login.clone(),
            password: auth.password.clone(),
            token: None,
        },
        WorldTarget::Visit(host) => ClientMsg::VisitFriend {
            login: auth.login.clone(),
            password: auth.password.clone(),
            host,
            token: None,
        },
        WorldTarget::Personal => ClientMsg::Login {
            login: auth.login.clone(),
            password: auth.password.clone(),
            token: None,
        },
    };
    #[cfg(not(target_arch = "wasm32"))]
    let dialed = frozen_city::net::client::connect_tcp_with(&session.join_addr, first_msg)
        .map_err(|e| mtxt::err_could_not_join(*lang, &session.join_addr, &e.to_string()));
    #[cfg(target_arch = "wasm32")]
    let dialed = frozen_city::net::ws::connect_with(&ws_url(&session.join_addr), first_msg)
        .map_err(|e| mtxt::err_could_not_join(*lang, &session.join_addr, &e.to_string()));
    match dialed {
        Ok(conn) => {
            // A token belongs to the world that minted it; the new world
            // issues its own in the coming `Welcome`.
            session.token = None;
            session.attempts = 0;
            session.reconnectable = true;
            session.central = central;
            session.visiting = visiting;
            *view = GameView::default();
            net.0 = Some(Mutex::new(conn));
            next.set(Screen::Game);
        }
        Err(e) => {
            for mut t in &mut error_text {
                t.0 = e.clone();
            }
        }
    }
}

/// Handle `--host`, `--join` and `--smoke`: act once, straight from the menu.
pub fn autostart(
    mut auto: ResMut<AutoStart>,
    settings: Res<Settings>,
    mut net: ResMut<NetConn>,
    mut server_res: ResMut<ServerRes>,
    mut view: ResMut<GameView>,
    mut session: ResMut<Session>,
    mut next: ResMut<NextState<Screen>>,
    mut error_text: Query<&mut Text, With<MenuErrorText>>,
    lang: Res<Lang>,
) {
    let Some(action) = auto.0.take() else { return };
    let result =
        start_game(action, &settings, &mut net, &mut server_res, &mut view, &mut session, *lang);
    match result {
        Ok(()) => next.set(Screen::Game),
        Err(e) => {
            for mut t in &mut error_text {
                t.0 = e.clone();
            }
        }
    }
}

pub(crate) fn start_game(
    action: AutoAction,
    settings: &Settings,
    net: &mut NetConn,
    server_res: &mut ServerRes,
    view: &mut GameView,
    session: &mut Session,
    lang: Lang,
) -> Result<(), String> {
    let conn = match action {
        AutoAction::Single | AutoAction::Host => {
            let seed = settings.seed.unwrap_or_else(random_seed);
            #[cfg(not(target_arch = "wasm32"))]
            let conn = {
                let config = ServerConfig {
                    port: (action == AutoAction::Host).then_some(settings.host_port),
                    seed,
                    win_days: settings.win_days,
                    persistent: false,
                    verbose: false,
                    save_path: None,
                    idle_shutdown: None,
                    central: false,
                    owner_account: None,
                    invites: None,
                    // Cross-world invite delivery only matters for the
                    // central world; local host/singleplayer has neither.
                    world_manager: None,
                };
                let handle = server::start(config)
                    .map_err(|e| mtxt::err_could_not_start_server(lang, &e.to_string()))?;
                let conn = server::connect_local(&handle, settings.name.clone());
                server_res.0 = Some(handle);
                conn
            };
            #[cfg(target_arch = "wasm32")]
            let conn = {
                if action == AutoAction::Host {
                    return Err(mtxt::err_hosting_desktop_only(lang).to_string());
                }
                let (local, conn) =
                    super::super::local_server::start(seed, settings.win_days, &settings.name);
                server_res.0 = Some(local);
                conn
            };
            conn
        }
        AutoAction::Join => {
            #[cfg(not(target_arch = "wasm32"))]
            let conn =
                frozen_city::net::client::connect_tcp(&settings.join_addr, &settings.name, None)
                    .map_err(|e| mtxt::err_could_not_join(lang, &settings.join_addr, &e.to_string()))?;
            #[cfg(target_arch = "wasm32")]
            let conn =
                frozen_city::net::ws::connect(&ws_url(&settings.join_addr), &settings.name, None)
                    .map_err(|e| mtxt::err_could_not_join(lang, &settings.join_addr, &e.to_string()))?;
            conn
        }
    };
    // Only a remote join can be transparently re-dialed after a drop; host and
    // singleplayer worlds live in-process and vanish with the connection.
    *session = Session {
        join_addr: settings.join_addr.clone(),
        name: settings.name.clone(),
        auth: None,
        token: None,
        reconnectable: action == AutoAction::Join,
        attempts: 0,
        central: false,
        visiting: None,
    };
    *view = GameView::default();
    net.0 = Some(Mutex::new(conn));
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn random_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64 ^ 0x9E37_79B9_7F4A_7C15)
        .unwrap_or(0xC0FFEE)
}

/// `SystemTime` panics on wasm32-unknown-unknown; use the JS clock instead.
#[cfg(target_arch = "wasm32")]
fn random_seed() -> u64 {
    (js_sys::Date::now() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

/// Accept both bare `host:port` and full `ws(s)://` URLs in the join field.
#[cfg(target_arch = "wasm32")]
pub(crate) fn ws_url(addr: &str) -> String {
    if addr.starts_with("ws://") || addr.starts_with("wss://") {
        addr.to_string()
    } else {
        format!("ws://{addr}")
    }
}
