//! Players & ownership panel: shows the roster with each player's role, and —
//! for the world owner — lets them kick a guest. All authority lives on the
//! server; this only sends `Kick` and reflects the authoritative snapshot.

use bevy::prelude::*;

use frozen_city::game::types::Role;
use frozen_city::net::protocol::ClientMsg;

use super::i18n::Lang;
use super::i18n_panels;
use super::theme::{self, BaseColor};
use super::ui::UiBlocker;
use super::{player_color, GameView, NetConn, Screen};

/// Max roster rows shown (co-op is 1–8 players).
const ROWS: usize = 8;

#[derive(Component)]
struct HeaderText;

#[derive(Component)]
struct RoleRow(usize);

#[derive(Component)]
struct RoleName(usize);

#[derive(Component)]
struct KickBtn {
    row: usize,
    target: Option<u64>,
}

#[derive(Component)]
struct KickLabel;

pub fn plugin(app: &mut App) {
    // `generic_button_hover` already runs globally in Screen::Game (ClientPlugin),
    // and it styles any BaseColor button — including this panel's — so we don't
    // re-register it here.
    app.add_systems(OnEnter(Screen::Game), spawn_panel).add_systems(
        Update,
        (update_panel, kick_buttons).run_if(in_state(Screen::Game)),
    );
}

fn spawn_panel(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                // Sits just below the minimap (top 78 + 184 + gap).
                top: Val::Px(272.0),
                width: Val::Px(236.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(theme::SP_XS + 1.0),
                padding: UiRect::all(Val::Px(theme::SP_SM)),
                border_radius: BorderRadius::all(Val::Px(theme::RAD_PANEL)),
                ..default()
            },
            BackgroundColor(theme::BG_PANEL),
            Interaction::default(),
            UiBlocker,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY), HeaderText));

            // Fixed roster rows (filled/hidden each frame).
            for i in 0..ROWS {
                p.spawn((
                    Node {
                        display: Display::None,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(theme::SP_XS),
                        ..default()
                    },
                    RoleRow(i),
                ))
                .with_children(|row| {
                    row.spawn((
                        theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY),
                        RoleName(i),
                        Node {
                            flex_grow: 1.0,
                            ..default()
                        },
                    ));
                    row.spawn((
                        Button,
                        Node {
                            width: Val::Px(44.0),
                            height: Val::Px(24.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                            ..default()
                        },
                        BackgroundColor(theme::BTN_DANGER),
                        BaseColor(theme::BTN_DANGER),
                        KickBtn {
                            row: i,
                            target: None,
                        },
                    ))
                    .with_children(|b| {
                        b.spawn((theme::text("", theme::FS_MICRO, theme::TEXT_PRIMARY), KickLabel));
                    });
                });
            }
        });
}

// Every `&mut Text`-touching query below must exclude every OTHER such
// query's marker component, not just some of them: Bevy only recognizes two
// `Query<&mut Text, With<A>>` / `Query<&mut Text, With<B>>` parameters as
// disjoint when at least one side *also* excludes the other's marker
// (`With<A>` alone vs `With<B>` alone is NOT sufficient — see
// `bevy_ecs::query::access::FilteredAccess::is_ruled_out_by`). `RoleName` and
// `KickLabel` stay separate components (rather than folding into one enum
// like `social.rs`'s `StaticLabel`) since `KickBtn` is read by another system
// (`kick_buttons`) for its row/target payload, not just this one.
#[allow(clippy::too_many_arguments)]
fn update_panel(
    view: Res<GameView>,
    lang: Res<Lang>,
    mut header: Query<&mut Text, (With<HeaderText>, Without<RoleName>, Without<KickLabel>)>,
    mut rows: Query<(&RoleRow, &mut Node), Without<KickBtn>>,
    #[allow(clippy::type_complexity)]
    mut names: Query<(&RoleName, &mut Text, &mut TextColor), (Without<HeaderText>, Without<KickLabel>)>,
    mut kicks: Query<(&mut KickBtn, &mut Node), Without<RoleRow>>,
    mut kick_labels: Query<&mut Text, (With<KickLabel>, Without<HeaderText>, Without<RoleName>)>,
) {
    let lang = *lang;
    let Some(state) = view.state.as_ref() else { return };
    let me = view.player_id.unwrap_or(0);
    let i_own = state.is_owner(me);

    if let Ok(mut h) = header.single_mut() {
        let new = if i_own {
            i18n_panels::players_you_owner(lang).to_string()
        } else {
            i18n_panels::players_you_guest(lang).to_string()
        };
        if h.0 != new {
            h.0 = new;
        }
    }

    for mut t in &mut kick_labels {
        if t.0 != i18n_panels::btn_kick(lang) {
            t.0 = i18n_panels::btn_kick(lang).to_string();
        }
    }

    // Row visibility.
    for (row, mut node) in &mut rows {
        let d = if row.0 < state.players.len() {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != d {
            node.display = d;
        }
    }

    // Names + colors.
    for (slot, mut t, mut c) in &mut names {
        if let Some(p) = state.players.get(slot.0) {
            let tag = if p.role == Role::Owner { i18n_panels::owner_tag(lang) } else { "" };
            let you = if p.id == me { i18n_panels::you_tag(lang) } else { "" };
            let new = format!("{}{}{}", p.name, tag, you);
            if t.0 != new {
                t.0 = new;
            }
            let col = player_color(p.color);
            if c.0 != col {
                c.0 = col;
            }
        }
    }

    // Kick buttons: owner may kick any connected guest (not themselves).
    for (mut kick, mut node) in &mut kicks {
        let target = state.players.get(kick.row).and_then(|p| {
            if i_own && p.role != Role::Owner && p.id != me {
                Some(p.id)
            } else {
                None
            }
        });
        kick.target = target;
        let d = if target.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != d {
            node.display = d;
        }
    }
}

fn kick_buttons(
    net: Res<NetConn>,
    clicked: Query<(&Interaction, &KickBtn), Changed<Interaction>>,
) {
    for (interaction, kick) in &clicked {
        if *interaction == Interaction::Pressed {
            if let Some(target) = kick.target {
                net.send(ClientMsg::Kick { target });
            }
        }
    }
}
