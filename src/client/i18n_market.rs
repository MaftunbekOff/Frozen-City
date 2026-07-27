//! V0.18 UI text for the global market panel. Same contract as every other
//! catalog module (see `i18n.rs`): one `pub fn` per string, `Lang` in,
//! `&'static str` (or `String` where interpolation is needed) out, matched
//! exhaustively over all three languages so a missing translation is a
//! compile error rather than a silent English fallback.

use super::i18n::Lang;

pub fn title(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Global bozor",
        Lang::En => "Global Market",
        Lang::Ru => "Глобальный рынок",
    }
}

/// Keyboard-hint suffix next to the title, same idiom as every other modal
/// (`research_hint`, `social_hint`).
pub fn hint(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "(M yoki Esc)",
        Lang::En => "(M or Esc)",
        Lang::Ru => "(M или Esc)",
    }
}

pub fn section_book(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Buyurtmalar kitobi",
        Lang::En => "Order book",
        Lang::Ru => "Книга заказов",
    }
}

pub fn section_post(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Buyurtma qo'yish",
        Lang::En => "Post an order",
        Lang::Ru => "Разместить заказ",
    }
}

/// Shown in place of the order list while the book has nothing open.
pub fn empty_book(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Hozircha ochiq buyurtma yo'q.",
        Lang::En => "No open orders yet.",
        Lang::Ru => "Открытых заказов пока нет.",
    }
}

/// Shown instead of the post form while standing in the Global World —
/// trading only touches a colony's own stockpile, and the central world's
/// stock is communal.
pub fn central_notice(l: Lang) -> &'static str {
    match l {
        Lang::Uz => {
            "Bozorda savdo qilish uchun o'z shahringizga qayting — Global Olamning zaxirasi \
             umumiy, shu sabab bu yerdan sotib bo'lmaydi. Kitobni hali ham ko'rishingiz mumkin."
        }
        Lang::En => {
            "Trade from your own city — the Global World's stock is shared by everyone, so \
             nothing here can be bought or sold from it. You can still browse the book."
        }
        Lang::Ru => {
            "Торговать можно только из своего города — запасы Глобального мира общие, \
             продавать из них нельзя. Книгу заказов всё же можно посмотреть."
        }
    }
}

/// Label prefix in front of the joined list of pending amounts, e.g.
/// "The market owes you: 40 gold, 12 wood".
pub fn wallet_label(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Bozor sizga qarzdor",
        Lang::En => "The market owes you",
        Lang::Ru => "Рынок вам должен",
    }
}

pub fn gold_label(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "oltin",
        Lang::En => "gold",
        Lang::Ru => "золота",
    }
}

pub fn btn_sell(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Sotish",
        Lang::En => "Sell",
        Lang::Ru => "Продать",
    }
}

pub fn btn_buy(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Sotib olish",
        Lang::En => "Buy",
        Lang::Ru => "Купить",
    }
}

pub fn btn_take(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Olish",
        Lang::En => "Take",
        Lang::Ru => "Забрать",
    }
}

pub fn btn_cancel(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Bekor qilish",
        Lang::En => "Cancel",
        Lang::Ru => "Отменить",
    }
}

pub fn amount_field_label(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Miqdor",
        Lang::En => "Amount",
        Lang::Ru => "Количество",
    }
}

pub fn price_field_label(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Narx/dona",
        Lang::En => "Price/unit",
        Lang::Ru => "Цена/шт",
    }
}

/// "Market [M]" HUD button (mobile has no keyboard) — same idiom as
/// `i18n_panels::friends_hud_button`.
pub fn hud_button(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Bozor [M]",
        Lang::En => "Market [M]",
        Lang::Ru => "Рынок [M]",
    }
}

/// Post button label: "Sell 10 wood for 20.00 gold" / "Buy 10 wood for
/// 13.00 gold" — computed client-side from the post form so the player sees
/// the total before committing (the server re-validates independently).
pub fn post_btn_label(selling: bool, amount: u32, good_name: &str, total: f32, l: Lang) -> String {
    match (selling, l) {
        (true, Lang::Uz) => format!("Sotish: {amount} {good_name} — {total:.2} oltin uchun"),
        (true, Lang::En) => format!("Sell {amount} {good_name} for {total:.2} gold"),
        (true, Lang::Ru) => format!("Продать {amount} {good_name} за {total:.2} золота"),
        (false, Lang::Uz) => format!("Sotib olish: {amount} {good_name} — {total:.2} oltinga"),
        (false, Lang::En) => format!("Buy {amount} {good_name} for {total:.2} gold"),
        (false, Lang::Ru) => format!("Купить {amount} {good_name} за {total:.2} золота"),
    }
}

/// One order-book row, from the reader's point of view: "{poster} is
/// selling/buying N good at P gold/unit". `poster` already carries the
/// "(you)" suffix (`i18n_panels::you_tag`) when it's the reader's own order.
pub fn order_row_sell(poster: &str, amount: u32, good_name: &str, price: f32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("{poster} sotmoqda: {amount} {good_name} — {price:.2} oltin/dona"),
        Lang::En => format!("{poster} selling: {amount} {good_name} — {price:.2} gold/unit"),
        Lang::Ru => format!("{poster} продаёт: {amount} {good_name} — {price:.2} золота/шт"),
    }
}

pub fn order_row_buy(poster: &str, amount: u32, good_name: &str, price: f32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("{poster} sotib olmoqda: {amount} {good_name} — {price:.2} oltin/dona"),
        Lang::En => format!("{poster} buying: {amount} {good_name} — {price:.2} gold/unit"),
        Lang::Ru => format!("{poster} покупает: {amount} {good_name} — {price:.2} золота/шт"),
    }
}
