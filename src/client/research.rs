//! Technology tree: a modal research panel toggled with `R`. Lists every
//! [`Tech`], its cost and status, and sends `Research` commands. The server is
//! authoritative; this only reflects `GameState.techs` and affordability.

use bevy::prelude::*;

use frozen_city::game::types::{PlayerCommand, Tech};
use frozen_city::net::protocol::ClientMsg;

use super::chat::ChatState;
use super::i18n_names;
use super::i18n_panels;
use super::theme::{self, FormFactor};
use super::ui::UiBlocker;
use super::{GameView, NetConn, Screen};

/// Whether the research modal is open (also gates world/camera input).
#[derive(Resource, Default)]
pub struct ResearchOpen(pub bool);

#[derive(Component)]
struct ResearchRoot;

#[derive(Component)]
struct ResearchBtn(Tech);

#[derive(Component)]
struct ResearchBtnLabel(Tech);

#[derive(Component)]
struct ResearchBtnBg(Tech);

pub fn plugin(app: &mut App) {
    app.init_resource::<ResearchOpen>()
        .add_systems(OnEnter(Screen::Game), spawn_research)
        .add_systems(
            Update,
            (toggle_research, update_research, research_buttons)
                .run_if(in_state(Screen::Game)),
        );
}

fn spawn_research(mut commands: Commands, ff: Res<FormFactor>) {
    let ff = *ff;
    commands
        .spawn((theme::scrim(ff), UiBlocker, ResearchRoot, DespawnOnExit(Screen::Game)))
        .with_children(|p| {
            p.spawn(theme::modal_panel(ff)).with_children(|panel| {
                panel.spawn((theme::title(""), TitleText));
                for tech in Tech::ALL {
                    panel
                        .spawn(theme::card())
                        .with_children(|row| {
                            row.spawn(Node {
                                flex_grow: 1.0,
                                flex_direction: FlexDirection::Column,
                                row_gap: Val::Px(theme::SP_XS),
                                ..default()
                            })
                            .with_children(|info| {
                                info.spawn((
                                    theme::text("", theme::FS_BODY, theme::TEXT_PRIMARY),
                                    TechNameText(tech),
                                ));
                                info.spawn((
                                    theme::text("", theme::FS_SMALL, theme::TEXT_MUTED),
                                    TechDescText(tech),
                                ));
                            });
                            row.spawn((
                                Button,
                                Node {
                                    width: Val::Px(150.0),
                                    height: Val::Px(ff.btn_h()),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                                    ..default()
                                },
                                // No BaseColor: this button's color is owned entirely by
                                // update_research (owned/affordable/dim), so the generic
                                // hover system must not fight it over BackgroundColor
                                // (same reasoning as missions.rs's TunnelInvestBtn).
                                BackgroundColor(theme::BTN),
                                ResearchBtn(tech),
                                ResearchBtnBg(tech),
                            ))
                            .with_children(|b| {
                                b.spawn((
                                    theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY),
                                    ResearchBtnLabel(tech),
                                ));
                            });
                        });
                }
            });
        });
}

#[derive(Component)]
struct TitleText;

#[derive(Component)]
struct TechNameText(Tech);

#[derive(Component)]
struct TechDescText(Tech);

fn toggle_research(
    keys: Res<ButtonInput<KeyCode>>,
    chat: Res<ChatState>,
    mut open: ResMut<ResearchOpen>,
) {
    if !chat.active && keys.just_pressed(KeyCode::KeyR) {
        open.0 = !open.0;
    }
    if open.0 && keys.just_pressed(KeyCode::Escape) {
        open.0 = false;
    }
}

#[allow(clippy::too_many_arguments)]
fn update_research(
    open: Res<ResearchOpen>,
    view: Res<GameView>,
    lang: Res<super::i18n::Lang>,
    mut root: Query<&mut Node, With<ResearchRoot>>,
    mut title: Query<&mut Text, (With<TitleText>, Without<TechNameText>, Without<TechDescText>)>,
    mut names: Query<(&TechNameText, &mut Text), (Without<TitleText>, Without<TechDescText>)>,
    mut descs: Query<(&TechDescText, &mut Text), (Without<TitleText>, Without<TechNameText>)>,
    mut btn_bgs: Query<(&ResearchBtnBg, &mut BackgroundColor)>,
    mut labels: Query<(&ResearchBtnLabel, &mut Text, &mut TextColor), (Without<TitleText>, Without<TechNameText>, Without<TechDescText>)>,
) {
    let lang = *lang;
    let display = if open.0 { Display::Flex } else { Display::None };
    for mut node in &mut root {
        if node.display != display {
            node.display = display;
        }
    }
    if let Ok(mut t) = title.single_mut() {
        let new = format!("{}   {}", i18n_panels::research_title(lang), i18n_panels::research_hint(lang));
        if t.0 != new {
            t.0 = new;
        }
    }
    if !open.0 {
        return;
    }
    let Some(state) = view.state.as_ref() else { return };

    for (name, mut t) in &mut names {
        let new = i18n_names::tech_name(name.0, lang);
        if t.0 != new {
            t.0 = new.to_string();
        }
    }
    for (desc, mut t) in &mut descs {
        let new = i18n_names::tech_desc(desc.0, lang);
        if t.0 != new {
            t.0 = new.to_string();
        }
    }
    for (btn, mut bg) in &mut btn_bgs {
        let owned = state.has_tech(btn.0);
        let affordable = state.stock.wood >= btn.0.cost_wood() as f32
            && state.stock.coal >= btn.0.cost_coal() as f32;
        let want = if owned {
            theme::BTN_SUCCESS
        } else if affordable {
            theme::BTN
        } else {
            theme::BTN_DIM
        };
        if bg.0 != want {
            bg.0 = want;
        }
    }
    for (label, mut t, mut c) in &mut labels {
        let owned = state.has_tech(label.0);
        let (s, col) = if owned {
            (i18n_panels::researched(lang).to_string(), theme::SUCCESS)
        } else {
            (
                i18n_panels::tech_cost(label.0.cost_wood(), label.0.cost_coal(), lang),
                theme::TEXT_PRIMARY,
            )
        };
        if t.0 != s {
            t.0 = s;
        }
        if c.0 != col {
            c.0 = col;
        }
    }
}

fn research_buttons(
    net: Res<NetConn>,
    view: Res<GameView>,
    clicked: Query<(&Interaction, &ResearchBtn), Changed<Interaction>>,
) {
    let Some(state) = view.state.as_ref() else { return };
    for (interaction, btn) in &clicked {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let owned = state.has_tech(btn.0);
        let affordable = state.stock.wood >= btn.0.cost_wood() as f32
            && state.stock.coal >= btn.0.cost_coal() as f32;
        if !owned && affordable {
            net.send(ClientMsg::Cmd(PlayerCommand::Research { tech: btn.0 }));
        }
    }
}
