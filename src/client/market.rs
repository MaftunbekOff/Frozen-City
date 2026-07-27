//! V0.18: the global market panel — the player-to-player order book.
//!
//! Reads `GameView.market`/`GameView.wallet` (filled by `ServerMsg::Market`,
//! NOT part of any world snapshot — the book lives in the accounts DB) and
//! sends `ClientMsg::RefreshMarket`/`PostOrder`/`TakeOrder`/`CancelOrder`.
//!
//! Trading happens from a player's OWN colony, where their stockpile is — the
//! server refuses market commands in the central world (its stock is
//! communal), so this panel explains that rather than offering dead buttons:
//! the post form hides entirely while `GameState.central`, replaced by a
//! short explanation; the book itself stays visible either way (reading it is
//! always allowed).
//!
//! Refusal feedback (not enough gold, price out of range, someone else took
//! it first, ...) is server text delivered as `ServerMsg::Bubble` — this
//! panel does nothing special for it, it rides the existing toast pipeline
//! (`social::drain_bubbles_to_toasts`) every other system feedback line
//! already uses.
//!
//! Modelled on `social::panel`/`spawn` (list-with-rows, mobile bottom-sheet
//! aware, fixed-size row pool toggled by `Display`) and `research`
//! (modal/scrim/`UiBlocker`/`theme::` widgets/`plugin(app)`).

use bevy::prelude::*;

use frozen_city::game::types::TradeGood;
use frozen_city::net::protocol::ClientMsg;

use super::chat::ChatState;
use super::i18n::Lang;
use super::i18n_hud;
use super::i18n_market;
use super::i18n_panels;
use super::theme::{self, BaseColor, FormFactor};
use super::ui::UiBlocker;
use super::{GameView, NetConn, Screen};

/// Whether the market modal is open (also gates world/camera input).
#[derive(Resource, Default)]
pub struct MarketOpen(pub bool);

/// The player's in-progress post form. Not reset on close (so flipping the
/// panel open/shut doesn't lose a half-set-up order) — only on leaving the
/// game entirely (`reset_market`).
#[derive(Resource)]
struct PostForm {
    good: TradeGood,
    selling: bool,
    amount: u32,
    unit_price: f32,
}

impl Default for PostForm {
    fn default() -> Self {
        PostForm { good: TradeGood::Wood, selling: true, amount: 10, unit_price: 1.0 }
    }
}

/// Visible order rows at once. The server already caps a `RefreshMarket`
/// answer at `MAX_BOOK_ROWS` (60, native-only `fc_net::market` constant, not
/// importable here — see `AMOUNT_MAX`'s doc); this panel shows a smaller
/// scrollable window of that, newest first, same trade-off `social::
/// FRIEND_ROWS` makes for the friends list.
const ROWS: usize = 20;

const AMOUNT_STEP: u32 = 5;
const AMOUNT_MIN: u32 = 1;
/// Mirrors `fc_net::market::MAX_ORDER_AMOUNT` — duplicated rather than
/// imported because that module is native-only (`#[cfg(not(target_arch =
/// "wasm32"))]`, it needs a real filesystem for the accounts DB) while this
/// panel also builds for the wasm client, which never links that crate
/// module at all and only ever talks to a market over the wire. The server
/// re-validates independently regardless of what this stepper allows.
const AMOUNT_MAX: u32 = 500;
const PRICE_STEP: f32 = 0.5;
const PRICE_MIN: f32 = 0.5;
/// Mirrors `fc_net::market::MAX_UNIT_PRICE` (see `AMOUNT_MAX`'s doc for why
/// this is a duplicate, not an import). The server's own floor (0.01) is
/// more permissive than this stepper's `PRICE_MIN`; that's fine, a stepper
/// only needs to reach sane values quickly, not every value the server
/// would accept.
const PRICE_MAX: f32 = 999.0;

#[derive(Component)]
struct MarketRoot;

#[derive(Component)]
struct MarketHudBtn;

#[derive(Component)]
struct CentralNoticeRow;

#[derive(Component)]
struct WalletRow;

#[derive(Component)]
struct WalletText;

#[derive(Component)]
struct PostFormSection;

#[derive(Component)]
struct GoodBtn(TradeGood);

