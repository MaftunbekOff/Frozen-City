//! Main menu: singleplayer, host co-op, join, quit.

use std::sync::Mutex;

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
use frozen_city::net::server::{self, ServerConfig};
use frozen_city::net::protocol::ClientMsg;

use super::ui::BaseColor;
use super::*;

const BTN_BG: Color = Color::srgb(0.14, 0.19, 0.27);
const TEXT_MAIN: Color = Color::srgb(0.90, 0.93, 0.97);
const TEXT_DIM: Color = Color::srgb(0.58, 0.65, 0.76);
const FIELD_BG: Color = Color::srgba(0.02, 0.04, 0.08, 0.92);
const FIELD_FOCUS_BG: Color = Color::srgb(0.10, 0.16, 0.26);
/// Highlight for whichever region button matches the live `join_addr` path.
#[cfg(target_arch = "wasm32")]
const BTN_ACTIVE: Color = Color::srgb(0.24, 0.36, 0.52);
/// Anti-runaway-buffer cap on the account fields; the server never trusts
/// the client anyway (a login/password pair is just looked up against the
/// accounts DB, whatever their length).
const MAX_FIELD_LEN: usize = 32;

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

/// One of the region-server picker buttons (browser build only). Holds the
/// `/ws`-style path this button dials; ops routes each path to an
/// independent region-server process at the nginx layer.
#[cfg(target_arch = "wasm32")]
#[derive(Component, Clone, Copy, PartialEq)]
pub struct RegionButton(&'static str);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AccountField {
    Login,
    Password,
}

/// Text typed into the account sign-in fields on the menu, and which of the
/// two fields (if any) currently has keyboard focus. A click on a field sets
/// focus; typing then goes to that field. Not tied to `Screen::Menu` entities
/// (survives a menu respawn) so a failed login leaves the fields populated.
#[derive(Resource, Default)]
pub struct LoginForm {
    pub login: String,
    pub password: String,
    pub focus: Option<AccountField>,
}

#[derive(Component)]
pub(crate) struct LoginFieldBox(AccountField);

#[derive(Component)]
pub(crate) struct LoginFieldText(AccountField);

