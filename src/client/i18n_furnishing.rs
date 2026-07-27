//! V0.20 UI text for building interiors — the fittings inside a workplace.
//! Same contract as every other catalog module (see `i18n.rs`): one `pub fn`
//! per string, `Lang` in, `&'static str` (or `String` where interpolation is
//! needed) out, matched exhaustively over all three languages so a missing
//! translation is a compile error rather than a silent English fallback.
//!
//! `FurnishingKind::name()`/`description()` (in `fc-game`, English-only — the
//! sim crate stays Bevy/i18n-free) are the source of truth for WHICH fitting
//! each function below is about; the translations here are a parallel lookup
//! keyed by the same `FurnishingKind`, not a wrapper around those strings —
//! mirrors how `i18n_laws.rs` treats `Law`.

use frozen_city::game::types::FurnishingKind;

use super::i18n::Lang;

pub fn furnishing_name(kind: FurnishingKind, l: Lang) -> &'static str {
    match (kind, l) {
        (FurnishingKind::Workbench, Lang::Uz) => "Ish stoli",
        (FurnishingKind::Workbench, Lang::En) => "Workbench",
        (FurnishingKind::Workbench, Lang::Ru) => "Верстак",
        (FurnishingKind::Seating, Lang::Uz) => "Stol-stullar",
        (FurnishingKind::Seating, Lang::En) => "Table and chairs",
        (FurnishingKind::Seating, Lang::Ru) => "Стол и стулья",
        // "Pechka" — a small ROOM stove, deliberately a different word from
        // the colony's own "Pech" (the Furnace building, see
        // `i18n_names::building_name`) so the two never read as the same
        // thing. Same reasoning gives Russian "Печка" (this) vs "Печь"
        // (Furnace) instead of reusing one word for both.
        (FurnishingKind::Heater, Lang::Uz) => "Pechka",
        (FurnishingKind::Heater, Lang::En) => "Stove",
        (FurnishingKind::Heater, Lang::Ru) => "Печка",
        (FurnishingKind::Shelving, Lang::Uz) => "Javonlar",
        (FurnishingKind::Shelving, Lang::En) => "Shelving",
        (FurnishingKind::Shelving, Lang::Ru) => "Полки",
    }
}

/// What the fitting does, in the player's words (mirrors
/// `FurnishingKind::description()`'s English).
pub fn furnishing_desc(kind: FurnishingKind, l: Lang) -> &'static str {
    match (kind, l) {
        (FurnishingKind::Workbench, Lang::Uz) => "Munosib asboblar. Ish yanada yaxshi bitadi.",
        (FurnishingKind::Workbench, Lang::En) => "Proper tools. The work goes better.",
        (FurnishingKind::Workbench, Lang::Ru) => "Подходящие инструменты. Работа спорится лучше.",
        (FurnishingKind::Seating, Lang::Uz) => "O'tiradigan joy. Shahar kayfiyati biroz ko'tariladi.",
        (FurnishingKind::Seating, Lang::En) => "Somewhere to sit. The city's spirits lift a little.",
        (FurnishingKind::Seating, Lang::Ru) => {
            "Есть, где посидеть. Настроение города немного поднимается."
        }
        (FurnishingKind::Heater, Lang::Uz) => "Ish joyida issiqlik. Bu yerdagilar sekinroq charchaydi.",
        (FurnishingKind::Heater, Lang::En) => "Warmth at the workplace. People here tire slower.",
        (FurnishingKind::Heater, Lang::Ru) => "Тепло на рабочем месте. Люди здесь устают медленнее.",
        (FurnishingKind::Shelving, Lang::Uz) => "Hunar uchun joy. Bu yerdagi ishchilar tezroq o'rganadi.",
        (FurnishingKind::Shelving, Lang::En) => "A place for the craft. Workers here learn faster.",
        (FurnishingKind::Shelving, Lang::Ru) => "Место для ремесла. Работники здесь учатся быстрее.",
    }
}

/// One interior row's header line: the fitting's name plus its status —
/// "not fitted" at level 0 (nothing bought yet), "Level N/max" once it's
/// been bought at least once. One localized function rather than
/// concatenating `furnishing_name` with a separate status fragment at the
/// call site, since word order around the status differs per language.
pub fn furnishing_header(kind: FurnishingKind, level: u8, max: u8, l: Lang) -> String {
    let name = furnishing_name(kind, l);
    if level == 0 {
        match l {
            Lang::Uz => format!("{name} — o'rnatilmagan"),
            Lang::En => format!("{name} — not fitted"),
            Lang::Ru => format!("{name} — не установлено"),
        }
    } else {
        match l {
            Lang::Uz => format!("{name} — daraja {level}/{max}"),
            Lang::En => format!("{name} — level {level}/{max}"),
            Lang::Ru => format!("{name} — уровень {level}/{max}"),
        }
    }
}