#[derive(Component)]
struct GoodBtnLabel(TradeGood);

#[derive(Component)]
struct SideBtn(bool);

#[derive(Component, Clone, Copy, PartialEq)]
enum Stepper {
    AmountMinus,
    AmountPlus,
    PriceMinus,
    PricePlus,
}

#[derive(Component, Clone, Copy, PartialEq)]
enum FieldValueText {
    Amount,
    Price,
}

#[derive(Component)]
struct PostBtn;

#[derive(Component)]
struct PostBtnLabel;

#[derive(Component)]
struct EmptyBookRow;

#[derive(Component)]
struct OrderRow(usize);

#[derive(Component)]
struct OrderRowText(usize);

/// Bound to `view.market[row]` each frame by `update_row_buttons`; `order_id
/// == 0` (never a real SQLite rowid) means this row is either unused or
/// isn't a takeable order right now, and `row_take_click` refuses to act on
/// it — belt-and-braces alongside the `Node` visibility toggle.
#[derive(Component)]
struct RowTakeBtn {
    row: usize,
    order_id: i64,
    amount: u32,
}

#[derive(Component)]
struct RowCancelBtn {
    row: usize,
    order_id: i64,
}

/// Every static (language-only, not per-order) label in this panel — same
/// idiom as `social::StaticLabel` (see that type's doc comment): one enum,
/// one `&mut Text` query, so N label kinds never need N mutually-excluding
/// queries in the same system. `BtnTake`/`BtnCancel` are shared by every row
/// button's label (all say the same word), not one variant per row.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum StaticLabel {
    Title,
    SectionPost,
    SectionBook,
    EmptyBook,
    CentralNotice,
    BtnSell,
    BtnBuy,
    BtnTake,
    BtnCancel,
    AmountFieldLabel,
    PriceFieldLabel,
    Hud,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<MarketOpen>()
        .init_resource::<PostForm>()
        .add_systems(OnEnter(Screen::Game), (spawn_market_ui, spawn_hud_button))
        .add_systems(OnExit(Screen::Game), reset_market)
        .add_systems(
            Update,
            (
                toggle_market,
                market_hud_button,
                refresh_on_open,
                update_root_display,
                update_static_labels,
                update_central_gate,
                update_wallet_banner,
            )
                .run_if(in_state(Screen::Game)),
        )
        .add_systems(
            Update,
            (
                update_good_selector,
                update_side_selector,
                update_form_values,
                update_post_button,
                good_buttons_click,
                side_buttons_click,
                stepper_click,
                post_button_click,
            )
                .run_if(in_state(Screen::Game)),
        )
        .add_systems(
            Update,
            (
                update_empty_book,
                update_row_visibility,
                update_row_text,
                update_row_buttons,
                row_take_click,
                row_cancel_click,
            )
                .run_if(in_state(Screen::Game)),
        );
}

/// This account's id, resolved the same way `input.rs`/`roster::panel` find
/// "my own" player row: `GameView.player_id` (this connection's id within
/// the current world) looked up in `GameState.players` for its `account`.
/// `None` for a guest (no account) or before the first snapshot arrives.
fn my_account(view: &GameView) -> Option<i64> {
    let state = view.state.as_ref()?;
    let pid = view.player_id?;
    state.players.iter().find(|p| p.id == pid)?.account
}

fn reset_market(mut open: ResMut<MarketOpen>, mut form: ResMut<PostForm>) {
    *open = MarketOpen::default();
    *form = PostForm::default();
}

fn small_btn(bg: Color, w: f32, h: f32) -> impl Bundle {
    (
        Button,
        Node {
            width: Val::Px(w),
            height: Val::Px(h),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
            ..default()
        },
        BackgroundColor(bg),
        BaseColor(bg),
    )
}

