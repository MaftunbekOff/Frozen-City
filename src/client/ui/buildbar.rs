use bevy::prelude::*;

use frozen_city::game::types::{BuildingKind, PlayerCommand, ROAD_COST_WOOD, ROAD_REFUND};
use frozen_city::net::protocol::ClientMsg;

use super::super::chat::ChatState;
use super::super::i18n::Lang;
use super::super::i18n_hud;
use super::super::i18n_names;
use super::super::i18n_roads;
use super::super::roads::RoadMode;
use super::super::theme::{
    self, BG_PANEL, BTN, BTN_ACTIVE, BTN_DIM, BTN_HOVER, FS_SMALL, SP_SM, SP_XS, TEXT_MUTED,
    TEXT_PRIMARY,
};
use super::super::*;
use super::*;

/// V0.19: a road/erase tool tile in the Infrastructure category — mirrors
/// `BuildBtn` in shape, but clicking one arms `RoadMode` instead of
/// `BuildMode` (a drag paints a batch of tiles, there's no single kind to
/// place). See `road_tool_buttons`.
#[derive(Component)]
pub struct RoadToolBtn(pub RoadMode);

#[allow(clippy::too_many_arguments)]
pub fn build_buttons(
    view: Res<GameView>,
    lang: Res<Lang>,
    ff: Res<theme::FormFactor>,
    time: Res<Time>,
    mut build: ResMut<BuildMode>,
    mut pending: ResMut<PendingPlace>,
    mut road: super::super::roads::RoadModes,
    mut open: ResMut<BuildPanelOpen>,
    clicked: Query<(&Interaction, &BuildBtn), Changed<Interaction>>,
    mut all: Query<(&Interaction, &BuildBtn, &mut BackgroundColor)>,
    mut tooltip: Query<&mut Text, (With<TooltipText>, Without<BuildDenyText>)>,
    mut badges: Query<(&BuildCostBadge, &mut TextColor)>,
    mut deny_text: Query<&mut Text, (With<BuildDenyText>, Without<TooltipText>)>,
    mut deny: Local<Option<(String, f32)>>,
) {
    let wood = view
        .state
        .as_ref()
        .map(|s| s.stock.wood)
        .unwrap_or_default();

    for (interaction, btn) in &clicked {
        if *interaction == Interaction::Pressed {
            // Resurs yetmasa bino TANLANMAYDI: menyu ochiq qoladi va sabab
            // qizil qatorda ko'rsatiladi — o'yinchi "qurib bo'lmaydigan"
            // arvoh bilan olamga tushib qolmasin.
            if wood < btn.0.cost_wood() as f32 {
                *deny = Some((
                    i18n_hud::build_need_wood(
                        i18n_names::building_name(btn.0, *lang),
                        btn.0.cost_wood(),
                        wood as i64,
                        *lang,
                    ),
                    3.0,
                ));
                continue;
            }
            build.0 = if build.0 == Some(btn.0) {
                None
            } else {
                Some(btn.0)
            };
            // V0.16: switching (or clearing) the armed kind drops any
            // half-dropped pending building, exactly like the keyboard
            // hotkeys in `input::build_input` — otherwise the frozen ghost
            // and its ✓ button would still commit the OLD kind.
            pending.0 = None;
            // V0.19: a normal building and the road tool are mutually
            // exclusive — picking one always backs the other out cleanly,
            // same reasoning as `road_tool_buttons` clearing `BuildMode`.
            *road.mode = RoadMode::Off;
            road.paint.reset();
            // Bino tanlandi — modal yopiladi, joylashtirish uchun olam ochiq
            // bo'lsin (bekor qilish bosishida esa ochiq qoladi).
            if build.0.is_some() {
                open.0 = false;
            }
        }
    }

    // Rad-xabar bir necha soniyadan keyin o'zi so'nadi.
    if let Some((_, ttl)) = deny.as_mut() {
        *ttl -= time.delta_secs();
        if *ttl <= 0.0 {
            *deny = None;
        }
    }
    let deny_want = deny.as_ref().map(|(m, _)| m.as_str()).unwrap_or("");
    for mut t in &mut deny_text {
        if t.0 != deny_want {
            t.0 = deny_want.to_string();
        }
    }

    // Narx yorlig'i resurs yetmaganda qizil — bosishdan OLDIN ham ko'rinsin.
    for (badge, mut color) in &mut badges {
        let want = if wood >= badge.0.cost_wood() as f32 {
            TEXT_MUTED
        } else {
            theme::DANGER
        };
        if color.0 != want {
            color.0 = want;
        }
    }

    let mut hovered: Option<BuildingKind> = None;
    for (interaction, btn, mut bg) in &mut all {
        let affordable = wood >= btn.0.cost_wood() as f32;
        if *interaction == Interaction::Hovered {
            hovered = Some(btn.0);
        }
        let color = if build.0 == Some(btn.0) {
            BTN_ACTIVE
        } else if *interaction == Interaction::Hovered {
            BTN_HOVER
        } else if !affordable {
            BTN_DIM
        } else {
            BTN
        };
        if bg.0 != color {
            bg.0 = color;
        }
    }

    if let Ok(mut tip) = tooltip.single_mut() {
        let lang = *lang;
        let new = match hovered.or(build.0) {
            Some(k) => i18n_hud::build_tooltip(
                i18n_names::building_name(k, lang),
                k.cost_wood(),
                i18n_names::building_desc(k, lang),
                lang,
            ),
            None if ff.compact() => i18n_hud::default_hint_mobile(lang).to_string(),
            None => i18n_hud::default_hint_desktop(lang).to_string(),
        };
        if tip.0 != new {
            tip.0 = new;
        }
    }
}

