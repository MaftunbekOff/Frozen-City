//! Social panel: friends list, add/remove, visit invites and system/nearby
//! chat-bubble toasts. Toggled with `F` (mirroring `P` for roster, `R` for
//! research) or the "Friends" HUD button (mobile has no keyboard). All state
//! is driven by the existing [`SocialState`] resource — server plumbing
//! (`ClientMsg::AddFriend/RemoveFriend/RefreshSocial/VisitFriend/Invite`,
//! `ServerMsg::Social/Invited/Bubble`) already lands there via `net_sync.rs`;
//! this module only renders it and forwards clicks.

use bevy::prelude::*;

use frozen_city::net::protocol::ClientMsg;

use super::chat::ChatState;
use super::ui::{BaseColor, UiBlocker};
use super::{GameView, NetConn, PendingSwitch, Screen, Session, SocialState, WorldTarget};

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

const PANEL_BG: Color = Color::srgba(0.05, 0.08, 0.13, 0.98);
const BACKDROP: Color = Color::srgba(0.0, 0.0, 0.0, 0.55);
const BTN_BG: Color = Color::srgb(0.16, 0.20, 0.28);
const VISIT_BG: Color = Color::srgb(0.16, 0.34, 0.44);
const INVITE_BG: Color = Color::srgb(0.20, 0.36, 0.30);
const REMOVE_BG: Color = Color::srgb(0.45, 0.16, 0.14);
const FIELD_BG: Color = Color::srgba(0.02, 0.04, 0.08, 0.92);
const FIELD_FOCUS_BG: Color = Color::srgb(0.10, 0.16, 0.26);
const TEXT_MAIN: Color = Color::srgb(0.90, 0.93, 0.97);
const TEXT_DIM: Color = Color::srgb(0.62, 0.68, 0.78);
const ONLINE_COL: Color = Color::srgb(0.55, 0.90, 0.50);
const INVITE_COL: Color = Color::srgb(0.95, 0.80, 0.35);
const SYSTEM_TOAST_COL: Color = Color::srgb(0.70, 0.80, 0.95);

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

fn text(t: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(t.into()), TextFont::from_font_size(size), TextColor(color))
}

