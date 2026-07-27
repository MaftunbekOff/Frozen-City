//! V0.18 UI text for the book of laws. Same contract as every other catalog
//! module (see `i18n.rs`): one `pub fn` per string, `Lang` in, `&'static str`
//! (or `String` where interpolation is needed) out, matched exhaustively over
//! all three languages so a missing translation is a compile error rather
//! than a silent English fallback.
//!
//! `Law::name()`/`benefit()`/`cost()` (in `fc-game`, English-only — the sim
//! crate stays Bevy/i18n-free) are the source of truth for WHICH law each
//! function below is about; the client-side translations here are a parallel
//! lookup keyed by the same `Law`, not a wrapper around those strings.

use frozen_city::game::types::Law;

use super::i18n::Lang;

pub fn title(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Qonunlar kitobi",
        Lang::En => "Book of Laws",
        Lang::Ru => "Книга законов",
    }
}

/// Sarlavha yonidagi xira klaviatura-maslahat (research/roster/social bilan
/// bir xil uslub).
pub fn hint(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "(L yoki Esc)",
        Lang::En => "(L or Esc)",
        Lang::Ru => "(L или Esc)",
    }
}

/// Short label for the mobile-friendly HUD toggle button (no `L` key on
/// touch) — mirrors `social`'s `SocialHudBtn` "Friends" label, which is why
/// this stays a few characters shorter than `title` (fits a ~90px chip).
pub fn hud_label(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Qonunlar",
        Lang::En => "Laws",
        Lang::Ru => "Законы",
    }
}

/// "{n}/{max} laws in force" — panel subtitle, so the cap and the cooldown
/// (implied by the greyed rows themselves) both read as one coherent budget.
pub fn active_count(n: usize, max: usize, l: Lang) -> String {
    match l {
        Lang::Uz => format!("{n}/{max} qonun amalda"),
        Lang::En => format!("{n}/{max} laws in force"),
        Lang::Ru => format!("{n}/{max} законов в силе"),
    }
}

pub fn law_name(law: Law, l: Lang) -> &'static str {
    match (law, l) {
        (Law::LongShifts, Lang::Uz) => "Uzun ish smenasi",
        (Law::LongShifts, Lang::En) => "Long Shifts",
        (Law::LongShifts, Lang::Ru) => "Долгие смены",
        (Law::ExtraRations, Lang::Uz) => "Qo'shimcha ratsion",
        (Law::ExtraRations, Lang::En) => "Extra Rations",
        (Law::ExtraRations, Lang::Ru) => "Усиленный паёк",
        (Law::Curfew, Lang::Uz) => "Komendantlik soati",
        (Law::Curfew, Lang::En) => "Curfew",
        (Law::Curfew, Lang::Ru) => "Комендантский час",
        (Law::CommunalMeals, Lang::Uz) => "Umumiy ovqatlanish",
        (Law::CommunalMeals, Lang::En) => "Communal Meals",
        (Law::CommunalMeals, Lang::Ru) => "Общие трапезы",
        (Law::Quarantine, Lang::Uz) => "Karantin",
        (Law::Quarantine, Lang::En) => "Quarantine",
        (Law::Quarantine, Lang::Ru) => "Карантин",
        (Law::Apprenticeship, Lang::Uz) => "Shogirdlik",
        (Law::Apprenticeship, Lang::En) => "Apprenticeship",
        (Law::Apprenticeship, Lang::Ru) => "Ученичество",
        (Law::FuneralRites, Lang::Uz) => "Dafn marosimi",
        (Law::FuneralRites, Lang::En) => "Funeral Rites",
        (Law::FuneralRites, Lang::Ru) => "Похоронные обряды",
    }
}

