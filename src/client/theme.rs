//! Frozen City dizayn-tizimi: palitra, tipografika/masofa shkalasi,
//! form-faktor (Desktop/Tablet/Mobile) va umumiy vidjet-quruvchilar.
//!
//! Barcha UI modullari rang/o'lcham/panel-qurilishni shu yerdan oladi —
//! har modulda lokal konstanta va helper takrorlamaslik uchun. "Muzlagan
//! shahar" vizual tili: chuqur slate-navy panellar, muz-ko'k urg'u
//! (interfeys/sovuq), amber urg'u (issiqlik/faol holat).

#![allow(dead_code)] // qayta-ishlash bosqichida iste'molchilar bosqichma-bosqich ulanadi

use bevy::prelude::*;

// ---------- Palitra ----------

/// Modal/panel foni — chuqur, biroz shaffof slate.
pub const BG_PANEL: Color = Color::srgba(0.050, 0.080, 0.130, 0.94);
/// Panel ichidagi ikkilamchi bo'lim/karta foni.
pub const BG_SECTION: Color = Color::srgba(0.085, 0.120, 0.180, 0.85);
/// Modal orqasidagi butun-ekran qoraytiruvchi parda.
pub const BG_SCRIM: Color = Color::srgba(0.008, 0.016, 0.040, 0.55);

/// Oddiy tugma / hover / bosilgan-faol (amber) / o'chirilgan-xira.
pub const BTN: Color = Color::srgb(0.155, 0.200, 0.285);
pub const BTN_HOVER: Color = Color::srgb(0.265, 0.320, 0.420);
pub const BTN_ACTIVE: Color = Color::srgb(0.820, 0.500, 0.180);
pub const BTN_DIM: Color = Color::srgba(0.100, 0.120, 0.160, 0.90);
/// Xavfli amal (kick, buzish) va tasdiq (qabul) tugmalari.
pub const BTN_DANGER: Color = Color::srgb(0.560, 0.180, 0.160);
pub const BTN_SUCCESS: Color = Color::srgb(0.180, 0.430, 0.240);

/// Panel chegarasi — nozik muz tusi; kuchlisi sarlavha ostki chizig'i uchun.
pub const BORDER: Color = Color::srgba(0.450, 0.650, 0.850, 0.22);
pub const BORDER_STRONG: Color = Color::srgba(0.450, 0.650, 0.850, 0.45);

pub const TEXT_PRIMARY: Color = Color::srgb(0.900, 0.930, 0.970);
pub const TEXT_MUTED: Color = Color::srgb(0.620, 0.680, 0.780);
pub const TEXT_FAINT: Color = Color::srgba(0.620, 0.680, 0.780, 0.60);

/// Muz-ko'k urg'u (belgilangan/interaktiv), amber urg'u (issiqlik, ogohlik).
pub const ACCENT_ICE: Color = Color::srgb(0.480, 0.780, 0.930);
pub const ACCENT_WARM: Color = Color::srgb(0.950, 0.680, 0.290);
pub const DANGER: Color = Color::srgb(0.930, 0.420, 0.380);
pub const SUCCESS: Color = Color::srgb(0.480, 0.820, 0.500);

/// Resurs ranglari (HUD va tooltiplarda bir xil bo'lsin).
pub const RES_WOOD: Color = Color::srgb(0.850, 0.680, 0.420);
pub const RES_COAL: Color = Color::srgb(0.620, 0.660, 0.750);
pub const RES_FOOD: Color = Color::srgb(0.550, 0.820, 0.480);

// ---------- Tipografika va masofa shkalasi ----------

pub const FS_TITLE: f32 = 20.0;
pub const FS_SECTION: f32 = 16.5;
pub const FS_BODY: f32 = 15.0;
pub const FS_SMALL: f32 = 13.0;
pub const FS_MICRO: f32 = 11.5;

pub const SP_XS: f32 = 4.0;
pub const SP_SM: f32 = 8.0;
pub const SP_MD: f32 = 12.0;
pub const SP_LG: f32 = 18.0;
pub const SP_XL: f32 = 26.0;

pub const RAD_PANEL: f32 = 12.0;
pub const RAD_BTN: f32 = 8.0;

// ---------- Form-faktor ----------

/// Oyna kengligi (logik piksel) bo'yicha qurilma sinfi. Layoutlar spawn
/// paytida shundan o'qiydi; o'zgarsa ochiq panellar keyingi ochilishda
/// yangi shaklga tushadi.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FormFactor {
    Mobile,
    Tablet,
    #[default]
    Desktop,
}

impl FormFactor {
    pub fn from_width(w: f32) -> Self {
        if w < 720.0 {
            FormFactor::Mobile
        } else if w < 1160.0 {
            FormFactor::Tablet
        } else {
            FormFactor::Desktop
        }
    }

    /// Barmoq bilan boshqariladigan deb hisoblanadigan sinflar.
    pub fn compact(self) -> bool {
        matches!(self, FormFactor::Mobile)
    }

