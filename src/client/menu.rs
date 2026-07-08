//! Main menu: singleplayer, host co-op, join, quit.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;

use frozen_city::net::{client, server};
use frozen_city::net::server::ServerConfig;

use super::ui::BaseColor;
use super::*;

const BTN_BG: Color = Color::srgb(0.14, 0.19, 0.27);
const TEXT_MAIN: Color = Color::srgb(0.90, 0.93, 0.97);
const TEXT_DIM: Color = Color::srgb(0.58, 0.65, 0.76);

#[derive(Component, Clone, Copy, PartialEq)]
pub enum MenuAction {
    Single,
    Host,
    Join,
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

            let buttons: [(MenuAction, String); 4] = [
                (MenuAction::Single, "Singleplayer".to_string()),
                (
                    MenuAction::Host,
                    format!("Host Co-op (port {})", settings.host_port),
                ),
                (
                    MenuAction::Join,
                    format!("Join {}", settings.join_addr),
                ),
                (MenuAction::Quit, "Quit".to_string()),
            ];
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
                    "In game: LMB place/select   RMB cancel   1-4 quick build   WASD pan   wheel zoom",
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
        }
        AutoAction::Join => client::connect_tcp(&settings.join_addr, &settings.name)
            .map_err(|e| format!("Could not join {}: {e}", settings.join_addr))?,
    };
    *view = GameView::default();
    net.0 = Some(Mutex::new(conn));
    Ok(())
}

fn random_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64 ^ 0x9E37_79B9_7F4A_7C15)
        .unwrap_or(0xC0FFEE)
}