#[allow(clippy::too_many_lines)]
fn spawn_market_ui(mut commands: Commands, ff: Res<FormFactor>) {
    let ff = *ff;
    let btn_h = ff.btn_h();
    commands
        .spawn((theme::scrim(ff), UiBlocker, MarketRoot, DespawnOnExit(Screen::Game)))
        .with_children(|p| {
            p.spawn(theme::modal_panel(ff)).with_children(|panel| {
                panel.spawn((theme::title(""), StaticLabel::Title));

                // Central-world explainer, shown instead of the post form.
                panel
                    .spawn((
                        Node {
                            display: Display::None,
                            padding: UiRect::all(Val::Px(theme::SP_SM)),
                            border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                            ..default()
                        },
                        BackgroundColor(theme::BG_SECTION),
                        CentralNoticeRow,
                    ))
                    .with_children(|row| {
                        row.spawn((
                            theme::text("", theme::FS_SMALL, theme::TEXT_MUTED),
                            StaticLabel::CentralNotice,
                        ));
                    });

                // Wallet banner, shown only while the market owes something.
                panel
                    .spawn((
                        Node {
                            display: Display::None,
                            padding: UiRect::all(Val::Px(theme::SP_SM)),
                            border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                            ..default()
                        },
                        BackgroundColor(theme::BG_SECTION),
                        WalletRow,
                    ))
                    .with_children(|row| {
                        row.spawn((theme::text("", theme::FS_SMALL, theme::RES_GOLD), WalletText));
                    });

                // --- Post an order (hidden centrally) ---
                panel
                    .spawn((
                        Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(theme::SP_XS),
                            ..default()
                        },
                        PostFormSection,
                    ))
                    .with_children(|form| {
                        form.spawn((theme::section(""), StaticLabel::SectionPost));
                        form.spawn(theme::divider());

                        // Good selector: one toggle per TradeGood.
                        form.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            flex_wrap: FlexWrap::Wrap,
                            column_gap: Val::Px(theme::SP_XS),
                            row_gap: Val::Px(theme::SP_XS),
                            ..default()
                        })
                        .with_children(|row| {
                            for good in TradeGood::ALL {
                                row.spawn((
                                    Button,
                                    Node {
                                        min_width: Val::Px(78.0),
                                        height: Val::Px(btn_h),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        padding: UiRect::horizontal(Val::Px(theme::SP_SM)),
                                        border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                                        ..default()
                                    },
                                    // No BaseColor: this button's color is owned
                                    // entirely by `update_good_selector`
                                    // (selected/unselected), so the generic hover
                                    // system must not fight it over
                                    // BackgroundColor (same reasoning as
                                    // research.rs's ResearchBtn).
                                    BackgroundColor(theme::BTN),
                                    GoodBtn(good),
                                ))
                                .with_children(|b| {
                                    b.spawn((
                                        theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY),
                                        GoodBtnLabel(good),
                                    ));
                                });
                            }
                        });

                        // Sell/Buy toggle.
                        form.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(theme::SP_XS),
                            ..default()
                        })
                        .with_children(|row| {
                            for selling in [true, false] {
                                row.spawn((
                                    Button,
                                    Node {
                                        flex_grow: 1.0,
                                        height: Val::Px(btn_h),
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                                        ..default()
                                    },
                                    // No BaseColor: same reasoning as the good
                                    // selector above.
                                    BackgroundColor(theme::BTN),
                                    SideBtn(selling),
                                ))
                                .with_children(|b| {
                                    b.spawn((
                                        theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY),
                                        if selling { StaticLabel::BtnSell } else { StaticLabel::BtnBuy },
                                    ));
                                });
                            }
                        });

                        // Amount stepper.
                        form.spawn((
                            theme::text("", theme::FS_MICRO, theme::TEXT_FAINT),
                            StaticLabel::AmountFieldLabel,
                        ));
                        form.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::SpaceBetween,
                            column_gap: Val::Px(theme::SP_SM),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((small_btn(theme::BTN, 44.0, btn_h), Stepper::AmountMinus))
                                .with_children(|b| {
                                    b.spawn(theme::text("-", theme::FS_BODY + 2.0, theme::TEXT_PRIMARY));
                                });
                            row.spawn((
                                theme::text("", theme::FS_BODY, theme::TEXT_PRIMARY),
                                FieldValueText::Amount,
                            ));
                            row.spawn((small_btn(theme::BTN, 44.0, btn_h), Stepper::AmountPlus))
                                .with_children(|b| {
                                    b.spawn(theme::text("+", theme::FS_BODY + 2.0, theme::TEXT_PRIMARY));
                                });
                        });

                        // Price stepper.
                        form.spawn((
                            theme::text("", theme::FS_MICRO, theme::TEXT_FAINT),
                            StaticLabel::PriceFieldLabel,
                        ));
                        form.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::SpaceBetween,
                            column_gap: Val::Px(theme::SP_SM),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((small_btn(theme::BTN, 44.0, btn_h), Stepper::PriceMinus))
                                .with_children(|b| {
                                    b.spawn(theme::text("-", theme::FS_BODY + 2.0, theme::TEXT_PRIMARY));
                                });
                            row.spawn((
                                theme::text("", theme::FS_BODY, theme::TEXT_PRIMARY),
                                FieldValueText::Price,
                            ));
                            row.spawn((small_btn(theme::BTN, 44.0, btn_h), Stepper::PricePlus))
                                .with_children(|b| {
                                    b.spawn(theme::text("+", theme::FS_BODY + 2.0, theme::TEXT_PRIMARY));
                                });
                        });

                        // Post button.
                        form.spawn((
                            Button,
                            Node {
                                width: Val::Percent(100.0),
                                height: Val::Px(btn_h),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                margin: UiRect::top(Val::Px(theme::SP_XS)),
                                border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                                ..default()
                            },
                            // No BaseColor: owned by `update_post_button`
                            // (affordability dim/bright).
                            BackgroundColor(theme::BTN_SUCCESS),
                            PostBtn,
                        ))
                        .with_children(|b| {
                            b.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY), PostBtnLabel));
                        });
                    });

                // --- Order book ---
                panel.spawn((theme::section(""), StaticLabel::SectionBook));
                panel.spawn(theme::divider());
                panel
                    .spawn((Node { display: Display::None, ..default() }, EmptyBookRow))
                    .with_children(|row| {
                        row.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_FAINT), StaticLabel::EmptyBook));
                    });

                for i in 0..ROWS {
                    panel
                        .spawn((
                            Node {
                                display: Display::None,
                                flex_direction: FlexDirection::Row,
                                align_items: AlignItems::Center,
                                column_gap: Val::Px(theme::SP_XS),
                                padding: UiRect::axes(Val::Px(theme::SP_SM), Val::Px(theme::SP_XS)),
                                border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                                ..default()
                            },
                            BackgroundColor(theme::BG_SECTION),
                            OrderRow(i),
                        ))
                        .with_children(|row| {
                            row.spawn((
                                theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY),
                                OrderRowText(i),
                                Node { flex_grow: 1.0, ..default() },
                            ));
                            row.spawn((
                                small_btn(theme::BTN_SUCCESS, 60.0, btn_h * 0.8),
                                RowTakeBtn { row: i, order_id: 0, amount: 0 },
                            ))
                            .with_children(|b| {
                                b.spawn((theme::text("", theme::FS_MICRO, theme::TEXT_PRIMARY), StaticLabel::BtnTake));
                            });
                            row.spawn((
                                small_btn(theme::BTN_DANGER, 74.0, btn_h * 0.8),
                                RowCancelBtn { row: i, order_id: 0 },
                            ))
                            .with_children(|b| {
                                b.spawn((
                                    theme::text("", theme::FS_MICRO, theme::TEXT_PRIMARY),
                                    StaticLabel::BtnCancel,
                                ));
                            });
                        });
                }
            });
        });
}

