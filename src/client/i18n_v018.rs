//! V0.18 shared UI text: the round trip between a colony and the Global World.
//!
//! Same contract as every other catalog module (see `i18n.rs`): one `pub fn`
//! per string, `Lang` in, `&'static str` (or `String` where interpolation is
//! needed) out, matched exhaustively over all three languages so a missing
//! translation is a compile error rather than a silent English fallback.
//!
//! The three V0.18 panels each keep their own catalog next door
//! (`i18n_expedition`, `i18n_laws`, `i18n_market`); this one holds only what
//! the HUD itself says about travelling between worlds.

use super::i18n::Lang;

/// Label on the HUD button that brings this account's settlers home out of
/// the Global World (`ClientMsg::ReturnHome`).
pub fn return_home(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Uyga qaytish",
        Lang::En => "Return Home",
        Lang::Ru => "Вернуться домой",
    }
}

/// Overlay line shown while the return trip is being made.
pub fn returning(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Tunnel orqali uyga qaytilmoqda…",
        Lang::En => "Coming home through the Tunnel…",
        Lang::Ru => "Возвращение домой через Туннель…",
    }
}
