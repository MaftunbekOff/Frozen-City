use bevy::prelude::*;

use frozen_city::game::types::GamePhase;

use super::super::i18n_hud;
use super::super::i18n_v018;
use super::super::theme::TEXT_MUTED;
use super::super::*;
use super::*;

pub fn game_over_ui(
    view: Res<GameView>,
    session: Res<Session>,
    lang: Res<super::super::i18n::Lang>,
    mut pending: ResMut<PendingSwitch>,
    mut transition: ResMut<TransitionMsg>,
    mut next: ResMut<NextState<Screen>>,
    mut root: Query<&mut Node, With<GameOverRoot>>,
    mut texts: Query<(&mut Text, &mut TextColor, &GoText)>,
    back: Query<&Interaction, (Changed<Interaction>, With<GameOverBack>)>,
    quit: Query<&Interaction, (Changed<Interaction>, With<QuitToMenuBtn>)>,
    enter_central: Query<&Interaction, (Changed<Interaction>, With<EnterCentralBtn>)>,
    mut central_btn: Query<&mut Node, (With<EnterCentralBtn>, Without<GameOverRoot>)>,
) {
    if back.iter().any(|i| *i == Interaction::Pressed)
        || quit.iter().any(|i| *i == Interaction::Pressed)
    {
        next.set(Screen::Menu);
        return;
    }
    if enter_central.iter().any(|i| *i == Interaction::Pressed) {
        // The dial happens in `menu::pending_switch` after the game scene
        // tears down — see `PendingSwitch`.
        pending.0 = Some(WorldTarget::Central);
        transition.text = Some(WorldTarget::Central.transition_label(None, *lang));
        transition.age = 0.0;
        next.set(Screen::Menu);
        return;
    }

    let Some(state) = view.state.as_ref() else { return };
    let display = if state.phase == GamePhase::Running {
        Display::None
    } else {
        Display::Flex
    };
    for mut node in &mut root {
        if node.display != display {
            node.display = display;
        }
    }
    if state.phase == GamePhase::Running {
        return;
    }

    // The Tunnel exit is only offered where it can work: a graduated win, an
    // account signed in to own the settlers, and not already in the central
    // world (its overlay never shows anyway — no win/lose there).
    let offer_central = state.graduated && session.auth.is_some() && !state.central;
    for mut node in &mut central_btn {
        let want = if offer_central { Display::Flex } else { Display::None };
        if node.display != want {
            node.display = want;
        }
    }

    // Persistent worlds auto-reset shortly after game-over (the server
    // freezes commands meanwhile); surface its countdown so the pause reads
    // as "new run incoming", not a hang. Singleplayer never sends one.
    let countdown = view
        .reset_countdown
        .map(|s| i18n_hud::go_reset_countdown(s, *lang))
        .unwrap_or_default();
    for (mut text, mut color, kind) in &mut texts {
        let (new, col) = match (kind, state.phase) {
            (GoText::Title, GamePhase::Won) if state.graduated => (
                i18n_hud::go_title_tunnel(*lang).to_string(),
                Color::srgb(0.55, 0.85, 1.00),
            ),
            (GoText::Title, GamePhase::Won) => (
                i18n_hud::go_title_victory(*lang).to_string(),
                Color::srgb(0.55, 0.90, 0.50),
            ),
            (GoText::Title, _) => (
                i18n_hud::go_title_defeat(*lang).to_string(),
                Color::srgb(0.95, 0.35, 0.30),
            ),
            (GoText::Info, GamePhase::Won) if state.graduated => (
                i18n_hud::go_info_graduated(
                    state.day(),
                    state.stock.wood as i64,
                    state.stock.coal as i64,
                    state.stock.food as i64,
                    &countdown,
                    *lang,
                ),
                TEXT_MUTED,
            ),
            (GoText::Info, _) => (
                i18n_hud::go_info_plain(
                    state.day(),
                    state.survivors.len(),
                    state.stock.wood as i64,
                    state.stock.coal as i64,
                    state.stock.food as i64,
                    &countdown,
                    *lang,
                ),
                TEXT_MUTED,
            ),
        };
        if text.0 != new {
            text.0 = new;
        }
        color.0 = col;
    }
}