pub fn furnace_buttons(
    view: Res<GameView>,
    net: Res<NetConn>,
    clicked: Query<(&Interaction, &FurnaceLvlBtn), Changed<Interaction>>,
    mut all: Query<(&Interaction, &FurnaceLvlBtn, &mut BackgroundColor)>,
) {
    for (interaction, btn) in &clicked {
        if *interaction == Interaction::Pressed {
            net.send(ClientMsg::Cmd(PlayerCommand::SetFurnaceLevel { level: btn.0 }));
        }
    }
    let current = view
        .state
        .as_ref()
        .map(|s| s.furnace_level)
        .unwrap_or(1);
    for (interaction, btn, mut bg) in &mut all {
        let color = if btn.0 == current {
            BTN_ACTIVE
        } else if *interaction == Interaction::Hovered {
            BTN_HOVER
        } else {
            BTN
        };
        if bg.0 != color {
            bg.0 = color;
        }
    }
}

/// V0.13: fixed amount each `CaravanBtn` quick-trade dispatches — a caravan
/// carries one "load" at a time rather than an arbitrary player-chosen
/// amount, keeping the UI to a small fixed button grid instead of a full
/// amount-picker form.
const QUICK_TRADE_AMOUNT: u32 = 20;

pub fn caravan_buttons(
    view: Res<GameView>,
    net: Res<NetConn>,
    clicked: Query<(&Interaction, &CaravanBtn), Changed<Interaction>>,
    mut all: Query<(&Interaction, &mut BackgroundColor), With<CaravanBtn>>,
) {
    let busy = view.state.as_ref().is_some_and(|s| s.pending_caravan.is_some());
    for (interaction, btn) in &clicked {
        if *interaction == Interaction::Pressed && !busy {
            net.send(ClientMsg::Cmd(PlayerCommand::DispatchTradeCaravan {
                good: btn.good,
                amount: QUICK_TRADE_AMOUNT,
                selling: btn.selling,
            }));
        }
    }
    for (interaction, mut bg) in &mut all {
        // Dimmed (and inert, via the `!busy` guard above) while a caravan is
        // already on the road — only one trip at a time.
        let color = if busy {
            BTN_DIM
        } else if *interaction == Interaction::Hovered {
            BTN_HOVER
        } else {
            BTN
        };
        if bg.0 != color {
            bg.0 = color;
        }
    }
}

