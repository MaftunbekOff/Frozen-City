//! HUD, qurish paneli, game-over ekrani matnlari katalogi (uz/en/ru).
//! Naqsh: har matn — `pub fn nomi(l: Lang) -> &'static str`, exhaustive match;
//! parametrli matnlar `String` qaytaradi. Qarang: `i18n::connection_lost`.

#![allow(dead_code)] // qayta-ishlash bosqichida iste'molchilar bosqichma-bosqich ulanadi

use super::i18n::Lang;

// ------------------------------------------------------------- top bar HUD

/// Resource readouts (`HudField::Wood/Coal/Food/Pop`). `n` is already
/// formatted by the caller (integer stock, or the "N  (idle M)" population
/// pair) so these only localize the label word.
pub fn hud_wood(n: i64, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Yog'och {n}"),
        Lang::En => format!("Wood {n}"),
        Lang::Ru => format!("Дерево {n}"),
    }
}

pub fn hud_coal(n: i64, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Ko'mir {n}"),
        Lang::En => format!("Coal {n}"),
        Lang::Ru => format!("Уголь {n}"),
    }
}

pub fn hud_food(n: i64, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Oziq {n}"),
        Lang::En => format!("Food {n}"),
        Lang::Ru => format!("Еда {n}"),
    }
}

/// `pop` is the survivor count, `idle` the currently unassigned worker count.
pub fn hud_pop(pop: usize, idle: u32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Aholi {pop}  (bo'sh {idle})"),
        Lang::En => format!("Pop {pop}  (idle {idle})"),
        Lang::Ru => format!("Насел. {pop}  (простой {idle})"),
    }
}

/// `day`/`win_days` — current/target day; `hh`/`mm` — in-game clock.
pub fn hud_clock(day: u32, win_days: u32, hh: u32, mm: u32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Kun {day}/{win_days}   {hh:02}:{mm:02}"),
        Lang::En => format!("Day {day}/{win_days}   {hh:02}:{mm:02}"),
        Lang::Ru => format!("День {day}/{win_days}   {hh:02}:{mm:02}"),
    }
}

/// Cold-snap suffix appended to the temperature readout.
pub fn hud_cold_snap(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "   SOVUQ TO'LQINI!",
        Lang::En => "   COLD SNAP!",
        Lang::Ru => "   ЛЮТЫЙ ХОЛОД!",
    }
}

/// `temp` — signed, already rounded to whole degrees; `snap` — the
/// `hud_cold_snap` suffix (or "" outside a cold snap).
pub fn hud_temp(temp: f32, snap: &str, l: Lang) -> String {
    // The unit itself ("C") is identical in all three languages, so only the
    // sign/number formatting is duplicated per arm to keep this exhaustive.
    match l {
        Lang::Uz => format!("{:+.0} C{}", temp, snap),
        Lang::En => format!("{:+.0} C{}", temp, snap),
        Lang::Ru => format!("{:+.0} C{}", temp, snap),
    }
}

pub fn furnace_status_burning(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "yonmoqda",
        Lang::En => "burning",
        Lang::Ru => "горит",
    }
}

pub fn furnace_status_off(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "o'chirilgan",
        Lang::En => "off",
        Lang::Ru => "выключена",
    }
}

pub fn furnace_status_out_of_fuel(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "YOQILG'I TUGADI",
        Lang::En => "OUT OF FUEL",
        Lang::Ru => "НЕТ ТОПЛИВА",
    }
}

/// `level` — furnace level; `per_day` — coal/day at that level; `status` —
/// one of `furnace_status_*`.
pub fn hud_furnace(level: u8, per_day: f32, status: &str, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Pech L{level} ({per_day:.0}/kun) {status}"),
        Lang::En => format!("Furnace L{level} ({per_day:.0}/day) {status}"),
        Lang::Ru => format!("Печь Ур.{level} ({per_day:.0}/день) {status}"),
    }
}

/// Mobile top-bar variant of `hud_furnace`: the full status word (especially
/// `furnace_status_out_of_fuel`) is long enough at `FS_MICRO` on a phone-width
/// bar to wrap onto a second line and collide with the row below, so this
/// drops it to a single trailing glyph — "!" only when fuel is the problem
/// (unlit and level > 0); an unlit level-0 furnace ("off") needs no glyph,
/// same reasoning `morale_tier_*` uses for its bracketed tier tag. E.g.
/// "Pech L1 12/k" (fine) vs "Pech L1 12/k!" (out of fuel).
pub fn hud_furnace_short(level: u8, per_day: f32, out_of_fuel: bool, l: Lang) -> String {
    let warn = if out_of_fuel { "!" } else { "" };
    match l {
        Lang::Uz => format!("Pech L{level} {per_day:.0}/k{warn}"),
        Lang::En => format!("Furn L{level} {per_day:.0}/d{warn}"),
        Lang::Ru => format!("Печь {level} {per_day:.0}/д{warn}"),
    }
}

