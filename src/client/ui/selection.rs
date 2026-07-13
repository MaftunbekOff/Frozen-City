use bevy::prelude::*;

use frozen_city::game::types::{
    BuildingKind, PlayerCommand, BUILDING_MAX_LEVEL, CONSTRUCTION_CREW_MAX,
    FURNACE_COAL_PER_DAY_PER_LEVEL,
};
use frozen_city::net::protocol::ClientMsg;

use super::super::i18n::Lang;
use super::super::i18n_hud;
use super::super::i18n_names;
use super::super::theme;
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
        Query<&mut Node, With<FurnaceRow>>,
        Query<(&mut Node, &mut BackgroundColor), With<UpgradeBtn>>,
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

    // V0.8: qurilish maydonchasi ham ishchi (usta) boshqaruvini ko'rsatadi —
    // hatto bitganda ishchisiz turlar (Chodir) uchun ham.
    let has_workers = b.under_construction() || b.kind.max_workers() > 0;
    let workers_display = if has_workers { Display::Flex } else { Display::None };
    for mut node in &mut nodes.p1() {
        if node.display != workers_display {
            node.display = workers_display;
        }
    }

    // V0.8: Yangilash tugmasi — faqat bitgan, qurilsa bo'ladigan va
    // maksimumga yetmagan binoda; rangi yog'och yetarliligini aks ettiradi.
    let show_upgrade = b.kind.buildable() && !b.under_construction() && !b.at_max_level();
    let affordable = show_upgrade
        && state.stock.wood >= b.kind.upgrade_cost_wood(b.level + 1) as f32;
    for (mut node, mut bg) in &mut nodes.p4() {
        let d = if show_upgrade { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
        }
        let want = if affordable { theme::BTN_SUCCESS } else { theme::BTN_DIM };
        if bg.0 != want {
            bg.0 = want;
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
    // Furnace level buttons appear only for the Furnace itself.
    let furnace_display = if b.kind == BuildingKind::Furnace {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut nodes.p3() {
        if node.display != furnace_display {
            node.display = furnace_display;
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
            SelText::Count => {
                // Qurilishda sig'im — brigada capi; bitganda kasb o'rinlari.
                let cap = if b.under_construction() {
                    CONSTRUCTION_CREW_MAX
                } else {
                    b.kind.max_workers()
                };
                i18n_hud::worker_count(b.workers, cap, lang)
            }
            SelText::Avail => i18n_hud::workers_available(state.idle_workers(), lang),
            SelText::Level => {
                if !b.kind.buildable() {
                    // Pechning o'z darajasi (0-3) bor — V0.8 qatori unga
                    // taalluqli emas.
                    String::new()
                } else if b.under_construction() {
                    let total = if b.level <= 1 {
                        b.kind.build_workdays()
                    } else {
                        b.kind.upgrade_workdays(b.level)
                    };
                    let pct = (((1.0 - b.build_left / total.max(1e-6)).clamp(0.0, 1.0)) * 100.0) as u32;
                    i18n_hud::construction_line(pct, b.workers, CONSTRUCTION_CREW_MAX, lang)
                } else {
                    i18n_hud::level_line(b.level, BUILDING_MAX_LEVEL, lang)
                }
            }
            SelText::Upgrade => {
                if b.at_max_level() {
                    i18n_hud::upgrade_btn_max(lang).to_string()
                } else {
                    i18n_hud::upgrade_btn(b.level + 1, b.kind.upgrade_cost_wood(b.level + 1), lang)
                }
            }
        };
        if text.0 != new {
            text.0 = new;
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn selection_panel_buttons(
    net: Res<NetConn>,
    view: Res<GameView>,
    mut selection: ResMut<Selection>,
    minus: Query<&Interaction, (Changed<Interaction>, With<WorkerMinus>)>,
    plus: Query<&Interaction, (Changed<Interaction>, With<WorkerPlus>)>,
    none_btn: Query<&Interaction, (Changed<Interaction>, With<WorkerNoneBtn>)>,
    max_btn: Query<&Interaction, (Changed<Interaction>, With<WorkerMaxBtn>)>,
    upgrade: Query<&Interaction, (Changed<Interaction>, With<UpgradeBtn>)>,
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
    // Tez-tugmalar: deltani joriy snapshot'dan hisoblaymiz; server baribir
    // [0, max]ga, named-floor'ga va bo'sh-ishchi zaxirasiga qarab klamplaydi
    // (`sim::command`ning `AdjustWorkers` tarmog'i), shuning uchun oshirib
    // so'rash xavfsiz.
    let sel_building = view.state.as_ref().and_then(|s| s.find_building(id));
    if none_btn.iter().any(|i| *i == Interaction::Pressed) {
        if let Some(b) = sel_building {
            if b.workers > 0 {
                net.send(ClientMsg::Cmd(PlayerCommand::AdjustWorkers {
                    building: id,
                    delta: -(b.workers.min(127) as i8),
                }));
            }
        }
    }
    if max_btn.iter().any(|i| *i == Interaction::Pressed) {
        if let Some(b) = sel_building {
            let room = b.kind.max_workers().saturating_sub(b.workers);
            if room > 0 {
                net.send(ClientMsg::Cmd(PlayerCommand::AdjustWorkers {
                    building: id,
                    delta: room.min(127) as i8,
                }));
            }
        }
    }
    // V0.8: yangilash — validatsiya server tomonида (`apply_command`ning
    // UpgradeBuilding tarmog'i narx/daraja/holatni tekshiradi), tugma rangi
    // esa `selection_panel_update`da oldindan ogohlantiradi.
    if upgrade.iter().any(|i| *i == Interaction::Pressed) {
        net.send(ClientMsg::Cmd(PlayerCommand::UpgradeBuilding { building: id }));
    }
    if demolish.iter().any(|i| *i == Interaction::Pressed) {
        net.send(ClientMsg::Cmd(PlayerCommand::Demolish { building: id }));
        selection.0 = None;
    }
}
