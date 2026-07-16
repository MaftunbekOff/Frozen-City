//! Panel matnlari katalogi: roster/social/roles/events/missions/research
//! (uz/en/ru). Naqsh: har matn — `pub fn nomi(l: Lang) -> &'static str`,
//! exhaustive match; parametrli matnlar `String` qaytaradi.

#[allow(unused_imports)]
use super::i18n::Lang;

// --------------------------------------------------------------- roster.rs

/// Roster modal sarlavhasi.
pub fn roster_title(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Aholi",
        Lang::En => "Survivors",
        Lang::Ru => "Жители",
    }
}

/// Sarlavha yonidagi xira klaviatura-maslahat.
pub fn roster_hint(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "(P yoki Esc)",
        Lang::En => "(P or Esc)",
        Lang::Ru => "(P или Esc)",
    }
}

pub fn roster_subtitle(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Aholini tanlang, so'ng biriktirish uchun bino tanlang.",
        Lang::En => "Pick a survivor, then select a building to assign them.",
        Lang::Ru => "Выберите жителя, затем здание для назначения.",
    }
}

pub fn status_idle(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Bo'sh",
        Lang::En => "Idle",
        Lang::Ru => "Без дела",
    }
}

pub fn status_moving(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Yo'lda",
        Lang::En => "Moving",
        Lang::Ru => "В пути",
    }
}

pub fn status_leader(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Yetakchi",
        Lang::En => "Leader",
        Lang::Ru => "Лидер",
    }
}

/// "+N more (not shown)" qatori.
pub fn roster_more(n: usize, l: Lang) -> String {
    match l {
        Lang::Uz => format!("+{n} ko'proq (ko'rsatilmagan)"),
        Lang::En => format!("+{n} more (not shown)"),
        Lang::Ru => format!("+{n} ещё (не показано)"),
    }
}

/// "Assign {name} here" tugma matni.
pub fn assign_here(name: &str, l: Lang) -> String {
    match l {
        Lang::Uz => format!("{name}ni shu yerga biriktirish"),
        Lang::En => format!("Assign {name} here"),
        Lang::Ru => format!("Назначить {name} сюда"),
    }
}

pub fn make_leader(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Yetakchi qilish",
        Lang::En => "Make Leader",
        Lang::Ru => "Назначить лидером",
    }
}

pub fn unassign(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Ishdan olish",
        Lang::En => "Unassign",
        Lang::Ru => "Снять с работы",
    }
}

/// Detail-karta "(Leader)" teg qo'shimchasi.
pub fn leader_tag(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "  (Yetakchi)",
        Lang::En => "  (Leader)",
        Lang::Ru => "  (Лидер)",
    }
}

/// Detail-karta stat qatori: "HP {hp}   Ochlik {hunger}".
pub fn card_stats_line(hp: f32, hunger: f32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("SP {hp:.0}   Ochlik {hunger:.0}"),
        Lang::En => format!("HP {hp:.0}   Hunger {hunger:.0}"),
        Lang::Ru => format!("ОЗ {hp:.0}   Голод {hunger:.0}"),
    }
}

/// Detail-karta "Working: {workplace}" qatori.
pub fn card_working(workplace: &str, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Ish joyi: {workplace}"),
        Lang::En => format!("Working: {workplace}"),
        Lang::Ru => format!("Работает: {workplace}"),
    }
}

// --------------------------------------------------------------- social.rs

pub fn social_title(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Do'stlar",
        Lang::En => "Friends",
        Lang::Ru => "Друзья",
    }
}

pub fn social_hint(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "(F yoki Esc)",
        Lang::En => "(F or Esc)",
        Lang::Ru => "(F или Esc)",
    }
}

pub fn section_friends(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Do'stlar ro'yxati",
        Lang::En => "Friends list",
        Lang::Ru => "Список друзей",
    }
}

pub fn section_invite(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Taklif",
        Lang::En => "Invite",
        Lang::Ru => "Приглашение",
    }
}