/// The **Build** UI, Frostpunk-style, in two pieces:
/// 1. A SINGLE always-visible circular corner button bottom-right (brass
///    medallion + hammer glyph + label). A click (or `B`) toggles the modal
///    ([`build_panel_toggle`]).
/// 2. The menu itself is a centered MODAL (scrim + `theme::modal_panel`, same
///    shell as roster/social): a brass "Qurish" header and every building
///    grouped by category (Housing / Production / Services). Visibility is
///    synced from the `BuildPanelOpen` resource by [`build_panel_visibility`].
///
/// Picking a building auto-closes the modal (`build_buttons`) — placement
/// needs the world visible.
/// Reuses the existing `BuildBtn` markers, so `build_buttons` styles and
/// handles the tiles unchanged. (Furnace controls live in the furnace's own
/// selection panel; roster/research stay on their `P`/`R` keys.)
pub fn spawn_build_panel(commands: &mut Commands, ff: theme::FormFactor, lang: Lang) {
    // --- 1. Corner toggle: circular brass medallion + hammer glyph + label.
    let d = if ff.compact() { 60.0 } else { 54.0 };
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(SP_SM + 4.0),
                bottom: Val::Px(SP_SM + 4.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(2.0),
                ..default()
            },
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|col| {
            col.spawn((
                Button,
                Node {
                    width: Val::Px(d),
                    height: Val::Px(d),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(2.0)),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(BG_PANEL),
                BaseColor(BG_PANEL),
                BorderColor::all(theme::BRASS),
                BoxShadow::new(
                    Color::srgba(0.0, 0.0, 0.0, 0.45),
                    Val::Px(0.0),
                    Val::Px(4.0),
                    Val::Px(1.0),
                    Val::Px(12.0),
                ),
                BuildPanelToggleBtn,
            ))
            .with_children(|b| spawn_hammer_glyph(b, d * 0.44));
            col.spawn(theme::text(i18n_hud::tab_build(lang), theme::FS_MICRO, TEXT_MUTED));
        });

    // --- 2. The modal. The scrim is born `Flex` and `build_panel_visibility`
    // corrects it to the resource's `false` in the same frame — the one-frame
    // idiom the game-over overlay documents (nothing is ever presented).
    // Desktopda kontent O'NG qirradan ochiladigan drawer (Frostpunk'ning
    // o'ng bino-paneli uslubi); Mobile/Tabletda avvalgidek markaziy modal /
    // pastki varaq (`theme::modal_panel`).
    let desktop = ff == theme::FormFactor::Desktop;
    let mut root = commands.spawn((
        theme::scrim(ff),
        UiBlocker,
        BuildPanelRoot,
        DespawnOnExit(Screen::Game),
    ));
    if desktop {
        // The scrim still dims the whole screen, but the menu is now docked
        // along the BOTTOM rather than the right edge: a construction bar
        // spanning the width of the window, which is where a city-builder
        // player expects to build from and what the reference design uses.
        // Right-edge docking put it on top of the missions panel and left the
        // whole bottom of the screen empty.
        root.insert(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexEnd,
            padding: UiRect::bottom(Val::Px(SP_SM)),
            ..default()
        });
    }
    root.with_children(|scrim| {
        if desktop {
            scrim
                .spawn((
                    Node {
                        // Leaves a margin either side rather than running edge
                        // to edge, so the dock reads as a panel sitting over
                        // the world instead of a letterbox bar.
                        width: Val::Percent(97.0),
                        max_height: Val::Percent(42.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(SP_SM),
                        padding: UiRect::axes(Val::Px(theme::SP_MD), Val::Px(theme::SP_MD)),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(theme::RAD_PANEL)),
                        ..default()
                    },
                    // To'liq tiniqmas fon: drawer o'ngdagi Missiyalar paneli
                    // USTIDA turadi — yarim-shaffof BG_PANEL'da ortidagi matn
                    // xira ko'rinib, ikkala panel bir-biriga qorishib ketadi.
                    BackgroundColor(Color::srgb(0.050, 0.080, 0.130)),
                    BorderColor::all(theme::BORDER),
                    BoxShadow::new(
                        Color::srgba(0.0, 0.0, 0.0, 0.50),
                        Val::Px(0.0),
                        Val::Px(-4.0),
                        Val::Px(2.0),
                        Val::Px(24.0),
                    ),
                ))
                .with_children(|p| build_menu_content(p, ff, lang));
        } else {
            scrim
                .spawn(theme::modal_panel(ff))
                .with_children(|p| build_menu_content(p, ff, lang));
        }
    });
}

