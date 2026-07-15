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

/// V0.11: colony water stockpile — mirrors `hud_food` exactly.
pub fn hud_water(n: i64, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Suv {n}"),
        Lang::En => format!("Water {n}"),
        Lang::Ru => format!("Вода {n}"),
    }
}

/// V0.13: gold earned/spent trading through the Tunnel — mirrors `hud_food`.
pub fn hud_gold(n: i64, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Oltin {n}"),
        Lang::En => format!("Gold {n}"),
        Lang::Ru => format!("Золото {n}"),
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
        Lang::Uz => "Chap tugma qo'yish/tanlash   O'ng tugma bekor qilish   B qurish menyusi   1-8 qurish   WASD siljitish   Q/E aylantirish   g'ildirak masshtab   R tadqiqot   P ro'yxat   Enter chat   Alt+bosish signal",
        Lang::En => "LMB place/select   RMB cancel   B build menu   1-8 build   WASD pan   Q/E rotate   wheel zoom   R research   P roster   Enter chat   Alt+click ping",
        Lang::Ru => "ЛКМ поставить/выбрать   ПКМ отмена   B меню построек   1-8 постройка   WASD перемещение   Q/E поворот   колесо масштаб   R исследования   P список   Enter чат   Alt+клик метка",
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

/// Build-menu deny line: the player clicked `name` but can't afford it —
/// `cost` is the wood price, `have` the current stock (floored).
pub fn build_need_wood(name: &str, cost: u32, have: i64, l: Lang) -> String {
    match l {
        Lang::Uz => format!("{name} uchun yog'och yetarli emas: {cost} kerak, sizda {have}"),
        Lang::En => format!("Not enough wood for {name}: need {cost}, you have {have}"),
        Lang::Ru => format!("Недостаточно дерева для {name}: нужно {cost}, у вас {have}"),
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

// --- Bottom-right Build/Manage panel: tab and building-category labels ---

pub fn tab_build(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Qurish",
        Lang::En => "Build",
        Lang::Ru => "Стройка",
    }
}

pub fn tab_manage(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Boshqaruv",
        Lang::En => "Manage",
        Lang::Ru => "Управление",
    }
}

pub fn build_cat_housing(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Turar-joy",
        Lang::En => "Housing",
        Lang::Ru => "Жильё",
    }
}

pub fn build_cat_production(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Ishlab chiqarish",
        Lang::En => "Production",
        Lang::Ru => "Производство",
    }
}

pub fn build_cat_services(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Xizmat",
        Lang::En => "Services",
        Lang::Ru => "Службы",
    }
}

pub fn build_cat_defense(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Mudofaa",
        Lang::En => "Defense",
        Lang::Ru => "Оборона",
    }
}

pub fn demolish_label(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Buzish (40% qaytadi)",
        Lang::En => "Demolish (40% refund)",
        Lang::Ru => "Снести (40% возврат)",
    }
}

/// V0.14: enter `RelocateMode` — free (no wood), but takes rebuild time.
pub fn relocate_label(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Ko'chirish (bepul)",
        Lang::En => "Relocate (free)",
        Lang::Ru => "Перенести (бесплатно)",
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

/// Selection panel "WORKFORCE" section heading (uppercase by convention —
/// it's a section label, not a sentence).
pub fn workforce_section(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "ISHCHI KUCHI",
        Lang::En => "WORKFORCE",
        Lang::Ru => "РАБОЧАЯ СИЛА",
    }
}

/// Workforce quick-set: clear the building's anonymous workers.
pub fn worker_none_label(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Hech kim",
        Lang::En => "None",
        Lang::Ru => "Никого",
    }
}

/// Workforce quick-set: fill the building from the idle pool.
pub fn worker_max_label(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Maksimum",
        Lang::En => "Max",
        Lang::Ru => "Максимум",
    }
}

/// Colony-wide idle pool, shown under the workforce controls.
pub fn workers_available(n: u32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Bo'sh ishchi: {n}"),
        Lang::En => format!("Available workers: {n}"),
        Lang::Ru => format!("Свободных рабочих: {n}"),
    }
}

