//! Main menu: singleplayer, host co-op, join, quit.

use std::sync::Mutex;

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
use frozen_city::net::server::{self, ServerConfig};
use frozen_city::net::protocol::ClientMsg;

use super::i18n::{self, Lang};
use super::i18n_menu as mtxt;
use super::theme::{self, BaseColor, FormFactor};
use super::*;

/// Vertical section card for the menu (Play/Account/Region/Settings): unlike
/// `theme::card()` (a horizontal list-row idiom used by `research.rs`), each
/// menu section stacks a header row on top of its content, and its content
/// must stay within the card's own width — a plain `theme::card()` here would
/// lay the section label and its content out side-by-side, pushing content
/// off the right edge of the card (and off-screen on Mobile).
fn menu_section() -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Column,
            width: Val::Percent(100.0),
            padding: UiRect::axes(Val::Px(theme::SP_MD), Val::Px(theme::SP_SM)),
            row_gap: Val::Px(theme::SP_SM),
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
            ..default()
        },
        BackgroundColor(theme::BG_SECTION),
        BorderColor::all(theme::BORDER),
        BoxShadow::new(
            Color::srgba(0.0, 0.0, 0.0, 0.35),
            Val::Px(0.0),
            Val::Px(6.0),
            Val::Px(0.0),
            Val::Px(16.0),
        ),
    )
}

/// A wrapping content row inside a `menu_section` (e.g. the Language or
/// Graphics button group): full card width, and `FlexWrap::Wrap` so a long
/// label/localization or a narrow (Mobile) card reflows onto a second line
/// instead of spilling past the card's right edge.
fn menu_row() -> Node {
    Node {
        width: Val::Percent(100.0),
        flex_wrap: FlexWrap::Wrap,
        column_gap: Val::Px(theme::SP_SM),
        row_gap: Val::Px(theme::SP_SM),
        ..default()
    }
}

/// Field-box background: idle / focused. Distinct from `theme::BG_SECTION`
/// so the account fields still read as inputs rather than cards.
const FIELD_BG: Color = Color::srgba(0.020, 0.040, 0.080, 0.92);
const FIELD_FOCUS_BG: Color = Color::srgb(0.100, 0.160, 0.260);
/// Highlight for whichever region button matches the live `join_addr` path,
/// and (native and web alike) whichever Language/Graphics/Sound setting
/// button matches the current preference.
const BTN_ACTIVE: Color = theme::BTN_ACTIVE;
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
    /// Desired display name — only shown/used in `RegisterMode::Register`.
    Name,
}

/// Text typed into the account sign-in fields on the menu, and which of the
/// two fields (if any) currently has keyboard focus. A click on a field sets
/// focus; typing then goes to that field. Not tied to `Screen::Menu` entities
/// (survives a menu respawn) so a failed login leaves the fields populated.
#[derive(Resource, Default)]
pub struct LoginForm {
    pub login: String,
    pub password: String,
    /// Desired display name, register mode only.
    pub name: String,
    pub focus: Option<AccountField>,
    /// Toggles the sign-in row between "Kirish" (Login) and "Ro'yxatdan
    /// o'tish" (Register) — both post through the same three text fields,
    /// only the button label/behavior and the visibility of the Name field
    /// change. Kept on `LoginForm` (not a separate resource) so it survives
    /// a menu respawn the same way the field text does.
    pub register: bool,
}

#[derive(Component)]
pub(crate) struct LoginFieldBox(AccountField);

#[derive(Component)]
pub(crate) struct LoginFieldText(AccountField);

#[derive(Component)]
pub(crate) struct AccountLoginButton;

#[derive(Component)]
pub(crate) struct AccountLoginButtonLabel;

/// Switches `LoginForm::register` between sign-in and create-account mode.
#[derive(Component)]
pub(crate) struct RegisterToggleButton;

#[derive(Component)]
pub(crate) struct RegisterToggleLabel;

