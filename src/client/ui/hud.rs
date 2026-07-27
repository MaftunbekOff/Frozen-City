use bevy::prelude::*;

use frozen_city::game::types::FURNACE_COAL_PER_DAY_PER_LEVEL;

use super::super::i18n::Lang;
use super::super::i18n_furnishing;
use super::super::i18n_hud;
use super::super::theme::{
    self, BG_PANEL, BORDER, BTN, BTN_DANGER, FS_BODY, FS_MICRO, FS_SMALL, FS_TITLE, RES_COAL,
    RES_FOOD, RES_GOLD, RES_WATER, RES_WOOD, SP_MD, SP_SM, SP_XS, TEXT_MUTED, TEXT_PRIMARY,
};
use super::super::*;
use super::*;

/// Convenience wrapper around [`theme::button`] for the common case of a
/// fixed pixel width, so call sites below don't all spell `Val::Px(..)`.
fn btn_px(w: f32, h: f32, bg: Color) -> impl Bundle {
    theme::button(Val::Px(w), h, bg)
}

pub fn spawn_hud(
    mut commands: Commands,
    ff: Res<theme::FormFactor>,
    lang: Res<Lang>,
    mut panel_open: ResMut<BuildPanelOpen>,
) {
    let ff = *ff;
    let lang = *lang;
    // Every game entry starts with the Build modal CLOSED (the resource
    // otherwise persists last session's state).
    panel_open.0 = false;

    // --- Top bar: one compact row on Desktop/Tablet; two rows on Mobile so
    // nothing gets clipped at phone widths (resources up top, status +
    // Menu below). Chosen once at spawn time, like every other `FormFactor`
    // layout in this codebase (see `theme::modal_panel`) — a live resize
    // mid-session just waits for the next HUD (re)spawn to pick up the
    // other layout, same tradeoff `theme::FormFactor`'s doc comment states.
    if ff.compact() {
        spawn_top_bar_mobile(&mut commands, lang);
    } else {
        spawn_top_bar_desktop(&mut commands, ff, lang);
    }
    // Pastki morale bari faqat Desktopda: planshet/telefonda pastki chekka
    // joy tanqis — u yerda kayfiyat yuqori panel satrida qoladi. (Markaziy
    // harorat medalyoni yuqori panelning O'Z bolasi — `spawn_top_bar_desktop`
    // ichida — shunda u har doim panel USTIDA chiziladi.)
    if ff == theme::FormFactor::Desktop {
        spawn_morale_bar(&mut commands);
    }

    // --- Qurish: pastki-o'ng burchakda YAGONA dumaloq tugma, menyu esa
    // bosilganda ochiladigan modal — qarang `spawn_build_panel`
    // (`buildbar.rs`). ---
    spawn_build_panel(&mut commands, ff, lang);

    // --- Tooltip / hint line, bottom-left (clear of the bottom-right panel) ---
    let hint = if ff.compact() {
        i18n_hud::default_hint_mobile(lang)
    } else {
        i18n_hud::default_hint_desktop(lang)
    };
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(14.0),
            bottom: Val::Px(SP_SM),
            // Desktopda 40%: markazdagi morale bariga deyarli tegmaydi, lekin
            // boshqaruv eslatmasi 2-3 qatordan oshib chat zonasiga (bottom
            // 120+) ko'tarilib ketmaydi; mobilda pastki markaz bo'sh — kengroq.
            max_width: Val::Percent(if ff.compact() { 55.0 } else { 40.0 }),
            ..default()
        },
        theme::text(hint, FS_SMALL, TEXT_MUTED),
        TooltipText,
        DespawnOnExit(Screen::Game),
    ));

    // --- World-switch transition banner: a big, brief, center-screen line
    // ("Entering the Global World...") that fades out once the new world has
    // actually loaded (see `TransitionMsg`). Deliberately not part of the
    // top bar's fixed-position rows so it can't collide with any of them.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            // Markaziy medalyon (~130px) va event/social bannerlari
            // (134/172px) ostida.
            top: Val::Px(216.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        theme::text("", FS_TITLE + 6.0, TEXT_PRIMARY),
        TransitionText,
        DespawnOnExit(Screen::Game),
    ));

    // --- FPS readout (below the top bar) ---
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(14.0),
            top: Val::Px(54.0),
            ..default()
        },
        theme::text("", FS_SMALL, TEXT_MUTED),
        FpsText,
        DespawnOnExit(Screen::Game),
    ));

    // --- Event feed (right side) --- (capped narrower on Mobile, with a
    // smaller font, so it can't collide with the minimap on the opposite
    // side of a phone-width screen)
    let events_font = if ff.compact() { FS_MICRO } else { FS_SMALL };
    let mut events_node = Node {
        position_type: PositionType::Absolute,
        right: Val::Px(12.0),
        top: Val::Px(54.0),
        padding: UiRect::all(Val::Px(SP_SM)),
        // Chap qirradagi muz-ko'k urg'u chizig'i — lenta "jurnal kartasi"
        // bo'lib o'qilsin (dizayn-tizim panellariga hamohang).
        border: UiRect::left(Val::Px(2.0)),
        border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
        ..default()
    };
    if ff.compact() {
        events_node.max_width = Val::Percent(52.0);
    } else {
        events_node.width = Val::Px(340.0);
    }
    commands
        .spawn((
            events_node,
            BackgroundColor(Color::srgba(0.010, 0.025, 0.055, 0.50)),
            BorderColor::all(Color::srgba(0.480, 0.780, 0.930, 0.35)),
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn((theme::text("", events_font, Color::srgba(0.85, 0.90, 1.0, 0.9)), HudField::Events));
        });

    // --- Selection panel --- floats next to whichever building is selected
    // (`sync_selection_panel_position` re-projects its world position to
    // screen space every frame) instead of a fixed screen corner; `left`/
    // `top` below are just a harmless first-frame default before that system
    // places it, and get overwritten immediately once a building is picked.
    //
    // V0.21: rebuilt to match the reference mobile design — a header (name +
    // level, an Upgrade button on the right), a Furniture/Survivors tab
    // strip, and each tab's own content below it (see `spawn_furniture_tab`/
    // `spawn_survivors_tab`). Per-kind extras that aren't part of either tab
    // (Furnace burn buttons, Tunnel caravan quick-trade, Relocate, Demolish)
    // still live at the bottom, unchanged in behavior — see `PanelAction`.
    commands
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                top: Val::Px(12.0),
                width: Val::Px(320.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(SP_SM),
                padding: UiRect::all(Val::Px(SP_MD)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(theme::RAD_PANEL)),
                ..default()
            },
            BackgroundColor(BG_PANEL),
            BorderColor::all(BORDER),
            BoxShadow::new(
                Color::srgba(0.0, 0.0, 0.0, 0.45),
                Val::Px(0.0),
                Val::Px(6.0),
                Val::Px(2.0),
                Val::Px(20.0),
            ),
            Interaction::default(),
            UiBlocker,
            SelPanelRoot,
            // V0.21: the panel's own tab/slot pick (see `SelTab`/
            // `SelFurnSlot`'s doc) lives on this SAME entity rather than a
            // `Resource`, so it survives frame to frame without touching
            // `ClientPlugin::build` in `mod.rs`.
            SelTab::default(),
            SelFurnSlot::default(),
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            // --- Header: "{Name} Lv. N" on the left, Upgrade on the right.
            p.spawn(Node {
                width: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                column_gap: Val::Px(SP_SM),
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    theme::text("Building", theme::FS_SECTION, theme::BRASS),
                    Node {
                        flex_grow: 1.0,
                        ..default()
                    },
                    SelText::Title,
                ));
                // V0.8/V0.21: raise the building a level — moved from a
                // full-width footer button into the header (matching the
                // reference design); color (afford/maxed) and the
                // furnish-first explainer are still owned by
                // `selection_panel_update`.
                row.spawn((
                    Button,
                    Node {
                        width: Val::Px(84.0),
                        height: Val::Px(28.0),
                        flex_shrink: 0.0,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                        ..default()
                    },
                    BackgroundColor(theme::BTN_SUCCESS),
                    UpgradeBtn,
                ))
                .with_children(|b| {
                    b.spawn((theme::text("", FS_MICRO + 1.0, TEXT_PRIMARY), SelText::Upgrade));
                });
            });
            p.spawn(theme::brass_divider());
            p.spawn((theme::text("", FS_MICRO + 1.0, TEXT_MUTED), SelText::Info));
            // V0.8: construction-progress status line — empty once the
            // building is standing (the header above already says "Lv. N").
            p.spawn((theme::text("", FS_MICRO + 1.0, theme::ACCENT_ICE), SelText::Level));

            spawn_furnace_row(p, lang);
            spawn_caravan_row(p, lang);

            // --- Tab strip: Furniture | Survivors.
            p.spawn(Node {
                width: Val::Percent(100.0),
                column_gap: Val::Px(SP_XS),
                ..default()
            })
            .with_children(|row| {
                for tab in [SelTab::Furniture, SelTab::Survivors] {
                    let label = match tab {
                        SelTab::Furniture => i18n_furnishing::tab_furniture(lang),
                        SelTab::Survivors => i18n_furnishing::tab_survivors(lang),
                    };
                    row.spawn((
                        Button,
                        Node {
                            flex_grow: 1.0,
                            flex_basis: Val::Px(0.0),
                            height: Val::Px(28.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                            ..default()
                        },
                        BackgroundColor(BTN),
                        TabBtn(tab),
                    ))
                    .with_children(|b| {
                        b.spawn(theme::text(label, FS_SMALL, TEXT_PRIMARY));
                    });
                }
            });

            spawn_furniture_tab(p, lang);
            spawn_survivors_tab(p, lang);

            // V0.14: Ko'chirish — bosilganda `RelocateMode`ga o'tadi;
            // ko'rinishini `selection_panel_update` boshqaradi (faqat
            // qurilishi mumkin, bitgan binolarda).
            //
            // V0.18: yonida alohida "Aylantirish" tugmasi bor edi — bitta
            // qarorni ("bino qayerda tursin va qay tomonga qarasin") ikkita
            // tugmaga bo'lib yuborardi, va binoni joyida burish uchun uni
            // ko'chirish rejimisiz bosish kerak edi. Endi burish ko'chirish
            // oqimining ichida: arvoh tushirilgach chiqadigan ✓/⟳/✗ paneli
            // (`ui::placement`) ikkalasini birga hal qiladi va bitta
            // `RelocateFacing` buyrug'i bilan yuboradi. Joyida burish ham
            // shu yerda — o'sha taylning o'ziga tasdiqlash kifoya.
            spawn_panel_action(
                p,
                theme::button(Val::Percent(100.0), 30.0, BTN),
                PanelAction::Relocate,
                i18n_hud::relocate_label(lang),
            );
            spawn_panel_action(
                p,
                theme::button(Val::Percent(100.0), 30.0, BTN_DANGER),
                PanelAction::Demolish,
                i18n_hud::demolish_label(lang),
            );
        });

    // --- Game over overlay --- (starts as `theme::scrim`'s default `Flex`
    // for a single `OnEnter`->`Update` transition, but `game_over_ui` runs
    // later in that same frame and immediately corrects it to `None` since
    // the phase always starts `Running` — nothing is ever actually presented
    // in between, so no flash).
    commands
        .spawn((
            theme::scrim(ff),
            GameOverRoot,
            UiBlocker,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn(theme::modal_panel(ff)).with_children(|panel| {
                panel.spawn((
                    theme::text("", FS_TITLE + 8.0, TEXT_PRIMARY),
                    Node {
                        align_self: AlignSelf::Center,
                        ..default()
                    },
                    GoText::Title,
                ));
                panel.spawn((theme::text("", FS_BODY + 1.0, TEXT_MUTED), GoText::Info));
                let central_h = ff.btn_h().max(46.0);
                panel
                    .spawn((
                        theme::button(Val::Percent(100.0), central_h, Color::srgb(0.13, 0.35, 0.45)),
                        EnterCentralBtn,
                    ))
                    .with_children(|b| {
                        b.spawn(theme::text(i18n_hud::enter_global_world_btn(lang), FS_BODY, TEXT_PRIMARY));
                    });
                panel
                    .spawn((theme::button(Val::Percent(100.0), central_h, BTN), GameOverBack))
                    .with_children(|b| {
                        b.spawn(theme::text(i18n_hud::return_to_menu_btn(lang), FS_BODY, TEXT_PRIMARY));
                    });
            });
        });
}

