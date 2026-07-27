//! V0.18 UI text for the expedition panel. Same contract as every other
//! catalog module (see `i18n.rs`): one `pub fn` per string, `Lang` in,
//! `&'static str` (or `String` where interpolation is needed) out, matched
//! exhaustively over all three languages so a missing translation is a
//! compile error rather than a silent English fallback.
//!
//! `GameState::can_launch_expedition`'s refusal text is NOT translated here —
//! like the caravan dialog body (`events.rs`) and the event log
//! (`ui::hud::HudField::Events`), it's sim-authored plain English shown as-is;
//! this catalog only covers strings the CLIENT itself composes.

use frozen_city::game::types::{ExpeditionHaul, ExpeditionSite};

use super::i18n::Lang;

pub fn title(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Ekspeditsiya",
        Lang::En => "Expedition",
        Lang::Ru => "Экспедиция",
    }
}

/// Sarlavha yonidagi xira klaviatura-maslahat, `research_hint`/`roster_hint`
/// bilan bir xil naqsh.
pub fn hint(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "(E yoki Esc)",
        Lang::En => "(E or Esc)",
        Lang::Ru => "(E или Esc)",
    }
}

/// Label on the floating HUD button that opens this panel (mobile has no `E`
/// key) — mirrors `social`'s `SocialHudBtn` label.
pub fn hud_btn(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Ekspeditsiya",
        Lang::En => "Expedition",
        Lang::Ru => "Экспедиция",
    }
}

pub fn site_name(site: ExpeditionSite, l: Lang) -> &'static str {
    match (site, l) {
        (ExpeditionSite::FrozenWoods, Lang::Uz) => "Muzli o'rmon",
        (ExpeditionSite::FrozenWoods, Lang::En) => "Frozen Woods",
        (ExpeditionSite::FrozenWoods, Lang::Ru) => "Замёрзший лес",
        (ExpeditionSite::AbandonedMine, Lang::Uz) => "Tashlab ketilgan kon",
        (ExpeditionSite::AbandonedMine, Lang::En) => "Abandoned Mine",
        (ExpeditionSite::AbandonedMine, Lang::Ru) => "Заброшенная шахта",
        (ExpeditionSite::RuinedTown, Lang::Uz) => "Vayron shahar",
        (ExpeditionSite::RuinedTown, Lang::En) => "Ruined Town",
        (ExpeditionSite::RuinedTown, Lang::Ru) => "Разрушенный город",
        (ExpeditionSite::IceFields, Lang::Uz) => "Muz dalalari",
        (ExpeditionSite::IceFields, Lang::En) => "Ice Fields",
        (ExpeditionSite::IceFields, Lang::Ru) => "Ледяные поля",
    }
}

pub fn site_desc(site: ExpeditionSite, l: Lang) -> &'static str {
    match (site, l) {
        (ExpeditionSite::FrozenWoods, Lang::Uz) => "Yaqin va xavfsiz safar. Yog'och va bir oz oziq.",
        (ExpeditionSite::FrozenWoods, Lang::En) => "A short, safe trip. Wood and a little food.",
        (ExpeditionSite::FrozenWoods, Lang::Ru) => "Короткая, безопасная поездка. Дрова и немного еды.",
        (ExpeditionSite::AbandonedMine, Lang::Uz) => "Ko'mir — agar tunnellar chidasa.",
        (ExpeditionSite::AbandonedMine, Lang::En) => "Coal, if the tunnels hold.",
        (ExpeditionSite::AbandonedMine, Lang::Ru) => "Уголь, если туннели выдержат.",
        (ExpeditionSite::RuinedTown, Lang::Uz) => "Uzoq va xavfli. U yerda hali odamlar bor.",
        (ExpeditionSite::RuinedTown, Lang::En) => "Far and dangerous. Survivors still live there.",
        (ExpeditionSite::RuinedTown, Lang::Ru) => "Далеко и опасно. Там всё ещё живут люди.",
        (ExpeditionSite::IceFields, Lang::Uz) => "Eng uzoq yo'l. Mo'yna va go'sht, va sovuq.",
        (ExpeditionSite::IceFields, Lang::En) => "The longest road. Furs and meat, and the cold.",
        (ExpeditionSite::IceFields, Lang::Ru) => "Самый долгий путь. Мех и мясо, и холод.",
    }
}

/// "N days · X% risk/day" stat line under a site's description.
pub fn site_stats(days: f32, hazard_pct: u32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("{days:.0} kun · xavf {hazard_pct}%/kun"),
        Lang::En => format!("{days:.0} days · {hazard_pct}% risk/day"),
        Lang::Ru => format!("{days:.0} дн. · риск {hazard_pct}%/день"),
    }
}