/// The top-bar world-switch button: visible as "Global World" in a graduated
/// personal world (account sessions only) and as "My City" inside the central
/// world; hidden for guests and ungraduated cities. A press routes through
/// `PendingSwitch` exactly like the game-over button.
pub fn world_switch_button(
    view: Res<GameView>,
    session: Res<Session>,
    lang: Res<super::super::i18n::Lang>,
    mut pending: ResMut<PendingSwitch>,
    mut transition: ResMut<TransitionMsg>,
    mut next: ResMut<NextState<Screen>>,
    mut btn: Query<(&Interaction, &mut Node), With<WorldSwitchBtn>>,
    mut label: Query<&mut Text, With<WorldSwitchLabel>>,
) {
    let Some(state) = view.state.as_ref() else { return };
    // V0.18: pressed from inside the Global World, this button now brings the
    // account's settlers home with it (`ClientMsg::ReturnHome`, chosen over a
    // plain `Login` in `menu::start::pending_switch` whenever the session was
    // central) — the label and transition line use the round-trip-specific
    // i18n_v018 catalog instead of the old generic "My City" text so the wait
    // reads as "people are travelling home", matching how the outbound
    // "Entering the Global World..." transition already reads.
    let target = if session.auth.is_none() {
        None
    } else if state.central {
        Some((WorldTarget::Personal, i18n_v018::return_home(*lang)))
    } else if state.graduated {
        Some((WorldTarget::Central, i18n_hud::world_switch_global(*lang)))
    } else {
        None
    };
    for (interaction, mut node) in &mut btn {
        let want = if target.is_some() { Display::Flex } else { Display::None };
        if node.display != want {
            node.display = want;
        }
        if let Some((world, _)) = target {
            if *interaction == Interaction::Pressed {
                pending.0 = Some(world);
                transition.text = Some(match world {
                    WorldTarget::Personal => i18n_v018::returning(*lang).to_string(),
                    _ => world.transition_label(None, *lang),
                });
                transition.age = 0.0;
                next.set(Screen::Menu);
                return;
            }
        }
    }
    if let Some((_, wanted)) = target {
        for mut text in &mut label {
            if text.0 != wanted {
                text.0 = wanted.to_string();
            }
        }
    }
}

/// Fades in, holds, then fades out the center-screen transition banner
/// (see `TransitionMsg`). `time.delta_secs()` is used directly rather than a
/// `Local` accumulator since `TransitionMsg::age` is the shared, resettable
/// clock every switch site restarts.
pub fn transition_overlay(
    time: Res<Time>,
    mut transition: ResMut<TransitionMsg>,
    mut q: Query<(&mut Text, &mut TextColor), With<TransitionText>>,
) {
    const HOLD: f32 = 1.6;
    const FADE: f32 = 0.8;
    let Some(msg) = transition.text.clone() else {
        return;
    };
    transition.age += time.delta_secs();
    let age = transition.age;
    let alpha = if age < 0.15 {
        age / 0.15
    } else if age < HOLD {
        1.0
    } else if age < HOLD + FADE {
        1.0 - (age - HOLD) / FADE
    } else {
        transition.text = None;
        0.0
    };
    for (mut text, mut color) in &mut q {
        if text.0 != msg {
            text.0 = msg.clone();
        }
        color.0.set_alpha(alpha.clamp(0.0, 1.0));
    }
}

pub fn generic_button_hover(
    mut q: Query<
        (&Interaction, &mut BackgroundColor, &BaseColor),
        (
            Changed<Interaction>,
            Without<BuildBtn>,
            Without<FurnaceLvlBtn>,
        ),
    >,
) {
    for (interaction, mut bg, base) in &mut q {
        let c = base.0.to_srgba();
        bg.0 = match interaction {
            Interaction::Pressed => Color::srgb(
                c.red + (1.0 - c.red) * 0.30,
                c.green + (1.0 - c.green) * 0.30,
                c.blue + (1.0 - c.blue) * 0.30,
            ),
            Interaction::Hovered => Color::srgb(
                c.red + (1.0 - c.red) * 0.15,
                c.green + (1.0 - c.green) * 0.15,
                c.blue + (1.0 - c.blue) * 0.15,
            ),
            Interaction::None => base.0,
        };
    }
}