/// V0.21: Furnace burn-level buttons (0-3) — shown only while the Furnace is
/// selected and controllable (`PanelAction::FurnaceControls`, gated in
/// `selection_panel_update`). `furnace_buttons` (`buildbar.rs`) drives each
/// `FurnaceLvlBtn`'s own color; this fn only lays the row out.
fn spawn_furnace_row(p: &mut ChildSpawnerCommands, lang: Lang) {
    p.spawn((
        Node {
            display: Display::None,
            width: Val::Percent(100.0),
            flex_wrap: FlexWrap::Wrap,
            column_gap: Val::Px(SP_XS),
            row_gap: Val::Px(SP_XS),
            ..default()
        },
        PanelAction::FurnaceControls,
    ))
    .with_children(|row| {
        for lvl in 0u8..=3 {
            row.spawn((
                Button,
                Node {
                    min_width: Val::Px(46.0),
                    height: Val::Px(32.0),
                    padding: UiRect::horizontal(Val::Px(SP_SM)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                    ..default()
                },
                BackgroundColor(BTN),
                FurnaceLvlBtn(lvl),
            ))
            .with_children(|b| {
                let label = if lvl == 0 {
                    i18n_hud::furnace_off_label(lang).to_string()
                } else {
                    lvl.to_string()
                };
                b.spawn(theme::text(label, FS_BODY - 1.0, TEXT_PRIMARY));
            });
        }
    });
}

/// V0.13: Tunnel trade caravan quick-actions — shown only when the Tunnel is
/// selected and unlocked (`PanelAction::CaravanControls`, gated in
/// `selection_panel_update`). Each button dispatches a fixed
/// `QUICK_TRADE_AMOUNT` of one good; `caravan_buttons` drives them.
fn spawn_caravan_row(p: &mut ChildSpawnerCommands, lang: Lang) {
    p.spawn((
        Node {
            display: Display::None,
            width: Val::Percent(100.0),
            flex_wrap: FlexWrap::Wrap,
            column_gap: Val::Px(SP_XS),
            row_gap: Val::Px(SP_XS),
            ..default()
        },
        PanelAction::CaravanControls,
    ))
    .with_children(|row| {
        for good in frozen_city::game::types::TradeGood::ALL {
            for selling in [true, false] {
                row.spawn((
                    Button,
                    Node {
                        min_width: Val::Px(84.0),
                        height: Val::Px(32.0),
                        padding: UiRect::horizontal(Val::Px(SP_SM)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                        ..default()
                    },
                    BackgroundColor(BTN),
                    CaravanBtn { good, selling },
                ))
                .with_children(|b| {
                    b.spawn(theme::text(
                        i18n_hud::caravan_btn_label(good, selling, lang),
                        FS_BODY - 2.0,
                        TEXT_PRIMARY,
                    ));
                });
            }
        }
    });
}

/// V0.21: Furniture tab — a horizontal strip of up to 3 fitting tiles (icon
/// glyph + corner level badge, the selected one highlighted), a detail card
/// for the SELECTED fitting (name/level, description, an Upgrade button with
/// cost-over-stock), and a 2-per-row stats grid (Production/Consumption,
/// Stats/Time) — the reference design's shape. Root visibility is
/// `TabRoot(SelTab::Furniture)`, driven by `selection_panel_update`.
fn spawn_furniture_tab(p: &mut ChildSpawnerCommands, lang: Lang) {
    p.spawn((
        Node {
            display: Display::None,
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(SP_SM),
            ..default()
        },
        TabRoot(SelTab::Furniture),
    ))
    .with_children(|tab| {
        // Tile strip — one entry per possible slot; `FurnitureCard::Tile`
        // hides the ones this building's `kind.furnishings()` doesn't have.
        tab.spawn(Node {
            width: Val::Percent(100.0),
            column_gap: Val::Px(SP_XS),
            ..default()
        })
        .with_children(|row| {
            for slot in 0u8..3 {
                row.spawn((
                    Button,
                    Node {
                        width: Val::Px(52.0),
                        height: Val::Px(52.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(2.0)),
                        border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                        ..default()
                    },
                    BackgroundColor(theme::BG_SECTION),
                    BorderColor::all(BORDER),
                    FurnitureCard::Tile(slot),
                ))
                .with_children(|tile| {
                    tile.spawn((theme::text("", FS_BODY, TEXT_PRIMARY), SelText::TileGlyph(slot)));
                    tile.spawn((
                        theme::text("", FS_MICRO - 1.0, theme::ACCENT_ICE),
                        Node {
                            position_type: PositionType::Absolute,
                            right: Val::Px(3.0),
                            bottom: Val::Px(2.0),
                            ..default()
                        },
                        SelText::TileBadge(slot),
                    ));
                });
            }
        });
        // Detail card for whichever tile is selected (`SelFurnSlot`).
        tab.spawn((
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                padding: UiRect::all(Val::Px(SP_SM)),
                border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                ..default()
            },
            BackgroundColor(theme::BG_SECTION),
        ))
        .with_children(|card| {
            card.spawn((theme::text("", FS_SMALL, TEXT_PRIMARY), SelText::FurnName));
            card.spawn((theme::text("", FS_MICRO, TEXT_MUTED), SelText::FurnDesc));
            // No BaseColor: `selection_panel_update` owns this button's
            // color entirely (affordability/maxed), same reasoning as the
            // header `UpgradeBtn`.
            card.spawn((
                Button,
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(28.0),
                    margin: UiRect::top(Val::Px(SP_XS)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                    ..default()
                },
                BackgroundColor(BTN),
                FurnitureCard::UpgradeBtn,
            ))
            .with_children(|b| {
                b.spawn((theme::text("", FS_MICRO + 1.0, TEXT_PRIMARY), SelText::FurnUpgrade));
            });
        });
        // Stats grid, two per row.
        stat_row(
            tab,
            [
                (i18n_furnishing::stat_label_production(lang), SelText::FurnStatProduction),
                (i18n_furnishing::stat_label_consumption(lang), SelText::FurnStatConsumption),
            ],
        );
        stat_row(
            tab,
            [
                (i18n_furnishing::stat_label_stats(lang), SelText::FurnStatStats),
                (i18n_furnishing::stat_label_time(lang), SelText::FurnStatTime),
            ],
        );
    });
}

/// One row of the stats grid: exactly two cells, side by side.
fn stat_row(parent: &mut ChildSpawnerCommands, cells: [(&str, SelText); 2]) {
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            column_gap: Val::Px(SP_XS),
            ..default()
        })
        .with_children(|row| {
            for (label, marker) in cells {
                stat_cell(row, label, marker);
            }
        });
}