/// Morale banding tier tags. Kept short since they're appended inline after
/// the numeric value (`hud_morale`'s `[tier]`).
pub fn morale_tier_critical(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "!!",
        Lang::En => "!!",
        Lang::Ru => "!!",
    }
}

pub fn morale_tier_low(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "!",
        Lang::En => "!",
        Lang::Ru => "!",
    }
}

pub fn morale_tier_steady(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "=",
        Lang::En => "=",
        Lang::Ru => "=",
    }
}

pub fn morale_tier_high(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "+",
        Lang::En => "+",
        Lang::Ru => "+",
    }
}

/// Mourning indicator shown while `GameState::mourning_active()` — a
/// temporary morale penalty following a leader's death.
pub fn hud_mourning_tag(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "  Motam -15%",
        Lang::En => "  Mourning -15%",
        Lang::Ru => "  Траур -15%",
    }
}

/// `morale` — rounded value; `tier` — one of `morale_tier_*`; `mourn_tag` —
/// `hud_mourning_tag` or "".
pub fn hud_morale(morale: f32, tier: &str, mourn_tag: &str, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Kayfiyat {:.0} [{tier}]{mourn_tag}", morale),
        Lang::En => format!("Morale {:.0} [{tier}]{mourn_tag}", morale),
        Lang::Ru => format!("Мораль {:.0} [{tier}]{mourn_tag}", morale),
    }
}

pub fn menu_button(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Menyu",
        Lang::En => "Menu",
        Lang::Ru => "Меню",
    }
}

// ------------------------------------------------------------------ world switch

pub fn world_switch_global(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Global Olam",
        Lang::En => "Global World",
        Lang::Ru => "Глобальный мир",
    }
}

pub fn world_switch_my_city(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Mening shahrim",
        Lang::En => "My City",
        Lang::Ru => "Мой город",
    }
}

// -------------------------------------------------------------------- hints

/// Full desktop hint/tooltip line (keyboard + mouse affordances).
pub fn default_hint_desktop(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Chap tugma qo'yish/tanlash   O'ng tugma bekor qilish   1-8 qurish   WASD siljitish   Q/E aylantirish   g'ildirak masshtab   R tadqiqot   P ro'yxat   Enter chat   Alt+bosish signal",
        Lang::En => "LMB place/select   RMB cancel   1-8 build   WASD pan   Q/E rotate   wheel zoom   R research   P roster   Enter chat   Alt+click ping",
        Lang::Ru => "ЛКМ поставить/выбрать   ПКМ отмена   1-8 постройка   WASD перемещение   Q/E поворот   колесо масштаб   R исследования   P список   Enter чат   Alt+клик метка",
    }
}

/// Short touch-friendly hint for Mobile `FormFactor` — the desktop line's
/// keyboard shortcuts don't apply on a phone/tablet.
pub fn default_hint_mobile(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Bosish: qo'yish/tanlash   Ikki barmoq: masshtab/aylantirish",
        Lang::En => "Tap: place/select   Two fingers: zoom/rotate",
        Lang::Ru => "Тап: поставить/выбрать   Два пальца: масштаб/поворот",
    }
}

/// Build-bar/selection tooltip: `name`/`desc` already localized via
/// `i18n_names::building_name`/`building_desc`; `cost` is the wood price.
pub fn build_tooltip(name: &str, cost: u32, desc: &str, l: Lang) -> String {
    match l {
        Lang::Uz => format!("{name} — {cost} yog'och. {desc}"),
        Lang::En => format!("{name} — {cost} wood. {desc}"),
        Lang::Ru => format!("{name} — {cost} дерева. {desc}"),
    }
}

/// Build-bar cost badge under a building's name, e.g. "15w  [2]" — kept
/// numeric/compact so it still fits the tile at any language's word length;
/// `hotkey` is `Some("[n]")` on Desktop/Tablet, `None` on Mobile (hidden).
pub fn build_cost_badge(cost: u32, hotkey: Option<usize>, l: Lang) -> String {
    let unit = match l {
        Lang::Uz => "y",
        Lang::En => "w",
        Lang::Ru => "д",
    };
    match hotkey {
        Some(n) => format!("{cost}{unit}  [{n}]"),
        None => format!("{cost}{unit}"),
    }
}

// --------------------------------------------------------- selection panel

pub fn furnace_level_label(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Pech darajasi",
        Lang::En => "Furnace level",
        Lang::Ru => "Уровень печи",
    }
}

pub fn furnace_off_label(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "O'chiq",
        Lang::En => "Off",
        Lang::Ru => "Выкл",
    }
}

