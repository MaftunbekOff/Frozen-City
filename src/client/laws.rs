//! V0.18: the book of laws — a modal listing every [`Law`], what it buys and
//! what it costs, with enact/repeal buttons.
//!
//! Modelled on [`super::research`] (the closest existing panel: a list of
//! colony-wide choices with a cost/benefit and a button per row), but a law
//! is a toggle with a cooldown rather than a one-way purchase, so each row
//! shows its CURRENT state (in force or not) and greys with the refusal
//! reason straight from `GameState::can_enact_law`/`can_repeal_law` — the
//! same authority the server itself checks, so the button never promises
//! something the server would then refuse.
//!
//! Touch-usable (unlike `research`, key-only): a HUD button toggles this too,
//! spawned independently the same way `social`'s `SocialHudBtn` is, so it
//! survives regardless of the main HUD's own layout.
//!
//! [`Law`]: frozen_city::game::types::Law

use bevy::prelude::*;

use frozen_city::game::types::{Law, PlayerCommand, MAX_ACTIVE_LAWS};
use frozen_city::net::protocol::ClientMsg;

use super::chat::ChatState;
use super::i18n::Lang;
use super::i18n_laws;
use super::theme::{self, BaseColor, FormFactor};
use super::ui::UiBlocker;
use super::{GameView, NetConn, Screen};

/// Whether the law modal is open (also gates world/camera input).
#[derive(Resource, Default)]
pub struct LawsOpen(pub bool);

#[derive(Component)]
struct LawsRoot;

#[derive(Component)]
struct TitleText;

#[derive(Component)]
struct SubtitleText;

#[derive(Component)]
struct LawNameText(Law);

#[derive(Component)]
struct LawBenefitText(Law);

#[derive(Component)]
struct LawCostText(Law);

#[derive(Component)]
struct LawBtn(Law);

#[derive(Component)]
struct LawBtnBg(Law);

#[derive(Component)]
struct LawBtnLabel(Law);

/// The HUD toggle button (mobile has no `L` key) — see `social::spawn::spawn_hud_button`'s
/// doc for why this is spawned as its own small independent root rather than
/// folded into `ui::spawn_hud`.
#[derive(Component)]
struct LawsHudBtn;

pub fn plugin(app: &mut App) {
    app.init_resource::<LawsOpen>()
        .add_systems(OnEnter(Screen::Game), (spawn_laws, spawn_hud_button))
        .add_systems(
            Update,
            (toggle_laws, laws_hud_button, update_laws, law_buttons).run_if(in_state(Screen::Game)),
        );
}

fn spawn_laws(mut commands: Commands, ff: Res<FormFactor>) {
    let ff = *ff;
    commands
        .spawn((theme::scrim(ff), UiBlocker, LawsRoot, DespawnOnExit(Screen::Game)))
        .with_children(|p| {
            p.spawn(theme::modal_panel(ff)).with_children(|panel| {
                panel.spawn((theme::title(""), TitleText));
                panel.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_MUTED), SubtitleText));
                panel.spawn(theme::brass_divider());
                for law in Law::ALL {
                    panel.spawn(theme::card()).with_children(|row| {
                        row.spawn(Node {
                            flex_grow: 1.0,
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(theme::SP_XS),
                            ..default()
                        })
                        .with_children(|info| {
                            info.spawn((
                                theme::text("", theme::FS_BODY, theme::TEXT_PRIMARY),
                                LawNameText(law),
                            ));
                            info.spawn((
                                theme::text("", theme::FS_SMALL, theme::SUCCESS),
                                LawBenefitText(law),
                            ));
                            info.spawn((
                                theme::text("", theme::FS_SMALL, theme::DANGER),
                                LawCostText(law),
                            ));
                        });
                        row.spawn((
                            Button,
                            Node {
                                width: Val::Px(150.0),
                                min_height: Val::Px(ff.btn_h()),
                                height: Val::Auto,
                                padding: UiRect::axes(Val::Px(theme::SP_XS), Val::Px(theme::SP_XS)),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                                ..default()
                            },
                            // No BaseColor: this button's color is owned entirely by
                            // `update_laws` (in-force/enactable/refused), so the
                            // generic hover system must not fight it over
                            // BackgroundColor (same reasoning as research.rs's
                            // ResearchBtn / missions.rs's TunnelInvestBtn).
                            BackgroundColor(theme::BTN),
                            LawBtn(law),
                            LawBtnBg(law),
                        ))
                        .with_children(|b| {
                            b.spawn((
                                theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY),
                                TextLayout::new(Justify::Center, LineBreak::WordBoundary),
                                LawBtnLabel(law),
                            ));
                        });
                    });
                }
            });
        });
}