/// One stats-grid cell: a muted label on top, the live value below. `label`
/// is fixed at spawn time (it never changes with the selected fitting);
/// `value_marker` tags the value text for `selection_panel_update` to fill.
fn stat_cell(parent: &mut ChildSpawnerCommands, label: &str, value_marker: SelText) {
    parent
        .spawn((
            Node {
                flex_grow: 1.0,
                flex_basis: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(SP_XS)),
                border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                ..default()
            },
            BackgroundColor(theme::BG_SECTION),
        ))
        .with_children(|cell| {
            cell.spawn(theme::text(label, FS_MICRO - 1.0, TEXT_MUTED));
            cell.spawn((theme::text("", FS_SMALL, TEXT_PRIMARY), value_marker));
        });
}

/// V0.21: Survivors tab — up to 3 portrait slots (filled = a profession-
/// colored circle with the survivor's initial, empty/locked = a padlock
/// glyph), the anonymous `−  N/M  +` staffing control (plus a None/Max
/// quick-set and the colony's idle-worker count — all four hidden together
/// in the central world, where only named `AssignSurvivor` works), and the
/// "assign the roster-selected survivor here" button. Root visibility is
/// `TabRoot(SelTab::Survivors)`.
fn spawn_survivors_tab(p: &mut ChildSpawnerCommands, lang: Lang) {
    p.spawn((
        Node {
            display: Display::None,
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(SP_SM),
            ..default()
        },
        TabRoot(SelTab::Survivors),
    ))
    .with_children(|tab| {
        tab.spawn(Node {
            width: Val::Percent(100.0),
            column_gap: Val::Px(SP_SM),
            ..default()
        })
        .with_children(|row| {
            for slot in 0u8..3 {
                row.spawn((
                    Node {
                        width: Val::Px(44.0),
                        height: Val::Px(44.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::MAX,
                        ..default()
                    },
                    BackgroundColor(theme::BG_SECTION),
                    BorderColor::all(BORDER),
                    SurvivorSlot::Root(slot),
                ))
                .with_children(|circle| {
                    circle.spawn((theme::text("", FS_BODY, TEXT_PRIMARY), SelText::SurvivorInitial(slot)));
                    spawn_lock_glyph(circle, 14.0, slot);
                });
            }
        });
        // − / count / + control, wrapped together with the None/Max
        // quick-set and the idle-worker count so all four hide as one block
        // in the central world.
        tab.spawn((
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(SP_XS + 2.0),
                ..default()
            },
            PanelAction::WorkerAdjustControls,
        ))
        .with_children(|w| {
            w.spawn(Node {
                width: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                column_gap: Val::Px(SP_SM),
                ..default()
            })
            .with_children(|row| {
                row.spawn((btn_px(44.0, 32.0, BTN), WorkerMinus))
                    .with_children(|b| {
                        b.spawn(theme::text("-", FS_BODY + 2.0, TEXT_PRIMARY));
                    });
                row.spawn((theme::text("0/0", FS_TITLE, TEXT_PRIMARY), SelText::Count));
                row.spawn((btn_px(44.0, 32.0, BTN), WorkerPlus))
                    .with_children(|b| {
                        b.spawn(theme::text("+", FS_BODY + 2.0, TEXT_PRIMARY));
                    });
            });
            w.spawn(Node {
                width: Val::Percent(100.0),
                column_gap: Val::Px(SP_SM),
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    Button,
                    Node {
                        flex_grow: 1.0,
                        flex_basis: Val::Px(0.0),
                        height: Val::Px(26.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                        ..default()
                    },
                    BackgroundColor(BTN),
                    BaseColor(BTN),
                    BorderColor::all(BORDER),
                    WorkerNoneBtn,
                ))
                .with_children(|b| {
                    b.spawn(theme::text(i18n_hud::worker_none_label(lang), FS_MICRO + 1.0, TEXT_PRIMARY));
                });
                row.spawn((
                    Button,
                    Node {
                        flex_grow: 1.0,
                        flex_basis: Val::Px(0.0),
                        height: Val::Px(26.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                        ..default()
                    },
                    BackgroundColor(BTN),
                    BaseColor(BTN),
                    WorkerMaxBtn,
                ))
                .with_children(|b| {
                    b.spawn(theme::text(i18n_hud::worker_max_label(lang), FS_MICRO + 1.0, TEXT_PRIMARY));
                });
            });
            w.spawn((
                theme::text("", FS_MICRO, TEXT_MUTED),
                Node {
                    align_self: AlignSelf::Center,
                    ..default()
                },
                SelText::Avail,
            ));
        });
        // Visibility/label owned entirely by `roster/panel.rs`
        // (`update_assign_here`), unrelated to this tab's own toggle —
        // nesting it here just means it only shows alongside the portraits
        // it fills.
        tab.spawn((
            Button,
            Node {
                display: Display::None,
                width: Val::Percent(100.0),
                height: Val::Px(30.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                ..default()
            },
            BackgroundColor(BTN),
            BaseColor(BTN),
            AssignHereBtn,
        ))
        .with_children(|b| {
            b.spawn((theme::text("", FS_MICRO + 1.0, TEXT_PRIMARY), AssignHereLabel));
        });
    });
}

/// A small padlock silhouette (a rounded-top shackle arc over a body) built
/// from flat colored rectangles — no image asset, same convention
/// `spawn_hammer_glyph` (`buildbar.rs`) uses. Both pieces are tagged
/// `SurvivorSlot::Lock(slot)`; `selection_panel_update` hides the whole
/// group once that slot is filled.
fn spawn_lock_glyph(parent: &mut ChildSpawnerCommands, w: f32, slot: u8) {
    parent
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            },
            SurvivorSlot::Lock(slot),
        ))
        .with_children(|g| {
            g.spawn((
                Node {
                    width: Val::Px(w * 0.55),
                    height: Val::Px(w * 0.45),
                    border: UiRect {
                        left: Val::Px(2.0),
                        right: Val::Px(2.0),
                        top: Val::Px(2.0),
                        bottom: Val::Px(0.0),
                    },
                    border_radius: BorderRadius {
                        top_left: Val::Px(w * 0.3),
                        top_right: Val::Px(w * 0.3),
                        bottom_left: Val::Px(0.0),
                        bottom_right: Val::Px(0.0),
                    },
                    ..default()
                },
                BorderColor::all(TEXT_MUTED),
            ));
            g.spawn((
                Node {
                    width: Val::Px(w),
                    height: Val::Px(w * 0.68),
                    margin: UiRect::top(Val::Px(-1.0)),
                    border_radius: BorderRadius::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(TEXT_MUTED),
            ));
        });
}

