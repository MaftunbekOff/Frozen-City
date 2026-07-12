//! Localization infrastructure: the active language, a persisted-settings
//! key/value store (native file / wasm `localStorage`), and the text
//! catalog itself.
//!
//! This module only builds the *plumbing* — every catalog function returns
//! the same English/Russian text as a placeholder next to the real Uzbek
//! copy; a later pass fills in accurate En/Ru translations. New catalog
//! entries should follow [`connection_lost`]'s shape: a `pub fn` taking
//! `Lang` and returning `&'static str` (or `String` for anything that needs
//! interpolation), matched exhaustively over the three variants so a new
//! `Lang` (there won't be one) or a missed arm fails to compile rather than
//! silently falling back.

use bevy::asset::AssetId;
use bevy::prelude::*;

/// The active UI language. Defaults to Uzbek — this project's primary
/// audience — and is otherwise picked up from a URL/CLI override or a saved
/// preference (priority resolved once at startup in `main.rs`, then inserted
/// as this resource).
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Lang {
    #[default]
    Uz,
    En,
    Ru,
}

impl Lang {
    /// Short code used in the saved preference and the `?lang=`/`--lang`
    /// override — never shown to the player.
    pub fn code(self) -> &'static str {
        match self {
            Lang::Uz => "uz",
            Lang::En => "en",
            Lang::Ru => "ru",
        }
    }

    /// Parses a `code()` string back into a `Lang`; unknown codes are `None`
    /// so callers can fall through to the next priority (saved pref, then
    /// the default) instead of silently picking something.
    pub fn from_code(s: &str) -> Option<Lang> {
        match s {
            "uz" => Some(Lang::Uz),
            "en" => Some(Lang::En),
            "ru" => Some(Lang::Ru),
            _ => None,
        }
    }

    /// Human-readable name of the language, in that language itself — the
    /// label shown on its own picker button.
    pub fn label(self) -> &'static str {
        match self {
            Lang::Uz => "O'zbek",
            Lang::En => "English",
            Lang::Ru => "Русский",
        }
    }
}

// ------------------------------------------------------------- text catalog

/// Shown when a connection drop exhausts its reconnect attempts (or wasn't
/// reconnectable to begin with) and the player is dropped back to the menu.
/// Pattern for every future catalog entry: one `pub fn` per string, `Lang`
/// in, `&'static str` (or `String`, for interpolated text) out, matched
/// exhaustively.
pub fn connection_lost(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Server bilan aloqa uzildi.",
        Lang::En => "Connection to the server was lost.",
        Lang::Ru => "Соединение с сервером потеряно.",
    }
}

// ------------------------------------------------------------------ prefs

/// Prefix every wasm `localStorage` key gets, so this game's prefs never
/// collide with anything else sharing the same browser origin. The native
/// side needs no such prefix — its settings file already lives in a
/// dedicated `~/.frozen-city/` directory.
#[cfg(target_arch = "wasm32")]
const PREF_PREFIX: &str = "fc_";

/// Reads a previously saved preference. `None` if it was never set, the
/// backing store is unavailable (no `HOME`, no `localStorage`), or the key
/// isn't present.
pub fn pref_get(key: &str) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let storage = web_sys::window()?.local_storage().ok()??;
        storage.get_item(&format!("{PREF_PREFIX}{key}")).ok()?
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let contents = std::fs::read_to_string(settings_path()?).ok()?;
        contents.lines().find_map(|line| {
            let (k, v) = line.split_once('=')?;
            (k == key).then(|| v.to_string())
        })
    }
}

/// Saves a preference, overwriting any previous value for the same key.
/// Best-effort: a failure (no `HOME`, storage disabled, read-only fs) is
/// silently ignored — settings persistence is a convenience, not something
/// worth interrupting the player over.
pub fn pref_set(key: &str, val: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(Ok(Some(storage))) = web_sys::window().map(|w| w.local_storage()) {
            let _ = storage.set_item(&format!("{PREF_PREFIX}{key}"), val);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(path) = settings_path() else { return };
        let Some(dir) = path.parent() else { return };
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        // Read-modify-write the whole small file rather than appending, so
        // setting an already-present key overwrites it instead of
        // duplicating a line.
        let mut lines: Vec<(String, String)> = std::fs::read_to_string(&path)
            .ok()
            .map(|contents| {
                contents
                    .lines()
                    .filter_map(|line| line.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        match lines.iter_mut().find(|(k, _)| k == key) {
            Some((_, v)) => *v = val.to_string(),
            None => lines.push((key.to_string(), val.to_string())),
        }
        let out = lines
            .into_iter()
            .map(|(k, v)| format!("{k}={v}\n"))
            .collect::<String>();
        let _ = std::fs::write(&path, out);
    }
}

/// `$HOME/.frozen-city/settings.kv`; `None` when `HOME` isn't set (matches
/// this function's only caller, which then treats prefs as unavailable).
#[cfg(not(target_arch = "wasm32"))]
fn settings_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(std::path::PathBuf::from(home).join(".frozen-city").join("settings.kv"))
}

// -------------------------------------------------------------------- font

/// DejaVu Sans Mono, embedded so it works identically on native and wasm
/// (this project loads no runtime asset files at all — see `render.rs`'s
/// module doc). Covers Cyrillic (needed for Russian), unlike Bevy's built-in
/// default font (a FiraMono subset with Latin glyphs only).
const FONT_BYTES: &[u8] = include_bytes!("../../assets/fonts/DejaVuSansMono.ttf");

/// Replaces Bevy's built-in default font with [`FONT_BYTES`] by overwriting
/// the `Font` asset at the well-known default `AssetId` — the same id
/// `Handle::<Font>::default()` resolves to, and thus the id every `TextFont`
/// that doesn't name an explicit handle implicitly renders with (see
/// `TextFont`/`FontSource`'s `Default` impls in `bevy_text`). Must run before
/// any UI text is spawned, hence `Startup` (Bevy's own default-font
/// registration happens in `TextPlugin::build`, during `add_plugins`, i.e.
/// strictly before `Startup` even runs — so this is never later overwritten).
pub fn install_default_font(mut fonts: ResMut<Assets<Font>>) {
    let font = Font::from_bytes(FONT_BYTES.to_vec());
    let _ = fonts.insert(AssetId::<Font>::default(), font);
}