pub fn demolish_label(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Buzish (40% qaytadi)",
        Lang::En => "Demolish (40% refund)",
        Lang::Ru => "Снести (40% возврат)",
    }
}

/// Worker count row, e.g. "3/4 workers".
pub fn worker_count(cur: u8, max: u8, l: Lang) -> String {
    match l {
        Lang::Uz => format!("{cur}/{max} ishchi"),
        Lang::En => format!("{cur}/{max} workers"),
        Lang::Ru => format!("{cur}/{max} рабочих"),
    }
}

/// Furnace selection-panel info block.
pub fn sel_info_furnace(level: u8, per_day: f32, wood_penalty: f32, heat_radius: f32, l: Lang) -> String {
    match l {
        Lang::Uz => format!(
            "Daraja {level} — {per_day:.0} ko'mir/kun yondiradi\n(ko'mir tugasa yog'och x{wood_penalty})\nIssiqlik radiusi {heat_radius:.0} katak\nDarajani pastdagi tugmalar bilan tanlang.",
        ),
        Lang::En => format!(
            "Level {level} — burns {per_day:.0} coal/day\n(wood x{wood_penalty} when coal runs out)\nHeat radius {heat_radius:.0} tiles\nSet the level with the buttons below.",
        ),
        Lang::Ru => format!(
            "Уровень {level} — сжигает {per_day:.0} угля/день\n(дерево x{wood_penalty}, когда уголь кончится)\nРадиус тепла {heat_radius:.0} клеток\nВыберите уровень кнопками ниже.",
        ),
    }
}

pub fn sel_info_tent(housing: usize, pop: usize, l: Lang) -> String {
    match l {
        Lang::Uz => format!(
            "4 kishini sig'diradi.\nShahar turar-joyi: {pop} kishiga {housing}.\nIssiqlik doirasidagi chodirlar\ntunda odamlarni iliq saqlaydi.",
        ),
        Lang::En => format!(
            "Houses 4 people.\nCity housing: {housing} for {pop} people.\nTents inside the heat glow keep\npeople warm at night.",
        ),
        Lang::Ru => format!(
            "Вмещает 4 человек.\nЖильё города: {housing} на {pop} человек.\nПалатки в радиусе тепла\nсогревают людей ночью.",
        ),
    }
}

pub fn sel_info_sawmill(production: f32, forest: u32, l: Lang) -> String {
    match l {
        Lang::Uz => format!(
            "To'liq ekipaj bilan +{production:.0} yog'och/kun.\nYaqin o'rmonda: {forest} yog'och.",
        ),
        Lang::En => format!(
            "+{production:.0} wood/day at full crew.\nForest within reach: {forest} wood.",
        ),
        Lang::Ru => format!(
            "+{production:.0} дерева/день при полном составе.\nЛес поблизости: {forest} дерева.",
        ),
    }
}

pub fn sel_info_coal_mine(production: f32, remaining: u16, l: Lang) -> String {
    match l {
        Lang::Uz => format!(
            "To'liq ekipaj bilan +{production:.0} ko'mir/kun.\nQolgan zaxira: {remaining}.",
        ),
        Lang::En => format!(
            "+{production:.0} coal/day at full crew.\nDeposit remaining: {remaining}.",
        ),
        Lang::Ru => format!(
            "+{production:.0} угля/день при полном составе.\nОсталось месторождение: {remaining}.",
        ),
    }
}

pub fn sel_info_hunter_hut(production: f32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("To'liq ekipaj bilan +{production:.0} oziq/kun."),
        Lang::En => format!("+{production:.0} food/day at full crew."),
        Lang::Ru => format!("+{production:.0} еды/день при полном составе."),
    }
}

pub fn sel_info_greenhouse(production: f32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("To'liq ekipaj bilan +{production:.0} oziq/kun.\nYuqori hosilli yopiq fermerlik."),
        Lang::En => format!("+{production:.0} food/day at full crew.\nHigh-output indoor farming."),
        Lang::Ru => format!("+{production:.0} еды/день при полном составе.\nВысокоурожайное крытое земледелие."),
    }
}

pub fn sel_info_hospital(per_worker: f32, workers: u8, total: f32, l: Lang) -> String {
    match l {
        Lang::Uz => format!(
            "Ishchili bo'lsa: aholiga kuniga +{per_worker:.0} JS\nishchi boshiga ({workers} ishchi = +{total:.0}/kun).",
        ),
        Lang::En => format!(
            "Staffed: +{per_worker:.0} HP/day to survivors\nper worker ({workers} workers = +{total:.0}/day).",
        ),
        Lang::Ru => format!(
            "С персоналом: +{per_worker:.0} ОЗ/день жителям\nна рабочего ({workers} раб. = +{total:.0}/день).",
        ),
    }
}

