use bevy::prelude::*;

use frozen_city::net::protocol::ClientMsg;

use crate::client::i18n_panels;
use crate::client::{i18n, GameView, NetConn, SocialState};

use super::*;

/// Ask the server for a fresh friends list the moment the panel opens (the
/// task brief: "also send RefreshSocial once when the panel opens").
pub(crate) fn refresh_on_open(net: Res<NetConn>, open: Res<SocialOpen>, mut was_open: Local<bool>) {
    if open.0 && !*was_open {
        net.send(ClientMsg::RefreshSocial);
        net.send(ClientMsg::RefreshShowcase);
    }
    *was_open = open.0;
}

pub(crate) fn refresh_button(
    net: Res<NetConn>,
    clicked: Query<&Interaction, (Changed<Interaction>, With<RefreshBtn>)>,
) {
    if clicked.iter().any(|i| *i == Interaction::Pressed) {
        net.send(ClientMsg::RefreshSocial);
        net.send(ClientMsg::RefreshShowcase);
    }
}

/// Refreshes every static (non-per-row) label whenever the language changes —
/// title, section headers, placeholders, HUD/toast button labels. Split out
/// from the per-row systems (`update_social_panel`, etc.) since those already
/// have `#[allow(clippy::too_many_arguments)]` Query lists at the Bevy cap.
pub(crate) fn update_static_labels(lang: Res<i18n::Lang>, mut labels: Query<(&StaticLabel, &mut Text)>) {
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
pub(crate) fn update_social_panel(
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
