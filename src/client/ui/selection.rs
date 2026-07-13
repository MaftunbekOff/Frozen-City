use bevy::prelude::*;

use frozen_city::game::types::{BuildingKind, PlayerCommand, FURNACE_COAL_PER_DAY_PER_LEVEL};
use frozen_city::net::protocol::ClientMsg;

use super::super::i18n::Lang;
use super::super::i18n_hud;
use super::super::i18n_names;
use super::super::*;
use super::*;

pub fn selection_panel_update(
    view: Res<GameView>,
    lang: Res<Lang>,
    mut selection: ResMut<Selection>,
    mut nodes: ParamSet<(
        Query<&mut Node, With<SelPanelRoot>>,
        Query<&mut Node, With<WorkerRow>>,
        Query<&mut Node, With<DemolishBtn>>,
    )>,
    mut texts: Query<(&mut Text, &SelText)>,
) {
    let lang = *lang;
    let Some(state) = view.ready() else { return };

    // Drop selection if the building disappeared.
    if let Some(id) = selection.0 {
        if state.find_building(id).is_none() {
            selection.0 = None;
        }
    }
    let sel = selection.0.and_then(|id| state.find_building(id)).cloned();

    let display = if sel.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut nodes.p0() {
        if node.display != display {
            node.display = display;
        }
    }
    let Some(b) = sel else { return };

    let has_workers = b.kind.max_workers() > 0;
    let workers_display = if has_workers { Display::Flex } else { Display::None };
    for mut node in &mut nodes.p1() {
        if node.display != workers_display {
            node.display = workers_display;
        }
    }
    let demolish_display = if b.kind.buildable() {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut nodes.p2() {
        if node.display != demolish_display {
            node.display = demolish_display;
        }
    }

    let info = match b.kind {
        BuildingKind::Furnace => i18n_hud::sel_info_furnace(
            state.furnace_level,
            state.furnace_level as f32 * FURNACE_COAL_PER_DAY_PER_LEVEL,
            frozen_city::game::types::WOOD_FUEL_PENALTY,
            state.heat_radius(),
            lang,
        ),
        BuildingKind::Tent => i18n_hud::sel_info_tent(
            state.housing_capacity(),
            state.survivors.len(),
            lang,
        ),
        BuildingKind::Sawmill => i18n_hud::sel_info_sawmill(
            b.kind.production_per_worker_day() * b.kind.max_workers() as f32,
            state.forest_near(b.x, b.y, 4),
            lang,
        ),
        BuildingKind::CoalMine => i18n_hud::sel_info_coal_mine(
            b.kind.production_per_worker_day() * b.kind.max_workers() as f32,
            state.tile(b.x, b.y).map_or(0, |t| t.deposit),
            lang,
        ),
        BuildingKind::HunterHut => i18n_hud::sel_info_hunter_hut(
            b.kind.production_per_worker_day() * b.kind.max_workers() as f32,
            lang,
        ),
        BuildingKind::Greenhouse => i18n_hud::sel_info_greenhouse(
            b.kind.production_per_worker_day() * b.kind.max_workers() as f32,
            lang,
        ),
        BuildingKind::Hospital => i18n_hud::sel_info_hospital(
            frozen_city::game::types::HOSPITAL_CARE_PER_WORKER_DAY,
            b.workers,
            b.workers as f32 * frozen_city::game::types::HOSPITAL_CARE_PER_WORKER_DAY,
            lang,
        ),
        BuildingKind::Kitchen => {
            let cut = (1.0 - frozen_city::game::types::KITCHEN_FOOD_EFFICIENCY) * 100.0;
            if b.workers > 0 {
                i18n_hud::sel_info_kitchen_staffed(cut, lang)
            } else {
                i18n_hud::sel_info_kitchen_unstaffed(cut, lang)
            }
        }
        BuildingKind::Warehouse => {
            let cut = (1.0 - frozen_city::game::types::WAREHOUSE_BUILD_DISCOUNT) * 100.0;
            if b.workers > 0 {
                i18n_hud::sel_info_warehouse_staffed(cut, lang)
            } else {
                i18n_hud::sel_info_warehouse_unstaffed(cut, lang)
            }
        }
    };

    for (mut text, kind) in &mut texts {
        let new = match kind {
            SelText::Title => i18n_names::building_name(b.kind, lang).to_string(),
            SelText::Info => info.clone(),
            SelText::Count => i18n_hud::worker_count(b.workers, b.kind.max_workers(), lang),
        };
        if text.0 != new {
            text.0 = new;
        }
    }
}

pub fn selection_panel_buttons(
    net: Res<NetConn>,
    mut selection: ResMut<Selection>,
    minus: Query<&Interaction, (Changed<Interaction>, With<WorkerMinus>)>,
    plus: Query<&Interaction, (Changed<Interaction>, With<WorkerPlus>)>,
    demolish: Query<&Interaction, (Changed<Interaction>, With<DemolishBtn>)>,
) {
    let Some(id) = selection.0 else { return };
    if minus.iter().any(|i| *i == Interaction::Pressed) {
        net.send(ClientMsg::Cmd(PlayerCommand::AdjustWorkers {
            building: id,
            delta: -1,
        }));
    }
    if plus.iter().any(|i| *i == Interaction::Pressed) {
        net.send(ClientMsg::Cmd(PlayerCommand::AdjustWorkers {
            building: id,
            delta: 1,
        }));
    }
    if demolish.iter().any(|i| *i == Interaction::Pressed) {
        net.send(ClientMsg::Cmd(PlayerCommand::Demolish { building: id }));
        selection.0 = None;
    }
}
