use bevy::prelude::*;

use frozen_city::net::protocol::ClientMsg;

use crate::client::i18n_panels;
use crate::client::theme;
use crate::client::{i18n, NetConn, PendingSwitch, Screen, Session, SocialState, WorldTarget};

use super::*;

pub(crate) fn update_invite_toast(
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

pub(crate) fn invite_accept_button(
    clicked: Query<&Interaction, (Changed<Interaction>, With<InviteAcceptBtn>)>,
    social: Res<SocialState>,
    lang: Res<i18n::Lang>,
    mut pending: ResMut<PendingSwitch>,
    mut transition: ResMut<crate::client::TransitionMsg>,
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

pub(crate) fn update_visiting_indicator(
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

pub(crate) fn go_home_button(
    clicked: Query<&Interaction, (Changed<Interaction>, With<GoHomeBtn>)>,
    lang: Res<i18n::Lang>,
    mut pending: ResMut<PendingSwitch>,
    mut transition: ResMut<crate::client::TransitionMsg>,
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
pub(crate) fn drain_bubbles_to_toasts(
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
            (format!("{}: {}", b.name, b.text), crate::client::player_color(b.color))
        };
        commands.entity(root).with_children(|p| {
            p.spawn((theme::text(line, theme::FS_SMALL, color), Toast { age: 0.0 }));
        });
        // Only real chat (not player_id==0 system feedback) gets a
        // world-space bubble — a system line like "friend not found" has no
        // sender position to float above.
        if b.player_id != 0 {
            crate::client::chat::spawn_bubble(&mut commands, b.player_id, b.name.clone(), b.color, b.text);
        }
    }
}

pub(crate) fn animate_toasts(
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
pub(crate) fn update_policy_row(
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
pub(crate) fn policy_button(
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