pub fn section_showcase(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Vitrina",
        Lang::En => "Showcase",
        Lang::Ru => "Витрина",
    }
}

pub fn section_offline_policy(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Offline-siyosat",
        Lang::En => "Offline policy",
        Lang::Ru => "Политика офлайн",
    }
}

pub fn add_friend_placeholder(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Do'stni ism bo'yicha qo'shish...",
        Lang::En => "Add friend by name...",
        Lang::Ru => "Добавить друга по имени...",
    }
}

pub fn btn_add(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Qo'shish",
        Lang::En => "Add",
        Lang::Ru => "Добавить",
    }
}

pub fn btn_refresh(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Yangilash",
        Lang::En => "Refresh",
        Lang::Ru => "Обновить",
    }
}

pub fn btn_visit(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Tashrif",
        Lang::En => "Visit",
        Lang::Ru => "Визит",
    }
}

pub fn btn_invite(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Taklif",
        Lang::En => "Invite",
        Lang::Ru => "Пригласить",
    }
}

/// "{name} (online)" — do'st onlayn holatida ro'yxat qatoriga qo'shiladi.
pub fn friend_online_suffix(name: &str, l: Lang) -> String {
    match l {
        Lang::Uz => format!("{name} (onlayn)"),
        Lang::En => format!("{name} (online)"),
        Lang::Ru => format!("{name} (в сети)"),
    }
}

pub fn policy_row_label(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Men oflaynda mehmonlar kirishi mumkinmi:",
        Lang::En => "Guests may enter while I'm offline:",
        Lang::Ru => "Гости могут заходить, пока я офлайн:",
    }
}

pub fn policy_on(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "YOQILGAN",
        Lang::En => "ON",
        Lang::Ru => "ВКЛ",
    }
}

pub fn policy_off(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "O'CHIQ",
        Lang::En => "OFF",
        Lang::Ru => "ВЫКЛ",
    }
}

pub fn btn_my_city(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Mening shahrim",
        Lang::En => "My City",
        Lang::Ru => "Мой город",
    }
}

/// Do'stlar HUD tugmasi ("Friends [F]").
pub fn friends_hud_button(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Do'stlar [F]",
        Lang::En => "Friends [F]",
        Lang::Ru => "Друзья [F]",
    }
}

/// Kirish taklifi toast: "{host} sizni o'z shahriga tashrifga taklif qildi!".
pub fn invite_toast(host_name: &str, l: Lang) -> String {
    match l {
        Lang::Uz => format!("{host_name} sizni o'z shahriga tashrifga taklif qildi!"),
        Lang::En => format!("{host_name} invited you to visit their city!"),
        Lang::Ru => format!("{host_name} пригласил(а) вас посетить свой город!"),
    }
}

pub fn btn_accept_visit(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Qabul qilish -> tashrif",
        Lang::En => "Accept -> visit",
        Lang::Ru => "Принять -> визит",
    }
}

/// "Visiting {name}'s city" ko'rsatkichi.
pub fn visiting_indicator(name: &str, l: Lang) -> String {
    match l {
        Lang::Uz => format!("{name}ning shahriga tashrifda"),
        Lang::En => format!("Visiting {name}'s city"),
        Lang::Ru => format!("В гостях у {name}"),
    }
}

/// `update_visiting_indicator`dagi ism topilmasa ishlatiladigan fallback.
pub fn a_friend(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "bir do'st",
        Lang::En => "a friend",
        Lang::Ru => "друг",
    }
}

// --------------------------------------------------------------- roles.rs

pub fn players_you_owner(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "O'yinchilar — siz Egasiz",
        Lang::En => "Players — you are the Owner",
        Lang::Ru => "Игроки — вы Владелец",
    }
}

/// "Players — you are a Guest". Every guest has full co-op authority
/// alongside the owner; the only owner-only action left is `Kick`.
pub fn players_you_guest(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "O'yinchilar — siz Mehmonsiz",
        Lang::En => "Players — you are a Guest",
        Lang::Ru => "Игроки — вы Гость",
    }
}