/// The Market HUD button (mobile has no `M` key). Spawned separately from
/// `social::spawn_hud_button`, same "own file, own entity" reasoning that
/// comment explains — sits beside it on Desktop/Tablet (`left: 302`, a gap
/// past its `left: 206` + 90px width) and stacked underneath it on Mobile
/// (`top: 234`, below its `top: 196` + 28px height + a gap), since the
/// Friends button already claimed the space to the minimap's right/below.
fn spawn_hud_button(mut commands: Commands, ff: Res<FormFactor>) {
    commands
        .spawn((
            Button,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(if ff.compact() { 12.0 } else { 302.0 }),
                top: Val::Px(if ff.compact() { 234.0 } else { 78.0 }),
                width: Val::Px(96.0),
                height: Val::Px(28.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                ..default()
            },
            BackgroundColor(theme::BTN),
            BaseColor(theme::BTN),
            MarketHudBtn,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|b| {
            b.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY), StaticLabel::Hud));
        });
}

fn toggle_market(keys: Res<ButtonInput<KeyCode>>, chat: Res<ChatState>, mut open: ResMut<MarketOpen>) {
    if !chat.active && keys.just_pressed(KeyCode::KeyM) {
        open.0 = !open.0;
    }
    if open.0 && keys.just_pressed(KeyCode::Escape) {
        open.0 = false;
    }
}