/// Spawned separately from the panel above (own root, own `OnEnter`) so it
/// survives independently of the modal's open/closed `Display` — mirrors
/// `social::spawn::spawn_hud_button`'s doc for placement reasoning. Sits just
/// below the Friends button (itself beside the minimap): both buttons are a
/// fixed 90x28 chip, stacked with a small gap, so a phone-width screen still
/// has room for both without touching the minimap or the event feed.
fn spawn_hud_button(mut commands: Commands, ff: Res<FormFactor>, lang: Res<Lang>) {
    let compact = ff.compact();
    commands
        .spawn((
            Button,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(if compact { 12.0 } else { 206.0 }),
                top: Val::Px(if compact { 230.0 } else { 112.0 }),
                width: Val::Px(90.0),
                height: Val::Px(28.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                ..default()
            },
            BackgroundColor(theme::BTN),
            BaseColor(theme::BTN),
            LawsHudBtn,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|b| {
            b.spawn(theme::text(i18n_laws::hud_label(*lang), theme::FS_SMALL, theme::TEXT_PRIMARY));
        });
}

fn toggle_laws(keys: Res<ButtonInput<KeyCode>>, chat: Res<ChatState>, mut open: ResMut<LawsOpen>) {
    if !chat.active && keys.just_pressed(KeyCode::KeyL) {
        open.0 = !open.0;
    }
    if open.0 && keys.just_pressed(KeyCode::Escape) {
        open.0 = false;
    }
}

fn laws_hud_button(q: Query<&Interaction, (With<LawsHudBtn>, Changed<Interaction>)>, mut open: ResMut<LawsOpen>) {
    for interaction in &q {
        if *interaction == Interaction::Pressed {
            open.0 = !open.0;
        }
    }
}

/// Refusal reasons come straight from `can_enact_law`/`can_repeal_law` — the
/// sim's own `&'static str`s, in English. Deliberately NOT run through
/// `i18n_laws`: these are server-authoritative text in the same vein as
/// `GameEvent.text` (see `ui/hud.rs`'s `HudField::Events` doc — "Server
/// event-stream text is NOT localized"), and mirroring the exact match arms
/// of two sim-side functions here would be one more place a future refusal
/// reason gets silently forgotten. Full localization of server-authoritative
/// text is tracked as a follow-up in ROADMAP.md ("Server matnlarini
/// lokalizatsiya qilish") for event text; the same follow-up covers this.
#[allow(clippy::too_many_arguments)]
fn update_laws(
    open: Res<LawsOpen>,
    view: Res<GameView>,
    lang: Res<Lang>,
    mut root: Query<&mut Node, With<LawsRoot>>,
    mut title: Query<&mut Text, (With<TitleText>, Without<SubtitleText>, Without<LawNameText>, Without<LawBenefitText>, Without<LawCostText>)>,
    mut subtitle: Query<&mut Text, (With<SubtitleText>, Without<TitleText>, Without<LawNameText>, Without<LawBenefitText>, Without<LawCostText>)>,
    mut names: Query<(&LawNameText, &mut Text, &mut TextColor), (Without<TitleText>, Without<SubtitleText>, Without<LawBenefitText>, Without<LawCostText>)>,
    mut benefits: Query<(&LawBenefitText, &mut Text), (Without<TitleText>, Without<SubtitleText>, Without<LawNameText>, Without<LawCostText>)>,
    mut costs: Query<(&LawCostText, &mut Text), (Without<TitleText>, Without<SubtitleText>, Without<LawNameText>, Without<LawBenefitText>)>,
    mut btn_bgs: Query<(&LawBtnBg, &mut BackgroundColor)>,
    mut labels: Query<(&LawBtnLabel, &mut Text, &mut TextColor), (Without<TitleText>, Without<SubtitleText>, Without<LawNameText>, Without<LawBenefitText>, Without<LawCostText>)>,
) {
    let lang = *lang;
    let display = if open.0 { Display::Flex } else { Display::None };
    for mut node in &mut root {
        if node.display != display {
            node.display = display;
        }
    }
    if let Ok(mut t) = title.single_mut() {
        let new = format!("{}   {}", i18n_laws::title(lang), i18n_laws::hint(lang));
        if t.0 != new {
            t.0 = new;
        }
    }
    if !open.0 {
        return;
    }
    let Some(state) = view.state.as_ref() else { return };

    if let Ok(mut t) = subtitle.single_mut() {
        let new = i18n_laws::active_count(state.laws.len(), MAX_ACTIVE_LAWS, lang);
        if t.0 != new {
            t.0 = new;
        }
    }
    for (name, mut t, mut c) in &mut names {
        let has = state.has_law(name.0);
        let new = if has {
            format!("{} — {}", i18n_laws::law_name(name.0, lang), i18n_laws::in_force(lang))
        } else {
            i18n_laws::law_name(name.0, lang).to_string()
        };
        if t.0 != new {
            t.0 = new;
        }
        let want = if has { theme::SUCCESS } else { theme::TEXT_PRIMARY };
        if c.0 != want {
            c.0 = want;
        }
    }
    for (benefit, mut t) in &mut benefits {
        let new = format!("+ {}", i18n_laws::law_benefit(benefit.0, lang));
        if t.0 != new {
            t.0 = new;
        }
    }
    for (cost, mut t) in &mut costs {
        let new = format!("- {}", i18n_laws::law_cost(cost.0, lang));
        if t.0 != new {
            t.0 = new;
        }
    }
    for (btn, mut bg) in &mut btn_bgs {
        let has = state.has_law(btn.0);
        let ok = if has { state.can_repeal_law(btn.0).is_ok() } else { state.can_enact_law(btn.0).is_ok() };
        let want = if !ok {
            theme::BTN_DIM
        } else if has {
            theme::BTN_DANGER
        } else {
            theme::BTN
        };
        if bg.0 != want {
            bg.0 = want;
        }
    }
    for (label, mut t, mut c) in &mut labels {
        let has = state.has_law(label.0);
        let (s, col) = if has {
            match state.can_repeal_law(label.0) {
                Ok(()) => (i18n_laws::repeal_btn(lang).to_string(), theme::TEXT_PRIMARY),
                Err(reason) => (reason.to_string(), theme::TEXT_MUTED),
            }
        } else {
            match state.can_enact_law(label.0) {
                Ok(()) => (i18n_laws::enact_btn(lang).to_string(), theme::TEXT_PRIMARY),
                Err(reason) => (reason.to_string(), theme::TEXT_MUTED),
            }
        };
        if t.0 != s {
            t.0 = s;
        }
        if c.0 != col {
            c.0 = col;
        }
    }
}

fn law_buttons(net: Res<NetConn>, view: Res<GameView>, clicked: Query<(&Interaction, &LawBtn), Changed<Interaction>>) {
    let Some(state) = view.state.as_ref() else { return };
    for (interaction, btn) in &clicked {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let law = btn.0;
        if state.has_law(law) {
            if state.can_repeal_law(law).is_ok() {
                net.send(ClientMsg::Cmd(PlayerCommand::RepealLaw { law }));
            }
        } else if state.can_enact_law(law).is_ok() {
            net.send(ClientMsg::Cmd(PlayerCommand::EnactLaw { law }));
        }
    }
}