/// Marks the single despawn/respawn root of the whole menu screen, so a
/// settings change that needs the menu rebuilt (currently: language) can find
/// and despawn it without relying on `DespawnOnExit(Screen::Menu)` (which only
/// fires on an actual state exit, not a same-state respawn).
#[derive(Component)]
pub(crate) struct MenuRoot;

/// One of the three language-picker buttons in the settings block.
#[derive(Component, Clone, Copy, PartialEq)]
pub(crate) struct LangButton(pub Lang);

/// One of the four graphics-quality-picker buttons in the settings block.
#[derive(Component, Clone, Copy, PartialEq)]
pub(crate) struct QualityButton(pub QualityPref);

/// The Sound on/off toggle in the settings block.
#[derive(Component)]
pub(crate) struct AudioToggleButton;

#[derive(Component)]
pub(crate) struct AudioToggleLabel;

pub fn spawn_menu(
    commands: Commands,
    settings: Res<Settings>,
    view: Res<GameView>,
    lang: Res<Lang>,
    quality_pref: Res<QualityPref>,
    audio: Res<AudioSettings>,
    ff: Res<FormFactor>,
) {
    let error = view.error.clone().unwrap_or_default();
    build_menu(commands, &settings, error, *lang, *quality_pref, *audio, *ff);
}