fn market_hud_button(
    q: Query<&Interaction, (With<MarketHudBtn>, Changed<Interaction>)>,
    mut open: ResMut<MarketOpen>,
) {
    for interaction in &q {
        if *interaction == Interaction::Pressed {
            open.0 = !open.0;
        }
    }
}

/// Ask the server for a fresh book the moment the panel opens — same idiom
/// as `social::refresh_on_open`.
fn refresh_on_open(net: Res<NetConn>, open: Res<MarketOpen>, mut was_open: Local<bool>) {
    if open.0 && !*was_open {
        net.send(ClientMsg::RefreshMarket);
    }
    *was_open = open.0;
}

fn update_root_display(open: Res<MarketOpen>, mut root: Query<&mut Node, With<MarketRoot>>) {
    let d = if open.0 { Display::Flex } else { Display::None };
    for mut node in &mut root {
        if node.display != d {
            node.display = d;
        }
    }
}

fn update_static_labels(lang: Res<Lang>, mut labels: Query<(&StaticLabel, &mut Text)>) {
    let lang = *lang;
    for (marker, mut t) in &mut labels {
        let new = match marker {
            StaticLabel::Title => format!("{}   {}", i18n_market::title(lang), i18n_market::hint(lang)),
            StaticLabel::SectionPost => i18n_market::section_post(lang).to_string(),
            StaticLabel::SectionBook => i18n_market::section_book(lang).to_string(),
            StaticLabel::EmptyBook => i18n_market::empty_book(lang).to_string(),
            StaticLabel::CentralNotice => i18n_market::central_notice(lang).to_string(),
            StaticLabel::BtnSell => i18n_market::btn_sell(lang).to_string(),
            StaticLabel::BtnBuy => i18n_market::btn_buy(lang).to_string(),
            StaticLabel::BtnTake => i18n_market::btn_take(lang).to_string(),
            StaticLabel::BtnCancel => i18n_market::btn_cancel(lang).to_string(),
            StaticLabel::AmountFieldLabel => i18n_market::amount_field_label(lang).to_string(),
            StaticLabel::PriceFieldLabel => i18n_market::price_field_label(lang).to_string(),
            StaticLabel::Hud => i18n_market::hud_button(lang).to_string(),
        };
        if t.0 != new {
            t.0 = new;
        }
    }
}

/// Post form vs. central-world explainer — exactly one of the two is shown,
/// gated on `GameState.central`. Reading the book stays available either way
/// (only these two toggle).
fn update_central_gate(
    view: Res<GameView>,
    mut form_section: Query<&mut Node, (With<PostFormSection>, Without<CentralNoticeRow>)>,
    mut notice: Query<&mut Node, (With<CentralNoticeRow>, Without<PostFormSection>)>,
) {
    let central = view.state.as_ref().map(|s| s.central).unwrap_or(false);
    for mut node in &mut form_section {
        let d = if central { Display::None } else { Display::Flex };
        if node.display != d {
            node.display = d;
        }
    }
    for mut node in &mut notice {
        let d = if central { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
        }
    }
}

fn update_wallet_banner(
    view: Res<GameView>,
    lang: Res<Lang>,
    mut row: Query<&mut Node, With<WalletRow>>,
    mut text: Query<&mut Text, With<WalletText>>,
) {
    let lang = *lang;
    let show = view.wallet.as_ref().is_some_and(|w| !w.is_empty());
    for mut node in &mut row {
        let d = if show { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
        }
    }
    if !show {
        return;
    }
    let Some(w) = view.wallet.as_ref() else { return };
    let mut parts = Vec::new();
    if w.gold > 0.0 {
        parts.push(format!("{:.0} {}", w.gold, i18n_market::gold_label(lang)));
    }
    for (amount, good) in [
        (w.wood, TradeGood::Wood),
        (w.coal, TradeGood::Coal),
        (w.food, TradeGood::Food),
        (w.fur, TradeGood::Fur),
        (w.cloth, TradeGood::Cloth),
    ] {
        if amount > 0.0 {
            parts.push(format!("{:.0} {}", amount, i18n_hud::trade_good_name(good, lang)));
        }
    }
    let new = format!("{}: {}", i18n_market::wallet_label(lang), parts.join(", "));
    if let Ok(mut t) = text.single_mut() {
        if t.0 != new {
            t.0 = new;
        }
    }
}