/// V0.8: bitgan binoning daraja qatori.
pub fn level_line(level: u8, max: u8, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Daraja {level}/{max}"),
        Lang::En => format!("Level {level}/{max}"),
        Lang::Ru => format!("Уровень {level}/{max}"),
    }
}

/// V0.8: qurilish maydonchasining holat qatori — foiz va usta-brigada.
pub fn construction_line(pct: u32, crew: u8, cap: u8, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Qurilmoqda: {pct}%   ustalar {crew}/{cap}"),
        Lang::En => format!("Under construction: {pct}%   builders {crew}/{cap}"),
        Lang::Ru => format!("Стройка: {pct}%   строители {crew}/{cap}"),
    }
}

/// The Furnace's construction status while unbuilt — a literal log count
/// (`FURNACE_LOGS_NEEDED`) instead of `construction_line`'s abstract percent,
/// since progress here really is "how many logs got carried here."
pub fn furnace_construction_line(delivered: u32, needed: u32, crew: u8, cap: u8, l: Lang) -> String {
    match l {
        Lang::Uz => format!("O'tin: {delivered}/{needed}   ustalar {crew}/{cap}"),
        Lang::En => format!("Logs: {delivered}/{needed}   builders {crew}/{cap}"),
        Lang::Ru => format!("Дрова: {delivered}/{needed}   строители {crew}/{cap}"),
    }
}

/// V0.8: Yangilash tugmasi — keyingi daraja va yog'och narxi.
pub fn upgrade_btn(next_level: u8, cost: u32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Yangilash → L{next_level}  ({cost} yog'och)"),
        Lang::En => format!("Upgrade → L{next_level}  ({cost} wood)"),
        Lang::Ru => format!("Улучшить → L{next_level}  ({cost} дерева)"),
    }
}

/// V0.8: maksimal darajaga yetgan binoning tugma matni (tugma baribir
/// yashiriladi — bu matn faqat bir freymlik oraliqlar uchun zaxira).
pub fn upgrade_btn_max(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Maksimal daraja",
        Lang::En => "Max level",
        Lang::Ru => "Макс. уровень",
    }
}

/// Furnace selection-panel info block while it's still an unbuilt
/// construction site (the leader hasn't lit it yet) — `sel_info_furnace`'s
/// "Level 0, heat radius 0" framing would otherwise read as an operating
/// furnace turned down to nothing, instead of one that doesn't exist yet.
pub fn sel_info_furnace_building(l: Lang) -> String {
    match l {
        Lang::Uz => "Hali qurilmagan — hech kimni isitmaydi.\nBirovni tanlab pechni bosing (yoki\nro'yxatda \"shu yerga biriktirish\"ni\nbosing) — u o'tin chopib qurishni\nboshlaydi.".to_string(),
        Lang::En => "Not built yet — warms no one.\nSelect someone and click the furnace\n(or use \"assign here\" in the roster) —\nthey'll start chopping wood to build it.".to_string(),
        Lang::Ru => "Ещё не построена — никого не греет.\nВыберите кого-нибудь и нажмите на\nпечь (или \"назначить сюда\" в списке) —\nони начнут рубить дрова для стройки.".to_string(),
    }
}