    /// Tugma balandligi: barmoq nishoni mobil/planshetda kattaroq.
    pub fn btn_h(self) -> f32 {
        match self {
            FormFactor::Desktop => 34.0,
            FormFactor::Tablet => 40.0,
            FormFactor::Mobile => 46.0,
        }
    }

    /// Modal panel kengligi: mobilda to'liq en (pastki varaq uslubi),
    /// kattaroqlarda markazlashgan belgilangan en.
    pub fn panel_width(self) -> Val {
        match self {
            FormFactor::Mobile => Val::Percent(100.0),
            FormFactor::Tablet => Val::Px(560.0),
            FormFactor::Desktop => Val::Px(520.0),
        }
    }

    /// Panel ichki maydonlari.
    pub fn panel_pad(self) -> UiRect {
        match self {
            FormFactor::Mobile => UiRect::all(Val::Px(SP_MD)),
            _ => UiRect::all(Val::Px(SP_LG)),
        }
    }
}

/// Oyna o'lchamidan form-faktorni yangilab turadi (Update'da arzon).
pub fn update_form_factor(
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut ff: ResMut<FormFactor>,
) {
    let Ok(w) = window.single() else { return };
    let next = FormFactor::from_width(w.resolution.width());
    if *ff != next {
        *ff = next;
    }
}

// ---------- Hover uchun asos rang ----------

/// Umumiy hover tizimi tugmani qo'yib yuborganda qaytaradigan asl rang.
/// (Ilgari ui.rs'da edi; endi dizayn-tizimning bir qismi.)
#[derive(Component)]
pub struct BaseColor(pub Color);

// ---------- Vidjet-quruvchilar ----------

/// Matn uchligi — butun klientdagi yagona matn idiomasi.
pub fn text(t: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(t.into()),
        TextFont::from_font_size(size),
        TextColor(color),
    )
}

/// Panel sarlavhasi (title) va bo'lim sarlavhasi (section).
pub fn title(t: impl Into<String>) -> impl Bundle {
    text(t, FS_TITLE, TEXT_PRIMARY)
}

pub fn section(t: impl Into<String>) -> impl Bundle {
    text(t, FS_SECTION, ACCENT_ICE)
}

/// Standart tugma: radius + hover-asos bilan. Kenglik `Val` (Px/Percent/Auto).
pub fn button(w: Val, h: f32, bg: Color) -> impl Bundle {
    (
        Button,
        Node {
            width: w,
            height: Val::Px(h),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            padding: UiRect::horizontal(Val::Px(SP_MD)),
            border_radius: BorderRadius::all(Val::Px(RAD_BTN)),
            ..default()
        },
        BackgroundColor(bg),
        BaseColor(bg),
    )
}

/// Modal panelning tashqi qobig'i: mobilda pastki varaq (bottom sheet),
/// planshet/desktopda markazlashgan karta. Chaqiruvchi buni butun-ekran
/// scrim konteyner ichiga spawn qiladi.
pub fn modal_panel(ff: FormFactor) -> impl Bundle {
    let radius = match ff {
        // Pastki varaq: faqat ustki burchaklar yumaloq.
        FormFactor::Mobile => BorderRadius {
            top_left: Val::Px(RAD_PANEL),
            top_right: Val::Px(RAD_PANEL),
            bottom_left: Val::Px(0.0),
            bottom_right: Val::Px(0.0),
        },
        _ => BorderRadius::all(Val::Px(RAD_PANEL)),
    };
    (
        Node {
            width: ff.panel_width(),
            max_height: Val::Percent(if ff.compact() { 72.0 } else { 82.0 }),
            flex_direction: FlexDirection::Column,
            padding: ff.panel_pad(),
            row_gap: Val::Px(SP_SM),
            border: UiRect::all(Val::Px(1.0)),
            border_radius: radius,
            overflow: Overflow::scroll_y(),
            ..default()
        },
        BackgroundColor(BG_PANEL),
        BorderColor::all(BORDER),
        ScrollPosition::default(),
    )
}

/// Modal orqasidagi butun-ekran parda: mobilda kontentni pastga (varaq),
/// boshqalarda markazga tekislaydi.
pub fn scrim(ff: FormFactor) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            justify_content: JustifyContent::Center,
            align_items: if ff.compact() {
                AlignItems::FlexEnd
            } else {
                AlignItems::Center
            },
            ..default()
        },
        BackgroundColor(BG_SCRIM),
        Interaction::default(),
    )
}

/// Panel ichidagi karta/qator foni (ro'yxat elementi uslubi).
pub fn card() -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            padding: UiRect::axes(Val::Px(SP_MD), Val::Px(SP_SM)),
            column_gap: Val::Px(SP_SM),
            border_radius: BorderRadius::all(Val::Px(RAD_BTN)),
            ..default()
        },
        BackgroundColor(BG_SECTION),
    )
}

/// Sarlavha ostidagi nozik ajratuvchi chiziq.
pub fn divider() -> impl Bundle {
    (
        Node {
            width: Val::Percent(100.0),
            height: Val::Px(1.0),
            margin: UiRect::vertical(Val::Px(SP_XS)),
            ..default()
        },
        BackgroundColor(BORDER_STRONG),
    )
}
