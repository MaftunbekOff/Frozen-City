//! Social panel: friends list, add/remove, visit invites and system/nearby
//! chat-bubble toasts. Toggled with `F` (mirroring `P` for roster, `R` for
//! research) or the "Friends" HUD button (mobile has no keyboard). All state
//! is driven by the existing [`SocialState`] resource — server plumbing
//! (`ClientMsg::AddFriend/RemoveFriend/RefreshSocial/VisitFriend/Invite`,
//! `ServerMsg::Social/Invited/Bubble`) already lands there via `net_sync.rs`;
//! this module only renders it and forwards clicks. Chat-bubble/system-toast
//! TEXT comes from the server (or is a player's own typed chat) and is never
//! translated — only this module's own frame/labels are localized.

use bevy::prelude::*;

use frozen_city::net::protocol::ClientMsg;

use super::i18n_panels;
use super::theme::{self, BaseColor, FormFactor};
use super::chat::ChatState;
use super::ui::UiBlocker;
use super::{i18n, GameView, NetConn, PendingSwitch, Screen, Session, SocialState, WorldTarget};

/// Visible friend rows at once — friends lists are small (this is a co-op
/// hub, not a social network), so no "+N more" overflow line like the
/// roster needs.
const FRIEND_ROWS: usize = 12;
/// Same cap as the menu's login/password fields (`menu::MAX_FIELD_LEN`);
/// display names are capped server-side at `MAX_NAME_LEN` (24) but a little
/// slack here just avoids the input silently refusing to grow before the
/// server's own validation ever runs.
const MAX_NAME_INPUT: usize = 32;
/// How long a chat/system bubble toast stays on screen before it's dropped.
const TOAST_LIFETIME: f32 = 7.0;
const TOAST_FADE: f32 = 1.5;

const FIELD_BG: Color = Color::srgba(0.020, 0.040, 0.080, 0.92);
const FIELD_FOCUS_BG: Color = Color::srgb(0.100, 0.160, 0.260);
const ONLINE_COL: Color = Color::srgb(0.550, 0.900, 0.500);
const INVITE_COL: Color = Color::srgb(0.950, 0.800, 0.350);
const SYSTEM_TOAST_COL: Color = Color::srgb(0.700, 0.800, 0.950);
/// "Visit" action button — a distinct ice-blue the shared palette has no
/// button-background equivalent for (`ACCENT_ICE` is a text/border accent).
const VISIT_BG: Color = Color::srgb(0.160, 0.340, 0.440);
/// "Go home" button — same ice-blue family as `VISIT_BG`, slightly darker.
const GO_HOME_BG: Color = Color::srgb(0.130, 0.300, 0.400);

/// Whether the social modal is open (also gates world/camera input, same
/// idiom as `research::ResearchOpen`).
#[derive(Resource, Default)]
pub struct SocialOpen(pub bool);

#[derive(Component)]
struct SocialRoot;

#[derive(Component)]
struct FriendRow(usize);

#[derive(Component)]
struct FriendName(usize);

#[derive(Component)]
struct VisitBtn {
    row: usize,
    account: Option<i64>,
}

#[derive(Component)]
struct InviteBtn {
    row: usize,
    account: Option<i64>,
}

#[derive(Component)]
struct RemoveBtn {
    row: usize,
    account: Option<i64>,
}

/// Text typed into the "Add friend" field, and whether it has keyboard focus.
/// Kept separate from `menu::LoginForm` — different screen, different shape.
#[derive(Resource, Default)]
struct AddFriendForm {
    text: String,
    focus: bool,
}

#[derive(Component)]
struct AddFriendBox;

#[derive(Component)]
struct AddFriendText;

#[derive(Component)]
struct AddFriendSubmitBtn;

#[derive(Component)]
struct RefreshBtn;

/// Incoming-invite toast (separate from the panel — visible even with the
/// panel closed, like a normal notification).
#[derive(Component)]
struct InviteToastRoot;

#[derive(Component)]
struct InviteToastText;

#[derive(Component)]
struct InviteAcceptBtn;

/// HUD button that toggles the panel — mobile has no `F` key.
#[derive(Component)]
struct SocialHudBtn;

/// "Visiting <name>'s city" indicator + return-home button, shown only while
/// `Session.visiting.is_some()`. Distinct from `ui::WorldSwitchBtn` (which
/// only flips between the personal world and the central world) since
/// visiting is a third state layered on top of either.
#[derive(Component)]
struct VisitingRoot;

