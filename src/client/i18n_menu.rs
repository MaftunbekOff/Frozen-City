//! Menyu, chat, ulanish-o'tish matnlari katalogi (uz/en/ru).
//! Naqsh: har matn — `pub fn nomi(l: Lang) -> &'static str`, exhaustive match;
//! parametrli matnlar `String` qaytaradi. Qarang: `i18n::connection_lost`.

#[allow(unused_imports)]
use super::i18n::Lang;

// ------------------------------------------------------------ landing/title

pub fn title(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "FROZEN CITY",
        Lang::En => "FROZEN CITY",
        Lang::Ru => "FROZEN CITY",
    }
}

pub fn subtitle(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Abadiy qishda birgalikda omon qolish koloniyasi",
        Lang::En => "A cooperative survival colony in the endless winter",
        Lang::Ru => "Кооперативная колония выживания в бесконечную зиму",
    }
}

// ------------------------------------------------------------ section titles

pub fn section_play(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "O'YNASH",
        Lang::En => "PLAY",
        Lang::Ru => "ИГРА",
    }
}

pub fn section_account(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "AKKAUNT",
        Lang::En => "ACCOUNT",
        Lang::Ru => "АККАУНТ",
    }
}

/// Only shown on wasm (the region picker section, see `RegionButton`'s doc).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn section_region(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "MINTAQA",
        Lang::En => "REGION",
        Lang::Ru => "РЕГИОН",
    }
}

pub fn section_settings(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "SOZLAMALAR",
        Lang::En => "SETTINGS",
        Lang::Ru => "НАСТРОЙКИ",
    }
}

// ------------------------------------------------------------------ buttons

pub fn btn_singleplayer(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Yakka o'yin",
        Lang::En => "Singleplayer",
        Lang::Ru => "Одиночная игра",
    }
}

/// `"Host Co-op (port {port})"` — the port a hosted game listens on. Native
/// only: the browser cannot listen for connections (see `MenuAction::Host`).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn btn_host_coop(l: Lang, port: u16) -> String {
    match l {
        Lang::Uz => format!("Ko'p o'yinchi (port {port})"),
        Lang::En => format!("Host Co-op (port {port})"),
        Lang::Ru => format!("Кооператив (порт {port})"),
    }
}

/// `"Mehmon sifatida: Join {addr}"` — the address a guest join dials.
pub fn btn_join_guest(l: Lang, addr: &str) -> String {
    match l {
        Lang::Uz => format!("Mehmon sifatida: {addr}"),
        Lang::En => format!("Join as guest: {addr}"),
        Lang::Ru => format!("Как гость: {addr}"),
    }
}

/// Native only: the browser cannot quit the page (see `MenuAction::Quit`).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn btn_quit(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Chiqish",
        Lang::En => "Quit",
        Lang::Ru => "Выход",
    }
}

// ------------------------------------------------------------------- region

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn region_label(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Mintaqa:",
        Lang::En => "Region:",
        Lang::Ru => "Регион:",
    }
}

/// `region_name(1, l)` .. `region_name(3, l)` — labels for the three
/// region-server picker buttons (browser build only).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn region_name(n: u8, l: Lang) -> String {
    match l {
        Lang::Uz => format!("{n}-mintaqa"),
        Lang::En => format!("Region {n}"),
        Lang::Ru => format!("Регион {n}"),
    }
}

// ----------------------------------------------------------------- settings

pub fn language_label(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Til / Language",
        Lang::En => "Til / Language",
        Lang::Ru => "Til / Language",
    }
}

pub fn graphics_label(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Grafika:",
        Lang::En => "Graphics:",
        Lang::Ru => "Графика:",
    }
}

pub fn quality_auto(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Avto",
        Lang::En => "Auto",
        Lang::Ru => "Авто",
    }
}

pub fn quality_low(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Past",
        Lang::En => "Low",
        Lang::Ru => "Низкая",
    }
}

pub fn quality_medium(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "O'rta",
        Lang::En => "Medium",
        Lang::Ru => "Средняя",
    }
}

pub fn quality_high(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Yuqori",
        Lang::En => "High",
        Lang::Ru => "Высокая",
    }
}

pub fn sound_label(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Ovoz:",
        Lang::En => "Sound:",
        Lang::Ru => "Звук:",
    }
}

pub fn sound_on(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Yoniq",
        Lang::En => "On",
        Lang::Ru => "Вкл",
    }
}

pub fn sound_off(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "O'chiq",
        Lang::En => "Off",
        Lang::Ru => "Выкл",
    }
}

// -------------------------------------------------------------------- login

pub fn account_intro(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Akkaunt bilan kiring, yoki shu yerdan ro'yxatdan o'ting:",
        Lang::En => "Sign in with an account, or register right here:",
        Lang::Ru => "Войдите в аккаунт или зарегистрируйтесь здесь:",
    }
}

