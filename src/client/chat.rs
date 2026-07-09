//! Co-op text chat: a rolling log of authored lines plus a single-line input
//! box opened with Enter. The authoritative chat log lives in the snapshot
//! (`GameState.chat`); this module only renders it and sends `ClientMsg::Chat`.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;

use frozen_city::game::types::MAX_CHAT_LEN;
use frozen_city::net::protocol::ClientMsg;

use super::{player_color, GameView, NetConn, Screen};

/// How many recent chat lines the on-screen log shows.
const CHAT_LINES: usize = 7;
const SYSTEM_COLOR: Color = Color::srgb(0.62, 0.68, 0.78);
const INPUT_BG: Color = Color::srgba(0.02, 0.04, 0.08, 0.92);

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

pub fn plugin(app: &mut App) {
    app.init_resource::<ChatState>()
        .add_systems(OnEnter(Screen::Game), spawn_chat_ui)
        .add_systems(
            Update,
            (chat_keyboard, update_chat_log, update_chat_input)
                .run_if(in_state(Screen::Game)),
        );
}

fn spawn_chat_ui(mut commands: Commands) {
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

    // Single-line input box, hidden until the player presses Enter.
    commands
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                bottom: Val::Px(120.0),
                width: Val::Px(420.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
                ..default()
            },
            BackgroundColor(INPUT_BG),
            ChatInputRoot,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new(""),
                TextFont::from_font_size(14.0),
                TextColor(Color::srgb(0.92, 0.95, 1.0)),
                ChatInputText,
            ));
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
                let text = chat.buffer.trim().to_string();
                if !text.is_empty() {
                    net.send(ClientMsg::Chat { text });
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
    mut root: Query<&mut Node, With<ChatInputRoot>>,
    mut text: Query<&mut Text, With<ChatInputText>>,
) {
    let display = if chat.active { Display::Flex } else { Display::None };
    for mut node in &mut root {
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