fn small_btn(bg: Color, w: f32) -> impl Bundle {
    (
        Button,
        Node {
            width: Val::Px(w),
            height: Val::Px(24.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(bg),
        BaseColor(bg),
    )
}

#[allow(clippy::too_many_arguments)]
fn spawn_social_ui(mut commands: Commands) {
    // --- Main modal (friends list, add-friend field, refresh) ---
    commands
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(BACKDROP),
            Interaction::default(),
            UiBlocker,
            SocialRoot,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: Val::Px(380.0),
                    max_height: Val::Px(560.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    padding: UiRect::all(Val::Px(16.0)),
                    ..default()
                },
                BackgroundColor(PANEL_BG),
            ))
            .with_children(|panel| {
                panel.spawn(text("Friends   (F or Esc to close)", 18.0, TEXT_MAIN));

                // Add-friend row.
                panel
                    .spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(6.0),
                        align_items: AlignItems::Center,
                        margin: UiRect::bottom(Val::Px(4.0)),
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            Button,
                            Node {
                                flex_grow: 1.0,
                                height: Val::Px(32.0),
                                padding: UiRect::horizontal(Val::Px(6.0)),
                                justify_content: JustifyContent::FlexStart,
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            BackgroundColor(FIELD_BG),
                            AddFriendBox,
                        ))
                        .with_children(|b| {
                            b.spawn((text("", 13.0, TEXT_DIM), AddFriendText));
                        });
                        row.spawn((small_btn(BTN_BG, 60.0), AddFriendSubmitBtn))
                            .with_children(|b| {
                                b.spawn(text("Add", 12.0, TEXT_MAIN));
                            });
                        row.spawn((small_btn(BTN_BG, 60.0), RefreshBtn))
                            .with_children(|b| {
                                b.spawn(text("Refresh", 11.0, TEXT_MAIN));
                            });
                    });

                for i in 0..FRIEND_ROWS {
                    panel
                        .spawn((
                            Node {
                                display: Display::None,
                                align_items: AlignItems::Center,
                                column_gap: Val::Px(5.0),
                                ..default()
                            },
                            FriendRow(i),
                        ))
                        .with_children(|row| {
                            row.spawn((
                                text("", 13.0, TEXT_MAIN),
                                FriendName(i),
                                Node {
                                    flex_grow: 1.0,
                                    ..default()
                                },
                            ));
                            row.spawn((
                                small_btn(VISIT_BG, 46.0),
                                VisitBtn { row: i, account: None },
                            ))
                            .with_children(|b| {
                                b.spawn(text("Visit", 10.5, TEXT_MAIN));
                            });
                            row.spawn((
                                small_btn(INVITE_BG, 50.0),
                                InviteBtn { row: i, account: None },
                            ))
                            .with_children(|b| {
                                b.spawn(text("Invite", 10.5, TEXT_MAIN));
                            });
                            row.spawn((
                                small_btn(REMOVE_BG, 22.0),
                                RemoveBtn { row: i, account: None },
                            ))
                            .with_children(|b| {
                                b.spawn(text("x", 11.0, TEXT_MAIN));
                            });
                        });
                }

                // V0.6 owner-offline policy toggle — hidden until the server
                // reports this account's setting (guest sessions never see it).
                panel
                    .spawn((
                        Node {
                            display: Display::None,
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(6.0),
                            align_items: AlignItems::Center,
                            margin: UiRect::top(Val::Px(8.0)),
                            ..default()
                        },
                        PolicyRow,
                    ))
                    .with_children(|row| {
                        row.spawn((
                            text("Guests may enter while I'm offline:", 11.5, TEXT_DIM),
                            Node {
                                flex_grow: 1.0,
                                ..default()
                            },
                        ));
                        row.spawn((small_btn(BTN_BG, 44.0), PolicyBtn))
                            .with_children(|b| {
                                b.spawn((text("OFF", 11.0, TEXT_MAIN), PolicyText));
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
                    row_gap: Val::Px(8.0),
                    padding: UiRect::all(Val::Px(12.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.10, 0.09, 0.03, 0.96)),
                Interaction::default(),
                UiBlocker,
            ))
            .with_children(|panel| {
                panel.spawn((text("", 13.5, INVITE_COL), InviteToastText));
                panel
                    .spawn((
                        Button,
                        Node {
                            width: Val::Px(200.0),
                            height: Val::Px(30.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(VISIT_BG),
                        BaseColor(VISIT_BG),
                        InviteAcceptBtn,
                    ))
                    .with_children(|b| {
                        b.spawn(text("Accept -> visit", 13.0, TEXT_MAIN));
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
                column_gap: Val::Px(12.0),
                ..default()
            },
            VisitingRoot,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn((text("", 14.0, ONLINE_COL), VisitingText));
            p.spawn((
                Button,
                Node {
                    width: Val::Px(110.0),
                    height: Val::Px(28.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.13, 0.30, 0.40)),
                BaseColor(Color::srgb(0.13, 0.30, 0.40)),
                GoHomeBtn,
            ))
            .with_children(|b| {
                b.spawn(text("My City", 12.5, TEXT_MAIN));
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
                ..default()
            },
            BackgroundColor(BTN_BG),
            BaseColor(BTN_BG),
            SocialHudBtn,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|b| {
            b.spawn(text("Friends [F]", 12.0, TEXT_MAIN));
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

#[allow(clippy::too_many_arguments)]
fn update_social_panel(
    open: Res<SocialOpen>,
    view: Res<GameView>,
    social: Res<SocialState>,
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
) {
    let display = if open.0 { Display::Flex } else { Display::None };
    for mut node in &mut root {
        if node.display != display {
            node.display = display;
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
                    format!("{} (online)", f.name)
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
                transition.text = Some(target.transition_label(name));
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
            ("Add friend by name...".to_string(), TEXT_DIM)
        } else {
            let cursor = if form.focus { "_" } else { "" };
            (format!("{}{cursor}", form.text), TEXT_MAIN)
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
    let new = format!("{host_name} invited you to visit their city!");
    if let Ok(mut t) = text.single_mut() {
        if t.0 != new {
            t.0 = new;
        }
    }
}

fn invite_accept_button(
    clicked: Query<&Interaction, (Changed<Interaction>, With<InviteAcceptBtn>)>,
    social: Res<SocialState>,
    mut pending: ResMut<PendingSwitch>,
    mut transition: ResMut<super::TransitionMsg>,
    mut next: ResMut<NextState<Screen>>,
    mut open: ResMut<SocialOpen>,
) {
    if clicked.iter().any(|i| *i == Interaction::Pressed) {
        if let Some((host, host_name)) = social.invite.clone() {
            let target = WorldTarget::Visit(host);
            pending.0 = Some(target);
            transition.text = Some(target.transition_label(Some(&host_name)));
            transition.age = 0.0;
            open.0 = false;
            next.set(Screen::Menu);
        }
    }
}

fn update_visiting_indicator(
    session: Res<Session>,
    social: Res<SocialState>,
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
        .unwrap_or_else(|| "a friend".to_string());
    let new = format!("Visiting {name}'s city");
    if let Ok(mut t) = text.single_mut() {
        if t.0 != new {
            t.0 = new;
        }
    }
}

fn go_home_button(
    clicked: Query<&Interaction, (Changed<Interaction>, With<GoHomeBtn>)>,
    mut pending: ResMut<PendingSwitch>,
    mut transition: ResMut<super::TransitionMsg>,
    mut next: ResMut<NextState<Screen>>,
) {
    if clicked.iter().any(|i| *i == Interaction::Pressed) {
        pending.0 = Some(WorldTarget::Personal);
        transition.text = Some(WorldTarget::Personal.transition_label(None));
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
            p.spawn((text(line, 13.0, color), Toast { age: 0.0 }));
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
        Some(true) => "ON",
        _ => "OFF",
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