pub fn field_login_placeholder(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Login",
        Lang::En => "Login",
        Lang::Ru => "Логин",
    }
}

pub fn field_password_placeholder(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Parol",
        Lang::En => "Password",
        Lang::Ru => "Пароль",
    }
}

pub fn field_name_placeholder(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Ism",
        Lang::En => "Name",
        Lang::Ru => "Имя",
    }
}

pub fn btn_sign_in(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Kirish",
        Lang::En => "Sign in",
        Lang::Ru => "Войти",
    }
}

pub fn btn_sign_up(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Ro'yxatdan o'tish",
        Lang::En => "Sign up",
        Lang::Ru => "Регистрация",
    }
}

/// Caption on the mode-toggle button while in register mode — offers to
/// switch back to sign-in.
pub fn btn_switch_to_sign_in(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Kirish rejimi",
        Lang::En => "Sign-in mode",
        Lang::Ru => "Режим входа",
    }
}

pub fn err_login_password_required(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Login va parolni kiriting.",
        Lang::En => "Enter a login and password.",
        Lang::Ru => "Введите логин и пароль.",
    }
}

pub fn err_register_fields_required(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Ism, login va parolni kiriting.",
        Lang::En => "Enter a name, login and password.",
        Lang::Ru => "Введите имя, логин и пароль.",
    }
}

// ------------------------------------------------------------------- errors

/// `"Could not join {addr}: {e}"` — a failed dial (Join, region switch,
/// account login/register, reconnection).
pub fn err_could_not_join(l: Lang, addr: &str, e: &str) -> String {
    match l {
        Lang::Uz => format!("{addr} manziliga ulanib bo'lmadi: {e}"),
        Lang::En => format!("Could not join {addr}: {e}"),
        Lang::Ru => format!("Не удалось подключиться к {addr}: {e}"),
    }
}

/// `"Could not start the server: {e}"` — hosting failed to bind/start.
/// Native only: hosting a `ServerConfig` is not compiled on wasm.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn err_could_not_start_server(l: Lang, e: &str) -> String {
    match l {
        Lang::Uz => format!("Serverni ishga tushirib bo'lmadi: {e}"),
        Lang::En => format!("Could not start the server: {e}"),
        Lang::Ru => format!("Не удалось запустить сервер: {e}"),
    }
}

/// Shown (browser build only) if `Host` is somehow triggered — the browser
/// can only join, never listen for connections.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn err_hosting_desktop_only(l: Lang) -> &'static str {
    match l {
        Lang::Uz => {
            "Ko'p o'yinchi rejimi kompyuter yoki maxsus serverda ishlaydi; brauzer faqat qo'shilishi mumkin."
        }
        Lang::En => {
            "Hosting runs on desktop or a dedicated server; the browser can only join."
        }
        Lang::Ru => {
            "Хостинг работает на компьютере или выделенном сервере; браузер может только подключаться."
        }
    }
}

// -------------------------------------------------------------------- hints

/// `"Playing as {name}   |   survive {days} days   |   change with --name / --days / --join <ip:port>"`.
pub fn hint_playing_as(l: Lang, name: &str, days: u32) -> String {
    match l {
        Lang::Uz => format!(
            "{name} sifatida o'ynayapsiz   |   {days} kun omon qoling   |   --name / --days / --join <ip:port> bilan o'zgartiring"
        ),
        Lang::En => format!(
            "Playing as {name}   |   survive {days} days   |   change with --name / --days / --join <ip:port>"
        ),
        Lang::Ru => format!(
            "Игра за {name}   |   продержитесь {days} дней   |   изменить через --name / --days / --join <ip:port>"
        ),
    }
}

pub fn hint_controls(l: Lang) -> &'static str {
    match l {
        Lang::Uz => {
            "O'yinda: LMB qo'yish/tanlash   RMB bekor qilish   1-7 tezkor qurilish   WASD siljitish   Q/E burish   MMB moyillik   g'ildirak — masshtab"
        }
        Lang::En => {
            "In game: LMB place/select   RMB cancel   1-7 quick build   WASD pan   Q/E rotate   MMB tilt   wheel zoom"
        }
        Lang::Ru => {
            "В игре: ЛКМ поставить/выбрать   ПКМ отмена   1-7 быстрая постройка   WASD движение   Q/E поворот   СКМ наклон   колесо — масштаб"
        }
    }
}

// --------------------------------------------------------------------- chat

pub fn chat_hint(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Enter — yuborish   \"/l matn\" — atrofdagi chat (pufakcha, jurnalsiz)   Esc — bekor qilish",
        Lang::En => "Enter to send   \"/l text\" = nearby chat (bubble, no log)   Esc cancel",
        Lang::Ru => "Enter — отправить   \"/l текст\" — чат рядом (пузырь, без журнала)   Esc — отмена",
    }
}
