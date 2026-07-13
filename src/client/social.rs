//! Social panel: friends list, add/remove, visit invites and system/nearby
//! chat-bubble toasts. Toggled with `F` (mirroring `P` for roster, `R` for
//! research) or the "Friends" HUD button (mobile has no keyboard). All state
//! is driven by the existing [`SocialState`] resource — server plumbing
//! (`ClientMsg::AddFriend/RemoveFriend/RefreshSocial/VisitFriend/Invite`,
//! `ServerMsg::Social/Invited/Bubble`) already lands there via `net_sync.rs`;
//! this module only renders it and forwards clicks. Chat-bubble/system-toast
//! TEXT comes from the server (or is a player's own typed chat) and is never
//! translated — only this module's own frame/labels are localized.

mod components;
mod friends;
mod panel;
mod spawn;
mod toast;

pub use components::*;

use bevy::prelude::*;

use friends::*;
use panel::*;
use spawn::*;
use toast::*;

use super::Screen;

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
