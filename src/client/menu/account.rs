//! Account sign-in / registration: the login form fields and their focus/
//! keyboard handling, and the login/register submit paths.

use std::sync::Mutex;

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;

use frozen_city::net::protocol::ClientMsg;

use super::super::i18n::Lang;
use super::super::i18n_menu as mtxt;
use super::super::theme;
use super::super::{AccountAuth, GameView, NetConn, Screen, Session, Settings};
use super::*;

/// Field-box background: idle / focused. Distinct from `theme::BG_SECTION`
/// so the account fields still read as inputs rather than cards.
pub(crate) const FIELD_BG: Color = Color::srgba(0.020, 0.040, 0.080, 0.92);
const FIELD_FOCUS_BG: Color = Color::srgb(0.100, 0.160, 0.260);
/// Anti-runaway-buffer cap on the account fields; the server never trusts
/// the client anyway (a login/password pair is just looked up against the
/// accounts DB, whatever their length).
const MAX_FIELD_LEN: usize = 32;

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
pub(crate) struct LoginFieldBox(pub(crate) AccountField);

#[derive(Component)]
pub(crate) struct LoginFieldText(pub(crate) AccountField);

#[derive(Component)]
pub(crate) struct AccountLoginButton;

#[derive(Component)]
pub(crate) struct AccountLoginButtonLabel;

/// Switches `LoginForm::register` between sign-in and create-account mode.
#[derive(Component)]
pub(crate) struct RegisterToggleButton;

#[derive(Component)]
pub(crate) struct RegisterToggleLabel;

/// The field's current text and whether it should type masked (password) —
/// wasm-only: feeds the proxy `<input>`'s initial value/`type` whenever
/// focus moves to a field (`login_field_focus`, `handle_control_action`'s
/// Tab arm), so both call sites agree on which field maps to which
/// string/masking.
#[cfg(target_arch = "wasm32")]
fn field_value(form: &LoginForm, field: AccountField) -> (&str, bool) {
    match field {
        AccountField::Login => (form.login.as_str(), false),
        AccountField::Password => (form.password.as_str(), true),
        AccountField::Name => (form.name.as_str(), false),
    }
}

/// Click a login/password field to give it keyboard focus.
pub fn login_field_focus(
    q: Query<(&Interaction, &LoginFieldBox), Changed<Interaction>>,
    mut form: ResMut<LoginForm>,
) {
    for (interaction, field) in &q {
        if *interaction == Interaction::Pressed {
            form.focus = Some(field.0);
            // Real DOM focus on the invisible proxy <input> — this is what
            // actually opens the mobile keyboard and makes paste work (a
            // <canvas> can do neither). See `mobile_input`.
            #[cfg(target_arch = "wasm32")]
            {
                let (value, masked) = field_value(&form, field.0);
                focus_field(value, masked);
            }
        }
    }
}