fn update_good_selector(
    form: Res<PostForm>,
    lang: Res<Lang>,
    mut buttons: Query<(&GoodBtn, &mut BackgroundColor)>,
    mut labels: Query<(&GoodBtnLabel, &mut Text)>,
) {
    let lang = *lang;
    for (btn, mut bg) in &mut buttons {
        let want = if btn.0 == form.good { theme::BTN_ACTIVE } else { theme::BTN };
        if bg.0 != want {
            bg.0 = want;
        }
    }
    for (label, mut t) in &mut labels {
        let new = i18n_hud::trade_good_name(label.0, lang);
        if t.0 != new {
            t.0 = new.to_string();
        }
    }
}

fn update_side_selector(form: Res<PostForm>, mut buttons: Query<(&SideBtn, &mut BackgroundColor)>) {
    for (btn, mut bg) in &mut buttons {
        let want = if btn.0 == form.selling { theme::BTN_ACTIVE } else { theme::BTN };
        if bg.0 != want {
            bg.0 = want;
        }
    }
}

fn update_form_values(form: Res<PostForm>, mut texts: Query<(&FieldValueText, &mut Text)>) {
    for (field, mut t) in &mut texts {
        let new = match field {
            FieldValueText::Amount => form.amount.to_string(),
            FieldValueText::Price => format!("{:.2}", form.unit_price),
        };
        if t.0 != new {
            t.0 = new;
        }
    }
}

fn update_post_button(
    form: Res<PostForm>,
    view: Res<GameView>,
    lang: Res<Lang>,
    mut bg: Query<&mut BackgroundColor, With<PostBtn>>,
    mut label: Query<&mut Text, With<PostBtnLabel>>,
) {
    let lang = *lang;
    let good_name = i18n_hud::trade_good_name(form.good, lang);
    let total = form.amount as f32 * form.unit_price;
    let new_label = i18n_market::post_btn_label(form.selling, form.amount, good_name, total, lang);
    if let Ok(mut t) = label.single_mut() {
        if t.0 != new_label {
            t.0 = new_label;
        }
    }
    let affordable = view.state.as_ref().is_some_and(|s| {
        if form.selling {
            form.good.amount_in(&s.stock) >= form.amount as f32
        } else {
            s.stock.gold >= total
        }
    });
    let want = if affordable { theme::BTN_SUCCESS } else { theme::BTN_DIM };
    if let Ok(mut c) = bg.single_mut() {
        if c.0 != want {
            c.0 = want;
        }
    }
}

fn good_buttons_click(mut form: ResMut<PostForm>, clicked: Query<(&Interaction, &GoodBtn), Changed<Interaction>>) {
    for (interaction, btn) in &clicked {
        if *interaction == Interaction::Pressed {
            form.good = btn.0;
        }
    }
}

fn side_buttons_click(mut form: ResMut<PostForm>, clicked: Query<(&Interaction, &SideBtn), Changed<Interaction>>) {
    for (interaction, btn) in &clicked {
        if *interaction == Interaction::Pressed {
            form.selling = btn.0;
        }
    }
}

fn stepper_click(mut form: ResMut<PostForm>, clicked: Query<(&Interaction, &Stepper), Changed<Interaction>>) {
    for (interaction, step) in &clicked {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match step {
            Stepper::AmountMinus => form.amount = form.amount.saturating_sub(AMOUNT_STEP).max(AMOUNT_MIN),
            Stepper::AmountPlus => form.amount = (form.amount + AMOUNT_STEP).min(AMOUNT_MAX),
            Stepper::PriceMinus => form.unit_price = (form.unit_price - PRICE_STEP).max(PRICE_MIN),
            Stepper::PricePlus => form.unit_price = (form.unit_price + PRICE_STEP).min(PRICE_MAX),
        }
    }
}

