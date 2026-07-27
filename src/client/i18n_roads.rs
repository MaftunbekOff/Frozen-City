//! V0.19: road-drawing tool text catalog (uz/en/ru). Same contract as every
//! other catalog module — one `pub fn` per string, `Lang` in, exhaustive
//! match. See `i18n::connection_lost`'s doc comment.

use super::i18n::Lang;

/// Build-bar category heading for the road tools + `BuildingKind::SnowCrew`
/// (`ui::buildbar`'s Infrastructure row) — grouped together because the Snow
/// Crew's whole job is keeping this category's roads open.
pub fn build_cat_infra(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Infratuzilma",
        Lang::En => "Infrastructure",
        Lang::Ru => "Инфраструктура",
    }
}

/// Draw-road tool tile label — kept short like every other build-bar tile
/// name (`i18n_names::building_name`'s tiles are one or two words).
pub fn road_tool_draw(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Yo'l",
        Lang::En => "Road",
        Lang::Ru => "Дорога",
    }
}

/// Draw-road tile's cost badge, e.g. "3y/tayl" — mirrors `i18n_hud::build_
/// cost_badge`'s unit letters (y/w/д) but per-tile rather than per-building,
/// since a road's price only makes sense per tile of a drag.
pub fn road_tool_draw_badge(cost_per_tile: u32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("{cost_per_tile}y/tayl"),
        Lang::En => format!("{cost_per_tile}w/tile"),
        Lang::Ru => format!("{cost_per_tile}д/клет"),
    }
}

/// Erase-road tool tile label.
pub fn road_tool_erase(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "O'chirish",
        Lang::En => "Erase",
        Lang::Ru => "Убрать",
    }
}

/// Erase-road tile's refund badge, e.g. "+1y/tayl".
pub fn road_tool_erase_badge(refund_per_tile: u32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("+{refund_per_tile}y/tayl"),
        Lang::En => format!("+{refund_per_tile}w/tile"),
        Lang::Ru => format!("+{refund_per_tile}д/клет"),
    }
}

/// Short hint line under the Infrastructure category explaining the drag
/// gesture — the only tool in the build bar that isn't "click to place".
pub fn road_tool_hint(l: Lang) -> &'static str {
    match l {
        Lang::Uz => "Torting — yo'l chiziladi, ✓ bilan tasdiqlang",
        Lang::En => "Drag to paint, confirm with ✓",
        Lang::Ru => "Тяните, чтобы провести дорогу, подтвердите ✓",
    }
}

/// Confirm bar's live cost readout while drawing, e.g. "Road: 45 wood".
pub fn road_draw_cost(cost: f32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Yo'l: {cost:.0} yog'och"),
        Lang::En => format!("Road: {cost:.0} wood"),
        Lang::Ru => format!("Дорога: {cost:.0} дерева"),
    }
}

/// Confirm bar's live refund readout while erasing, e.g. "Removal: +12 wood".
pub fn road_erase_refund(refund: f32, l: Lang) -> String {
    match l {
        Lang::Uz => format!("Buzish: +{refund:.0} yog'och"),
        Lang::En => format!("Removal: +{refund:.0} wood"),
        Lang::Ru => format!("Снос: +{refund:.0} дерева"),
    }
}