/// One footer action button — Relocate/Demolish — tagged with `PanelAction`
/// so `selection_panel_update` can toggle it via the SAME query as the
/// Furnace/Caravan/staffing rows (see that enum's doc for why they share one
/// component type).
fn spawn_panel_action(parent: &mut ChildSpawnerCommands, bundle: impl Bundle, action: PanelAction, label: &str) {
    parent.spawn((bundle, action)).with_children(|b| {
        b.spawn(theme::text(label, FS_SMALL, TEXT_PRIMARY));
    });
}

/// One top-bar stat chip: colored marker dot + a live `HudField` text, inside
/// the design system's pill capsule ([`theme::chip`]).
fn res_chip(p: &mut ChildSpawnerCommands, dot_color: Color, initial: impl Into<String>, field: HudField) {
    p.spawn(theme::chip()).with_children(|c| {
        c.spawn(theme::dot(8.0, dot_color));
        c.spawn((
            theme::text(initial, FS_SMALL, TEXT_PRIMARY),
            TextLayout::new(Justify::Left, LineBreak::NoWrap),
            field,
        ));
    });
}

/// Desktop/Tablet top bar — Frostpunk-style three-zone strip: resource chips
/// on the left, status chips + actions on the right. On Desktop the middle is
/// deliberately left empty for the circular temperature medallion
/// (`spawn_center_gauge`); Tablet has no medallion (too narrow), so the
/// clock/temperature/morale readouts stay inline in the row instead.
fn spawn_top_bar_desktop(commands: &mut Commands, ff: theme::FormFactor, lang: Lang) {
    let desktop = ff == theme::FormFactor::Desktop;
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                height: Val::Px(46.0),
                padding: UiRect::horizontal(Val::Px(14.0)),
                align_items: AlignItems::Center,
                column_gap: Val::Px(SP_SM),
                border: UiRect::bottom(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(BG_PANEL),
            // Ostki chiziq mis tusda — panelning "ornate" metall detali.
            BorderColor::all(theme::BRASS_DIM),
            Interaction::default(),
            UiBlocker,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            res_chip(p, RES_WOOD, i18n_hud::hud_wood(0, lang), HudField::Wood);
            res_chip(p, RES_COAL, i18n_hud::hud_coal(0, lang), HudField::Coal);
            res_chip(p, RES_FOOD, i18n_hud::hud_food(0, lang), HudField::Food);
            res_chip(p, RES_WATER, i18n_hud::hud_water(0, lang), HudField::Water);
            res_chip(p, RES_GOLD, i18n_hud::hud_gold(0, lang), HudField::Gold);
            res_chip(p, theme::ACCENT_ICE, i18n_hud::hud_pop(0, 0, lang), HudField::Pop);
            // V0.17: sick/exhausted alert — plain text (no chip/dot) so an
            // empty string, the common case, truly takes up no row space,
            // same convention the Clock/Temp entries below already use.
            p.spawn((
                theme::text("", FS_SMALL, theme::DANGER),
                TextLayout::new(Justify::Left, LineBreak::NoWrap),
                HudField::Health,
            ));
            p.spawn(Node {
                flex_grow: 1.0,
                ..default()
            });
            if !desktop {
                p.spawn((
                    theme::text(i18n_hud::hud_clock(1, 1, 6, 0, lang), FS_SMALL, TEXT_PRIMARY),
                    TextLayout::new(Justify::Left, LineBreak::NoWrap),
                    HudField::Clock,
                ));
                p.spawn((
                    theme::text("-0 C", FS_SMALL, Color::srgb(0.55, 0.80, 0.95)),
                    TextLayout::new(Justify::Left, LineBreak::NoWrap),
                    HudField::Temp,
                ));
                p.spawn(Node {
                    flex_grow: 1.0,
                    ..default()
                });
            }
            res_chip(p, theme::ACCENT_WARM, "Furnace L1", HudField::Furnace);
            if !desktop {
                // Desktopda kayfiyat pastki-markaz barga ko'chgan
                // (`spawn_morale_bar`); planshetda satrda qoladi.
                res_chip(p, theme::SUCCESS, "Morale --", HudField::Morale);
            }
            p.spawn((
                Button,
                Node {
                    width: Val::Px(110.0),
                    height: Val::Px(30.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                    // Hidden until `world_switch_button` decides this session
                    // may switch worlds at all.
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.13, 0.30, 0.40)),
                BaseColor(Color::srgb(0.13, 0.30, 0.40)),
                WorldSwitchBtn,
            ))
            .with_children(|b| {
                b.spawn((theme::text(i18n_hud::world_switch_global(lang), FS_SMALL, TEXT_PRIMARY), WorldSwitchLabel));
            });
            p.spawn((btn_px(70.0, 30.0, BTN), QuitToMenuBtn))
                .with_children(|b| {
                    b.spawn(theme::text(i18n_hud::menu_button(lang), FS_SMALL, TEXT_PRIMARY));
                });
            if desktop {
                spawn_center_gauge(p, lang);
            }
        });
}