fn post_button_click(
    net: Res<NetConn>,
    form: Res<PostForm>,
    view: Res<GameView>,
    clicked: Query<&Interaction, (Changed<Interaction>, With<PostBtn>)>,
) {
    if !clicked.iter().any(|i| *i == Interaction::Pressed) {
        return;
    }
    // The form is hidden centrally (`update_central_gate`), but a click that
    // landed just before a world switch shouldn't slip through either.
    if view.state.as_ref().is_none_or(|s| s.central) {
        return;
    }
    net.send(ClientMsg::PostOrder {
        good: form.good,
        amount: form.amount,
        unit_price: form.unit_price,
        selling: form.selling,
    });
}

fn update_empty_book(view: Res<GameView>, mut row: Query<&mut Node, With<EmptyBookRow>>) {
    let show = view.market.is_empty();
    for mut node in &mut row {
        let d = if show { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
        }
    }
}

fn update_row_visibility(view: Res<GameView>, mut rows: Query<(&OrderRow, &mut Node)>) {
    let shown = view.market.len().min(ROWS);
    for (row, mut node) in &mut rows {
        let d = if row.0 < shown { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
        }
    }
}

fn update_row_text(view: Res<GameView>, lang: Res<Lang>, mut texts: Query<(&OrderRowText, &mut Text)>) {
    let lang = *lang;
    let mine = my_account(&view);
    for (marker, mut t) in &mut texts {
        let new = match view.market.get(marker.0) {
            Some(o) => {
                let good_name = i18n_hud::trade_good_name(o.good, lang);
                let poster = if Some(o.account) == mine {
                    format!("{}{}", o.name, i18n_panels::you_tag(lang))
                } else {
                    o.name.clone()
                };
                if o.selling {
                    i18n_market::order_row_sell(&poster, o.amount, good_name, o.unit_price, lang)
                } else {
                    i18n_market::order_row_buy(&poster, o.amount, good_name, o.unit_price, lang)
                }
            }
            None => String::new(),
        };
        if t.0 != new {
            t.0 = new;
        }
    }
}

/// Refreshes both the Take and Cancel buttons' bound order id/amount AND
/// their visibility (a row shows exactly one of the two: Cancel for the
/// player's own order, Take for anyone else's — never both, never neither
/// while the row holds a real order). Central world: neither ever shows,
/// even though the row TEXT stays visible (reading the book is always
/// allowed, acting on it isn't).
fn update_row_buttons(
    view: Res<GameView>,
    mut take_btns: Query<(&mut RowTakeBtn, &mut Node), Without<RowCancelBtn>>,
    mut cancel_btns: Query<(&mut RowCancelBtn, &mut Node), Without<RowTakeBtn>>,
) {
    let central = view.state.as_ref().map(|s| s.central).unwrap_or(false);
    let mine = my_account(&view);
    for (mut btn, mut node) in &mut take_btns {
        let order = view.market.get(btn.row);
        let is_mine = order.is_some_and(|o| Some(o.account) == mine);
        btn.order_id = order.map(|o| o.id).unwrap_or(0);
        btn.amount = order.map(|o| o.amount).unwrap_or(0);
        let show = !central && order.is_some() && !is_mine;
        let d = if show { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
        }
    }
    for (mut btn, mut node) in &mut cancel_btns {
        let order = view.market.get(btn.row);
        let is_mine = order.is_some_and(|o| Some(o.account) == mine);
        btn.order_id = order.map(|o| o.id).unwrap_or(0);
        let show = !central && order.is_some() && is_mine;
        let d = if show { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
        }
    }
}

fn row_take_click(net: Res<NetConn>, clicked: Query<(&Interaction, &RowTakeBtn), Changed<Interaction>>) {
    for (interaction, btn) in &clicked {
        if *interaction == Interaction::Pressed && btn.order_id != 0 {
            net.send(ClientMsg::TakeOrder { order: btn.order_id, amount: btn.amount });
        }
    }
}

fn row_cancel_click(net: Res<NetConn>, clicked: Query<(&Interaction, &RowCancelBtn), Changed<Interaction>>) {
    for (interaction, btn) in &clicked {
        if *interaction == Interaction::Pressed && btn.order_id != 0 {
            net.send(ClientMsg::CancelOrder { order: btn.order_id });
        }
    }
}