/// V0.21: detail card's buy/upgrade button label — "Fit" the first time
/// (level 0→1), "Upgrade" every time after (mirrors `furnishing_header`'s
/// "not fitted" vs "level N" distinction). Unlike the old per-row button this
/// also shows what the player currently holds alongside the price — the
/// reference design's "Upgrade 8.0K/15" cost-over-stock shape, just with
/// this economy's smaller numbers spelled out in full rather than
/// abbreviated. Leaves "L" untranslated, same convention
/// `i18n_hud::upgrade_btn` uses.
pub fn furniture_upgrade_btn(cur_level: u8, next_level: u8, cost: f32, have: f32, l: Lang) -> String {
    match (cur_level == 0, l) {
        (true, Lang::Uz) => format!("O'rnatish → L{next_level}   {cost:.0}/{have:.0} yog'och"),
        (true, Lang::En) => format!("Fit → L{next_level}   {cost:.0}/{have:.0} wood"),
        (true, Lang::Ru) => format!("Обустроить → L{next_level}   {cost:.0}/{have:.0} дерева"),
        (false, Lang::Uz) => format!("Yangilash → L{next_level}   {cost:.0}/{have:.0} yog'och"),
        (false, Lang::En) => format!("Upgrade → L{next_level}   {cost:.0}/{have:.0} wood"),
        (false, Lang::Ru) => format!("Улучшить → L{next_level}   {cost:.0}/{have:.0} дерева"),
    }
}

// --------------------------------------------------------- V0.21 panel shell

/// Selection panel header — building name + current level, e.g.
/// "Cookhouse Lv. 3" (the reference design's header shape). `name` is
/// already localized (`i18n_names::building_name`); only called for kinds
/// where a level means something (`BuildingKind::upgradeable`) — see the
/// call site in `selection_panel_update`.
pub fn panel_header(name: &str, level: u8, l: Lang) -> String {
    match l {
        Lang::Uz => format!("{name} — {level}-daraja"),
        Lang::En => format!("{name} Lv. {level}"),
        Lang::Ru => format!("{name} Ур. {level}"),
    }
}

/// The two-tab strip's labels.
pub fn tab_furniture(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Jihozlar",
        Lang::En => "Furniture",
        Lang::Ru => "Мебель",
    }
}

pub fn tab_survivors(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Aholi",
        Lang::En => "Survivors",
        Lang::Ru => "Жители",
    }
}

// ---------------------------------------------------- V0.21 stats grid (2x2)

/// Stats grid row/column headers — same four words the reference design
/// uses: Production, Consumption, Stats, Time, two per row.
pub fn stat_label_production(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Ishlab chiqarish",
        Lang::En => "Production",
        Lang::Ru => "Производство",
    }
}

pub fn stat_label_consumption(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Sarf",
        Lang::En => "Consumption",
        Lang::Ru => "Расход",
    }
}

pub fn stat_label_stats(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Xususiyat",
        Lang::En => "Stats",
        Lang::Ru => "Свойство",
    }
}

pub fn stat_label_time(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Vaqt",
        Lang::En => "Time",
        Lang::Ru => "Время",
    }
}

/// Production cell value — `output` is `FurnishingCycle::output` at the
/// selected fitting's current level, `None` when it isn't this room's
/// producer (a Seating/Heater/Shelving slot has no cycle at all — see
/// `FurnishingKind::cycle`'s doc). A dash reads better than a fabricated 0.
pub fn stat_value_production(output: Option<f32>, l: Lang) -> String {
    match (output, l) {
        (Some(v), Lang::Uz) => format!("{v:.0}"),
        (Some(v), Lang::En) => format!("{v:.0}"),
        (Some(v), Lang::Ru) => format!("{v:.0}"),
        (None, Lang::Uz) => "—".to_string(),
        (None, Lang::En) => "—".to_string(),
        (None, Lang::Ru) => "—".to_string(),
    }
}