#[derive(Component)]
pub(crate) struct AccountLoginButton;

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
            buttons.push((
                MenuAction::Join,
                format!("Mehmon sifatida: Join {}", settings.join_addr),
            ));
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

            // Region picker: which reverse-proxy path (and thus which
            // independent region-server process) Join/Kirish dial. Native
            // desktop only ever dials a LAN address directly, so this row
            // is browser-only.
            #[cfg(target_arch = "wasm32")]
            {
                p.spawn((
                    Text::new("Mintaqa:"),
                    TextFont::from_font_size(13.0),
                    TextColor(TEXT_DIM),
                ));
                p.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(8.0),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    for (path, label) in [
                        ("/ws", "1-mintaqa"),
                        ("/ws-r2", "2-mintaqa"),
                        ("/ws-r3", "3-mintaqa"),
                    ] {
                        row.spawn((
                            Button,
                            Node {
                                width: Val::Px(90.0),
                                height: Val::Px(36.0),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            BackgroundColor(BTN_BG),
                            RegionButton(path),
                        ))
                        .with_children(|b| {
                            b.spawn((
                                Text::new(label),
                                TextFont::from_font_size(14.0),
                                TextColor(TEXT_MAIN),
                            ));
                        });
                    }
                });
            }

            p.spawn((
                Text::new("Yoki akkaunt bilan kiring (Login/Parol — Telegram bot orqali ro'yxatdan o'ting):"),
                TextFont::from_font_size(13.0),
                TextColor(TEXT_DIM),
            ));
            p.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(8.0),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                for field in [AccountField::Login, AccountField::Password] {
                    row.spawn((
                        Button,
                        Node {
                            width: Val::Px(150.0),
                            height: Val::Px(40.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(FIELD_BG),
                        LoginFieldBox(field),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            Text::new(""),
                            TextFont::from_font_size(14.0),
                            TextColor(TEXT_DIM),
                            LoginFieldText(field),
                        ));
                    });
                }
                row.spawn((
                    Button,
                    Node {
                        width: Val::Px(100.0),
                        height: Val::Px(40.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(BTN_BG),
                    BaseColor(BTN_BG),
                    AccountLoginButton,
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new("Kirish"),
                        TextFont::from_font_size(15.0),
                        TextColor(TEXT_MAIN),
                    ));
                });
            });

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
                    "In game: LMB place/select   RMB cancel   1-7 quick build   WASD pan   Q/E rotate   MMB tilt   wheel zoom",
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
    mut session: ResMut<Session>,
    mut next: ResMut<NextState<Screen>>,
    mut error_text: Query<&mut Text, With<MenuErrorText>>,
) {
    let Some(action) = auto.0.take() else { return };
    let result = start_game(action, &settings, &mut net, &mut server_res, &mut view, &mut session);
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
    mut session: ResMut<Session>,
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
        match start_game(auto, &settings, &mut net, &mut server_res, &mut view, &mut session) {
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

/// Click handling and per-frame active-region highlight for the region
/// picker, combined in one system (same shape as `ui::build_buttons`). A
/// click rewrites only the path component of `settings.join_addr`, so it
/// keeps working under a custom `?server=` host and under a `/game/`-style
/// mount alike.
#[cfg(target_arch = "wasm32")]
pub fn region_buttons(
    clicked: Query<(&Interaction, &RegionButton), Changed<Interaction>>,
    mut settings: ResMut<Settings>,
    mut all: Query<(&RegionButton, &mut BackgroundColor)>,
) {
    for (interaction, btn) in &clicked {
        if *interaction == Interaction::Pressed {
            settings.join_addr = with_path(&settings.join_addr, btn.0);
        }
    }
    for (btn, mut bg) in &mut all {
        let color = if settings.join_addr.ends_with(btn.0) {
            BTN_ACTIVE
        } else {
            BTN_BG
        };
        if bg.0 != color {
            bg.0 = color;
        }
    }
}

/// Replaces everything after `scheme://host` in `addr` with `path`,
/// leaving the scheme and host (and thus any custom `?server=` override)
/// untouched.
#[cfg(target_arch = "wasm32")]
fn with_path(addr: &str, path: &str) -> String {
    let after_scheme = addr.find("://").map(|i| i + 3).unwrap_or(0);
    let prefix_end = addr[after_scheme..]
        .find('/')
        .map(|i| after_scheme + i)
        .unwrap_or(addr.len());
    format!("{}{path}", &addr[..prefix_end])
}

fn start_game(
    action: AutoAction,
    settings: &Settings,
    net: &mut NetConn,
    server_res: &mut ServerRes,
    view: &mut GameView,
    session: &mut Session,
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
            let conn =
                frozen_city::net::client::connect_tcp(&settings.join_addr, &settings.name, None)
                    .map_err(|e| format!("Could not join {}: {e}", settings.join_addr))?;
            #[cfg(target_arch = "wasm32")]
            let conn =
                frozen_city::net::ws::connect(&ws_url(&settings.join_addr), &settings.name, None)
                    .map_err(|e| format!("Could not join {}: {e}", settings.join_addr))?;
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
    };
    *view = GameView::default();
    net.0 = Some(Mutex::new(conn));
    Ok(())
}

/// Click a login/password field to give it keyboard focus.
pub fn login_field_focus(
    q: Query<(&Interaction, &LoginFieldBox), Changed<Interaction>>,
    mut form: ResMut<LoginForm>,
) {
    for (interaction, field) in &q {
        if *interaction == Interaction::Pressed {
            form.focus = Some(field.0);
        }
    }
}

/// Capture keystrokes into whichever account field has focus. Tab swaps
/// focus between the two fields, Escape clears it, Enter submits (same as
/// clicking Kirish).
#[allow(clippy::too_many_arguments)]
pub fn login_form_keyboard(
    mut events: MessageReader<KeyboardInput>,
    mut form: ResMut<LoginForm>,
    settings: Res<Settings>,
    mut net: ResMut<NetConn>,
    mut view: ResMut<GameView>,
    mut session: ResMut<Session>,
    mut next: ResMut<NextState<Screen>>,
    mut error_text: Query<&mut Text, With<MenuErrorText>>,
) {
    let Some(focus) = form.focus else {
        return;
    };
    for ev in events.read() {
        if ev.state != ButtonState::Pressed {
            continue;
        }
        match &ev.logical_key {
            Key::Tab => {
                form.focus = Some(match focus {
                    AccountField::Login => AccountField::Password,
                    AccountField::Password => AccountField::Login,
                });
            }
            Key::Enter if !ev.repeat => {
                match submit_login(&form, &settings, &mut net, &mut view, &mut session) {
                    Ok(()) => next.set(Screen::Game),
                    Err(e) => {
                        if let Ok(mut t) = error_text.single_mut() {
                            t.0 = e;
                        }
                    }
                }
            }
            Key::Escape if !ev.repeat => {
                form.focus = None;
            }
            Key::Backspace => {
                match focus {
                    AccountField::Login => form.login.pop(),
                    AccountField::Password => form.password.pop(),
                };
            }
            Key::Character(s) => {
                let len = match focus {
                    AccountField::Login => form.login.chars().count(),
                    AccountField::Password => form.password.chars().count(),
                };
                if len < MAX_FIELD_LEN {
                    for c in s.chars() {
                        if !c.is_control() {
                            match focus {
                                AccountField::Login => form.login.push(c),
                                AccountField::Password => form.password.push(c),
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// The "Kirish" button — same submit path as pressing Enter in a field.
#[allow(clippy::too_many_arguments)]
pub fn account_login_button(
    q: Query<&Interaction, (With<AccountLoginButton>, Changed<Interaction>)>,
    form: Res<LoginForm>,
    settings: Res<Settings>,
    mut net: ResMut<NetConn>,
    mut view: ResMut<GameView>,
    mut session: ResMut<Session>,
    mut next: ResMut<NextState<Screen>>,
    mut error_text: Query<&mut Text, With<MenuErrorText>>,
) {
    for interaction in &q {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match submit_login(&form, &settings, &mut net, &mut view, &mut session) {
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

/// Dials `settings.join_addr` with an account `Login` instead of a guest
/// `Hello`. The actual accept/reject happens server-side (`ServerMsg::
/// AuthFailed` if the login/password don't match); this only rejects an
/// obviously-empty form before spending a connection attempt.
fn submit_login(
    form: &LoginForm,
    settings: &Settings,
    net: &mut NetConn,
    view: &mut GameView,
    session: &mut Session,
) -> Result<(), String> {
    let login = form.login.trim();
    let password = form.password.trim();
    if login.is_empty() || password.is_empty() {
        return Err("Login va parolni kiriting.".to_string());
    }
    let hello = ClientMsg::Login {
        login: login.to_string(),
        password: password.to_string(),
        token: None,
    };
    #[cfg(not(target_arch = "wasm32"))]
    let conn = frozen_city::net::client::connect_tcp_with(&settings.join_addr, hello)
        .map_err(|e| format!("Could not join {}: {e}", settings.join_addr))?;
    #[cfg(target_arch = "wasm32")]
    let conn = frozen_city::net::ws::connect_with(&ws_url(&settings.join_addr), hello)
        .map_err(|e| format!("Could not join {}: {e}", settings.join_addr))?;

    *session = Session {
        join_addr: settings.join_addr.clone(),
        name: settings.name.clone(),
        auth: Some(AccountAuth {
            login: login.to_string(),
            password: password.to_string(),
        }),
        token: None,
        reconnectable: true,
        attempts: 0,
    };
    *view = GameView::default();
    net.0 = Some(Mutex::new(conn));
    Ok(())
}

/// Reflects `LoginForm` onto the field boxes/text every frame: focus tint,
/// typed value (password masked), and an idle placeholder when empty.
pub fn update_login_fields(
    form: Res<LoginForm>,
    mut boxes: Query<(&LoginFieldBox, &mut BackgroundColor)>,
    mut texts: Query<(&LoginFieldText, &mut Text, &mut TextColor)>,
) {
    for (marker, mut bg) in &mut boxes {
        let target = if form.focus == Some(marker.0) {
            FIELD_FOCUS_BG
        } else {
            FIELD_BG
        };
        if bg.0 != target {
            bg.0 = target;
        }
    }
    for (marker, mut text, mut color) in &mut texts {
        let (value, masked, placeholder) = match marker.0 {
            AccountField::Login => (form.login.as_str(), false, "Login"),
            AccountField::Password => (form.password.as_str(), true, "Parol"),
        };
        let focused = form.focus == Some(marker.0);
        let (new, c) = if value.is_empty() && !focused {
            (placeholder.to_string(), TEXT_DIM)
        } else {
            let shown = if masked {
                "*".repeat(value.chars().count())
            } else {
                value.to_string()
            };
            let cursor = if focused { "_" } else { "" };
            (format!("{shown}{cursor}"), TEXT_MAIN)
        };
        if text.0 != new {
            text.0 = new;
        }
        if color.0 != c {
            color.0 = c;
        }
    }
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