/// The upside, in the player's words (mirrors `Law::benefit()`'s English).
pub fn law_benefit(law: Law, l: Lang) -> &'static str {
    match (law, l) {
        (Law::LongShifts, Lang::Uz) => "Hamma ko'proq ishlaydi. Ko'proq narsa bitadi.",
        (Law::LongShifts, Lang::En) => "Everyone works longer. More gets done.",
        (Law::LongShifts, Lang::Ru) => "Все работают дольше. Дела продвигаются быстрее.",
        (Law::ExtraRations, Lang::Uz) => "To'liqroq idishlar. Shahar kayfiyati ko'tariladi.",
        (Law::ExtraRations, Lang::En) => "Fuller plates. The city's spirits lift.",
        (Law::ExtraRations, Lang::Ru) => "Более сытные порции. Настроение города поднимается.",
        (Law::Curfew, Lang::Uz) => "Hamma kechasi to'g'ri dam oladi.",
        (Law::Curfew, Lang::En) => "Everyone rests properly at night.",
        (Law::Curfew, Lang::Ru) => "Все нормально отдыхают ночью.",
        (Law::CommunalMeals, Lang::Uz) => "Bitta qozon hammani boqadi. Oziq kamroq isrof bo'ladi.",
        (Law::CommunalMeals, Lang::En) => "One pot feeds all. Less food is wasted.",
        (Law::CommunalMeals, Lang::Ru) => "Один котёл кормит всех. Меньше еды пропадает зря.",
        (Law::Quarantine, Lang::Uz) => "Kasallar ajratib qo'yiladi. Kasallik sekinroq tarqaladi.",
        (Law::Quarantine, Lang::En) => "The sick are kept apart. Illness spreads slowly.",
        (Law::Quarantine, Lang::Ru) => "Больных изолируют. Болезнь распространяется медленнее.",
        (Law::Apprenticeship, Lang::Uz) => "Ishchilar shunchaki ishlatilmaydi — ular o'rgatiladi. Tez o'rganishadi.",
        (Law::Apprenticeship, Lang::En) => "Workers are taught, not just used. They learn fast.",
        (Law::Apprenticeship, Lang::Ru) => "Рабочих не просто используют, а обучают. Они быстро учатся.",
        (Law::FuneralRites, Lang::Uz) => "O'liklar hurmat bilan dafn etiladi. Yo'qotish og'irligi kamayadi.",
        (Law::FuneralRites, Lang::En) => "The dead are honoured. A loss cuts less deep.",
        (Law::FuneralRites, Lang::Ru) => "Умерших чтят подобающе. Потеря переживается легче.",
    }
}

/// The price, in the player's words (mirrors `Law::cost()`'s English).
pub fn law_cost(law: Law, l: Lang) -> &'static str {
    match (law, l) {
        (Law::LongShifts, Lang::Uz) => "Odamlar ancha tezroq charchaydi.",
        (Law::LongShifts, Lang::En) => "People tire much faster.",
        (Law::LongShifts, Lang::Ru) => "Люди устают гораздо быстрее.",
        (Law::ExtraRations, Lang::Uz) => "Shahar sezilarli darajada ko'proq oziq yeydi.",
        (Law::ExtraRations, Lang::En) => "The city eats noticeably more.",
        (Law::ExtraRations, Lang::Ru) => "Город заметно больше расходует еды.",
        (Law::Curfew, Lang::Uz) => "Kunduzi kamroq ish bitadi.",
        (Law::Curfew, Lang::En) => "Less is done during the day.",
        (Law::Curfew, Lang::Ru) => "Днём делается меньше.",
        (Law::CommunalMeals, Lang::Uz) => "Hammaga pishirish ishlab chiqarishdan biroz oladi.",
        (Law::CommunalMeals, Lang::En) => "Cooking for all costs some output.",
        (Law::CommunalMeals, Lang::Ru) => "Готовка на всех немного снижает производство.",
        (Law::Quarantine, Lang::Uz) => "Shahar kam qo'l bilan ishlaydi.",
        (Law::Quarantine, Lang::En) => "The city works short-handed.",
        (Law::Quarantine, Lang::Ru) => "Город работает с нехваткой рабочих рук.",
        (Law::Apprenticeship, Lang::Uz) => "O'qitish ishni sekinlashtiradi.",
        (Law::Apprenticeship, Lang::En) => "Teaching slows the work down.",
        (Law::Apprenticeship, Lang::Ru) => "Обучение замедляет работу.",
        (Law::FuneralRites, Lang::Uz) => "Har bir dafn marosimi yog'och sarflaydi.",
        (Law::FuneralRites, Lang::En) => "Each funeral burns wood.",
        (Law::FuneralRites, Lang::Ru) => "Каждые похороны сжигают древесину.",
    }
}

/// Badge shown on a row whose law is currently enacted.
pub fn in_force(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Amalda",
        Lang::En => "In force",
        Lang::Ru => "В силе",
    }
}

pub fn enact_btn(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Qabul qilish",
        Lang::En => "Enact",
        Lang::Ru => "Принять",
    }
}

pub fn repeal_btn(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Bekor qilish",
        Lang::En => "Repeal",
        Lang::Ru => "Отменить",
    }
}
