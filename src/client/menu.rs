//! Main menu: singleplayer, host co-op, join, quit.

use std::sync::Mutex;

use bevy::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
use frozen_city::net::server::{self, ServerConfig};

use super::ui::BaseColor;
use super::*;

const BTN_BG: Color = Color::srgb(0.14, 0.19, 0.27);
const TEXT_MAIN: Color = Color::srgb(0.90, 0.93, 0.97);
const TEXT_DIM: Color = Color::srgb(0.58, 0.65, 0.76);

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

pub fn spawn_menu(mut commands: Commands, settings: Res<Settings>, view: Res<GameView>) {
    let error = view.error.clone().unwrap_or_default();

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(14.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.035, 0.055, 0.095)),
            DespawnOnExit(Screen::Menu),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("FROZEN CITY"),
                TextFont::from_font_size(58.0),
                TextColor(Color::srgb(0.72, 0.86, 1.0)),
            ));
            p.spawn((
                Text::new("A cooperative survival colony in the endless winter"),
                TextFont::from_font_size(16.0),
                TextColor(TEXT_DIM),
            ));
            p.spawn((
                Text::new(error),
                TextFont::from_font_size(15.0),
                TextColor(Color::srgb(0.95, 0.40, 0.35)),
                MenuErrorText,
            ));

            // The browser cannot listen for connections or quit the page, so
            // it only offers Singleplayer and Join.
            let mut buttons: Vec<(MenuAction, String)> =
                vec![(MenuAction::Single, "Singleplayer".to_string())];
            #[cfg(not(target_arch = "wasm32"))]
            buttons.push((
                MenuAction::Host,
                format!("Host Co-op (port {})", settings.host_port),
            ));
            buttons.push((MenuAction::Join, format!("Join {}", settings.join_addr)));
            #[cfg(not(target_arch = "wasm32"))]
            buttons.push((MenuAction::Quit, "Quit".to_string()));
            for (action, label) in buttons {
                p.spawn((
                    Button,
                    Node {
                        width: Val::Px(300.0),
                        height: Val::Px(52.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(BTN_BG),
                    BaseColor(BTN_BG),
                    action,
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new(label),
                        TextFont::from_font_size(17.0),
                        TextColor(TEXT_MAIN),
                    ));
                });
            }

            p.spawn((
                Text::new(format!(
                    "Playing as {}   |   survive {} days   |   change with --name / --days / --join <ip:port>",
                    settings.name, settings.win_days
                )),
                TextFont::from_font_size(13.0),
                TextColor(TEXT_DIM),
            ));
            p.spawn((
                Text::new(
                    "In game: LMB place/select   RMB cancel   1-4 quick build   WASD pan   Q/E rotate   MMB tilt   wheel zoom",
                ),
                TextFont::from_font_size(13.0),
                TextColor(TEXT_DIM),
            ));
        });
}

/// Handle `--host`, `--join` and `--smoke`: act once, straight from the menu.
pub fn autostart(
    mut auto: ResMut<AutoStart>,
    settings: Res<Settings>,
    mut net: ResMut<NetConn>,
    mut server_res: ResMut<ServerRes>,
    mut view: ResMut<GameView>,
    mut next: ResMut<NextState<Screen>>,
    mut error_text: Query<&mut Text, With<MenuErrorText>>,
) {
    let Some(action) = auto.0.take() else { return };
    let result = start_game(action, &settings, &mut net, &mut server_res, &mut view);
    match result {
        Ok(()) => next.set(Screen::Game),
        Err(e) => {
            if let Ok(mut t) = error_text.single_mut() {
                t.0 = e;
            }
        }
    }
}

pub fn menu_buttons(
    q: Query<(&Interaction, &MenuAction), Changed<Interaction>>,
    settings: Res<Settings>,
    mut net: ResMut<NetConn>,
    mut server_res: ResMut<ServerRes>,
    mut view: ResMut<GameView>,
    mut next: ResMut<NextState<Screen>>,
    mut error_text: Query<&mut Text, With<MenuErrorText>>,
    mut exit: MessageWriter<AppExit>,
) {
    for (interaction, action) in &q {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let auto = match action {
            MenuAction::Single => AutoAction::Single,
            MenuAction::Host => AutoAction::Host,
            MenuAction::Join => AutoAction::Join,
            MenuAction::Quit => {
                exit.write(AppExit::Success);
                return;
            }
        };
        match start_game(auto, &settings, &mut net, &mut server_res, &mut view) {
            Ok(()) => next.set(Screen::Game),
            Err(e) => {
                if let Ok(mut t) = error_text.single_mut() {
                    t.0 = e;
                }
            }
        }
        return;
    }
}

fn start_game(
    action: AutoAction,
    settings: &Settings,
    net: &mut NetConn,
    server_res: &mut ServerRes,
    view: &mut GameView,
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
                };
                let handle = server::start(config)
                    .map_err(|e| format!("Could not start the server: {e}"))?;
                let conn = server::connect_local(&handle, settings.name.clone());
                server_res.0 = Some(handle);
                conn
            };
            #[cfg(target_arch = "wasm32")]
            let conn = {
                if action == AutoAction::Host {
                    return Err(
                        "Hosting runs on desktop or a dedicated server; the browser can only join."
                            .to_string(),
                    );
                }
                let (local, conn) =
                    super::local_server::start(seed, settings.win_days, &settings.name);
                server_res.0 = Some(local);
                conn
            };
            conn
        }
        AutoAction::Join => {
            #[cfg(not(target_arch = "wasm32"))]
            let conn = frozen_city::net::client::connect_tcp(&settings.join_addr, &settings.name)
                .map_err(|e| format!("Could not join {}: {e}", settings.join_addr))?;
            #[cfg(target_arch = "wasm32")]
            let conn = frozen_city::net::ws::connect(&ws_url(&settings.join_addr), &settings.name)
                .map_err(|e| format!("Could not join {}: {e}", settings.join_addr))?;
            conn
        }
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
fn ws_url(addr: &str) -> String {
    if addr.starts_with("ws://") || addr.starts_with("wss://") {
        addr.to_string()
    } else {
        format!("ws://{addr}")
    }
}