#[derive(Component)]
struct VisitingText;

#[derive(Component)]
struct GoHomeBtn;

/// "Offline guests" policy row (V0.6 owner-offline entry): shown only for
/// account sessions (`SocialState::visit_policy` is `Some` once the server
/// reported it), toggles `ClientMsg::SetVisitPolicy`.
#[derive(Component)]
struct PolicyRow;

#[derive(Component)]
struct PolicyBtn;

#[derive(Component)]
struct PolicyText;

/// One transient toast line spawned from a `BubbleEvent` (chat bubble or
/// player_id==0 system feedback), independent of the drained inbox itself.
#[derive(Component)]
struct Toast {
    age: f32,
}

#[derive(Component)]
struct ToastRoot;

pub fn plugin(app: &mut App) {
    app.init_resource::<SocialOpen>()
        .init_resource::<AddFriendForm>()
        .add_systems(OnEnter(Screen::Game), (spawn_social_ui, spawn_hud_button))
        .add_systems(OnExit(Screen::Game), reset_add_friend_form)
        .add_systems(
            Update,
            (
                toggle_social,
                refresh_on_open,
                update_social_panel,
                friend_buttons,
                add_friend_focus,
                add_friend_keyboard,
                add_friend_submit,
                refresh_button,
                update_add_friend_field,
                update_invite_toast,
                invite_accept_button,
                update_visiting_indicator,
                go_home_button,
                social_hud_button,
                drain_bubbles_to_toasts,
                animate_toasts,
                update_policy_row,
                policy_button,
                update_static_labels,
            )
                .run_if(in_state(Screen::Game)),
        );
}

