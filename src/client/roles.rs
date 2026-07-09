//! Players & ownership panel: shows the roster with each player's role, and —
//! for the world owner — lets them set what guests may do and kick a guest.
//! All authority lives on the server; this only sends `SetGuestPermission` /
//! `Kick` and reflects the authoritative snapshot.

use bevy::prelude::*;

use frozen_city::game::types::{GuestPermission, Role};
use frozen_city::net::protocol::ClientMsg;

use super::ui::{BaseColor, UiBlocker};
use super::{player_color, GameView, NetConn, Screen};

/// Max roster rows shown (co-op is 1–8 players).
const ROWS: usize = 8;

const PANEL_BG: Color = Color::srgba(0.045, 0.075, 0.125, 0.9);
const BTN_BG: Color = Color::srgb(0.16, 0.20, 0.28);
const BTN_ACTIVE: Color = Color::srgb(0.82, 0.50, 0.18);
const KICK_BG: Color = Color::srgb(0.45, 0.16, 0.14);
const TEXT_MAIN: Color = Color::srgb(0.90, 0.93, 0.97);
const TEXT_DIM: Color = Color::srgb(0.62, 0.68, 0.78);

#[derive(Component)]
struct HeaderText;

#[derive(Component)]
struct PermRow;

#[derive(Component)]
struct PermBtn(GuestPermission);

#[derive(Component)]
struct RoleRow(usize);

#[derive(Component)]
struct RoleName(usize);

#[derive(Component)]
struct KickBtn {
    row: usize,
    target: Option<u64>,
}

pub fn plugin(app: &mut App) {
    // `generic_button_hover` already runs globally in Screen::Game (ClientPlugin),
    // and it styles any BaseColor button — including this panel's — so we don't
    // re-register it here.
    app.add_systems(OnEnter(Screen::Game), spawn_panel).add_systems(
        Update,
        (update_panel, perm_buttons, kick_buttons).run_if(in_state(Screen::Game)),
    );
}

fn text(t: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(t.into()), TextFont::from_font_size(size), TextColor(color))
}

fn small_btn(label: &str, bg: Color, w: f32) -> impl Bundle {
    (
        Button,
        Node {
            width: Val::Px(w),
            height: Val::Px(24.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(bg),
        BaseColor(bg),
        Name::new(label.to_string()),
    )
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
                row_gap: Val::Px(5.0),
                padding: UiRect::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
            Interaction::default(),
            UiBlocker,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|p| {
            p.spawn((text("Players", 14.0, TEXT_MAIN), HeaderText));

            // Owner-only guest-permission selector.
            p.spawn((
                Node {
                    display: Display::None,
                    column_gap: Val::Px(4.0),
                    ..default()
                },
                PermRow,
            ))
            .with_children(|row| {
                row.spawn(text("Guests:", 12.0, TEXT_DIM));
                for (perm, label) in [
                    (GuestPermission::ViewOnly, "View"),
                    (GuestPermission::Build, "Build"),
                    (GuestPermission::Full, "Full"),
                ] {
                    row.spawn((small_btn(label, BTN_BG, 52.0), PermBtn(perm)))
                        .with_children(|b| {
                            b.spawn(text(label, 12.0, TEXT_MAIN));
                        });
                }
            });

            // Fixed roster rows (filled/hidden each frame).
            for i in 0..ROWS {
                p.spawn((
                    Node {
                        display: Display::None,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(6.0),
                        ..default()
                    },
                    RoleRow(i),
                ))
                .with_children(|row| {
                    row.spawn((
                        text("", 13.0, TEXT_MAIN),
                        RoleName(i),
                        Node {
                            flex_grow: 1.0,
                            ..default()
                        },
                    ));
                    row.spawn((
                        small_btn("kick", KICK_BG, 44.0),
                        KickBtn {
                            row: i,
                            target: None,
                        },
                    ))
                    .with_children(|b| {
                        b.spawn(text("kick", 11.0, TEXT_MAIN));
                    });
                });
            }
        });
}

fn update_panel(
    view: Res<GameView>,
    mut header: Query<&mut Text, With<HeaderText>>,
    mut perm_row: Query<&mut Node, (With<PermRow>, Without<RoleRow>, Without<KickBtn>)>,
    mut rows: Query<(&RoleRow, &mut Node), Without<KickBtn>>,
    mut names: Query<(&RoleName, &mut Text, &mut TextColor), Without<HeaderText>>,
    mut kicks: Query<(&mut KickBtn, &mut Node), (Without<RoleRow>, Without<PermRow>)>,
) {
    let Some(state) = view.state.as_ref() else { return };
    let me = view.player_id.unwrap_or(0);
    let i_own = state.is_owner(me);

    if let Ok(mut h) = header.single_mut() {
        let new = if i_own {
            "Players — you are the Owner".to_string()
        } else {
            let perm = match state.guest_perm {
                GuestPermission::ViewOnly => "view only",
                GuestPermission::Build => "build",
                GuestPermission::Full => "full",
            };
            format!("Players — you are a Guest ({perm})")
        };
        if h.0 != new {
            h.0 = new;
        }
    }

    for mut node in &mut perm_row {
        let d = if i_own { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
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
            let tag = if p.role == Role::Owner { " (owner)" } else { "" };
            let you = if p.id == me { " — you" } else { "" };
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

fn perm_buttons(
    net: Res<NetConn>,
    view: Res<GameView>,
    clicked: Query<(&Interaction, &PermBtn), Changed<Interaction>>,
    mut all: Query<(&PermBtn, &mut BackgroundColor)>,
) {
    for (interaction, btn) in &clicked {
        if *interaction == Interaction::Pressed {
            net.send(ClientMsg::SetGuestPermission { perm: btn.0 });
        }
    }
    let current = view.state.as_ref().map(|s| s.guest_perm);
    for (btn, mut bg) in &mut all {
        let active = current == Some(btn.0);
        let want = if active { BTN_ACTIVE } else { BTN_BG };
        if bg.0 != want {
            bg.0 = want;
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