/// The actual menu layout, factored out of the `spawn_menu` system so
/// `lang_buttons` can rebuild it too (from a click handler, it already holds
/// plain values/`&mut`s, not fresh `Res<T>` system params — `Res`/`ResMut`
/// can only be obtained by the scheduler injecting them into a system's
/// signature, not constructed by hand). Both callers pass an owned/copied
/// snapshot of each resource, never the resource references themselves.
///
/// Layout: a single scrolling column (full-width on Mobile, a centered
/// ~480px card on Tablet/Desktop) — big title, subtitle, error line, then
/// four `theme::card` sections (Play / Account / Region / Settings) and a
/// dim hint footer. Region only appears on wasm (see `RegionButton`'s doc).
#[allow(clippy::too_many_arguments)]
fn build_menu(
    mut commands: Commands,
    settings: &Settings,
    error: String,
    lang: Lang,
    quality_pref: QualityPref,
    audio: AudioSettings,
    ff: FormFactor,
) {
    let column_width = if ff.compact() { Val::Percent(100.0) } else { Val::Px(480.0) };
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::Center,
                overflow: Overflow::scroll_y(),
                ..default()
            },
            // Fon menu_fx gradient qatlamidan keladi (GlobalZIndex -2).
            BackgroundColor(Color::NONE),
            ScrollPosition::default(),
            DespawnOnExit(Screen::Menu),
            MenuRoot,
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: column_width,
                    max_width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Stretch,
                    padding: UiRect::all(Val::Px(theme::SP_MD)).with_top(Val::Px(theme::SP_XL)),
                    row_gap: Val::Px(theme::SP_LG),
                    ..default()
                },
            ))
            .with_children(|col| {
                col.spawn(Node {
                    align_items: AlignItems::Center,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(theme::SP_XS),
                    ..default()
                })
                .with_children(|head| {
                    head.spawn((
                        theme::text(
                            mtxt::title(lang),
                            if ff.compact() { 44.0 } else { 54.0 },
                            theme::TEXT_PRIMARY,
                        ),
                        // Muzli "porlash": pastga siljigan ko'k soya.
                        TextShadow {
                            offset: Vec2::new(0.0, 4.0),
                            color: Color::srgba(0.15, 0.55, 0.90, 0.35),
                        },
                    ));
                    head.spawn((
                        Node {
                            width: Val::Px(64.0),
                            height: Val::Px(3.0),
                            margin: UiRect::vertical(Val::Px(theme::SP_XS)),
                            border_radius: BorderRadius::MAX,
                            ..default()
                        },
                        BackgroundColor(theme::ACCENT_ICE),
                    ));
                    head.spawn(theme::text(mtxt::subtitle(lang), theme::FS_BODY, theme::ACCENT_ICE));
                    head.spawn((theme::text(error, theme::FS_BODY, theme::DANGER), MenuErrorText));
                });

                // ---------------------------------------------------- Play
                col.spawn(menu_section()).with_children(|section| {
                    section.spawn(theme::section(mtxt::section_play(lang)));
                    section.spawn(theme::divider());

                    // The browser cannot listen for connections or quit the
                    // page, so it only offers Singleplayer and Join.
                    let mut buttons: Vec<(MenuAction, String)> =
                        vec![(MenuAction::Single, mtxt::btn_singleplayer(lang).to_string())];
                    #[cfg(not(target_arch = "wasm32"))]
                    buttons.push((MenuAction::Host, mtxt::btn_host_coop(lang, settings.host_port)));
                    buttons.push((MenuAction::Join, mtxt::btn_join_guest(lang, &settings.join_addr)));
                    #[cfg(not(target_arch = "wasm32"))]
                    buttons.push((MenuAction::Quit, mtxt::btn_quit(lang).to_string()));
                    for (action, label) in buttons {
                        // Yakka o'yin — asosiy CTA: kattaroq, issiq amber
                        // ("pechni yoq") — qolganlari ikkilamchi.
                        let primary = action == MenuAction::Single;
                        let bg = if primary { theme::BTN_ACTIVE } else { theme::BTN };
                        section
                            .spawn((
                                Button,
                                Node {
                                    width: Val::Percent(100.0),
                                    height: Val::Auto,
                                    min_height: Val::Px(if primary {
                                        ff.btn_h() + 12.0
                                    } else {
                                        ff.btn_h()
                                    }),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    padding: UiRect::axes(Val::Px(theme::SP_MD), Val::Px(theme::SP_SM)),
                                    border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                                    ..default()
                                },
                                BackgroundColor(bg),
                                BaseColor(bg),
                            ))
                            .insert(action)
                            .with_children(|b| {
                                b.spawn(theme::text(
                                    label,
                                    if primary { 17.0 } else { theme::FS_BODY },
                                    theme::TEXT_PRIMARY,
                                ));
                            });
                    }
                });

                // ---------------------------------------------------Account
                col.spawn(menu_section()).with_children(|section| {
                    section.spawn(theme::section(mtxt::section_account(lang)));
                    section.spawn(theme::divider());
                    // Its own full-width row so long/localized intro text
                    // wraps at the card's width instead of squeezed into a
                    // narrow auto-width column alongside the fields below.
                    section.spawn((
                        theme::text(mtxt::account_intro(lang), theme::FS_SMALL, theme::TEXT_MUTED),
                        Node {
                            width: Val::Percent(100.0),
                            ..default()
                        },
                    ));

                    section
                        .spawn(Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(theme::SP_SM),
                            ..default()
                        })
                        .with_children(|fields| {
                            // Name is register-only; its box is hidden
                            // (Display::None) in login mode by
                            // `update_login_fields`.
                            for field in
                                [AccountField::Name, AccountField::Login, AccountField::Password]
                            {
                                fields
                                    .spawn((
                                        Button,
                                        Node {
                                            width: Val::Percent(100.0),
                                            height: Val::Px(ff.btn_h()),
                                            justify_content: JustifyContent::FlexStart,
                                            align_items: AlignItems::Center,
                                            padding: UiRect::horizontal(Val::Px(theme::SP_MD)),
                                            border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                                            ..default()
                                        },
                                        BackgroundColor(FIELD_BG),
                                        LoginFieldBox(field),
                                    ))
                                    .with_children(|b| {
                                        b.spawn((
                                            theme::text("", theme::FS_BODY, theme::TEXT_MUTED),
                                            LoginFieldText(field),
                                        ));
                                    });
                            }
                        });

                    section
                        .spawn(menu_row())
                        .with_children(|row| {
                            row.spawn((
                                Button,
                                Node {
                                    width: Val::Auto,
                                    min_height: Val::Px(ff.btn_h()),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    padding: UiRect::axes(Val::Px(theme::SP_MD), Val::Px(theme::SP_XS)),
                                    border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                                    ..default()
                                },
                                BackgroundColor(theme::BTN),
                                BaseColor(theme::BTN),
                            ))
                            .insert(AccountLoginButton)
                            .with_children(|b| {
                                b.spawn((
                                    theme::text(mtxt::btn_sign_in(lang), theme::FS_BODY, theme::TEXT_PRIMARY),
                                    AccountLoginButtonLabel,
                                ));
                            });
                            row.spawn((
                                Button,
                                Node {
                                    width: Val::Auto,
                                    min_height: Val::Px(ff.btn_h()),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    padding: UiRect::axes(Val::Px(theme::SP_MD), Val::Px(theme::SP_XS)),
                                    border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                                    ..default()
                                },
                                BackgroundColor(FIELD_BG),
                                BaseColor(FIELD_BG),
                                RegisterToggleButton,
                            ))
                            .with_children(|b| {
                                b.spawn((
                                    theme::text(mtxt::btn_sign_up(lang), theme::FS_SMALL, theme::TEXT_MUTED),
                                    RegisterToggleLabel,
                                ));
                            });
                        });
                });

                // ---------------------------------------------------Region
                // Which reverse-proxy path (and thus which independent
                // region-server process) Join/sign-in dial. Native desktop
                // only ever dials a LAN address directly, so this section is
                // browser-only.
                #[cfg(target_arch = "wasm32")]
                col.spawn(menu_section()).with_children(|section| {
                    section.spawn(theme::section(mtxt::section_region(lang)));
                    section.spawn(theme::divider());
                    section.spawn(theme::text(mtxt::region_label(lang), theme::FS_SMALL, theme::TEXT_MUTED));
                    section
                        .spawn(menu_row())
                        .with_children(|row| {
                            for (n, path) in [(1u8, "/ws"), (2, "/ws-r2"), (3, "/ws-r3")] {
                                row.spawn(theme::button(Val::Percent(100.0 / 3.0), ff.btn_h(), theme::BTN))
                                    .insert(RegionButton(path))
                                    .with_children(|b| {
                                        b.spawn(theme::text(
                                            mtxt::region_name(n, lang),
                                            theme::FS_SMALL,
                                            theme::TEXT_PRIMARY,
                                        ));
                                    });
                            }
                        });
                });

                // -------------------------------------------------Settings
                col.spawn(menu_section()).with_children(|section| {
                    section.spawn(theme::section(mtxt::section_settings(lang)));
                    section.spawn(theme::divider());

                    section.spawn(theme::text(mtxt::language_label(lang), theme::FS_SMALL, theme::TEXT_MUTED));
                    section
                        .spawn(menu_row())
                        .with_children(|row| {
                            for l in [Lang::Uz, Lang::En, Lang::Ru] {
                                let bg = if lang == l { BTN_ACTIVE } else { theme::BTN };
                                row.spawn(theme::button(Val::Percent(100.0 / 3.0), ff.btn_h(), bg))
                                    .insert(LangButton(l))
                                    .with_children(|b| {
                                        b.spawn(theme::text(l.label(), theme::FS_SMALL, theme::TEXT_PRIMARY));
                                    });
                            }
                        });

                    section.spawn(theme::text(mtxt::graphics_label(lang), theme::FS_SMALL, theme::TEXT_MUTED));
                    section
                        .spawn(menu_row())
                        .with_children(|row| {
                            for (q, label) in [
                                (QualityPref::Auto, mtxt::quality_auto(lang)),
                                (QualityPref::Low, mtxt::quality_low(lang)),
                                (QualityPref::Medium, mtxt::quality_medium(lang)),
                                (QualityPref::High, mtxt::quality_high(lang)),
                            ] {
                                let bg = if quality_pref == q { BTN_ACTIVE } else { theme::BTN };
                                row.spawn(theme::button(Val::Percent(25.0), ff.btn_h(), bg))
                                    .insert(QualityButton(q))
                                    .with_children(|b| {
                                        b.spawn(theme::text(label, theme::FS_SMALL, theme::TEXT_PRIMARY));
                                    });
                            }
                        });

                    section.spawn(theme::text(mtxt::sound_label(lang), theme::FS_SMALL, theme::TEXT_MUTED));
                    section
                        .spawn(theme::button(
                            Val::Percent(100.0),
                            ff.btn_h(),
                            if audio.enabled { BTN_ACTIVE } else { theme::BTN },
                        ))
                        .insert(AudioToggleButton)
                        .with_children(|b| {
                            let label = if audio.enabled { mtxt::sound_on(lang) } else { mtxt::sound_off(lang) };
                            b.spawn((theme::text(label, theme::FS_SMALL, theme::TEXT_PRIMARY), AudioToggleLabel));
                        });
                });

                col.spawn(Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(theme::SP_XS),
                    ..default()
                })
                .with_children(|f| {
                    f.spawn(theme::text(
                        mtxt::hint_playing_as(lang, &settings.name, settings.win_days),
                        theme::FS_MICRO,
                        theme::TEXT_FAINT,
                    ));
                    f.spawn(theme::text(mtxt::hint_controls(lang), theme::FS_MICRO, theme::TEXT_FAINT));
                });
            });
        });
}

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
    // Accounts (and the central world) live on the main region process only —
    // see `main_region_addr`.
    #[cfg(target_arch = "wasm32")]
    {
        session.join_addr = main_region_addr(&session.join_addr);
    }
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
            if let Ok(mut t) = error_text.single_mut() {
                t.0 = e;
            }
        }
    }
}

