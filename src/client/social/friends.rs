use bevy::prelude::*;

use frozen_city::net::protocol::ClientMsg;

use crate::client::chat::ChatState;
use crate::client::i18n_panels;
use crate::client::theme;
use crate::client::{i18n, NetConn, PendingSwitch, Screen, SocialState, WorldTarget};

use super::*;

pub(crate) fn toggle_social(
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

pub(crate) fn social_hud_button(
    q: Query<&Interaction, (With<SocialHudBtn>, Changed<Interaction>)>,
    mut open: ResMut<SocialOpen>,
) {
    for interaction in &q {
        if *interaction == Interaction::Pressed {
            open.0 = !open.0;
        }
    }
}

pub(crate) fn friend_buttons(
    net: Res<NetConn>,
    social: Res<SocialState>,
    lang: Res<i18n::Lang>,
    visit: Query<(&Interaction, &VisitBtn), Changed<Interaction>>,
    invite: Query<(&Interaction, &InviteBtn), Changed<Interaction>>,
    remove: Query<(&Interaction, &RemoveBtn), Changed<Interaction>>,
    mut pending: ResMut<PendingSwitch>,
    mut transition: ResMut<crate::client::TransitionMsg>,
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

pub(crate) fn add_friend_focus(
    q: Query<&Interaction, (With<AddFriendBox>, Changed<Interaction>)>,
    mut form: ResMut<AddFriendForm>,
) {
    for interaction in &q {
        if *interaction == Interaction::Pressed {
            form.focus = true;
        }
    }
}

pub(crate) fn add_friend_keyboard(
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

pub(crate) fn add_friend_submit(
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

pub(crate) fn update_add_friend_field(
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
