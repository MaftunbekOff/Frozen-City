//! Co-op text chat: a rolling log of authored lines plus a single-line input
//! box opened with Enter. The authoritative chat log lives in the snapshot
//! (`GameState.chat`); this module only renders it and sends `ClientMsg::Chat`.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;

use frozen_city::game::types::MAX_CHAT_LEN;
use frozen_city::net::protocol::ClientMsg;

use super::i18n::Lang;
use super::i18n_menu as mtxt;
use super::render::{AvatarViz, AvatarWalk, CursorMarker, CursorViz};
use super::theme;
use super::{player_color, GameView, NetConn, Screen};

/// How many recent chat lines the on-screen log shows.
const CHAT_LINES: usize = 7;
const SYSTEM_COLOR: Color = theme::TEXT_MUTED;
/// How long a floating nearby-chat bubble stays over its sender before
/// despawning, and how much of that tail is spent fading out.
const BUBBLE_LIFETIME: f32 = 7.0;
const BUBBLE_FADE: f32 = 1.5;

/// Whether the chat input box is capturing keystrokes, and its current text.
/// While `active`, world/camera input is suppressed (see `input.rs`).
#[derive(Resource, Default)]
pub struct ChatState {
    pub active: bool,
    pub buffer: String,
}

/// One fixed slot in the chat log (index 0 = top / oldest shown).
#[derive(Component)]
struct ChatLogLine(usize);

#[derive(Component)]
struct ChatInputRoot;

#[derive(Component)]
struct ChatInputText;

#[derive(Component)]
struct ChatHintText;

/// A floating nearby-chat/system bubble rendered above its sender's
/// avatar/cursor in the 3D world (screen-projected UI text, same technique
/// `render.rs` uses for cursor/avatar name tags). `player_id` re-locates the
/// sender's current world position every frame so the bubble tracks their
/// walk instead of staying pinned to where they were when it was sent.
#[derive(Component)]
pub(crate) struct ChatBubble {
    player_id: u64,
    age: f32,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<ChatState>()
        .add_systems(OnEnter(Screen::Game), spawn_chat_ui)
        .add_systems(
            Update,
            (chat_keyboard, update_chat_log, update_chat_input, update_bubbles)
                .run_if(in_state(Screen::Game)),
        );
}

fn spawn_chat_ui(mut commands: Commands, lang: Res<Lang>) {
    // Rolling log, stacked upward from just above the input box.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                bottom: Val::Px(150.0),
                width: Val::Px(420.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                ..default()
            },
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            for i in 0..CHAT_LINES {
                p.spawn((
                    Text::new(""),
                    TextFont::from_font_size(13.5),
                    TextColor(SYSTEM_COLOR),
                    ChatLogLine(i),
                ));
            }
        });

    // Hint line just above the input box: reminds players of the "/l "
    // nearby-chat prefix. Shown/hidden alongside the input box itself.
    commands.spawn((
        Node {
            display: Display::None,
            position_type: PositionType::Absolute,
            left: Val::Px(12.0),
            bottom: Val::Px(148.0),
            ..default()
        },
        theme::text(mtxt::chat_hint(*lang), theme::FS_MICRO, theme::TEXT_MUTED),
        ChatHintText,
        DespawnOnExit(Screen::Game),
    ));

    // Single-line input box, hidden until the player presses Enter.
    commands
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                bottom: Val::Px(120.0),
                width: Val::Px(420.0),
                padding: UiRect::axes(Val::Px(theme::SP_SM), Val::Px(theme::SP_XS)),
                border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                ..default()
            },
            BackgroundColor(theme::BG_SECTION),
            ChatInputRoot,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn(theme::text("", theme::FS_BODY, theme::TEXT_PRIMARY))
                .insert(ChatInputText);
        });
}

/// Capture keystrokes into the chat buffer. Enter opens the box (when closed)
/// and sends (when open); Escape cancels; Backspace edits.
fn chat_keyboard(
    mut events: MessageReader<KeyboardInput>,
    mut chat: ResMut<ChatState>,
    net: Res<NetConn>,
) {
    for ev in events.read() {
        if ev.state != ButtonState::Pressed {
            continue;
        }
        if !chat.active {
            // Only a real (non-repeat) Enter opens the box; every other key
            // falls through to the world-input systems this frame. Ignoring
            // OS key-repeat stops a held Enter from flip-flopping the box.
            if !ev.repeat && (ev.key_code == KeyCode::Enter || ev.key_code == KeyCode::NumpadEnter) {
                chat.active = true;
            }
            continue;
        }
        match &ev.logical_key {
            // Enter/Escape act only on the true press, never on key-repeat, so a
            // held key can't send-then-reopen or instantly close the box.
            Key::Enter if !ev.repeat => {
                let raw = chat.buffer.trim();
                // "/l " sends nearby chat (`ChatLocal`, a transient
                // `Bubble` to players in earshot) instead of the persistent
                // world-wide log — same message box, just a different wire
                // message depending on the prefix.
                if let Some(rest) = raw.strip_prefix("/l ").or_else(|| raw.strip_prefix("/l")) {
                    let text = rest.trim().to_string();
                    if !text.is_empty() {
                        net.send(ClientMsg::ChatLocal { text });
                    }
                } else if !raw.is_empty() {
                    net.send(ClientMsg::Chat { text: raw.to_string() });
                }
                chat.buffer.clear();
                chat.active = false;
            }
            Key::Escape if !ev.repeat => {
                chat.buffer.clear();
                chat.active = false;
            }
            Key::Backspace => {
                chat.buffer.pop();
            }
            Key::Space => {
                if chat.buffer.chars().count() < MAX_CHAT_LEN {
                    chat.buffer.push(' ');
                }
            }
            Key::Character(s) => {
                for c in s.chars() {
                    if !c.is_control() && chat.buffer.chars().count() < MAX_CHAT_LEN {
                        chat.buffer.push(c);
                    }
                }
            }
            _ => {}
        }
    }
}

