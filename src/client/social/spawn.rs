use bevy::prelude::*;

use crate::client::theme::{self, BaseColor, FormFactor};
use crate::client::ui::UiBlocker;
use crate::client::Screen;

use super::*;

/// `SocialOpen` itself is reset centrally in `mod.rs::teardown_game`
/// (alongside research/roster); this only clears the add-friend text box,
/// which is private to this module.
pub(crate) fn reset_add_friend_form(mut form: ResMut<AddFriendForm>) {
    *form = AddFriendForm::default();
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_social_ui(mut commands: Commands, ff: Res<FormFactor>) {
    let ff = *ff;
    let btn_h = ff.btn_h();
    // --- Main modal (friends list, add-friend field, refresh) ---
    commands
        .spawn((theme::scrim(ff), UiBlocker, SocialRoot, DespawnOnExit(Screen::Game)))
        .with_children(|p| {
            p.spawn(theme::modal_panel(ff)).with_children(|panel| {
                panel.spawn((theme::title(""), StaticLabel::Title));

                // --- Invite: add a friend by name, and refresh the list. ---
                panel.spawn((theme::section(""), StaticLabel::SectionInvite));
                panel.spawn(theme::divider());
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(theme::SP_SM),
                        align_items: AlignItems::Center,
                        margin: UiRect::bottom(Val::Px(theme::SP_XS)),
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            Button,
                            Node {
                                flex_grow: 1.0,
                                height: Val::Px(btn_h),
                                padding: UiRect::horizontal(Val::Px(theme::SP_SM)),
                                justify_content: JustifyContent::FlexStart,
                                align_items: AlignItems::Center,
                                border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                                ..default()
                            },
                            BackgroundColor(FIELD_BG),
                            AddFriendBox,
                        ))
                        .with_children(|b| {
                            b.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_MUTED), AddFriendText));
                        });
                        row.spawn((small_btn(theme::BTN, 60.0, btn_h), AddFriendSubmitBtn))
                            .with_children(|b| {
                                b.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY), StaticLabel::AddFriendSubmit));
                            });
                        row.spawn((small_btn(theme::BTN, 66.0, btn_h), RefreshBtn))
                            .with_children(|b| {
                                b.spawn((theme::text("", theme::FS_MICRO, theme::TEXT_PRIMARY), StaticLabel::Refresh));
                            });
                    });

                // --- Friends list + Showcase (showcase stats ride along each
                // friend row: "d{days} p{population} b{buildings}"). ---
                panel.spawn((theme::section(""), StaticLabel::SectionFriends));
                panel.spawn(theme::divider());
                panel.spawn((theme::text("", theme::FS_MICRO, theme::TEXT_FAINT), StaticLabel::SectionShowcase));

                for i in 0..FRIEND_ROWS {
                    panel
                        .spawn((
                            Node {
                                display: Display::None,
                                align_items: AlignItems::Center,
                                column_gap: Val::Px(theme::SP_XS),
                                padding: UiRect::axes(Val::Px(theme::SP_SM), Val::Px(theme::SP_XS)),
                                border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                                ..default()
                            },
                            BackgroundColor(theme::BG_SECTION),
                            FriendRow(i),
                        ))
                        .with_children(|row| {
                            row.spawn((
                                theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY),
                                FriendName(i),
                                Node {
                                    flex_grow: 1.0,
                                    ..default()
                                },
                            ));
                            row.spawn((
                                small_btn(VISIT_BG, 46.0, btn_h * 0.8),
                                VisitBtn { row: i, account: None },
                            ))
                            .with_children(|b| {
                                b.spawn((theme::text("", theme::FS_MICRO, theme::TEXT_PRIMARY), VisitLabel));
                            });
                            row.spawn((
                                small_btn(theme::BTN_SUCCESS, 52.0, btn_h * 0.8),
                                InviteBtn { row: i, account: None },
                            ))
                            .with_children(|b| {
                                b.spawn((theme::text("", theme::FS_MICRO, theme::TEXT_PRIMARY), InviteLabel));
                            });
                            row.spawn((
                                small_btn(theme::BTN_DANGER, 24.0, btn_h * 0.8),
                                RemoveBtn { row: i, account: None },
                            ))
                            .with_children(|b| {
                                b.spawn(theme::text("x", theme::FS_MICRO, theme::TEXT_PRIMARY));
                            });
                        });
                }

                panel.spawn((theme::section(""), StaticLabel::SectionPolicy));
                panel.spawn(theme::divider());

                // V0.6 owner-offline policy toggle — hidden until the server
                // reports this account's setting (guest sessions never see it).
                panel
                    .spawn((
                        Node {
                            display: Display::None,
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(theme::SP_SM),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        PolicyRow,
                    ))
                    .with_children(|row| {
                        row.spawn((
                            theme::text("", theme::FS_SMALL, theme::TEXT_MUTED),
                            StaticLabel::Policy,
                            Node {
                                flex_grow: 1.0,
                                ..default()
                            },
                        ));
                        row.spawn((small_btn(theme::BTN, 50.0, btn_h * 0.8), PolicyBtn))
                            .with_children(|b| {
                                b.spawn((theme::text("", theme::FS_MICRO, theme::TEXT_PRIMARY), PolicyText));
                            });
                    });
            });
        });

    // --- Incoming-invite toast (top-center, independent of the panel) ---
    commands
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(96.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            InviteToastRoot,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(theme::SP_SM),
                    padding: UiRect::all(Val::Px(theme::SP_MD)),
                    border_radius: BorderRadius::all(Val::Px(theme::RAD_PANEL)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.100, 0.090, 0.030, 0.96)),
                Interaction::default(),
                UiBlocker,
            ))
            .with_children(|panel| {
                panel.spawn((theme::text("", theme::FS_SMALL + 1.0, INVITE_COL), InviteToastText));
                panel
                    .spawn((
                        Button,
                        Node {
                            width: Val::Px(200.0),
                            height: Val::Px(30.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                            ..default()
                        },
                        BackgroundColor(theme::BTN_SUCCESS),
                        BaseColor(theme::BTN_SUCCESS),
                        InviteAcceptBtn,
                    ))
                    .with_children(|b| {
                        b.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY), StaticLabel::InviteAccept));
                    });
            });
        });

    // --- "Visiting <name>'s city" indicator + Home button ---
    commands
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(50.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: Val::Px(theme::SP_MD),
                ..default()
            },
            VisitingRoot,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn((theme::text("", theme::FS_BODY, ONLINE_COL), VisitingText));
            p.spawn((
                Button,
                Node {
                    width: Val::Px(110.0),
                    height: Val::Px(28.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                    ..default()
                },
                BackgroundColor(GO_HOME_BG),
                BaseColor(GO_HOME_BG),
                GoHomeBtn,
            ))
            .with_children(|b| {
                b.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY), StaticLabel::GoHome));
            });
        });

    // --- Toast stack (bottom-right-ish, above the chat log) ---
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(12.0),
            bottom: Val::Px(150.0),
            width: Val::Px(320.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(3.0),
            ..default()
        },
        ToastRoot,
        DespawnOnExit(Screen::Game),
    ));
}