/// Rewrites `addr` to the main region's `/ws` path. Account logins and the
/// central world are main-region-only: every region process has its own
/// `WorldManager`, so signing in through `/ws-r2` would silently fork the
/// account's "single" personal world into a per-region copy.
#[cfg(target_arch = "wasm32")]
fn main_region_addr(addr: &str) -> String {
    with_path(addr, "/ws")
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
    lang: Res<Lang>,
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
        match start_game(auto, &settings, &mut net, &mut server_res, &mut view, &mut session, *lang) {
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
            theme::BTN
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

/// Click handling for the Language row: picks the clicked language, saves it,
/// and despawns/respawns the whole menu (the simplest reliable way to reflect
/// a language change everywhere labels appear — same idiom other screens use
/// for a display-toggle, just applied to the whole root instead of one node).
/// Rebuilds through the same [`build_menu`] helper `spawn_menu` uses, so the
/// new menu picks up the new language's highlight immediately.
#[allow(clippy::too_many_arguments)]
pub fn lang_buttons(
    mut commands: Commands,
    clicked: Query<(&Interaction, &LangButton), Changed<Interaction>>,
    root: Query<Entity, With<MenuRoot>>,
    mut lang: ResMut<Lang>,
    settings: Res<Settings>,
    view: Res<GameView>,
    quality_pref: Res<QualityPref>,
    audio: Res<AudioSettings>,
    ff: Res<FormFactor>,
) {
    for (interaction, btn) in &clicked {
        if *interaction != Interaction::Pressed || btn.0 == *lang {
            continue;
        }
        *lang = btn.0;
        i18n::pref_set("lang", lang.code());
        for e in &root {
            commands.entity(e).despawn();
        }
        let error = view.error.clone().unwrap_or_default();
        build_menu(commands, &settings, error, *lang, *quality_pref, *audio, *ff);
        return;
    }
}

/// Click handling and active-highlight for the Graphics row — same shape as
/// `region_buttons`, but native and web alike. Unlike language, a quality
/// change doesn't need a menu rebuild: nothing else on this screen displays
/// the quality tier, and `render::setup_camera_and_assets` (which does read
/// `Quality`) already ran once at startup — `QualityPref` only takes effect
/// on the *next* app launch, exactly like the task's other saved prefs.
pub fn quality_buttons(
    clicked: Query<(&Interaction, &QualityButton), Changed<Interaction>>,
    mut pref: ResMut<QualityPref>,
    mut all: Query<(&QualityButton, &mut BackgroundColor)>,
) {
    for (interaction, btn) in &clicked {
        if *interaction == Interaction::Pressed && btn.0 != *pref {
            *pref = btn.0;
            i18n::pref_set("quality", pref.code());
        }
    }
    for (btn, mut bg) in &mut all {
        let color = if btn.0 == *pref { BTN_ACTIVE } else { theme::BTN };
        if bg.0 != color {
            bg.0 = color;
        }
    }
}

/// Click handling for the Sound toggle. Takes effect immediately (unlike
/// Graphics): `audio.rs`'s systems read `AudioSettings` every frame, so
/// flipping it here silences/unsilences the running session right away.
pub fn audio_toggle_button(
    q: Query<&Interaction, (With<AudioToggleButton>, Changed<Interaction>)>,
    mut audio: ResMut<AudioSettings>,
) {
    for interaction in &q {
        if *interaction == Interaction::Pressed {
            audio.enabled = !audio.enabled;
            i18n::pref_set("audio", if audio.enabled { "on" } else { "off" });
        }
    }
}

/// Reflects `AudioSettings`/`QualityPref` onto their buttons' background and
/// the Sound button's own label every frame — split from the click handlers
/// above so it also runs when a respawned menu needs its very first paint to
/// already show the right highlight/label (a freshly spawned button's
/// `Interaction` never fires `Changed` on its own).
pub fn update_settings_buttons(
    audio: Res<AudioSettings>,
    lang: Res<Lang>,
    mut audio_bg: Query<&mut BackgroundColor, With<AudioToggleButton>>,
    mut audio_label: Query<&mut Text, With<AudioToggleLabel>>,
) {
    if let Ok(mut bg) = audio_bg.single_mut() {
        let color = if audio.enabled { BTN_ACTIVE } else { theme::BTN };
        if bg.0 != color {
            bg.0 = color;
        }
    }
    if let Ok(mut t) = audio_label.single_mut() {
        let label = if audio.enabled { mtxt::sound_on(*lang) } else { mtxt::sound_off(*lang) };
        if t.0 != label {
            t.0 = label.to_string();
        }
    }
}

fn start_game(
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

/// Capture keystrokes into whichever account field has focus. Tab cycles
/// focus through the fields (Name only reachable in register mode), Escape
/// clears it, Enter submits (same as clicking Kirish/Ro'yxatdan o'tish).
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
    lang: Res<Lang>,
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
                form.focus = Some(next_field(focus, form.register));
            }
            Key::Enter if !ev.repeat => {
                match submit(&form, &settings, &mut net, &mut view, &mut session, *lang) {
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
                    AccountField::Name => form.name.pop(),
                };
            }
            Key::Character(s) => {
                let len = match focus {
                    AccountField::Login => form.login.chars().count(),
                    AccountField::Password => form.password.chars().count(),
                    AccountField::Name => form.name.chars().count(),
                };
                if len < MAX_FIELD_LEN {
                    for c in s.chars() {
                        if !c.is_control() {
                            match focus {
                                AccountField::Login => form.login.push(c),
                                AccountField::Password => form.password.push(c),
                                AccountField::Name => form.name.push(c),
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Tab order: login mode skips the Name field entirely (it isn't shown, so it
/// must never gain focus); register mode cycles Name -> Login -> Password.
fn next_field(current: AccountField, register: bool) -> AccountField {
    if register {
        match current {
            AccountField::Name => AccountField::Login,
            AccountField::Login => AccountField::Password,
            AccountField::Password => AccountField::Name,
        }
    } else {
        match current {
            AccountField::Login | AccountField::Name => AccountField::Password,
            AccountField::Password => AccountField::Login,
        }
    }
}

/// The "Kirish"/"Ro'yxatdan o'tish" button — same submit path as pressing
/// Enter in a field.
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
    lang: Res<Lang>,
) {
    for interaction in &q {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match submit(&form, &settings, &mut net, &mut view, &mut session, *lang) {
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

/// Flips `LoginForm::register`; a fresh mode starts with a clean error line
/// but keeps whatever the player already typed (switching to fix a typo
/// shouldn't lose their login/password).
pub fn register_toggle_button(
    q: Query<&Interaction, (With<RegisterToggleButton>, Changed<Interaction>)>,
    mut form: ResMut<LoginForm>,
) {
    for interaction in &q {
        if *interaction == Interaction::Pressed {
            form.register = !form.register;
            form.focus = None;
        }
    }
}

/// Dispatches to `submit_login` or `submit_register` depending on the
/// current form mode.
fn submit(
    form: &LoginForm,
    settings: &Settings,
    net: &mut NetConn,
    view: &mut GameView,
    session: &mut Session,
    lang: Lang,
) -> Result<(), String> {
    if form.register {
        submit_register(form, settings, net, view, session, lang)
    } else {
        submit_login(form, settings, net, view, session, lang)
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
    lang: Lang,
) -> Result<(), String> {
    let login = form.login.trim();
    let password = form.password.trim();
    if login.is_empty() || password.is_empty() {
        return Err(mtxt::err_login_password_required(lang).to_string());
    }
    let hello = ClientMsg::Login {
        login: login.to_string(),
        password: password.to_string(),
        token: None,
    };
    // Region choice applies to guest co-op only: accounts are main-region-only
    // (see `main_region_addr`), so an account login ignores a picked `/ws-r2`.
    #[cfg(target_arch = "wasm32")]
    let addr = main_region_addr(&settings.join_addr);
    #[cfg(not(target_arch = "wasm32"))]
    let addr = settings.join_addr.clone();
    #[cfg(not(target_arch = "wasm32"))]
    let conn = frozen_city::net::client::connect_tcp_with(&addr, hello)
        .map_err(|e| mtxt::err_could_not_join(lang, &addr, &e.to_string()))?;
    #[cfg(target_arch = "wasm32")]
    let conn = frozen_city::net::ws::connect_with(&ws_url(&addr), hello)
        .map_err(|e| mtxt::err_could_not_join(lang, &addr, &e.to_string()))?;

    *session = Session {
        join_addr: addr,
        name: settings.name.clone(),
        auth: Some(AccountAuth {
            login: login.to_string(),
            password: password.to_string(),
        }),
        token: None,
        reconnectable: true,
        attempts: 0,
        central: false,
        visiting: None,
    };
    *view = GameView::default();
    net.0 = Some(Mutex::new(conn));
    Ok(())
}

/// Dials `settings.join_addr` with `ClientMsg::Register` — creates the
/// account right from the client (no Telegram bot) and, on success, signs
/// straight into the fresh personal world exactly like a `Login` would (the
/// server answers with the same `Welcome`/`AuthFailed` pair either way).
fn submit_register(
    form: &LoginForm,
    settings: &Settings,
    net: &mut NetConn,
    view: &mut GameView,
    session: &mut Session,
    lang: Lang,
) -> Result<(), String> {
    let login = form.login.trim();
    let password = form.password.trim();
    let name = form.name.trim();
    if login.is_empty() || password.is_empty() || name.is_empty() {
        return Err(mtxt::err_register_fields_required(lang).to_string());
    }
    let hello = ClientMsg::Register {
        login: login.to_string(),
        password: password.to_string(),
        name: name.to_string(),
    };
    // Accounts are main-region-only, same reasoning as `submit_login`.
    #[cfg(target_arch = "wasm32")]
    let addr = main_region_addr(&settings.join_addr);
    #[cfg(not(target_arch = "wasm32"))]
    let addr = settings.join_addr.clone();
    #[cfg(not(target_arch = "wasm32"))]
    let conn = frozen_city::net::client::connect_tcp_with(&addr, hello)
        .map_err(|e| mtxt::err_could_not_join(lang, &addr, &e.to_string()))?;
    #[cfg(target_arch = "wasm32")]
    let conn = frozen_city::net::ws::connect_with(&ws_url(&addr), hello)
        .map_err(|e| mtxt::err_could_not_join(lang, &addr, &e.to_string()))?;

    // The freshly created account signs in with the same login/password the
    // player just chose — later reconnects (and Tunnel world-switches) replay
    // a plain `Login`, matching `submit_login`'s session shape.
    *session = Session {
        join_addr: addr,
        name: settings.name.clone(),
        auth: Some(AccountAuth {
            login: login.to_string(),
            password: password.to_string(),
        }),
        token: None,
        reconnectable: true,
        attempts: 0,
        central: false,
        visiting: None,
    };
    *view = GameView::default();
    net.0 = Some(Mutex::new(conn));
    Ok(())
}

/// Reflects `LoginForm` onto the field boxes/text every frame: focus tint,
/// typed value (password masked — same as the existing Login/Password
/// fields, Name never masks), an idle placeholder when empty, and the Name
/// field's box only shown in register mode (`Display::None` in login mode,
/// matching the display-toggle idiom used by every modal panel in this
/// codebase, e.g. `roster.rs::update_roster`).
pub fn update_login_fields(
    form: Res<LoginForm>,
    lang: Res<Lang>,
    mut boxes: Query<(&LoginFieldBox, &mut BackgroundColor, &mut Node)>,
    mut texts: Query<(&LoginFieldText, &mut Text, &mut TextColor)>,
) {
    for (marker, mut bg, mut node) in &mut boxes {
        let target = if form.focus == Some(marker.0) {
            FIELD_FOCUS_BG
        } else {
            FIELD_BG
        };
        if bg.0 != target {
            bg.0 = target;
        }
        let display = if marker.0 == AccountField::Name && !form.register {
            Display::None
        } else {
            Display::Flex
        };
        if node.display != display {
            node.display = display;
        }
    }
    for (marker, mut text, mut color) in &mut texts {
        let (value, masked, placeholder) = match marker.0 {
            AccountField::Login => (form.login.as_str(), false, mtxt::field_login_placeholder(*lang)),
            AccountField::Password => {
                (form.password.as_str(), true, mtxt::field_password_placeholder(*lang))
            }
            AccountField::Name => (form.name.as_str(), false, mtxt::field_name_placeholder(*lang)),
        };
        let focused = form.focus == Some(marker.0);
        let (new, c) = if value.is_empty() && !focused {
            (placeholder.to_string(), theme::TEXT_MUTED)
        } else {
            let shown = if masked {
                "*".repeat(value.chars().count())
            } else {
                value.to_string()
            };
            let cursor = if focused { "_" } else { "" };
            (format!("{shown}{cursor}"), theme::TEXT_PRIMARY)
        };
        if text.0 != new {
            text.0 = new;
        }
        if color.0 != c {
            color.0 = c;
        }
    }
}

/// Swaps the submit button's label and the toggle button's own caption
/// between the two modes.
pub fn update_register_toggle(
    form: Res<LoginForm>,
    lang: Res<Lang>,
    mut submit_label: Query<
        &mut Text,
        (With<AccountLoginButtonLabel>, Without<RegisterToggleLabel>),
    >,
    mut toggle_label: Query<
        &mut Text,
        (With<RegisterToggleLabel>, Without<AccountLoginButtonLabel>),
    >,
) {
    let (submit, toggle) = if form.register {
        (mtxt::btn_sign_up(*lang), mtxt::btn_switch_to_sign_in(*lang))
    } else {
        (mtxt::btn_sign_in(*lang), mtxt::btn_sign_up(*lang))
    };
    if let Ok(mut t) = submit_label.single_mut() {
        if t.0 != submit {
            t.0 = submit.to_string();
        }
    }
    if let Ok(mut t) = toggle_label.single_mut() {
        if t.0 != toggle {
            t.0 = toggle.to_string();
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