/// Furnace selection-panel info block. `controllable` is `b.level >= 7` (an
/// established "Pech", not just a rough "gulxan" campfire — see
/// `render/buildings.rs`'s two-tier model) — below that the burn-level
/// buttons are hidden entirely, so the last line explains why instead of
/// pointing at controls that aren't there.
pub fn sel_info_furnace(
    level: u8,
    per_day: f32,
    wood_penalty: f32,
    heat_radius: f32,
    controllable: bool,
    l: Lang,
) -> String {
    let last_line = match (controllable, l) {
        (true, Lang::Uz) => "Darajani pastdagi tugmalar bilan tanlang.",
        (true, Lang::En) => "Set the level with the buttons below.",
        (true, Lang::Ru) => "Выберите уровень кнопками ниже.",
        (false, Lang::Uz) => "Hali gulxan holatida — yonish darajasini sozlash uchun uni 7-darajagacha yangilang.",
        (false, Lang::En) => "Still just a campfire — upgrade it to level 7 to control the burn setting.",
        (false, Lang::Ru) => "Пока просто костёр — улучшите до уровня 7, чтобы управлять горением.",
    };
    match l {
        Lang::Uz => format!(
            "Daraja {level} — {per_day:.0} ko'mir/kun yondiradi\n(ko'mir tugasa yog'och x{wood_penalty})\nIssiqlik radiusi {heat_radius:.0} katak\n{last_line}",
        ),
        Lang::En => format!(
            "Level {level} — burns {per_day:.0} coal/day\n(wood x{wood_penalty} when coal runs out)\nHeat radius {heat_radius:.0} tiles\n{last_line}",
        ),
        Lang::Ru => format!(
            "Уровень {level} — сжигает {per_day:.0} угля/день\n(дерево x{wood_penalty}, когда уголь кончится)\nРадиус тепла {heat_radius:.0} клеток\n{last_line}",
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

/// Tunnel selection-panel info block. `pending` is the traveler count
/// currently waiting at the mouth, if any (`GameState.pending_migrant`).
#[allow(clippy::too_many_arguments)]
pub fn sel_info_tunnel(
    unlocked: bool,
    stage: u8,
    stages: u8,
    pending: Option<u32>,
    gold: f32,
    caravan: Option<(bool, frozen_city::game::types::TradeGood, u32)>,
    l: Lang,
) -> String {
    if !unlocked {
        return match l {
            Lang::Uz => "Hali yopiq. Uni ochish uchun\nbarcha vazifalarni bajaring.".to_string(),
            Lang::En => "Still sealed. Complete every\nmission to break it open.".to_string(),
            Lang::Ru => "Пока запечатан. Выполните все\nзадания, чтобы его открыть.".to_string(),
        };
    }
    let mut base = match l {
        Lang::Uz => format!("Qazilgan: {stage}/{stages} bosqich.\nVaqti-vaqti bilan yo'lovchilar chiqib turadi."),
        Lang::En => format!("Excavated: stage {stage}/{stages}.\nTravelers emerge from it from time to time."),
        Lang::Ru => format!("Раскопано: этап {stage}/{stages}.\nВремя от времени оттуда выходят путники."),
    };
    if let (Some(n), _) = (pending, l) {
        let line = match l {
            Lang::Uz => format!("{n} kishi kirishni kutmoqda — joy bo'lsa qabul qilinadi, bo'lmasa qaytib ketadi."),
            Lang::En => format!("{n} waiting to come in — they'll join if there's room, or turn back if not."),
            Lang::Ru => format!("{n} ждут у входа — войдут, если есть место, иначе повернут назад."),
        };
        base = format!("{base}\n{line}");
    }
    let gold_line = match l {
        Lang::Uz => format!("Oltin: {:.0}.", gold),
        Lang::En => format!("Gold: {:.0}.", gold),
        Lang::Ru => format!("Золото: {:.0}.", gold),
    };
    base = format!("{base}\n{gold_line}");
    if let Some((selling, good, amount)) = caravan {
        let good_name = trade_good_name(good, l);
        let line = match (selling, l) {
            (true, Lang::Uz) => format!("Karvon {amount} {good_name} sotish uchun yo'lda."),
            (true, Lang::En) => format!("A caravan is on the road selling {amount} {good_name}."),
            (true, Lang::Ru) => format!("Караван в пути — продаёт {amount} {good_name}."),
            (false, Lang::Uz) => format!("Karvon {amount} {good_name} sotib olish uchun yo'lda."),
            (false, Lang::En) => format!("A caravan is on the road buying {amount} {good_name}."),
            (false, Lang::Ru) => format!("Караван в пути — покупает {amount} {good_name}."),
        };
        base = format!("{base}\n{line}");
    }
    base
}

pub fn trade_good_name(good: frozen_city::game::types::TradeGood, l: Lang) -> &'static str {
    use frozen_city::game::types::TradeGood;
    match (good, l) {
        (TradeGood::Wood, Lang::Uz) => "yog'och",
        (TradeGood::Wood, Lang::En) => "wood",
        (TradeGood::Wood, Lang::Ru) => "дерево",
        (TradeGood::Coal, Lang::Uz) => "ko'mir",
        (TradeGood::Coal, Lang::En) => "coal",
        (TradeGood::Coal, Lang::Ru) => "уголь",
        (TradeGood::Food, Lang::Uz) => "oziq",
        (TradeGood::Food, Lang::En) => "food",
        (TradeGood::Food, Lang::Ru) => "еду",
        (TradeGood::Fur, Lang::Uz) => "teri",
        (TradeGood::Fur, Lang::En) => "fur",
        (TradeGood::Fur, Lang::Ru) => "мех",
        (TradeGood::Cloth, Lang::Uz) => "mato",
        (TradeGood::Cloth, Lang::En) => "cloth",
        (TradeGood::Cloth, Lang::Ru) => "ткань",
    }
}

/// Short caravan quick-action button label, e.g. "Sell Wood" / "Buy Wood".
pub fn caravan_btn_label(good: frozen_city::game::types::TradeGood, selling: bool, l: Lang) -> String {
    let good_name = trade_good_name(good, l);
    match (selling, l) {
        (true, Lang::Uz) => format!("Sot: {good_name}"),
        (true, Lang::En) => format!("Sell {good_name}"),
        (true, Lang::Ru) => format!("Прод: {good_name}"),
        (false, Lang::Uz) => format!("Ol: {good_name}"),
        (false, Lang::En) => format!("Buy {good_name}"),
        (false, Lang::Ru) => format!("Куп: {good_name}"),
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

pub fn sel_info_hunter_hut(production: f32, deer: f32, rabbit: f32, l: Lang) -> String {
    match l {
        Lang::Uz => format!(
            "To'liq ekipaj bilan +{production:.0} oziq/kun.\nYovvoyi hayvonlar: {deer:.0} kiyik, {rabbit:.0} quyon.",
        ),
        Lang::En => format!(
            "+{production:.0} food/day at full crew.\nWildlife: {deer:.0} deer, {rabbit:.0} rabbits.",
        ),
        Lang::Ru => format!(
            "+{production:.0} еды/день при полном составе.\nДичь: {deer:.0} оленей, {rabbit:.0} кроликов.",
        ),
    }
}

pub fn sel_info_farmhouse(production: f32, cow: f32, sheep: f32, l: Lang) -> String {
    match l {
        Lang::Uz => format!(
            "To'liq ekipaj bilan +{production:.0} oziq/kun.\nChorva: {cow:.0} sigir, {sheep:.0} qo'y.",
        ),
        Lang::En => format!(
            "+{production:.0} food/day at full crew.\nLivestock: {cow:.0} cows, {sheep:.0} sheep.",
        ),
        Lang::Ru => format!(
            "+{production:.0} еды/день при полном составе.\nСкот: {cow:.0} коров, {sheep:.0} овец.",
        ),
    }
}

pub fn sel_info_tailor_shop(production: f32, fur: f32, l: Lang) -> String {
    match l {
        Lang::Uz => format!(
            "To'liq ekipaj bilan +{production:.0} mato/kun (agar teri yetsa).\nOmborda teri: {fur:.0}.",
        ),
        Lang::En => format!(
            "+{production:.0} cloth/day at full crew (fur permitting).\nFur in stock: {fur:.0}.",
        ),
        Lang::Ru => format!(
            "+{production:.0} ткани/день при полном составе (если хватает меха).\nМеха в запасе: {fur:.0}.",
        ),
    }
}

pub fn sel_info_well(production: f32, water: f32, l: Lang) -> String {
    match l {
        Lang::Uz => format!(
            "To'liq ekipaj bilan +{production:.0} suv/kun.\nOmborda suv: {water:.0}.",
        ),
        Lang::En => format!(
            "+{production:.0} water/day at full crew.\nWater in stock: {water:.0}.",
        ),
        Lang::Ru => format!(
            "+{production:.0} воды/день при полном составе.\nВоды в запасе: {water:.0}.",
        ),
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
