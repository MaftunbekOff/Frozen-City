use bevy::prelude::*;

use frozen_city::game::types::{
    BuildingKind, PlayerCommand, BUILDING_MAX_LEVEL, CONSTRUCTION_CREW_MAX,
    FURNACE_COAL_PER_DAY_PER_LEVEL, FURNACE_LOGS_NEEDED,
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
        Query<&mut Node, With<CaravanRow>>,
        Query<&mut Node, With<RelocateBtn>>,
        Query<&mut Node, With<RotateBuildingBtn>>,
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
    // hatto bitganda ishchisiz turlar (Chodir) uchun ham. Pechning ENG
    // BIRINCHI (hali yonmagan, `furnace_level == 0`) qurilishi bundan
    // mustasno: uning progressini FAQAT nomlangan `AssignSurvivor` (o'tin
    // chopish sikli) siljitadi, anonim `AdjustWorkers` +/- esa u yerda hech
    // narsaga ta'sir qilmaydi — shuning uchun bu qatorni o'sha bosqichda
    // ko'rsatmaymiz. Biroq V0.9 daraja-yangilash (`furnace_level > 0` bo'lgan
    // holda qayta `under_construction()`) — bu allaqachon oddiy usta-kunlar
    // tizimi, boshqa binolar kabi, shuning uchun bu qator u yerda ko'rinadi.
    // MARKAZIY olamda esa `AdjustWorkers` umuman rad etiladi (har bir aholi
    // akkauntga tegishli), shuning uchun bu qator u yerda ham ko'rsatilmaydi
    // — "Shu yerga tayinlash" (`AssignSurvivor`) orqali ishlash kerak.
    let has_workers = !state.central
        && ((b.under_construction() && (b.kind != BuildingKind::Furnace || state.furnace_level > 0))
            || b.kind.max_workers() > 0);
    let workers_display = if has_workers { Display::Flex } else { Display::None };
    for mut node in &mut nodes.p1() {
        if node.display != workers_display {
            node.display = workers_display;
        }
    }

    // V0.8/V0.9: Yangilash tugmasi — faqat bitgan, yangilansa bo'ladigan
    // (`upgradeable` — Pech ham shu jumladan, garchi `buildable` bo'lmasa
    // ham) va maksimumga yetmagan binoda; rangi yog'och yetarliligini aks
    // ettiradi.
    let show_upgrade = b.kind.upgradeable() && !b.under_construction() && !b.at_max_level();
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
    // V0.14: only a FINISHED buildable building can be relocated (mirrors
    // `GameState::can_relocate`'s own gate) — unlike Demolish, which works
    // on a construction site too.
    let relocate_display = if b.kind.buildable() && !b.under_construction() {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut nodes.p6() {
        if node.display != relocate_display {
            node.display = relocate_display;
        }
    }
    // V0.16: rotating shares the exact same gate as relocating (finished,
    // buildable) — reuse `relocate_display`.
    for mut node in &mut nodes.p7() {
        if node.display != relocate_display {
            node.display = relocate_display;
        }
    }
    // Burn-intensity buttons (0-3, `state.furnace_level`) appear only for
    // the Furnace, only once it's been lit at least once (`SetFurnaceLevel`
    // is a no-op before that — see `state.furnace_level > 0` below, kept
    // rather than `!b.under_construction()` since a V0.9 level upgrade,
    // a SEPARATE `b.level` 1-10 axis, re-sets `build_left` too but the
    // furnace is still lit throughout), AND only once it's grown past the
    // rough "gulxan" tier into an established `Pech` (`b.level >= 7`,
    // matching `render/buildings.rs`'s two-tier model) — a campfire has no
    // damper to dial in, only a real furnace does. `SetFurnaceLevel`
    // enforces the same level-7 floor server-side.
    let furnace_display = if b.kind == BuildingKind::Furnace && state.furnace_level > 0 && b.level >= 7 {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut nodes.p3() {
        if node.display != furnace_display {
            node.display = furnace_display;
        }
    }
    // V0.13: caravan quick-trade buttons — only for the Tunnel, and only
    // once it's at least breached (`tunnel.unlocked`, same gate as
    // `PlayerCommand::DispatchTradeCaravan` itself checks server-side).
    let caravan_display =
        if b.kind == BuildingKind::Tunnel && state.tunnel.unlocked && !state.central {
            Display::Flex
        } else {
            Display::None
        };
    for mut node in &mut nodes.p5() {
        if node.display != caravan_display {
            node.display = caravan_display;
        }
    }

    let info = match b.kind {
        // Only the very first (unlit) construction reads as "not built
        // yet" — a later V0.9 level upgrade is already a working furnace,
        // just being improved, so it keeps showing the normal burn stats.
        BuildingKind::Furnace if b.under_construction() && state.furnace_level == 0 => {
            i18n_hud::sel_info_furnace_building(lang)
        }
        BuildingKind::Furnace => i18n_hud::sel_info_furnace(
            state.furnace_level,
            state.furnace_level as f32 * FURNACE_COAL_PER_DAY_PER_LEVEL,
            frozen_city::game::types::WOOD_FUEL_PENALTY,
            state.heat_radius(),
            b.level >= 7,
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
            state.wildlife.deer,
            state.wildlife.rabbit,
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
        BuildingKind::TailorShop => i18n_hud::sel_info_tailor_shop(
            b.kind.production_per_worker_day() * b.kind.max_workers() as f32,
            state.stock.fur,
            lang,
        ),
        // Decorative, no stats to report — the building's own description
        // already says everything there is to say.
        BuildingKind::Wall | BuildingKind::Gate => i18n_names::building_desc(b.kind, lang).to_string(),
        BuildingKind::Well => i18n_hud::sel_info_well(
            b.kind.production_per_worker_day() * b.kind.max_workers() as f32,
            state.stock.water,
            lang,
        ),
        BuildingKind::Farmhouse => i18n_hud::sel_info_farmhouse(
            b.kind.production_per_worker_day() * b.kind.max_workers() as f32,
            state.livestock.cow,
            state.livestock.sheep,
            lang,
        ),
        BuildingKind::Tunnel => i18n_hud::sel_info_tunnel(
            state.tunnel.unlocked,
            state.tunnel.stage,
            frozen_city::game::types::TUNNEL_STAGES,
            state.pending_migrant.map(|m| m.count),
            state.stock.gold,
            state.pending_caravan.map(|c| (c.selling, c.good, c.amount)),
            lang,
        ),
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
                if b.kind == BuildingKind::Furnace && b.under_construction() && state.furnace_level == 0 {
                    // `build_left` counts down in whole logs here, not
                    // wood-cost-derived worker-days — see `FURNACE_LOGS_NEEDED`.
                    let delivered = FURNACE_LOGS_NEEDED.saturating_sub(b.build_left.round() as u32);
                    i18n_hud::furnace_construction_line(
                        delivered,
                        FURNACE_LOGS_NEEDED,
                        b.workers,
                        CONSTRUCTION_CREW_MAX,
                        lang,
                    )
                } else if b.under_construction() {
                    let total = if b.level <= 1 {
                        b.kind.build_workdays()
                    } else {
                        b.kind.upgrade_workdays(b.level)
                    };
                    let pct = (((1.0 - b.build_left / total.max(1e-6)).clamp(0.0, 1.0)) * 100.0) as u32;
                    i18n_hud::construction_line(pct, b.workers, CONSTRUCTION_CREW_MAX, lang)
                } else if !b.kind.buildable() {
                    // Pechning o'z darajasi (0-3) bor — V0.8 qatori unga
                    // taalluqli emas.
                    String::new()
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

/// Anchors the building info panel next to the selected building's on-screen
/// position instead of a fixed screen corner — re-projected every frame via
/// `Camera::world_to_viewport`, the same trick `chat.rs`'s speech bubbles and
/// `render::update_cursor_labels`'s nameplates use. Prefers floating above
/// the building; flips below when there isn't room, and clamps to the
/// window so it never slides off-screen as the camera orbits/zooms.
pub fn sync_selection_panel_position(
    view: Res<GameView>,
    selection: Res<Selection>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut panel: Query<&mut Node, With<SelPanelRoot>>,
) {
    // The panel's own width is fixed (`hud.rs`); its height varies with
    // content (Furnace level row, worker row, ...) but computed layout size
    // isn't available until after this frame renders, so a generous fixed
    // estimate is used for the above/below flip and the clamp instead.
    const WIDTH: f32 = 300.0;
    const EST_HEIGHT: f32 = 380.0;
    const GAP: f32 = 28.0;

    let Some(state) = view.ready() else { return };
    let Some(b) = selection.0.and_then(|id| state.find_building(id)) else { return };
    let Ok((cam, cam_gt)) = camera.single() else { return };
    let Ok(mut node) = panel.single_mut() else { return };
    let Ok(window) = windows.single() else { return };

    let Ok(p) = cam.world_to_viewport(cam_gt, building_center_world(b) + Vec3::Y * 0.9) else {
        return;
    };

    let max_left = (window.width() - WIDTH).max(0.0);
    let max_top = (window.height() - EST_HEIGHT).max(0.0);
    let above = p.y - EST_HEIGHT - GAP;
    let top = if above >= 0.0 { above } else { p.y + GAP };

    node.left = Val::Px((p.x - WIDTH / 2.0).clamp(0.0, max_left));
    node.top = Val::Px(top.clamp(0.0, max_top));
}

#[allow(clippy::too_many_arguments)]
pub fn selection_panel_buttons(
    net: Res<NetConn>,
    view: Res<GameView>,
    mut selection: ResMut<Selection>,
    mut relocate: ResMut<RelocateMode>,
    minus: Query<&Interaction, (Changed<Interaction>, With<WorkerMinus>)>,
    plus: Query<&Interaction, (Changed<Interaction>, With<WorkerPlus>)>,
    none_btn: Query<&Interaction, (Changed<Interaction>, With<WorkerNoneBtn>)>,
    max_btn: Query<&Interaction, (Changed<Interaction>, With<WorkerMaxBtn>)>,
    upgrade: Query<&Interaction, (Changed<Interaction>, With<UpgradeBtn>)>,
    demolish: Query<&Interaction, (Changed<Interaction>, With<DemolishBtn>)>,
    relocate_btn: Query<&Interaction, (Changed<Interaction>, With<RelocateBtn>)>,
    rotate_btn: Query<&Interaction, (Changed<Interaction>, With<RotateBuildingBtn>)>,
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
    // V0.14: hands off to `RelocateMode` — the actual `RelocateBuilding`
    // dispatch happens once the player picks a target tile (`input::build_input`).
    if relocate_btn.iter().any(|i| *i == Interaction::Pressed) {
        relocate.0 = Some(id);
    }
    // V0.16: rotate in place — validated server-side (`can_rotate`), reflected
    // next snapshot; the building re-enters construction at the discounted
    // timer, same as relocating.
    if rotate_btn.iter().any(|i| *i == Interaction::Pressed) {
        net.send(ClientMsg::Cmd(PlayerCommand::RotateBuilding { building: id }));
    }
}