/// Desktop markaziy harorat medalyoni — yuqori panelning ABSOLUTE bolasi
/// (alohida root emas!), shunda u panel fonidan KEYIN, ya'ni doim panel
/// USTIDA chiziladi va pastga erkin osilib turadi (Frostpunk'ning -40°
/// doirasi). Ichida joriy harorat, ostida kun/soat chipi; matnlarni
/// `hud_update` yangilaydi (`HudField::Temp`/`Clock`), sovuq to'lqin matn
/// suffiksi o'rniga qizil rang bilan bildiriladi (doiraga suffiks sig'maydi).
fn spawn_center_gauge(bar: &mut ChildSpawnerCommands, lang: Lang) {
    bar.spawn(Node {
        position_type: PositionType::Absolute,
        left: Val::Px(0.0),
        right: Val::Px(0.0),
        top: Val::Px(4.0),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        row_gap: Val::Px(SP_XS),
        ..default()
    })
    .with_children(|p| {
        p.spawn((
            Node {
                width: Val::Px(84.0),
                height: Val::Px(84.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(BG_PANEL),
            BorderColor::all(theme::BRASS),
            BoxShadow::new(
                Color::srgba(0.0, 0.0, 0.0, 0.45),
                Val::Px(0.0),
                Val::Px(6.0),
                Val::Px(2.0),
                Val::Px(16.0),
            ),
            Interaction::default(),
            UiBlocker,
        ))
        .with_children(|ring| {
            // Ichki disk panel fonidan ochroq (BTN toni) — medalyon panelga
            // "cho'kib" ketmasin, aniq qatlam bo'lib o'qilsin.
            ring.spawn((
                Node {
                    width: Val::Px(72.0),
                    height: Val::Px(72.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(BTN),
                BorderColor::all(theme::BORDER_STRONG),
            ))
            .with_children(|inner| {
                inner.spawn((
                    theme::text("-0 C", FS_TITLE + 2.0, theme::ACCENT_ICE),
                    TextLayout::new(Justify::Center, LineBreak::NoWrap),
                    HudField::Temp,
                ));
            });
        });
        p.spawn(theme::chip()).with_children(|c| {
            c.spawn((
                theme::text(i18n_hud::hud_clock(1, 1, 6, 0, lang), FS_SMALL, TEXT_PRIMARY),
                TextLayout::new(Justify::Center, LineBreak::NoWrap),
                HudField::Clock,
            ));
        });
    });
}

/// Pastki-markaz kayfiyat bari (Frostpunk'ning "Umid" chizig'i uslubida) —
/// faqat Desktop: yorliq matni + to'rt-darajali rangli to'ldirma. Matnni
/// `hud_update` (`HudField::Morale`), to'ldirmani [`morale_bar_update`]
/// boshqaradi.
fn spawn_morale_bar(commands: &mut Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(SP_SM),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(3.0),
                ..default()
            },
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            // Yorliq chip ichida — yorug' qor sahnasi ustida ham o'qilsin
            // (gauge ostidagi soat chipi bilan bir xil kapsula uslubi).
            p.spawn(theme::chip()).with_children(|c| {
                c.spawn((
                    theme::text("Morale --", FS_SMALL, TEXT_PRIMARY),
                    TextLayout::new(Justify::Center, LineBreak::NoWrap),
                    HudField::Morale,
                ));
            });
            p.spawn(theme::stat_bar_track(Val::Px(300.0), 12.0))
                .with_children(|track| {
                    track.spawn((
                        Node {
                            width: Val::Percent(50.0),
                            height: Val::Percent(100.0),
                            border_radius: BorderRadius::MAX,
                            ..default()
                        },
                        BackgroundColor(theme::ACCENT_WARM),
                        MoraleBarFill,
                    ));
                });
        });
}

/// `GameState::morale_multiplier` bilan aynan mos to'rt daraja rangi (motam
/// binafshasi ustuvor) — HUD matni (`hud_update`) va pastki bar to'ldirmasi
/// ([`morale_bar_update`]) bir xil rangda gapirsin.
pub(crate) fn morale_tier_color(morale: f32, mourning: bool) -> Color {
    if mourning {
        Color::srgb(0.70, 0.55, 0.85)
    } else if morale < 25.0 {
        Color::srgb(0.90, 0.30, 0.25)
    } else if morale < 50.0 {
        Color::srgb(0.92, 0.62, 0.28)
    } else if morale <= 75.0 {
        Color::srgb(0.85, 0.88, 0.60)
    } else {
        Color::srgb(0.55, 0.90, 0.50)
    }
}

/// Pastki-markaz kayfiyat barining to'ldirmasi: kengligi `morale`ning foizi,
/// rangi `morale_tier_color` darajasi. Desktop bo'lmagan sessiyalarda bar
/// spawn qilinmagani uchun query bo'sh — tizim bekorga ish qilmaydi.
pub fn morale_bar_update(
    view: Res<GameView>,
    mut fills: Query<(&mut Node, &mut BackgroundColor), With<MoraleBarFill>>,
) {
    let Some(state) = view.state.as_ref() else { return };
    let want_w = Val::Percent(state.morale.clamp(0.0, 100.0));
    let want_c = morale_tier_color(state.morale, state.mourning_active());
    for (mut node, mut bg) in &mut fills {
        if node.width != want_w {
            node.width = want_w;
        }
        if bg.0 != want_c {
            bg.0 = want_c;
        }
    }
}

/// A single-line HUD text node: `flex_shrink: 0.0` so a tight mobile row
/// never compresses it below its natural content width (which is what forces
/// a wrap mid-word, e.g. "Yog'och" and its number landing on separate lines).
fn hud_text_mobile(t: impl Into<String>, color: Color) -> impl Bundle {
    (
        theme::text(t, FS_MICRO, color),
        // NoWrap ham shart: flex_shrink 0 taffy'ga tegishli, lekin matn
        // o'lchagichi baribir mavjud enga qarab ichki o'rashga ruxsat beradi
        // ("Yog'och" va soni alohida qatorlarga tushib qolardi).
        TextLayout::new(Justify::Left, LineBreak::NoWrap),
        Node {
            flex_shrink: 0.0,
            ..default()
        },
    )
}

/// Mobile top bar: two compact rows instead of one wide one, so nothing
/// spills off-screen at phone widths. Row 1: the three resources + pop.
/// Row 2: clock, temperature, furnace, morale and the Menu button
/// (world-switch stays inside row 2 too, right next to Menu, since it's
/// rarely both visible and it's still just a single extra slot). Every field
/// is `FS_MICRO` + non-shrinking (see `hud_text_mobile`) and each row wraps
/// (`FlexWrap::Wrap`) as a last resort, so at a 390px-wide phone (UiScale
/// 0.8) both rows fit without any single field wrapping mid-text or the two
/// rows overlapping each other.
fn spawn_top_bar_mobile(commands: &mut Commands, lang: Lang) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                height: Val::Auto,
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(Val::Px(SP_SM), Val::Px(SP_XS)),
                row_gap: Val::Px(SP_XS),
                border: UiRect::bottom(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(BG_PANEL),
            BorderColor::all(BORDER),
            Interaction::default(),
            UiBlocker,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            // Row 1: resources + population.
            p.spawn(Node {
                height: Val::Auto,
                align_items: AlignItems::Center,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(SP_SM),
                row_gap: Val::Px(SP_XS),
                ..default()
            })
            .with_children(|row| {
                row.spawn((hud_text_mobile(i18n_hud::hud_wood(0, lang), RES_WOOD), HudField::Wood));
                row.spawn((hud_text_mobile(i18n_hud::hud_coal(0, lang), RES_COAL), HudField::Coal));
                row.spawn((hud_text_mobile(i18n_hud::hud_food(0, lang), RES_FOOD), HudField::Food));
                row.spawn((hud_text_mobile(i18n_hud::hud_water(0, lang), RES_WATER), HudField::Water));
                row.spawn((hud_text_mobile(i18n_hud::hud_gold(0, lang), RES_GOLD), HudField::Gold));
                row.spawn(Node {
                    flex_grow: 1.0,
                    ..default()
                });
                row.spawn((hud_text_mobile(i18n_hud::hud_pop(0, 0, lang), TEXT_PRIMARY), HudField::Pop));
                // V0.17: sick/exhausted alert, mirrors the desktop entry —
                // empty text while both counts are zero.
                row.spawn((hud_text_mobile("", theme::DANGER), HudField::Health));
            });
            // Row 2: clock, temp, furnace, morale, world-switch, Menu.
            p.spawn(Node {
                height: Val::Auto,
                align_items: AlignItems::Center,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(SP_SM),
                row_gap: Val::Px(SP_XS),
                ..default()
            })
            .with_children(|row| {
                row.spawn((hud_text_mobile(i18n_hud::hud_clock(1, 1, 6, 0, lang), TEXT_PRIMARY), HudField::Clock));
                row.spawn((hud_text_mobile("-0 C", Color::srgb(0.55, 0.80, 0.95)), HudField::Temp));
                row.spawn(Node {
                    flex_grow: 1.0,
                    ..default()
                });
                row.spawn((hud_text_mobile("Furnace L1", TEXT_PRIMARY), HudField::Furnace));
                row.spawn((hud_text_mobile("Morale --", TEXT_PRIMARY), HudField::Morale));
                row.spawn((
                    Button,
                    Node {
                        width: Val::Px(96.0),
                        height: Val::Px(30.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        flex_shrink: 0.0,
                        display: Display::None,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.13, 0.30, 0.40)),
                    BaseColor(Color::srgb(0.13, 0.30, 0.40)),
                    WorldSwitchBtn,
                ))
                .with_children(|b| {
                    b.spawn((theme::text(i18n_hud::world_switch_global(lang), FS_MICRO, TEXT_PRIMARY), WorldSwitchLabel));
                });
                row.spawn((
                    Button,
                    Node {
                        width: Val::Px(60.0),
                        height: Val::Px(30.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        flex_shrink: 0.0,
                        ..default()
                    },
                    BackgroundColor(BTN),
                    BaseColor(BTN),
                    QuitToMenuBtn,
                ))
                .with_children(|b| {
                    b.spawn(theme::text(i18n_hud::menu_button(lang), FS_MICRO, TEXT_PRIMARY));
                });
            });
        });
}