/// Consumption cell value — `fur` is fur spent per cloth cycle
/// (`FUR_PER_CLOTH`), the only fitting/building pair that actually consumes
/// a stockpile good per cycle today (the Tailor Shop's Workbench). `None`
/// everywhere else draws a dash instead of a fabricated number, per the
/// task's own instruction on this cell.
pub fn stat_value_consumption(fur: Option<f32>, l: Lang) -> String {
    match (fur, l) {
        (Some(v), Lang::Uz) => format!("{v:.0} teri"),
        (Some(v), Lang::En) => format!("{v:.0} fur"),
        (Some(v), Lang::Ru) => format!("{v:.0} меха"),
        (None, Lang::Uz) => "—".to_string(),
        (None, Lang::En) => "—".to_string(),
        (None, Lang::Ru) => "—".to_string(),
    }
}

/// Time cell value — `seconds` is the cycle's current-level duration (`None`
/// when this fitting has no cycle, same gate as `stat_value_production`);
/// `delta` is next-level-minus-current in seconds (negative = faster),
/// `None` at max level or when there's no next step. Mirrors the reference
/// design's "7.8s -0.2s" shape.
pub fn stat_value_time(seconds: Option<f32>, delta: Option<f32>, l: Lang) -> String {
    let Some(secs) = seconds else {
        return match l {
            Lang::Uz => "—".to_string(),
            Lang::En => "—".to_string(),
            Lang::Ru => "—".to_string(),
        };
    };
    match (delta, l) {
        (Some(d), Lang::Uz) => format!("{secs:.1}s   {d:+.1}s"),
        (Some(d), Lang::En) => format!("{secs:.1}s   {d:+.1}s"),
        (Some(d), Lang::Ru) => format!("{secs:.1}с   {d:+.1}с"),
        (None, Lang::Uz) => format!("{secs:.1}s"),
        (None, Lang::En) => format!("{secs:.1}s"),
        (None, Lang::Ru) => format!("{secs:.1}с"),
    }
}

/// Stats cell value — the fitting's own per-level effect, read straight from
/// `FurnishingKind::per_level()` rather than hardcoded, so this text can
/// never drift from the number the sim actually applies. `Workbench`/
/// `Heater`/`Shelving` are percentages; `Seating` is an absolute morale/day
/// rate (see `FurnishingKind::per_level`'s own doc for why they're not
/// comparable with each other).
pub fn furnishing_stat_line(kind: FurnishingKind, per_level: f32, l: Lang) -> String {
    use FurnishingKind::*;
    match (kind, l) {
        (Workbench, Lang::Uz) => format!("+{:.0}%/daraja ishlab chiqarish", per_level * 100.0),
        (Workbench, Lang::En) => format!("+{:.0}%/level output", per_level * 100.0),
        (Workbench, Lang::Ru) => format!("+{:.0}%/уровень производство", per_level * 100.0),
        (Seating, Lang::Uz) => format!("+{per_level:.1} kayfiyat/kun/daraja"),
        (Seating, Lang::En) => format!("+{per_level:.1} morale/day/level"),
        (Seating, Lang::Ru) => format!("+{per_level:.1} морали/день/уровень"),
        (Heater, Lang::Uz) => format!("-{:.0}%/daraja charchash", per_level * 100.0),
        (Heater, Lang::En) => format!("-{:.0}%/level fatigue", per_level * 100.0),
        (Heater, Lang::Ru) => format!("-{:.0}%/уровень усталость", per_level * 100.0),
        (Shelving, Lang::Uz) => format!("+{:.0}%/daraja tajriba", per_level * 100.0),
        (Shelving, Lang::En) => format!("+{:.0}%/level XP", per_level * 100.0),
        (Shelving, Lang::Ru) => format!("+{:.0}%/уровень опыт", per_level * 100.0),
    }
}

// ------------------------------------------------------------ Survivors tab

/// The −/+ control's compact count readout, e.g. "1/1" — deliberately just
/// the numbers (no "workers" word) to fit the reference design's narrow
/// `−  1/1  +` control; `i18n_hud::worker_count`'s worded form is used
/// elsewhere the count stands alone.
pub fn survivor_slot_count(cur: u8, max: u8, l: Lang) -> String {
    match l {
        Lang::Uz => format!("{cur}/{max}"),
        Lang::En => format!("{cur}/{max}"),
        Lang::Ru => format!("{cur}/{max}"),
    }
}

/// Replaces the ordinary "Upgrade → L{n} ({cost} wood)" label on the
/// BUILDING's own Upgrade button when `Building::furnishings_keep_pace()` is
/// false. The upgrade is refused either way — server-side, `can_issue` and
/// the `UpgradeBuilding` arm in `sim::command` both check the same gate — so
/// the button needs to say WHY instead of just sitting there greyed out for
/// no visible reason.
pub fn furnish_first_btn(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Avval xonani jihozlang",
        Lang::En => "Furnish the room first",
        Lang::Ru => "Сначала обустройте комнату",
    }
}