/// The Friends HUD button (mobile has no `F` key). Spawned separately from
/// `ui::spawn_hud` (a different plugin/file) so it survives independently of
/// that module's layout. The top bar's own right-hand buttons (Furnace
/// level, the World-Switch toggle when visible, Menu) are laid out by
/// flexbox with a variable on-screen position (window width, and whether
/// World-Switch is even shown), and the right column below it is already
/// crowded (event feed at `top:54`, then the missions panel at `top:232`) —
/// so this sits on the LEFT instead. FPS text lives at `left:14, top:54` and
/// its full diagnostic string ("FPS 60  |  High  |  WebGPU  |  1920x1080")
/// is wide enough to reach past x=210 at typical window widths, so this
/// button no longer shares that row; it sits to the right of the minimap
/// (184px wide at `left:12, top:78` on non-Mobile, see `minimap::minimap_px`)
/// instead, clear of both the FPS line above and the minimap beside it.
pub(crate) fn spawn_hud_button(mut commands: Commands, ff: Res<FormFactor>) {
    commands
        .spawn((
            Button,
            Node {
                position_type: PositionType::Absolute,
                // Mobil: minimap (110px) ostidagi bo'sh polosa — o'ng tomonda
                // voqealar lentasi band. Desktop/Tablet: minimap yonida.
                left: Val::Px(if ff.compact() { 12.0 } else { 206.0 }),
                top: Val::Px(if ff.compact() { 196.0 } else { 78.0 }),
                width: Val::Px(90.0),
                height: Val::Px(28.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                ..default()
            },
            BackgroundColor(theme::BTN),
            BaseColor(theme::BTN),
            SocialHudBtn,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|b| {
            b.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY), StaticLabel::Hud));
        });
}
