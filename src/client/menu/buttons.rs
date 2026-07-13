//! Play-section action buttons plus the Settings-section preference buttons
//! (region/language/graphics-quality/sound): click handling and the
//! per-frame active-highlight/label sync for each.

use bevy::prelude::*;

use super::super::i18n::{self, Lang};
use super::super::i18n_menu as mtxt;
use super::super::theme::{self, FormFactor};
use super::super::{
    AudioSettings, AutoAction, GameView, NetConn, QualityPref, Screen, ServerRes, Session, Settings,
};
use super::*;

/// Highlight for whichever region button matches the live `join_addr` path,
/// and (native and web alike) whichever Language/Graphics/Sound setting
/// button matches the current preference.
pub(crate) const BTN_ACTIVE: Color = theme::BTN_ACTIVE;

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
                // Every `MenuErrorText` (the landing's, and the account modal's
                // when open) so the message shows wherever it's visible.
                for mut t in &mut error_text {
                    t.0 = e.clone();
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
pub(crate) fn with_path(addr: &str, path: &str) -> String {
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
    roots: Query<Entity, Or<(With<MenuRoot>, With<OverlayRoot>)>>,
    mut lang: ResMut<Lang>,
    settings: Res<Settings>,
    view: Res<GameView>,
    quality_pref: Res<QualityPref>,
    audio: Res<AudioSettings>,
    ff: Res<FormFactor>,
    overlay: Res<MenuOverlay>,
) {
    for (interaction, btn) in &clicked {
        if *interaction != Interaction::Pressed || btn.0 == *lang {
            continue;
        }
        *lang = btn.0;
        i18n::pref_set("lang", lang.code());
        // Rebuild the landing AND the currently-open modal (the language
        // buttons live inside the Settings modal), so both repaint in the new
        // language and the modal stays open.
        for e in &roots {
            commands.entity(e).despawn();
        }
        let error = view.error.clone().unwrap_or_default();
        build_menu(commands, &settings, error, *lang, *quality_pref, *audio, *ff, *overlay);
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