/// `SocialOpen` itself is reset centrally in `mod.rs::teardown_game`
/// (alongside research/roster); this only clears the add-friend text box,
/// which is private to this module.
fn reset_add_friend_form(mut form: ResMut<AddFriendForm>) {
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
fn spawn_social_ui(mut commands: Commands, ff: Res<FormFactor>) {
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

#[derive(Component)]
struct VisitLabel;

#[derive(Component)]
struct InviteLabel;

/// The Friends HUD button (mobile has no `F` key). Spawned separately from
/// `ui::spawn_hud` (a different plugin/file) so it survives independently of
/// that module's layout. The top bar's own right-hand buttons (Furnace
/// level, the World-Switch toggle when visible, Menu) are laid out by
/// flexbox with a variable on-screen position (window width, and whether
/// World-Switch is even shown), and the right column below it is already
/// crowded (event feed at `top:54`, then the missions panel at `top:232`) —
/// so this sits on the LEFT instead, in the free strip between the top bar
/// and the minimap (FPS text already lives at `left:14, top:54`; this is
/// offset further right on the same row, clear of it).
fn spawn_hud_button(mut commands: Commands) {
    commands
        .spawn((
            Button,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(210.0),
                top: Val::Px(52.0),
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

fn toggle_social(
    keys: Res<ButtonInput<KeyCode>>,
    chat: Res<ChatState>,
    mut open: ResMut<SocialOpen>,
) {
    if !chat.active && keys.just_pressed(KeyCode::KeyF) {
        open.0 = !open.0;
    }
    if open.0 && keys.just_pressed(KeyCode::Escape) {
        open.0 = false;
    }
}

fn social_hud_button(
    q: Query<&Interaction, (With<SocialHudBtn>, Changed<Interaction>)>,
    mut open: ResMut<SocialOpen>,
) {
    for interaction in &q {
        if *interaction == Interaction::Pressed {
            open.0 = !open.0;
        }
    }
}

/// Ask the server for a fresh friends list the moment the panel opens (the
/// task brief: "also send RefreshSocial once when the panel opens").
fn refresh_on_open(net: Res<NetConn>, open: Res<SocialOpen>, mut was_open: Local<bool>) {
    if open.0 && !*was_open {
        net.send(ClientMsg::RefreshSocial);
        net.send(ClientMsg::RefreshShowcase);
    }
    *was_open = open.0;
}

fn refresh_button(
    net: Res<NetConn>,
    clicked: Query<&Interaction, (Changed<Interaction>, With<RefreshBtn>)>,
) {
    if clicked.iter().any(|i| *i == Interaction::Pressed) {
        net.send(ClientMsg::RefreshSocial);
        net.send(ClientMsg::RefreshShowcase);
    }
}

/// Every one of these labels is `&mut Text` on its own dedicated marker
/// entity — but Bevy's query-conflict check only recognizes two
/// `Query<&mut Text, With<A>>` / `Query<&mut Text, With<B>>` parameters as
/// disjoint when at least one side *also* excludes the other's marker
/// (`With<A>` alone vs `With<B>` alone is NOT sufficient — see
/// `bevy_ecs::query::access::FilteredAccess::is_ruled_out_by`, which only
/// rules two queries compatible via an explicit `with`/`without` overlap).
/// Rather than writing eleven mutually-excluding marker structs, all eleven
/// live on one `StaticLabel` enum component instead, read through a single
/// `Query<(&StaticLabel, &mut Text)>` — one `&mut Text` access, no conflict
/// possible.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum StaticLabel {
    Title,
    SectionInvite,
    SectionFriends,
    SectionShowcase,
    SectionPolicy,
    AddFriendSubmit,
    Refresh,
    Hud,
    GoHome,
    InviteAccept,
    Policy,
}

/// Refreshes every static (non-per-row) label whenever the language changes —
/// title, section headers, placeholders, HUD/toast button labels. Split out
/// from the per-row systems (`update_social_panel`, etc.) since those already
/// have `#[allow(clippy::too_many_arguments)]` Query lists at the Bevy cap.
fn update_static_labels(lang: Res<i18n::Lang>, mut labels: Query<(&StaticLabel, &mut Text)>) {
    let lang = *lang;
    for (marker, mut t) in &mut labels {
        let new = match marker {
            StaticLabel::Title => {
                format!("{}   {}", i18n_panels::social_title(lang), i18n_panels::social_hint(lang))
            }
            StaticLabel::SectionInvite => i18n_panels::section_invite(lang).to_string(),
            StaticLabel::SectionFriends => i18n_panels::section_friends(lang).to_string(),
            StaticLabel::SectionShowcase => i18n_panels::section_showcase(lang).to_string(),
            StaticLabel::SectionPolicy => i18n_panels::section_offline_policy(lang).to_string(),
            StaticLabel::AddFriendSubmit => i18n_panels::btn_add(lang).to_string(),
            StaticLabel::Refresh => i18n_panels::btn_refresh(lang).to_string(),
            StaticLabel::Hud => i18n_panels::friends_hud_button(lang).to_string(),
            StaticLabel::GoHome => i18n_panels::btn_my_city(lang).to_string(),
            StaticLabel::InviteAccept => i18n_panels::btn_accept_visit(lang).to_string(),
            StaticLabel::Policy => i18n_panels::policy_row_label(lang).to_string(),
        };
        if t.0 != new {
            t.0 = new;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_social_panel(
    open: Res<SocialOpen>,
    view: Res<GameView>,
    social: Res<SocialState>,
    lang: Res<i18n::Lang>,
    mut root: Query<&mut Node, With<SocialRoot>>,
    mut rows: Query<(&FriendRow, &mut Node), Without<SocialRoot>>,
    mut names: Query<(&FriendName, &mut Text)>,
    #[allow(clippy::type_complexity)]
    mut visit_btns: Query<
        (&mut VisitBtn, &mut Node),
        (Without<SocialRoot>, Without<FriendRow>, Without<InviteBtn>, Without<RemoveBtn>),
    >,
    #[allow(clippy::type_complexity)]
    mut invite_btns: Query<
        (&mut InviteBtn, &mut Node),
        (Without<SocialRoot>, Without<FriendRow>, Without<VisitBtn>, Without<RemoveBtn>),
    >,
    mut remove_btns: Query<
        &mut RemoveBtn,
        (Without<SocialRoot>, Without<FriendRow>, Without<VisitBtn>, Without<InviteBtn>),
    >,
    mut visit_labels: Query<&mut Text, (With<VisitLabel>, Without<FriendName>)>,
    mut invite_labels: Query<&mut Text, (With<InviteLabel>, Without<FriendName>, Without<VisitLabel>)>,
) {
    let lang = *lang;
    let display = if open.0 { Display::Flex } else { Display::None };
    for mut node in &mut root {
        if node.display != display {
            node.display = display;
        }
    }
    for mut t in &mut visit_labels {
        if t.0 != i18n_panels::btn_visit(lang) {
            t.0 = i18n_panels::btn_visit(lang).to_string();
        }
    }
    for mut t in &mut invite_labels {
        if t.0 != i18n_panels::btn_invite(lang) {
            t.0 = i18n_panels::btn_invite(lang).to_string();
        }
    }
    if !open.0 {
        return;
    }

    let central = view.state.as_ref().map(|s| s.central).unwrap_or(false);
    // A friend is visitable once a standing invite names them as the host —
    // `SocialState::invite` only ever holds the newest one, matching the
    // server's "newest unanswered invite" contract.
    let invited_host = social.invite.as_ref().map(|(host, _)| *host);

    for (row, mut node) in &mut rows {
        let shown = row.0 < social.friends.len().min(FRIEND_ROWS);
        let d = if shown { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
        }
    }
    for (name, mut t) in &mut names {
        let new = social
            .friends
            .get(name.0)
            .map(|f| {
                let mut s = if f.online_central {
                    i18n_panels::friend_online_suffix(&f.name, lang)
                } else {
                    f.name.clone()
                };
                // Showcase stats ride along once the server has them for this
                // friend: day / population / buildings, plus a Tunnel marker
                // (see `ClientMsg::RefreshShowcase`). ASCII only — the default
                // font has no fancy glyphs.
                if let Some(e) = social.showcase.iter().find(|e| e.account == f.account) {
                    use std::fmt::Write as _;
                    let _ = write!(
                        s,
                        "  d{} p{} b{}{}",
                        e.days_survived,
                        e.population,
                        e.buildings,
                        if e.graduated { " [T]" } else { "" }
                    );
                }
                s
            })
            .unwrap_or_default();
        if t.0 != new {
            t.0 = new;
        }
    }
    for (mut btn, mut node) in &mut visit_btns {
        let friend = social.friends.get(btn.row);
        btn.account = friend.map(|f| f.account);
        let can_visit = friend.is_some_and(|f| invited_host == Some(f.account));
        let d = if can_visit { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
        }
    }
    for (mut btn, mut node) in &mut invite_btns {
        let friend = social.friends.get(btn.row);
        btn.account = friend.map(|f| f.account);
        // Inviting only makes sense from the central world (you can only
        // invite someone INTO your personal world while you're both present
        // in the hub) and only for a friend currently online there.
        let can_invite = central && friend.is_some_and(|f| f.online_central);
        let d = if can_invite { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
        }
    }
    for mut btn in &mut remove_btns {
        btn.account = social.friends.get(btn.row).map(|f| f.account);
    }
}

fn friend_buttons(
    net: Res<NetConn>,
    social: Res<SocialState>,
    lang: Res<i18n::Lang>,
    visit: Query<(&Interaction, &VisitBtn), Changed<Interaction>>,
    invite: Query<(&Interaction, &InviteBtn), Changed<Interaction>>,
    remove: Query<(&Interaction, &RemoveBtn), Changed<Interaction>>,
    mut pending: ResMut<PendingSwitch>,
    mut transition: ResMut<super::TransitionMsg>,
    mut next: ResMut<NextState<Screen>>,
    mut open: ResMut<SocialOpen>,
) {
    for (interaction, btn) in &visit {
        if *interaction == Interaction::Pressed {
            if let Some(account) = btn.account {
                // Same routing as the Tunnel/central-world switch buttons in
                // `ui.rs`: park the target in `PendingSwitch`, trip through
                // `Screen::Menu` for one frame so the whole scene rebuilds,
                // then `menu::pending_switch` dials `VisitFriend`.
                let target = WorldTarget::Visit(account);
                let name = social.friends.iter().find(|f| f.account == account).map(|f| f.name.as_str());
                pending.0 = Some(target);
                transition.text = Some(target.transition_label(name, *lang));
                transition.age = 0.0;
                open.0 = false;
                next.set(Screen::Menu);
                return;
            }
        }
    }
    for (interaction, btn) in &invite {
        if *interaction == Interaction::Pressed {
            if let Some(account) = btn.account {
                net.send(ClientMsg::Invite { account });
            }
        }
    }
    for (interaction, btn) in &remove {
        if *interaction == Interaction::Pressed {
            if let Some(account) = btn.account {
                net.send(ClientMsg::RemoveFriend { account });
            }
        }
    }
}

fn add_friend_focus(
    q: Query<&Interaction, (With<AddFriendBox>, Changed<Interaction>)>,
    mut form: ResMut<AddFriendForm>,
) {
    for interaction in &q {
        if *interaction == Interaction::Pressed {
            form.focus = true;
        }
    }
}

fn add_friend_keyboard(
    mut events: MessageReader<bevy::input::keyboard::KeyboardInput>,
    mut form: ResMut<AddFriendForm>,
    net: Res<NetConn>,
) {
    use bevy::input::keyboard::Key;
    use bevy::input::ButtonState;

    if !form.focus {
        return;
    }
    for ev in events.read() {
        if ev.state != ButtonState::Pressed {
            continue;
        }
        match &ev.logical_key {
            Key::Enter if !ev.repeat => {
                submit_add_friend(&mut form, &net);
            }
            Key::Escape if !ev.repeat => {
                form.focus = false;
            }
            Key::Backspace => {
                form.text.pop();
            }
            Key::Character(s) if form.text.chars().count() < MAX_NAME_INPUT => {
                for c in s.chars() {
                    if !c.is_control() {
                        form.text.push(c);
                    }
                }
            }
            _ => {}
        }
    }
}

fn add_friend_submit(
    net: Res<NetConn>,
    mut form: ResMut<AddFriendForm>,
    clicked: Query<&Interaction, (Changed<Interaction>, With<AddFriendSubmitBtn>)>,
) {
    if clicked.iter().any(|i| *i == Interaction::Pressed) {
        submit_add_friend(&mut form, &net);
    }
}

fn submit_add_friend(form: &mut AddFriendForm, net: &NetConn) {
    let name = form.text.trim();
    if name.is_empty() {
        return;
    }
    net.send(ClientMsg::AddFriend { name: name.to_string() });
    form.text.clear();
}

fn update_add_friend_field(
    form: Res<AddFriendForm>,
    lang: Res<i18n::Lang>,
    mut boxes: Query<&mut BackgroundColor, With<AddFriendBox>>,
    mut texts: Query<(&mut Text, &mut TextColor), With<AddFriendText>>,
) {
    if let Ok(mut bg) = boxes.single_mut() {
        let target = if form.focus { FIELD_FOCUS_BG } else { FIELD_BG };
        if bg.0 != target {
            bg.0 = target;
        }
    }
    if let Ok((mut t, mut c)) = texts.single_mut() {
        let (new, col) = if form.text.is_empty() && !form.focus {
            (i18n_panels::add_friend_placeholder(*lang).to_string(), theme::TEXT_MUTED)
        } else {
            let cursor = if form.focus { "_" } else { "" };
            (format!("{}{cursor}", form.text), theme::TEXT_PRIMARY)
        };
        if t.0 != new {
            t.0 = new;
        }
        if c.0 != col {
            c.0 = col;
        }
    }
}

fn update_invite_toast(
    social: Res<SocialState>,
    lang: Res<i18n::Lang>,
    mut root: Query<&mut Node, With<InviteToastRoot>>,
    mut text: Query<&mut Text, With<InviteToastText>>,
) {
    let display = if social.invite.is_some() { Display::Flex } else { Display::None };
    for mut node in &mut root {
        if node.display != display {
            node.display = display;
        }
    }
    let Some((_, host_name)) = &social.invite else { return };
    let new = i18n_panels::invite_toast(host_name, *lang);
    if let Ok(mut t) = text.single_mut() {
        if t.0 != new {
            t.0 = new;
        }
    }
}

fn invite_accept_button(
    clicked: Query<&Interaction, (Changed<Interaction>, With<InviteAcceptBtn>)>,
    social: Res<SocialState>,
    lang: Res<i18n::Lang>,
    mut pending: ResMut<PendingSwitch>,
    mut transition: ResMut<super::TransitionMsg>,
    mut next: ResMut<NextState<Screen>>,
    mut open: ResMut<SocialOpen>,
) {
    if clicked.iter().any(|i| *i == Interaction::Pressed) {
        if let Some((host, host_name)) = social.invite.clone() {
            let target = WorldTarget::Visit(host);
            pending.0 = Some(target);
            transition.text = Some(target.transition_label(Some(&host_name), *lang));
            transition.age = 0.0;
            open.0 = false;
            next.set(Screen::Menu);
        }
    }
}

fn update_visiting_indicator(
    session: Res<Session>,
    social: Res<SocialState>,
    lang: Res<i18n::Lang>,
    mut root: Query<&mut Node, With<VisitingRoot>>,
    mut text: Query<&mut Text, With<VisitingText>>,
) {
    let display = if session.visiting.is_some() { Display::Flex } else { Display::None };
    for mut node in &mut root {
        if node.display != display {
            node.display = display;
        }
    }
    let Some(host) = session.visiting else { return };
    let name = social
        .friends
        .iter()
        .find(|f| f.account == host)
        .map(|f| f.name.clone())
        .unwrap_or_else(|| i18n_panels::a_friend(*lang).to_string());
    let new = i18n_panels::visiting_indicator(&name, *lang);
    if let Ok(mut t) = text.single_mut() {
        if t.0 != new {
            t.0 = new;
        }
    }
}

fn go_home_button(
    clicked: Query<&Interaction, (Changed<Interaction>, With<GoHomeBtn>)>,
    lang: Res<i18n::Lang>,
    mut pending: ResMut<PendingSwitch>,
    mut transition: ResMut<super::TransitionMsg>,
    mut next: ResMut<NextState<Screen>>,
) {
    if clicked.iter().any(|i| *i == Interaction::Pressed) {
        pending.0 = Some(WorldTarget::Personal);
        transition.text = Some(WorldTarget::Personal.transition_label(None, *lang));
        transition.age = 0.0;
        next.set(Screen::Menu);
    }
}

/// Drains `SocialState.bubbles` (chat bubbles + player_id==0 system
/// feedback, e.g. a failed `AddFriend`) — the single point that empties this
/// inbox, since `Vec::drain` can only be consumed once per arrival. Each
/// bubble becomes BOTH a flat UI toast line here (shown regardless of world
/// position, including the player's own and system lines) AND a
/// world-space floating bubble above the sender's avatar/cursor, spawned via
/// `chat::spawn_bubble` (`chat.rs` owns that rendering/fade logic since it's
/// conceptually a chat feature; this module only owns the toast feed).
/// Bubble/system text itself is server-authored (or the player's own typed
/// chat) and is never translated here.
fn drain_bubbles_to_toasts(
    mut commands: Commands,
    mut social: ResMut<SocialState>,
    root: Query<Entity, With<ToastRoot>>,
) {
    if social.bubbles.is_empty() {
        return;
    }
    let Ok(root) = root.single() else { return };
    for b in social.bubbles.drain(..) {
        let (line, color) = if b.player_id == 0 {
            (b.text.clone(), SYSTEM_TOAST_COL)
        } else {
            (format!("{}: {}", b.name, b.text), super::player_color(b.color))
        };
        commands.entity(root).with_children(|p| {
            p.spawn((theme::text(line, theme::FS_SMALL, color), Toast { age: 0.0 }));
        });
        // Only real chat (not player_id==0 system feedback) gets a
        // world-space bubble — a system line like "friend not found" has no
        // sender position to float above.
        if b.player_id != 0 {
            super::chat::spawn_bubble(&mut commands, b.player_id, b.name.clone(), b.color, b.text);
        }
    }
}

fn animate_toasts(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Toast, &mut TextColor)>,
) {
    for (e, mut toast, mut color) in &mut q {
        toast.age += time.delta_secs();
        if toast.age > TOAST_LIFETIME {
            commands.entity(e).despawn();
            continue;
        }
        let remaining = TOAST_LIFETIME - toast.age;
        if remaining < TOAST_FADE {
            let a = (remaining / TOAST_FADE).clamp(0.0, 1.0);
            color.0.set_alpha(a);
        }
    }
}

/// Show the offline-guest policy row only once the server has reported this
/// account's setting (guest sessions never get a `ServerMsg::VisitPolicy`),
/// and keep the ON/OFF label in sync with it.
fn update_policy_row(
    social: Res<SocialState>,
    lang: Res<i18n::Lang>,
    mut row: Query<&mut Node, With<PolicyRow>>,
    mut label: Query<&mut Text, With<PolicyText>>,
) {
    let d = if social.visit_policy.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut row {
        if node.display != d {
            node.display = d;
        }
    }
    let want = match social.visit_policy {
        Some(true) => i18n_panels::policy_on(*lang),
        _ => i18n_panels::policy_off(*lang),
    };
    for mut t in &mut label {
        if t.0 != want {
            t.0 = want.to_string();
        }
    }
}

/// The label flips only when the server echoes `ServerMsg::VisitPolicy` back,
/// so a click that never reaches the server can't show a lying toggle.
fn policy_button(
    net: Res<NetConn>,
    social: Res<SocialState>,
    clicked: Query<&Interaction, (Changed<Interaction>, With<PolicyBtn>)>,
) {
    if clicked.iter().any(|i| *i == Interaction::Pressed) {
        if let Some(cur) = social.visit_policy {
            net.send(ClientMsg::SetVisitPolicy { allow_offline: !cur });
        }
    }
}