pub fn btn_kick(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "chiqarish",
        Lang::En => "kick",
        Lang::Ru => "выгнать",
    }
}

pub fn owner_tag(l: Lang) -> &'static str {
    match l {
        Lang::Uz => " (ega)",
        Lang::En => " (owner)",
        Lang::Ru => " (владелец)",
    }
}

pub fn you_tag(l: Lang) -> &'static str {
    match l {
        Lang::Uz => " — siz",
        Lang::En => " — you",
        Lang::Ru => " — вы",
    }
}

// -------------------------------------------------------------- events.rs

/// Karvon dialogining klient tomonidan qo'shiladigan tugma matnlari (dialog
/// TANASI serverdan keladi va tarjima qilinmaydi).
pub fn caravan_accept(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Qabul qilish",
        Lang::En => "Take them in",
        Lang::Ru => "Принять",
    }
}

pub fn caravan_decline(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Rad etish",
        Lang::En => "Turn away",
        Lang::Ru => "Отказать",
    }
}

pub fn event_sickness(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "KASALLIK tarqalmoqda",
        Lang::En => "SICKNESS spreading",
        Lang::Ru => "БОЛЕЗНЬ распространяется",
    }
}

pub fn event_blizzard(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "BO'RON",
        Lang::En => "BLIZZARD",
        Lang::Ru => "МЕТЕЛЬ",
    }
}

pub fn event_sickness_blizzard(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "KASALLIK + BO'RON",
        Lang::En => "SICKNESS + BLIZZARD",
        Lang::Ru => "БОЛЕЗНЬ + МЕТЕЛЬ",
    }
}

// ----------------------------------------------------------- missions.rs

pub fn missions_title(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Missiyalar",
        Lang::En => "Missions",
        Lang::Ru => "Миссии",
    }
}

/// Bajarilgan missiya qatori: "[x] {label} {target}".
pub fn mission_done_row(label: &str, target: u32) -> String {
    format!("[x] {label} {target}")
}

/// Bajarilmagan missiya qatori: "[ ] {label} {target}  ({cur}/{target})".
pub fn mission_pending_row(label: &str, target: u32, cur: u32) -> String {
    format!("[ ] {label} {target}  ({cur}/{target})")
}

/// "Excavate  ({wood} wood, {coal} coal)" tunnel-investitsiya tugmasi.
pub fn tunnel_invest_btn(wood: f32, coal: f32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Qazish  ({wood:.0} yog'och, {coal:.0} ko'mir)"),
        Lang::En => format!("Excavate  ({wood:.0} wood, {coal:.0} coal)"),
        Lang::Ru => format!("Копать  ({wood:.0} дерева, {coal:.0} угля)"),
    }
}

/// "TUNNEL — stage {stage}/{total} ({pct}%)" holat qatori.
pub fn tunnel_status(stage: u8, total: u8, pct: u32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("TUNNEL  —  bosqich {stage}/{total}  ({pct}%)"),
        Lang::En => format!("TUNNEL  —  stage {stage}/{total}  ({pct}%)"),
        Lang::Ru => format!("ТОННЕЛЬ  —  этап {stage}/{total}  ({pct}%)"),
    }
}

// ---------------------------------------------------------- research.rs

pub fn research_title(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Tadqiqot",
        Lang::En => "Research",
        Lang::Ru => "Исследования",
    }
}

pub fn research_hint(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "(R yoki Esc)",
        Lang::En => "(R or Esc)",
        Lang::Ru => "(R или Esc)",
    }
}

pub fn researched(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Tadqiq qilingan",
        Lang::En => "Researched",
        Lang::Ru => "Изучено",
    }
}

/// "{wood}w {coal}c" narx yorlig'i.
pub fn tech_cost(wood: u32, coal: u32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("{wood}y {coal}k"),
        Lang::En => format!("{wood}w {coal}c"),
        Lang::Ru => format!("{wood}д {coal}у"),
    }
}