/// Qurish menyusining ichi — drawer (Desktop) va modal (Mobile/Tablet)
/// qobiqlari uchun umumiy: mis sarlavha + mis chiziq + uch kategoriya.
fn build_menu_content(p: &mut ChildSpawnerCommands, ff: theme::FormFactor, lang: Lang) {
    // Desktop docks the menu along the bottom of the screen, so its categories
    // run left-to-right in one scrolling strip; the phone/tablet modal keeps
    // the original stacked list.
    let horizontal = ff == theme::FormFactor::Desktop;
    // Header: the title centred, with a close button pinned to the right.
    // Positioned absolutely inside the header row so the title stays optically
    // centred on the panel rather than being pushed off-centre by the button.
    p.spawn(Node {
        width: Val::Percent(100.0),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    })
    .with_children(|h| {
        h.spawn(theme::text(i18n_hud::tab_build(lang), theme::FS_SECTION, theme::BRASS));
        h.spawn((
            Button,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(0.0),
                width: Val::Px(30.0),
                height: Val::Px(30.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                ..default()
            },
            BackgroundColor(BTN),
            BaseColor(BTN),
            BorderColor::all(theme::BORDER),
            BuildPanelCloseBtn,
        ))
        .with_children(|b| {
            b.spawn(theme::text("X", FS_SMALL, TEXT_PRIMARY));
        });
    });
    p.spawn(theme::brass_divider());
    // Rad-xabar qatori: resurs yetmay bosilganda `build_buttons` shu yerga
    // sababni yozadi (bo'sh bo'lsa ko'rinmas darajada kichik).
    p.spawn((
        theme::text("", FS_SMALL - 1.0, theme::DANGER),
        Node {
            align_self: AlignSelf::Center,
            ..default()
        },
        BuildDenyText,
    ));
    // Wraps rather than scrolls. A scrolling strip needs a wheel handler the
    // rest of this UI does not have, and at 1280px the categories overflow by
    // more than one card — so an un-scrollable strip silently hides the last
    // ones. Wrapping keeps every building reachable at any window width, and
    // the dock's `max_height` gives the second row room.
    p.spawn(Node {
        width: Val::Percent(100.0),
        flex_direction: if horizontal { FlexDirection::Row } else { FlexDirection::Column },
        flex_wrap: if horizontal { FlexWrap::Wrap } else { FlexWrap::NoWrap },
        column_gap: Val::Px(theme::SP_LG),
        row_gap: Val::Px(theme::SP_MD),
        align_items: if horizontal { AlignItems::FlexStart } else { AlignItems::Stretch },
        justify_content: if horizontal { JustifyContent::Center } else { JustifyContent::FlexStart },
        ..default()
    })
    .with_children(|p| {
    build_category(p, i18n_hud::build_cat_housing(lang), &[BuildingKind::Tent], ff, lang, horizontal);
    build_category(
        p,
        i18n_hud::build_cat_production(lang),
        &[
            BuildingKind::Sawmill,
            BuildingKind::CoalMine,
            BuildingKind::HunterHut,
            BuildingKind::Greenhouse,
            BuildingKind::TailorShop,
            BuildingKind::Well,
            BuildingKind::Farmhouse,
        ],
        ff,
        lang,
        horizontal,
    );
    build_category(
        p,
        i18n_hud::build_cat_services(lang),
        &[BuildingKind::Hospital, BuildingKind::Kitchen, BuildingKind::Warehouse],
        ff,
        lang,
        horizontal,
    );
    build_category(
        p,
        i18n_hud::build_cat_defense(lang),
        &[BuildingKind::Wall, BuildingKind::Gate],
        ff,
        lang,
        horizontal,
    );
    // V0.19: roads + the Snow Crew that keeps them clear.
    infra_category(p, ff, lang, horizontal);
    });
}