pub fn track_ui_hover(mut hover: ResMut<UiHover>, q: Query<&Interaction>) {
    hover.0 = q.iter().any(|i| *i != Interaction::None);
}

pub fn fps_update(
    time: Res<Time>,
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
    quality: Res<Quality>,
    adapter: Option<Res<bevy::render::renderer::RenderAdapterInfo>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut q: Query<&mut Text, With<FpsText>>,
    mut accum: Local<f32>,
) {
    // Throttle to ~4Hz: the diagnostic itself is smoothed, so redoing this
    // format! every single frame just churns the allocator for no visible
    // benefit.
    *accum += time.delta_secs();
    if *accum < 0.25 {
        return;
    }
    *accum = 0.0;

    let Ok(mut t) = q.single_mut() else { return };
    let fps = diagnostics
        .get(&bevy::diagnostic::FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    // Also surface the graphics tier, the actual GPU backend in use (so
    // players/devs can see WebGPU vs the WebGL2 fallback at a glance) and the
    // backing resolution, so performance problems (esp. mobile-web fill
    // rate) can be diagnosed on-device.
    let tier = match *quality {
        Quality::Low => "Low",
        Quality::Medium => "Med",
        Quality::High => "High",
    };
    // wgpu's `Backend::to_str()` returns lowercase machine names ("gl",
    // "webgpu", "vulkan", ...); render the ones players will actually see friendlier.
    let backend = adapter.map(|a| match a.0.backend.to_str() {
        "webgpu" => "WebGPU".to_string(),
        "gl" => "WebGL2".to_string(),
        other => other.to_string(),
    });
    let mut s = format!("FPS {fps:.0}  |  {tier}");
    if let Some(b) = backend {
        s.push_str(&format!("  |  {b}"));
    }
    if let Ok(w) = windows.single() {
        s.push_str(&format!(
            "  |  {}x{}",
            w.resolution.physical_width(),
            w.resolution.physical_height()
        ));
    }
    if t.0 != s {
        t.0 = s;
    }
}

/// Last-displayed value per HUD field, so `hud_update` only pays for a
/// `format!` (and, for the events log, the sort/truncate/join) when the
/// underlying number actually changed instead of on every frame. `Lang` rides
/// along in every key (not its own field) so a language switch — which
/// doesn't change any of these numbers — still invalidates every cache entry
/// and refreshes the on-screen text.
#[derive(Default)]
pub(crate) struct HudCache {
    wood: Option<(i64, Lang)>,
    coal: Option<(i64, Lang)>,
    food: Option<(i64, Lang)>,
    water: Option<(i64, Lang)>,
    gold: Option<(i64, Lang)>,
    pop: Option<(usize, u32, Lang)>,
    clock: Option<(u32, u32, u32, Lang)>,
    temp: Option<(i32, bool, Lang)>,
    furnace: Option<(u8, bool, Lang)>,
    events: Option<u64>,
    morale: Option<(i32, bool, Lang)>,
    /// V0.17: (sick count, exhausted count, lang) backing `HudField::Health`.
    health: Option<(usize, usize, Lang)>,
}

pub fn hud_update(
    view: Res<GameView>,
    lang: Res<Lang>,
    ff: Res<theme::FormFactor>,
    mut cache: Local<HudCache>,
    mut q: Query<(&mut Text, Option<&mut TextColor>, &HudField)>,
) {
    let Some(state) = view.state.as_ref() else { return };
    let lang = *lang;
    for (mut text, color, field) in &mut q {
        match field {
            HudField::Wood => {
                let v = state.stock.wood as i64;
                if cache.wood != Some((v, lang)) {
                    cache.wood = Some((v, lang));
                    text.0 = i18n_hud::hud_wood(v, lang);
                }
            }
            HudField::Coal => {
                let v = state.stock.coal as i64;
                if cache.coal != Some((v, lang)) {
                    cache.coal = Some((v, lang));
                    text.0 = i18n_hud::hud_coal(v, lang);
                }
            }
            HudField::Food => {
                let v = state.stock.food as i64;
                if cache.food != Some((v, lang)) {
                    cache.food = Some((v, lang));
                    text.0 = i18n_hud::hud_food(v, lang);
                }
            }
            HudField::Water => {
                let v = state.stock.water as i64;
                if cache.water != Some((v, lang)) {
                    cache.water = Some((v, lang));
                    text.0 = i18n_hud::hud_water(v, lang);
                }
            }
            HudField::Gold => {
                let v = state.stock.gold as i64;
                if cache.gold != Some((v, lang)) {
                    cache.gold = Some((v, lang));
                    text.0 = i18n_hud::hud_gold(v, lang);
                }
            }
            HudField::Pop => {
                let (pop, idle) = (state.survivors.len(), state.idle_workers());
                if cache.pop != Some((pop, idle, lang)) {
                    cache.pop = Some((pop, idle, lang));
                    text.0 = i18n_hud::hud_pop(pop, idle, lang);
                }
            }
            HudField::Clock => {
                let mins = (state.time_of_day() * 24.0 * 60.0) as u32;
                let key = (state.day(), state.win_days, mins);
                if cache.clock != Some((key.0, key.1, key.2, lang)) {
                    cache.clock = Some((key.0, key.1, key.2, lang));
                    text.0 = i18n_hud::hud_clock(key.0, key.1, mins / 60, mins % 60, lang);
                }
            }
            HudField::Temp => {
                let temp = state.temperature();
                let key = (temp.round() as i32, state.cold_snap);
                if cache.temp != Some((key.0, key.1, lang)) {
                    cache.temp = Some((key.0, key.1, lang));
                    if *ff == theme::FormFactor::Desktop {
                        // Desktopda harorat markaziy gauge doirasi ichida —
                        // "SOVUQ TO'LQINI!" suffiksi sig'maydi, o'rniga rang
                        // ogohlantiradi (muz-ko'k → xavf-qizil).
                        text.0 = i18n_hud::hud_temp(temp, "", lang);
                        if let Some(mut c) = color {
                            c.0 = if state.cold_snap { theme::DANGER } else { theme::ACCENT_ICE };
                        }
                    } else {
                        let snap = if state.cold_snap { i18n_hud::hud_cold_snap(lang) } else { "" };
                        text.0 = i18n_hud::hud_temp(temp, snap, lang);
                    }
                }
            }
            HudField::Furnace => {
                let key = (state.furnace_level, state.furnace_lit);
                if cache.furnace != Some((key.0, key.1, lang)) {
                    cache.furnace = Some((key.0, key.1, lang));
                    let out_of_fuel = !state.furnace_lit && state.furnace_level > 0;
                    if let Some(mut c) = color {
                        c.0 = if state.furnace_lit {
                            Color::srgb(0.95, 0.65, 0.30)
                        } else {
                            Color::srgb(0.95, 0.30, 0.25)
                        };
                    }
                    // Mobile: a short single-line form (see
                    // `i18n_hud::hud_furnace_short`'s doc) — the full status
                    // word would wrap at `FS_MICRO` on a phone-width bar and
                    // collide with the row below.
                    text.0 = if ff.compact() {
                        i18n_hud::hud_furnace_short(
                            state.furnace_level,
                            state.furnace_level as f32 * FURNACE_COAL_PER_DAY_PER_LEVEL,
                            out_of_fuel,
                            lang,
                        )
                    } else {
                        let status = if state.furnace_lit {
                            i18n_hud::furnace_status_burning(lang)
                        } else if state.furnace_level == 0 {
                            i18n_hud::furnace_status_off(lang)
                        } else {
                            i18n_hud::furnace_status_out_of_fuel(lang)
                        };
                        i18n_hud::hud_furnace(
                            state.furnace_level,
                            state.furnace_level as f32 * FURNACE_COAL_PER_DAY_PER_LEVEL,
                            status,
                            lang,
                        )
                    };
                }
            }
            HudField::Morale => {
                let mourning = state.mourning_active();
                let key = (state.morale.round() as i32, mourning);
                if cache.morale != Some((key.0, key.1, lang)) {
                    cache.morale = Some((key.0, key.1, lang));
                    // Four-tier band matching `GameState::morale_multiplier`'s
                    // thresholds exactly, so the HUD symbol always agrees with
                    // the actual production multiplier in effect (colors:
                    // `morale_tier_color`, shared with the bottom morale bar).
                    let tier = if state.morale < 25.0 {
                        i18n_hud::morale_tier_critical(lang)
                    } else if state.morale < 50.0 {
                        i18n_hud::morale_tier_low(lang)
                    } else if state.morale <= 75.0 {
                        i18n_hud::morale_tier_steady(lang)
                    } else {
                        i18n_hud::morale_tier_high(lang)
                    };
                    if let Some(mut c) = color {
                        c.0 = morale_tier_color(state.morale, mourning);
                    }
                    let mourn_tag = if mourning { i18n_hud::hud_mourning_tag(lang) } else { "" };
                    text.0 = i18n_hud::hud_morale(state.morale, tier, mourn_tag, lang);
                }
            }
            HudField::Health => {
                let key = (state.sick_count(), state.exhausted_count(), lang);
                if cache.health != Some(key) {
                    cache.health = Some(key);
                    text.0 = i18n_hud::hud_health_alert(key.0, key.1, lang);
                }
            }
            HudField::Events => {
                if cache.events != Some(state.total_events) {
                    cache.events = Some(state.total_events);
                    // Show up to 8 lines, prioritising system events (deaths,
                    // weather, victory) over cosmetic ones so the server's
                    // eviction protection actually reaches the player's eyes;
                    // then display the chosen lines chronologically. Server
                    // event-stream text is NOT localized (see `HudField`
                    // doc) — only the "Day N:" framing lives client-side,
                    // and that's still plain since it's just a number.
                    let mut idx: Vec<usize> = (0..state.events.len()).collect();
                    idx.sort_by_key(|&i| {
                        (
                            std::cmp::Reverse(state.events[i].system),
                            std::cmp::Reverse(i),
                        )
                    });
                    idx.truncate(8);
                    idx.sort_unstable();
                    text.0 = idx
                        .iter()
                        .map(|&i| {
                            let e = &state.events[i];
                            format!("Day {}: {}", e.day, e.text)
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                }
            }
        }
    }
}
