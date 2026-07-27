use bevy::prelude::*;

use frozen_city::game::types::{
    BuildingKind, FurnishingKind, PlayerCommand, CONSTRUCTION_CREW_MAX,
    FURNACE_COAL_PER_DAY_PER_LEVEL, FURNACE_LOGS_NEEDED, FURNISHING_MAX_LEVEL,
};
use frozen_city::net::protocol::ClientMsg;

use super::super::i18n::Lang;
use super::super::i18n_furnishing;
use super::super::i18n_hud;
use super::super::i18n_names;
use super::super::theme;
use super::super::*;
use super::*;

/// Furniture tile's one/two-letter glyph — a visual mark, not a word, so it
/// is NOT run through i18n (same convention as `BuildingKind::letter()`).
fn furnishing_glyph(kind: FurnishingKind) -> &'static str {
    match kind {
        FurnishingKind::Workbench => "Wb",
        FurnishingKind::Seating => "St",
        FurnishingKind::Heater => "Ht",
        FurnishingKind::Shelving => "Sh",
    }
}

/// A survivor's portrait-slot label: their name's first letter, uppercased.
fn survivor_initial(name: &str) -> String {
    name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
pub fn selection_panel_update(
    view: Res<GameView>,
    lang: Res<Lang>,
    mut selection: ResMut<Selection>,
    mut tab_state: Query<(&mut SelTab, &mut SelFurnSlot), With<SelPanelRoot>>,
    mut nodes: ParamSet<(
        Query<&mut Node, With<SelPanelRoot>>,
        // Furnace/Caravan/staffing rows and the Relocate/Demolish buttons —
        // all plain Node-visibility toggles, bundled under one component/
        // query (see `PanelAction`'s doc for why).
        Query<(&mut Node, &PanelAction)>,
        // Header Upgrade button: visibility + afford color.
        Query<(&mut Node, &mut BackgroundColor), With<UpgradeBtn>>,
        // Tab buttons: visibility (hidden if that tab doesn't apply to this
        // building) + active/inactive color.
        Query<(&mut Node, &mut BackgroundColor, &TabBtn)>,
        // Tab content roots: visible while active AND applicable.
        Query<(&mut Node, &TabRoot)>,
        // Furniture tab: tile existence/highlight + the detail card's own
        // Upgrade button — bundled, see `FurnitureCard`'s doc.
        Query<(&mut Node, &mut BackgroundColor, &FurnitureCard)>,
        // Survivors tab: portrait circle existence/tint + the padlock glyph
        // inside it — bundled, see `SurvivorSlot`'s doc. The Lock pieces
        // have no `BackgroundColor` of their own, hence `Option`.
        Query<(&mut Node, Option<&mut BackgroundColor>, &SurvivorSlot)>,
    )>,
    mut texts: Query<(&mut Text, &SelText)>,
    mut last_building: Local<Option<u32>>,
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
    let Some(b) = sel else {
        *last_building = None;
        return;
    };

    // V0.21: which building is selected changed (or this is the first frame
    // one is) — reset the tab/tile pick to sane defaults rather than
    // carrying over a choice that may not even apply to the new building.
    let building_changed = *last_building != Some(b.id);
    *last_building = Some(b.id);

    let furnishings = b.kind.furnishings();
    // Furniture tab: only a FINISHED building whose kind has fittings at all
    // (`furnishings()` is empty for Tent/Wall/Gate/Furnace/Tunnel — nothing
    // to furnish, see that method's doc).
    let furn_available = !furnishings.is_empty() && !b.under_construction();
    // Survivors tab: a construction site's named crew, or a finished
    // building with worker slots. Deliberately NOT gated by `!state.central`
    // (unlike `PanelAction::WorkerAdjustControls` below) — named assignment
    // via `AssignHereBtn`/roster still works in the central world, only the
    // ANONYMOUS `AdjustWorkers` controls are refused there (see
    // `GameState::can_issue`'s central branch).
    let can_work_here = (b.under_construction()
        && (b.kind != BuildingKind::Furnace || state.furnace_level > 0))
        || b.kind.max_workers() > 0;
    let survivors_available = can_work_here;

    let Ok((mut tab, mut slot)) = tab_state.single_mut() else { return };
    if building_changed {
        *tab = if furn_available { SelTab::Furniture } else { SelTab::Survivors };
        slot.0 = 0;
    } else {
        // Clamp: if the previously-active tab no longer applies to this
        // building, fall back to whichever one still does.
        if *tab == SelTab::Furniture && !furn_available && survivors_available {
            *tab = SelTab::Survivors;
        } else if *tab == SelTab::Survivors && !survivors_available && furn_available {
            *tab = SelTab::Furniture;
        }
        if slot.0 as usize >= furnishings.len() {
            slot.0 = 0;
        }
    }
    let active_tab = *tab;
    let sel_slot = slot.0;

    // V0.21: header — "{Name} Lv. N" for anything with a meaningful level
    // (`upgradeable`, same gate `show_upgrade` below uses); just the name
    // for the Tunnel, whose `level`/`build_left` are inert fixtures.
    let name = i18n_names::building_name(b.kind, lang);
    let header = if b.kind.upgradeable() {
        i18n_furnishing::panel_header(name, b.level, lang)
    } else {
        name.to_string()
    };

    // V0.8/V0.9: Yangilash tugmasi — faqat bitgan, yangilansa bo'ladigan
    // (`upgradeable` — Pech ham shu jumladan, garchi `buildable` bo'lmasa
    // ham) va maksimumga yetmagan binoda; rangi yog'och yetarliligini aks
    // ettiradi.
    let show_upgrade = b.kind.upgradeable() && !b.under_construction() && !b.at_max_level();
    // V0.20: `furnishings_keep_pace()` is the SAME gate `can_issue`/
    // `sim::command`'s `UpgradeBuilding` arm enforce server-side — a bare or
    // half-furnished room stalls the building's own level. Folded into
    // `affordable` so the button greys out for this reason too, and the text
    // below (`SelText::Upgrade`) explains WHY instead of the button just
    // silently doing nothing when clicked.
    let furnishings_ok = b.furnishings_keep_pace();
    let affordable = show_upgrade
        && furnishings_ok
        && state.stock.wood >= b.kind.upgrade_cost_wood(b.level + 1) as f32;
    for (mut node, mut bg) in &mut nodes.p2() {
        let d = if show_upgrade { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
        }
        let want = if affordable { theme::BTN_SUCCESS } else { theme::BTN_DIM };
        if bg.0 != want {
            bg.0 = want;
        }
    }

    // V0.21: per-kind extras + the anonymous staffing block — all plain
    // Node-visibility toggles bundled under `PanelAction`.
    let demolish_display = b.kind.buildable();
    // V0.14: only a FINISHED buildable building can be relocated (mirrors
    // `GameState::can_relocate`'s own gate) — unlike Demolish, which works
    // on a construction site too.
    let relocate_display = b.kind.buildable() && !b.under_construction();
    // Burn-intensity buttons (0-3, `state.furnace_level`) appear only for
    // the Furnace, only once it's been lit at least once, AND only once it's
    // grown past the rough "gulxan" tier into an established "Pech"
    // (`b.level >= 7`, matching `render/buildings.rs`'s two-tier model) — a
    // campfire has no damper to dial in, only a real furnace does.
    let furnace_display =
        b.kind == BuildingKind::Furnace && state.furnace_level > 0 && b.level >= 7;
    // V0.13: caravan quick-trade buttons — only for the Tunnel, and only
    // once it's at least breached (`tunnel.unlocked`, same gate
    // `PlayerCommand::DispatchTradeCaravan` itself checks server-side).
    let caravan_display =
        b.kind == BuildingKind::Tunnel && state.tunnel.unlocked && !state.central;
    // MARKAZIY olamda `AdjustWorkers` umuman rad etiladi (har bir aholi
    // akkauntga tegishli) — shu block faqat u yerda yashiriladi, "Shu yerga
    // tayinlash" (`AssignSurvivor`) esa (portret qatori bilan birga) ishlab
    // turaveradi.
    let worker_adjust_display = !state.central && can_work_here;
    for (mut node, action) in &mut nodes.p1() {
        let want = match action {
            PanelAction::Demolish => demolish_display,
            PanelAction::Relocate => relocate_display,
            PanelAction::FurnaceControls => furnace_display,
            PanelAction::CaravanControls => caravan_display,
            PanelAction::WorkerAdjustControls => worker_adjust_display,
        };
        let d = if want { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
        }
    }

    // V0.21: tab strip — hide a tab that has nothing to show for this
    // building, color the active one.
    for (mut node, mut bg, TabBtn(t)) in &mut nodes.p3() {
        let available = match t {
            SelTab::Furniture => furn_available,
            SelTab::Survivors => survivors_available,
        };
        let d = if available { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
        }
        let want = if *t == active_tab { theme::BTN_ACTIVE } else { theme::BTN };
        if bg.0 != want {
            bg.0 = want;
        }
    }
    for (mut node, TabRoot(t)) in &mut nodes.p4() {
        let available = match t {
            SelTab::Furniture => furn_available,
            SelTab::Survivors => survivors_available,
        };
        let want = if available && *t == active_tab { Display::Flex } else { Display::None };
        if node.display != want {
            node.display = want;
        }
    }

    // V0.21: the Furniture tab's numbers — all read straight off the
    // SELECTED tile's fitting; `sel_kind` is `None` only when this building
    // takes fewer than 3 fittings (the unused pre-spawned tiles).
    let sel_kind = furnishings.get(sel_slot as usize).copied();
    let sel_level = sel_kind.map(|_| b.furnishing_level(sel_slot as usize)).unwrap_or(0);
    let cycle_now = b.cycle_at(sel_slot as usize, sel_level);
    let cycle_next = if sel_level < FURNISHING_MAX_LEVEL {
        b.cycle_at(sel_slot as usize, sel_level + 1)
    } else {
        None
    };
    // "Time 7.8s -0.2s": what the pending upgrade would change the cycle
    // TIME by (negative = faster), previewed regardless of affordability.
    let time_delta = match (cycle_now, cycle_next) {
        (Some(now), Some(next)) => Some(next.seconds() - now.seconds()),
        _ => None,
    };
    // Consumption only applies where the building actually consumes a
    // stockpile good per cycle — today that's just the Tailor Shop's
    // Workbench (fur -> cloth, `FUR_PER_CLOTH`); everywhere else is a dash
    // rather than a fabricated number.
    let consumption = match sel_kind {
        Some(FurnishingKind::Workbench) if b.kind == BuildingKind::TailorShop && cycle_now.is_some() => {
            Some(frozen_city::game::types::FUR_PER_CLOTH)
        }
        _ => None,
    };
    let next_step = b.next_furnishing_step(sel_slot as usize);
    let furn_buyable = next_step.is_some_and(|(_, cost)| state.stock.wood >= cost);

    for (mut node, mut bg, marker) in &mut nodes.p5() {
        match marker {
            FurnitureCard::Tile(tile_slot) => {
                let exists = (*tile_slot as usize) < furnishings.len();
                let d = if exists { Display::Flex } else { Display::None };
                if node.display != d {
                    node.display = d;
                }
                let want = if exists && *tile_slot == sel_slot {
                    theme::BTN_ACTIVE
                } else {
                    theme::BG_SECTION
                };
                if bg.0 != want {
                    bg.0 = want;
                }
            }
            FurnitureCard::UpgradeBtn => {
                let want = if furn_buyable { theme::BTN_SUCCESS } else { theme::BTN_DIM };
                if bg.0 != want {
                    bg.0 = want;
                }
            }
        }
    }

    // V0.21: the Survivors tab's portrait strip — up to `cap` slots exist,
    // filled by whichever survivors are actually assigned here (stable
    // order by id, so a slot doesn't swap identities frame to frame).
    let cap = if b.under_construction() {
        CONSTRUCTION_CREW_MAX
    } else {
        b.kind.max_workers()
    };
    let mut assigned: Vec<&frozen_city::game::types::Survivor> = state
        .survivors
        .iter()
        .filter(|s| s.assigned_building == Some(b.id))
        .collect();
    assigned.sort_by_key(|s| s.id);

    for (mut node, bg, marker) in &mut nodes.p6() {
        match marker {
            SurvivorSlot::Root(s) => {
                let exists = *s < cap;
                let d = if exists { Display::Flex } else { Display::None };
                if node.display != d {
                    node.display = d;
                }
                if let Some(mut bg) = bg {
                    let want = match assigned.get(*s as usize) {
                        Some(survivor) if exists => profession_coat_color(survivor.profession),
                        _ => theme::BG_SECTION,
                    };
                    if bg.0 != want {
                        bg.0 = want;
                    }
                }
            }
            SurvivorSlot::Lock(s) => {
                let filled = (*s as usize) < assigned.len() && *s < cap;
                let d = if *s < cap && !filled { Display::Flex } else { Display::None };
                if node.display != d {
                    node.display = d;
                }
            }
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
        BuildingKind::SnowCrew => i18n_hud::sel_info_snow_crew(
            frozen_city::game::types::SNOW_CREW_RADIUS,
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
            SelText::Title => header.clone(),
            SelText::Info => info.clone(),
            SelText::Count => i18n_furnishing::survivor_slot_count(b.workers, cap, lang),
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
                } else {
                    // The header above already says "{Name} Lv. N" — no need
                    // to repeat a bare level line for a finished building.
                    String::new()
                }
            }
            SelText::Upgrade => {
                if b.at_max_level() {
                    i18n_hud::upgrade_btn_max(lang).to_string()
                } else if !b.furnishings_keep_pace() {
                    // V0.20: the room itself is why this button won't do
                    // anything yet — say so instead of showing a normal
                    // "Upgrade → L{n}" label the player could click forever.
                    i18n_furnishing::furnish_first_btn(lang).to_string()
                } else {
                    i18n_hud::upgrade_btn(b.level + 1, b.kind.upgrade_cost_wood(b.level + 1), lang)
                }
            }
            SelText::TileGlyph(tile_slot) => match furnishings.get(*tile_slot as usize) {
                Some(&k) => furnishing_glyph(k).to_string(),
                None => String::new(),
            },
            SelText::TileBadge(tile_slot) => match furnishings.get(*tile_slot as usize) {
                Some(_) => {
                    let lvl = b.furnishing_level(*tile_slot as usize);
                    if lvl == 0 { "-".to_string() } else { format!("L{lvl}") }
                }
                None => String::new(),
            },
            SelText::FurnName => match sel_kind {
                Some(k) => i18n_furnishing::furnishing_header(k, sel_level, FURNISHING_MAX_LEVEL, lang),
                None => String::new(),
            },
            SelText::FurnDesc => match sel_kind {
                Some(k) => i18n_furnishing::furnishing_desc(k, lang).to_string(),
                None => String::new(),
            },
            SelText::FurnUpgrade => match (sel_kind, next_step) {
                (Some(_), Some((next, cost))) => {
                    i18n_furnishing::furniture_upgrade_btn(sel_level, next, cost, state.stock.wood, lang)
                }
                // Maxed, or this tile doesn't exist on this building (row
                // hidden either way) — reuse the building's own "max level"
                // text rather than a near-duplicate string.
                _ => i18n_hud::upgrade_btn_max(lang).to_string(),
            },
            SelText::FurnStatProduction => {
                i18n_furnishing::stat_value_production(cycle_now.map(|c| c.output), lang)
            }
            SelText::FurnStatConsumption => i18n_furnishing::stat_value_consumption(consumption, lang),
            SelText::FurnStatStats => match sel_kind {
                Some(k) => i18n_furnishing::furnishing_stat_line(k, k.per_level(), lang),
                None => String::new(),
            },
            SelText::FurnStatTime => {
                i18n_furnishing::stat_value_time(cycle_now.map(|c| c.seconds()), time_delta, lang)
            }
            SelText::SurvivorInitial(s) => {
                if *s < cap {
                    assigned.get(*s as usize).map(|surv| survivor_initial(&surv.name)).unwrap_or_default()
                } else {
                    String::new()
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
    // content (tabs, tile strip, stats grid, ...) but computed layout size
    // isn't available until after this frame renders, so a generous fixed
    // estimate is used for the above/below flip and the clamp instead.
    const WIDTH: f32 = 320.0;
    const EST_HEIGHT: f32 = 480.0;
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
    mut tab_state: Query<(&mut SelTab, &mut SelFurnSlot), With<SelPanelRoot>>,
    minus: Query<&Interaction, (Changed<Interaction>, With<WorkerMinus>)>,
    plus: Query<&Interaction, (Changed<Interaction>, With<WorkerPlus>)>,
    none_btn: Query<&Interaction, (Changed<Interaction>, With<WorkerNoneBtn>)>,
    max_btn: Query<&Interaction, (Changed<Interaction>, With<WorkerMaxBtn>)>,
    upgrade: Query<&Interaction, (Changed<Interaction>, With<UpgradeBtn>)>,
    panel_action: Query<(&Interaction, &PanelAction), Changed<Interaction>>,
    tab_btn: Query<(&Interaction, &TabBtn), Changed<Interaction>>,
    furn_card: Query<(&Interaction, &FurnitureCard), Changed<Interaction>>,
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
    for (interaction, action) in &panel_action {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            PanelAction::Demolish => {
                net.send(ClientMsg::Cmd(PlayerCommand::Demolish { building: id }));
                selection.0 = None;
            }
            // V0.14: hands off to `RelocateMode`. V0.18: the click that
            // follows only DROPS the ghost on a tile — the confirm bar's
            // ✓/⟳/✗ then decides both the spot and the heading, and ✓
            // dispatches one `RelocateFacing` (`ui::placement::
            // placement_buttons`). Turning a building without moving it is
            // the same flow, confirmed on the tile it already occupies,
            // which is why there is no separate rotate button here anymore.
            PanelAction::Relocate => {
                relocate.0 = Some(id);
            }
            // These wrapper entities are never `Button`s themselves — only
            // the individual controls inside them are (`FurnaceLvlBtn`/
            // `CaravanBtn`, driven by `furnace_buttons`/`caravan_buttons` in
            // `buildbar.rs`; the staffing block's own buttons are handled
            // individually above) — so this query never actually yields
            // these variants. Kept for exhaustiveness.
            PanelAction::FurnaceControls | PanelAction::CaravanControls | PanelAction::WorkerAdjustControls => {}
        }
    }

    // V0.21: tab strip + furniture tile pick/buy — scoped so a (should never
    // happen) missing `SelPanelRoot` entity can't block the handlers above.
    if let Ok((mut tab, mut slot)) = tab_state.single_mut() {
        for (interaction, TabBtn(t)) in &tab_btn {
            if *interaction == Interaction::Pressed {
                *tab = *t;
            }
        }
        let cur_slot = slot.0;
        for (interaction, marker) in &furn_card {
            if *interaction != Interaction::Pressed {
                continue;
            }
            match marker {
                FurnitureCard::Tile(s) => slot.0 = *s,
                // V0.20: buy/upgrade the CURRENTLY SELECTED fitting.
                // Validation (cost, max level, slot existence) is
                // server-side (`sim::command`'s `UpgradeFurnishing` arm) —
                // the button's greyed color (`selection_panel_update`) is
                // only a preview, same convention as `UpgradeBtn` above.
                FurnitureCard::UpgradeBtn => {
                    net.send(ClientMsg::Cmd(PlayerCommand::UpgradeFurnishing {
                        building: id,
                        slot: cur_slot,
                    }));
                }
            }
        }
    }
}