pub fn sel_info_kitchen_staffed(cut: f32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Ishchili bo'lsa: shahar {cut:.0}% kam oziq iste'mol qiladi."),
        Lang::En => format!("Staffed: the city eats {cut:.0}% less food."),
        Lang::Ru => format!("С персоналом: город расходует на {cut:.0}% меньше еды."),
    }
}

pub fn sel_info_kitchen_unstaffed(cut: f32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Ishchisiz. Oziq sarfini {cut:.0}% ga qisqartirish uchun xodim tayinlang."),
        Lang::En => format!("Unstaffed. Staff it to cut food use by {cut:.0}%."),
        Lang::Ru => format!("Без персонала. Наймите рабочего, чтобы сократить расход еды на {cut:.0}%."),
    }
}

pub fn sel_info_warehouse_staffed(cut: f32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Ishchili bo'lsa: yangi qurilishlar {cut:.0}% kam yog'och sarflaydi."),
        Lang::En => format!("Staffed: new buildings cost {cut:.0}% less wood."),
        Lang::Ru => format!("С персоналом: новые постройки стоят на {cut:.0}% меньше дерева."),
    }
}

pub fn sel_info_warehouse_unstaffed(cut: f32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Ishchisiz. Qurilish narxini {cut:.0}% ga qisqartirish uchun xodim tayinlang."),
        Lang::En => format!("Unstaffed. Staff it to cut build costs by {cut:.0}%."),
        Lang::Ru => format!("Без персонала. Наймите рабочего, чтобы снизить стоимость стройки на {cut:.0}%."),
    }
}

// ------------------------------------------------------------- game over

pub fn go_title_tunnel(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "TUNNEL OCHILDI",
        Lang::En => "THE TUNNEL IS OPEN",
        Lang::Ru => "ТОННЕЛЬ ОТКРЫТ",
    }
}

pub fn go_title_victory(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "G'ALABA",
        Lang::En => "VICTORY",
        Lang::Ru => "ПОБЕДА",
    }
}

pub fn go_title_defeat(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "SHAHAR QULADI",
        Lang::En => "THE CITY HAS FALLEN",
        Lang::Ru => "ГОРОД ПАЛ",
    }
}

/// Countdown line appended to the game-over info block on a persistent
/// (multiplayer) world before it auto-resets; "" when there is none.
pub fn go_reset_countdown(seconds: u32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("\nYangi ekspeditsiya {seconds} soniyadan keyin keladi."),
        Lang::En => format!("\nA new expedition arrives in {seconds} s."),
        Lang::Ru => format!("\nНовая экспедиция прибудет через {seconds} с."),
    }
}

/// Graduated-win info block: day reached, Tunnel breakthrough line, final
/// stock. `countdown` is `go_reset_countdown`'s output or "".
pub fn go_info_graduated(day: u32, wood: i64, coal: i64, food: i64, countdown: &str, l: Lang) -> String {
    match l {
        Lang::Uz => format!(
            "Kun {day} — Tunnel ochildi. Global Olam sizni kutmoqda!\nYog'och {wood}   Ko'mir {coal}   Oziq {food}{countdown}",
        ),
        Lang::En => format!(
            "Day {day} — the Tunnel broke through. The Global World awaits!\nWood {wood}   Coal {coal}   Food {food}{countdown}",
        ),
        Lang::Ru => format!(
            "День {day} — тоннель пробит. Глобальный мир ждёт!\nДерево {wood}   Уголь {coal}   Еда {food}{countdown}",
        ),
    }
}

/// Plain win/loss info block (no graduation): day reached, population, final
/// stock. `countdown` is `go_reset_countdown`'s output or "".
pub fn go_info_plain(day: u32, pop: usize, wood: i64, coal: i64, food: i64, countdown: &str, l: Lang) -> String {
    match l {
        Lang::Uz => format!(
            "Kun {day} — aholi {pop}.\nYog'och {wood}   Ko'mir {coal}   Oziq {food}{countdown}",
        ),
        Lang::En => format!(
            "Day {day} — population {pop}.\nWood {wood}   Coal {coal}   Food {food}{countdown}",
        ),
        Lang::Ru => format!(
            "День {day} — население {pop}.\nДерево {wood}   Уголь {coal}   Еда {food}{countdown}",
        ),
    }
}

pub fn enter_global_world_btn(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Global Olamga kirish",
        Lang::En => "Enter the Global World",
        Lang::Ru => "Войти в Глобальный мир",
    }
}

pub fn return_to_menu_btn(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Menyuga qaytish",
        Lang::En => "Return to Menu",
        Lang::Ru => "Вернуться в меню",
    }
}