/// Capture keystrokes into whichever account field has focus. Tab cycles
/// focus through the fields (Name only reachable in register mode), Escape
/// clears it, Enter submits (same as clicking Kirish/Ro'yxatdan o'tish).
/// On wasm, Backspace/Character are NOT handled here at all — the invisible
/// proxy `<input>` (see `mobile_input`) owns text entry there, since while it
/// has DOM focus these raw `KeyboardInput` events (sourced from the
/// `<canvas>`) mostly don't fire anyway, and handling both would risk
/// double-inserting a pasted/typed character if a browser ever did
/// synthesize one. `drain_mobile_input` is wasm's equivalent of this
/// function's Tab/Enter/Escape handling.
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
    if form.focus.is_none() {
        return;
    };
    for ev in events.read() {
        if ev.state != ButtonState::Pressed {
            continue;
        }
        match &ev.logical_key {
            Key::Tab => handle_control_action(
                ControlAction::Tab,
                &mut form,
                &settings,
                &mut net,
                &mut view,
                &mut session,
                &mut next,
                &mut error_text,
                *lang,
            ),
            Key::Enter if !ev.repeat => handle_control_action(
                ControlAction::Submit,
                &mut form,
                &settings,
                &mut net,
                &mut view,
                &mut session,
                &mut next,
                &mut error_text,
                *lang,
            ),
            Key::Escape if !ev.repeat => handle_control_action(
                ControlAction::Escape,
                &mut form,
                &settings,
                &mut net,
                &mut view,
                &mut session,
                &mut next,
                &mut error_text,
                *lang,
            ),
            #[cfg(not(target_arch = "wasm32"))]
            Key::Backspace => {
                let focus = form.focus.expect("checked above");
                match focus {
                    AccountField::Login => form.login.pop(),
                    AccountField::Password => form.password.pop(),
                    AccountField::Name => form.name.pop(),
                };
            }
            #[cfg(not(target_arch = "wasm32"))]
            Key::Character(s) => {
                let focus = form.focus.expect("checked above");
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

/// The three control actions Tab/Enter/Escape trigger — shared by the
/// native raw-keyboard path above and (on wasm) the proxy `<input>`'s
/// `keydown` listener via `drain_mobile_input`, so submit/cycle-focus/
/// clear-focus behavior can never drift between the two input sources.
enum ControlAction {
    Tab,
    Submit,
    Escape,
}

#[allow(clippy::too_many_arguments)]
fn handle_control_action(
    action: ControlAction,
    form: &mut LoginForm,
    settings: &Settings,
    net: &mut NetConn,
    view: &mut GameView,
    session: &mut Session,
    next: &mut NextState<Screen>,
    error_text: &mut Query<&mut Text, With<MenuErrorText>>,
    lang: Lang,
) {
    match action {
        ControlAction::Tab => {
            if let Some(focus) = form.focus {
                form.focus = Some(next_field(focus, form.register));
                #[cfg(target_arch = "wasm32")]
                {
                    let (value, masked) = field_value(form, form.focus.unwrap());
                    focus_field(value, masked);
                }
            }
        }
        ControlAction::Submit => match submit(form, settings, net, view, session, lang) {
            Ok(()) => next.set(Screen::Game),
            Err(e) => {
                for mut t in error_text.iter_mut() {
                    t.0 = e.clone();
                }
            }
        },
        ControlAction::Escape => {
            form.focus = None;
            #[cfg(target_arch = "wasm32")]
            blur_proxy();
        }
    }
}

/// Wasm-only equivalent of `login_form_keyboard`'s Tab/Enter/Escape/typing
/// handling: drains the proxy `<input>`'s events (typed/pasted text and
/// control keydowns — see `mobile_input`) each frame and applies them.
#[cfg(target_arch = "wasm32")]
#[allow(clippy::too_many_arguments)]
pub fn drain_mobile_input(
    mut form: ResMut<LoginForm>,
    settings: Res<Settings>,
    mut net: ResMut<NetConn>,
    mut view: ResMut<GameView>,
    mut session: ResMut<Session>,
    mut next: ResMut<NextState<Screen>>,
    mut error_text: Query<&mut Text, With<MenuErrorText>>,
    lang: Res<Lang>,
) {
    let ev = drain_events();
    if let Some(mut text) = ev.text {
        if let Some(focus) = form.focus {
            if text.chars().count() > MAX_FIELD_LEN {
                // The browser's own `maxlength` attribute already stops
                // most overflow before it reaches us; this is a backstop
                // (e.g. a future proxy without that attribute) — truncate
                // and push the clamped value back so the visible/typed text
                // in the real DOM input never silently disagrees with what
                // Bevy kept.
                text = text.chars().take(MAX_FIELD_LEN).collect();
                focus_field(&text, focus == AccountField::Password);
            }
            match focus {
                AccountField::Login => form.login = text,
                AccountField::Password => form.password = text,
                AccountField::Name => form.name = text,
            }
        }
    }
    if ev.submit {
        handle_control_action(
            ControlAction::Submit,
            &mut form,
            &settings,
            &mut net,
            &mut view,
            &mut session,
            &mut next,
            &mut error_text,
            *lang,
        );
    }
    if ev.escape {
        handle_control_action(
            ControlAction::Escape,
            &mut form,
            &settings,
            &mut net,
            &mut view,
            &mut session,
            &mut next,
            &mut error_text,
            *lang,
        );
    }
    if ev.tab {
        handle_control_action(
            ControlAction::Tab,
            &mut form,
            &settings,
            &mut net,
            &mut view,
            &mut session,
            &mut next,
            &mut error_text,
            *lang,
        );
    }
    if ev.lost_focus && form.focus.is_some() {
        // A genuinely DOM-initiated blur (the user tapped elsewhere or
        // dismissed the on-screen keyboard) — every Rust-initiated blur
        // already cleared `form.focus` itself before this could observe
        // it still `Some`, so no origin-tracking is needed (see
        // `mobile_input::blur_proxy`'s doc comment).
        form.focus = None;
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
            Ok(()) => {
                // A phone tapping this button (not pressing a physical
                // Enter) is the more common real-world submit path — make
                // sure the keyboard doesn't stay visually up into the
                // `Screen::Game` transition.
                #[cfg(target_arch = "wasm32")]
                blur_proxy();
                next.set(Screen::Game);
            }
            Err(e) => {
                for mut t in &mut error_text {
                    t.0 = e.clone();
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