/// Bolg'a glifi — burchak tugmasining ichidagi yassi piktogramma (ko'ndalang
/// bosh + tik dasta) — bu kod bazasi ikonalarni tekis rangli
/// to'rtburchaklardan yasaydi, rasm-asset yo'q.
fn spawn_hammer_glyph(parent: &mut ChildSpawnerCommands, w: f32) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(1.0),
            ..default()
        })
        .with_children(|g| {
            g.spawn((
                Node {
                    width: Val::Px(w),
                    height: Val::Px((w * 0.34).max(4.0)),
                    border_radius: BorderRadius::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(theme::BRASS),
            ));
            g.spawn((
                Node {
                    width: Val::Px((w * 0.22).max(3.0)),
                    height: Val::Px(w * 0.70),
                    border_radius: BorderRadius::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(theme::BRASS),
            ));
        });
}

/// One labeled category inside the Build tab: a heading and a wrapping row of
/// its building buttons (each an existing `BuildBtn`, so `build_buttons` styles
/// and handles it exactly like the old bar's buttons did).
fn build_category(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    kinds: &[BuildingKind],
    ff: theme::FormFactor,
    lang: Lang,
    horizontal: bool,
) {
    // The category owns a column of its own (label над its tiles) rather than
    // dropping both as loose siblings. In the tall drawer that reads exactly
    // as before; in the wide bottom dock it is what lets categories sit side
    // by side instead of the label and its tiles splitting apart.
    parent
        .spawn(category_column(horizontal))
        .with_children(|col| {
            col.spawn(theme::text(label, FS_SMALL, TEXT_MUTED));
            col.spawn(tile_row(horizontal)).with_children(|row| {
                for &kind in kinds {
                    build_kind_tile(row, kind, ff, lang);
                }
            });
        });
}

/// Wrapper for one category. Auto-width and un-shrinkable when laid out
/// horizontally, so a long category keeps its tiles on one line and the dock
/// scrolls instead of squashing them.
fn category_column(horizontal: bool) -> Node {
    Node {
        width: if horizontal { Val::Auto } else { Val::Percent(100.0) },
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(SP_XS),
        flex_shrink: 0.0,
        ..default()
    }
}

fn tile_row(horizontal: bool) -> Node {
    Node {
        width: if horizontal { Val::Auto } else { Val::Percent(100.0) },
        flex_direction: FlexDirection::Row,
        flex_wrap: if horizontal { FlexWrap::NoWrap } else { FlexWrap::Wrap },
        column_gap: Val::Px(SP_XS),
        row_gap: Val::Px(SP_XS),
        ..default()
    }
}

/// One `BuildBtn` tile — split out of `build_category` so `infra_category`
/// can drop `BuildingKind::SnowCrew` into its own row alongside the (non-
/// `BuildingKind`) road tool tiles below, without duplicating this markup.
fn build_kind_tile(row: &mut ChildSpawnerCommands, kind: BuildingKind, ff: theme::FormFactor, lang: Lang) {
    let btn_h = if ff.compact() { 54.0_f32.max(ff.btn_h()) } else { 56.0 };
    // Number-key hotkey (desktop only) still keys off the building's
    // position in `BUILDABLE`, not its position in this panel.
    let hotkey = if ff.compact() {
        None
    } else {
        BuildingKind::BUILDABLE.iter().position(|&k| k == kind).map(|i| i + 1)
    };
    row.spawn((
        Button,
        Node {
            width: Val::Px(88.0),
            height: Val::Px(btn_h),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: Val::Px(2.0),
            flex_shrink: 0.0,
            border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
            ..default()
        },
        BackgroundColor(BTN),
        BaseColor(BTN),
        BuildBtn(kind),
    ))
    .with_children(|b| {
        b.spawn(theme::text(
            i18n_names::building_name(kind, lang),
            FS_SMALL - 1.0,
            TEXT_PRIMARY,
        ));
        b.spawn((
            theme::text(
                i18n_hud::build_cost_badge(kind.cost_wood(), hotkey, lang),
                FS_SMALL - 2.0,
                TEXT_MUTED,
            ),
            BuildCostBadge(kind),
        ));
    });
}

/// V0.19: roads + the Snow Crew that keeps them clear, grouped as their own
/// "Infrastructure" category — distinct from a plain `build_category` call
/// because two of its three tiles (`RoadToolBtn`) aren't `BuildingKind`s at
/// all: dragging a road paints a batch of tiles rather than dropping one
/// building, so they arm `roads::RoadMode` instead of `BuildMode` (see
/// `road_tool_buttons`).
fn infra_category(
    parent: &mut ChildSpawnerCommands,
    ff: theme::FormFactor,
    lang: Lang,
    horizontal: bool,
) {
    let col_node = category_column(horizontal);
    let row_node = tile_row(horizontal);
    parent.spawn(col_node).with_children(|col| {
    col.spawn(theme::text(i18n_roads::build_cat_infra(lang), FS_SMALL, TEXT_MUTED));
    col
        .spawn(row_node)
        .with_children(|row| {
            let btn_h = if ff.compact() { 54.0_f32.max(ff.btn_h()) } else { 56.0 };
            for (mode, label, badge) in [
                (
                    RoadMode::Draw,
                    i18n_roads::road_tool_draw(lang),
                    i18n_roads::road_tool_draw_badge(ROAD_COST_WOOD as u32, lang),
                ),
                (
                    RoadMode::Erase,
                    i18n_roads::road_tool_erase(lang),
                    i18n_roads::road_tool_erase_badge((ROAD_COST_WOOD * ROAD_REFUND) as u32, lang),
                ),
            ] {
                row.spawn((
                    Button,
                    Node {
                        width: Val::Px(88.0),
                        height: Val::Px(btn_h),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(2.0),
                        flex_shrink: 0.0,
                        border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                        ..default()
                    },
                    BackgroundColor(BTN),
                    RoadToolBtn(mode),
                ))
                .with_children(|b| {
                    b.spawn(theme::text(label, FS_SMALL - 1.0, TEXT_PRIMARY));
                    b.spawn(theme::text(badge, FS_SMALL - 2.0, TEXT_MUTED));
                });
            }
            // Snow Crew is a normal placeable building — reuses `BuildBtn`
            // exactly like every other category, just grouped here since its
            // whole job is keeping this category's roads open.
            build_kind_tile(row, BuildingKind::SnowCrew, ff, lang);
        });
    col.spawn(theme::text(i18n_roads::road_tool_hint(lang), FS_SMALL - 2.0, TEXT_MUTED));
    });
}

/// Handles the two road-tool tiles: click arms/toggles `RoadMode` (mirroring
/// `build_buttons`' toggle-on-reclick) and clears `BuildMode`/`PendingPlace`/
/// `RelocateMode` — a road tool and a building placement can never both be
/// live, same mutual exclusion `build_buttons` enforces in the other
/// direction.
pub fn road_tool_buttons(
    mut road: super::super::roads::RoadModes,
    mut build: ResMut<BuildMode>,
    mut pending: ResMut<PendingPlace>,
    mut relocate: ResMut<RelocateMode>,
    clicked: Query<(&Interaction, &RoadToolBtn), Changed<Interaction>>,
    mut all: Query<(&Interaction, &RoadToolBtn, &mut BackgroundColor)>,
) {
    for (interaction, btn) in &clicked {
        if *interaction == Interaction::Pressed {
            *road.mode = if *road.mode == btn.0 { RoadMode::Off } else { btn.0 };
            road.paint.reset();
            build.0 = None;
            pending.0 = None;
            relocate.0 = None;
        }
    }
    for (interaction, btn, mut bg) in &mut all {
        let color = if *road.mode == btn.0 {
            BTN_ACTIVE
        } else if *interaction == Interaction::Hovered {
            BTN_HOVER
        } else {
            BTN
        };
        if bg.0 != color {
            bg.0 = color;
        }
    }
}

/// Burchak tugmasi bosilishi (yoki `B`) modalni ochadi/yopadi; ochiq holatda
/// `Escape` yopadi. Chat faolligida klaviatura o'yin UI'siga tegmaydi —
/// roster/research panellari bilan bir xil qoida.
pub fn build_panel_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    chat: Res<ChatState>,
    clicked: Query<&Interaction, (With<BuildPanelToggleBtn>, Changed<Interaction>)>,
    closed: Query<&Interaction, (With<BuildPanelCloseBtn>, Changed<Interaction>)>,
    mut open: ResMut<BuildPanelOpen>,
) {
    if clicked.iter().any(|i| *i == Interaction::Pressed) {
        open.0 = !open.0;
    }
    // The dock's own X only ever closes — unlike the corner medallion, which
    // toggles. It is inside the panel, so there is nothing for it to open.
    if closed.iter().any(|i| *i == Interaction::Pressed) {
        open.0 = false;
    }
    if !chat.active {
        if keys.just_pressed(KeyCode::KeyB) {
            open.0 = !open.0;
        }
        if open.0 && keys.just_pressed(KeyCode::Escape) {
            open.0 = false;
        }
    }
}

/// `BuildPanelOpen` → scrim ko'rinishi + burchak tugmasi halqasi rangi
/// (ochiq = amber urg'u, yopiq = mis).
pub fn build_panel_visibility(
    open: Res<BuildPanelOpen>,
    mut root: Query<&mut Node, With<BuildPanelRoot>>,
    mut toggle: Query<&mut BorderColor, With<BuildPanelToggleBtn>>,
) {
    let display = if open.0 { Display::Flex } else { Display::None };
    for mut node in &mut root {
        if node.display != display {
            node.display = display;
        }
    }
    let want = BorderColor::all(if open.0 { BTN_ACTIVE } else { theme::BRASS });
    for mut bc in &mut toggle {
        if *bc != want {
            *bc = want;
        }
    }
}