/// Non-zero components of a site's per-member, per-day haul rate, so a site
/// with no fur (say) doesn't clutter the row with "0 fur".
pub fn site_haul_line(haul: ExpeditionHaul, l: Lang) -> String {
    let mut parts: Vec<String> = Vec::new();
    let (wood, coal, food, fur) = match l {
        Lang::Uz => ("yog'och", "ko'mir", "oziq", "mo'yna"),
        Lang::En => ("wood", "coal", "food", "fur"),
        Lang::Ru => ("дров", "угля", "еды", "меха"),
    };
    if haul.wood > 0.0 {
        parts.push(format!("{:.1} {wood}", haul.wood));
    }
    if haul.coal > 0.0 {
        parts.push(format!("{:.1} {coal}", haul.coal));
    }
    if haul.food > 0.0 {
        parts.push(format!("{:.1} {food}", haul.food));
    }
    if haul.fur > 0.0 {
        parts.push(format!("{:.1} {fur}", haul.fur));
    }
    let joined = parts.join(", ");
    match l {
        Lang::Uz => format!("Kuniga (a'zoga): {joined}"),
        Lang::En => format!("Per member, per day: {joined}"),
        Lang::Ru => format!("На человека в день: {joined}"),
    }
}

/// Shown on the Ruined Town's card only — the one site that can bring a
/// person home alongside goods.
pub fn rescue_note(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Omon qolganlarni topishi mumkin",
        Lang::En => "May bring a survivor home",
        Lang::Ru => "Может привести выжившего домой",
    }
}

pub fn party_section(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Guruh",
        Lang::En => "Party",
        Lang::Ru => "Отряд",
    }
}

/// "2/2-4 selected" counter next to the party section header.
pub fn party_count(selected: usize, min: usize, max: usize, l: Lang) -> String {
    match l {
        Lang::Uz => format!("{selected}/{min}-{max} tanlandi"),
        Lang::En => format!("{selected}/{min}-{max} selected"),
        Lang::Ru => format!("{selected}/{min}-{max} выбрано"),
    }
}

pub fn child_tag(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Bola",
        Lang::En => "Child",
        Lang::Ru => "Ребёнок",
    }
}

pub fn selected_tag(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Tanlandi",
        Lang::En => "Selected",
        Lang::Ru => "Выбран",
    }
}

pub fn party_full_tag(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Guruh to'la",
        Lang::En => "Party full",
        Lang::Ru => "Отряд полон",
    }
}

pub fn needed_at_home_tag(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Uyda kerak",
        Lang::En => "Needed at home",
        Lang::Ru => "Нужен дома",
    }
}

/// Provisions cost preview, shown once at least one member is selected.
pub fn provisions_line(food: f32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Oziq-ovqat zahirasi: {food:.0}"),
        Lang::En => format!("Provisions: {food:.0} food"),
        Lang::Ru => format!("Провизия: {food:.0} еды"),
    }
}

/// Shown instead of the provisions preview when the stockpile can't cover
/// the selected party's trip. `can_launch_expedition` doesn't check food
/// itself (it validates the roster shape, not the treasury), so this is the
/// client's own affordability read of `GameState.stock.food`.
pub fn not_enough_food(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Yo'lga yetarli oziq-ovqat yo'q",
        Lang::En => "Not enough food for the trip",
        Lang::Ru => "Недостаточно еды для похода",
    }
}

pub fn launch_btn(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Jo'natish",
        Lang::En => "Launch",
        Lang::Ru => "Отправить",
    }
}

/// "Party at the Ruined Town" heading over the active-expedition section.
pub fn active_title(site_name: &str, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Guruh: {site_name}"),
        Lang::En => format!("Party at the {site_name}"),
        Lang::Ru => format!("Отряд: {site_name}"),
    }
}

pub fn active_recalled(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Chaqirib olindi — yo'lda qaytmoqda",
        Lang::En => "Recalled — on the way home",
        Lang::Ru => "Отозван — возвращается домой",
    }
}

/// Days remaining until the party is due home.
pub fn active_days_left(days: f32, l: Lang) -> String {
    let days = days.max(0.0);
    match l {
        Lang::Uz => format!("Yana {days:.1} kunda qaytadi"),
        Lang::En => format!("{days:.1} days until they're due home"),
        Lang::Ru => format!("Ещё {days:.1} дн. до возвращения"),
    }
}

/// Comma-joined party member name list, prefixed by a label.
pub fn active_party_line(names: &str, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Guruh: {names}"),
        Lang::En => format!("Travelling: {names}"),
        Lang::Ru => format!("В пути: {names}"),
    }
}

pub fn recall_btn(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Uyga chaqirish",
        Lang::En => "Recall",
        Lang::Ru => "Отозвать",
    }
}