fn update_chat_log(view: Res<GameView>, mut lines: Query<(&mut Text, &mut TextColor, &ChatLogLine)>) {
    let Some(state) = view.state.as_ref() else { return };
    let chat = &state.chat;
    let start = chat.len().saturating_sub(CHAT_LINES);
    let shown = &chat[start..];
    for (mut text, mut color, slot) in &mut lines {
        // Slot 0 is the top line; fill from the top so the newest sits at the
        // bottom of the block.
        let new = shown.get(slot.0);
        let (s, c) = match new {
            Some(line) if line.player_id == 0 => (line.text.clone(), SYSTEM_COLOR),
            Some(line) => (format!("{}: {}", line.name, line.text), player_color(line.color)),
            None => (String::new(), SYSTEM_COLOR),
        };
        if text.0 != s {
            text.0 = s;
        }
        if color.0 != c {
            color.0 = c;
        }
    }
}

fn update_chat_input(
    chat: Res<ChatState>,
    mut root: Query<&mut Node, (With<ChatInputRoot>, Without<ChatHintText>)>,
    mut hint: Query<&mut Node, (With<ChatHintText>, Without<ChatInputRoot>)>,
    mut text: Query<&mut Text, With<ChatInputText>>,
) {
    let display = if chat.active { Display::Flex } else { Display::None };
    for mut node in &mut root {
        if node.display != display {
            node.display = display;
        }
    }
    for mut node in &mut hint {
        if node.display != display {
            node.display = display;
        }
    }
    if !chat.active {
        return;
    }
    if let Ok(mut t) = text.single_mut() {
        let new = format!("> {}_", chat.buffer);
        if t.0 != new {
            t.0 = new;
        }
    }
}

/// Spawns one floating world-space bubble for `player_id`'s message,
/// screen-projected above their current avatar/cursor position each frame by
/// `update_bubbles`. Called from `social.rs::drain_bubbles_to_toasts` (the
/// single drain point for `SocialState.bubbles`, see its doc comment) rather
/// than being a system itself, since the inbox must only be drained once.
pub(crate) fn spawn_bubble(commands: &mut Commands, player_id: u64, name: String, color: u8, text: String) {
    commands.spawn((
        Text::new(format!("{name}: {text}")),
        TextFont::from_font_size(13.0),
        TextColor(player_color(color)),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(-1000.0),
            top: Val::Px(-1000.0),
            ..default()
        },
        ChatBubble { player_id, age: 0.0 },
        DespawnOnExit(Screen::Game),
    ));
}

/// Positions every live bubble above its sender's current world anchor
/// (`render::sender_anchor` — the avatar in the central world, the remote
/// cursor marker elsewhere, or a direct snapshot lookup as a fallback that
/// also covers the local player, who never gets a marker of their own) each
/// frame, fades it out over the last `BUBBLE_FADE` seconds of its life, and
/// despawns it once `BUBBLE_LIFETIME` elapses.
pub fn update_bubbles(
    time: Res<Time>,
    view: Res<GameView>,
    avatars: Res<AvatarViz>,
    cursors: Res<CursorViz>,
    mut commands: Commands,
    avatar_q: Query<&Transform, With<AvatarWalk>>,
    cursor_q: Query<&Transform, With<CursorMarker>>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut bubbles: Query<(Entity, &mut ChatBubble, &mut Node, &mut TextColor, &mut Visibility)>,
) {
    let Some(state) = view.state.as_ref() else { return };
    let Ok((cam, cam_gt)) = camera.single() else { return };
    for (entity, mut bubble, mut node, mut color, mut vis) in &mut bubbles {
        bubble.age += time.delta_secs();
        if bubble.age > BUBBLE_LIFETIME {
            commands.entity(entity).despawn();
            continue;
        }
        let remaining = BUBBLE_LIFETIME - bubble.age;
        if remaining < BUBBLE_FADE {
            color.0.set_alpha((remaining / BUBBLE_FADE).clamp(0.0, 1.0));
        }
        let anchor = super::render::sender_anchor(
            bubble.player_id,
            state,
            &avatars,
            &cursors,
            &avatar_q,
            &cursor_q,
        );
        match anchor.and_then(|p| cam.world_to_viewport(cam_gt, p + Vec3::Y * 1.1).ok()) {
            Some(p) => {
                *vis = Visibility::Visible;
                node.left = Val::Px(p.x - 60.0);
                node.top = Val::Px(p.y - 46.0);
            }
            None => *vis = Visibility::Hidden,
        }
    }
}
