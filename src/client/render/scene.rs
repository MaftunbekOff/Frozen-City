use bevy::prelude::*;

use frozen_city::game::rng::Rng;

use super::*;
use crate::client::*;

// --------------------------------------------------------------- enter game

pub fn enter_game(
    mut commands: Commands,
    assets: Res<GameAssets>,
    quality: Res<Quality>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut rig: ResMut<crate::client::input::CamRig>,
    mut transition: ResMut<crate::client::TransitionMsg>,
) {
    *rig = crate::client::input::CamRig::default();
    // A pending transition message (set by whichever button requested this
    // world switch, just before the menu-frame trip) starts its on-screen
    // countdown now that the new world has actually loaded — see
    // `TransitionMsg`'s doc comment for why it isn't shown during the
    // (single-frame, too-fast-to-read) menu trip itself.
    transition.age = 0.0;

    // Full-screen cold haze for blizzards. `GlobalZIndex(-1)` keeps it behind
    // the HUD (so panels stay clear) and it has no `Interaction`, so it never
    // captures clicks — it only tints the visible world.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.80, 0.86, 0.95, 0.0)),
        GlobalZIndex(-1),
        BlizzardOverlay,
        DespawnOnExit(Screen::Game),
    ));

    // Furnace heat radius: flat ring on the ground, scaled by the radius.
    let heat_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.55, 0.18, 0.35),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    commands.spawn((
        Mesh3d(assets.ring.clone()),
        MeshMaterial3d(heat_mat.clone()),
        Transform::from_xyz(0.0, 0.03, 0.0)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        Visibility::Hidden,
        HeatRing { mat: heat_mat },
        DespawnOnExit(Screen::Game),
    ));

    // Selection highlight ring.
    let sel_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.5),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    commands.spawn((
        Mesh3d(assets.ring.clone()),
        MeshMaterial3d(sel_mat),
        Transform::from_xyz(0.0, 0.04, 0.0)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        Visibility::Hidden,
        SelectionRing,
        DespawnOnExit(Screen::Game),
    ));

    // Survivor selection highlight ring — a distinct color from the building
    // ring above so the two read as different kinds of selection at a glance.
    let survivor_sel_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.45, 0.85, 1.0, 0.65),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    commands.spawn((
        Mesh3d(assets.ring.clone()),
        MeshMaterial3d(survivor_sel_mat),
        Transform::from_xyz(0.0, 0.05, 0.0)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::splat(0.55)),
        Visibility::Hidden,
        SurvivorSelectionRing,
        DespawnOnExit(Screen::Game),
    ));

    // Build placement ghost.
    let ghost_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.3, 0.9, 0.4, 0.45),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    commands.spawn((
        Mesh3d(assets.cube.clone()),
        MeshMaterial3d(ghost_mat.clone()),
        Transform::from_xyz(0.0, 0.25, 0.0).with_scale(Vec3::new(0.95, 0.5, 0.95)),
        Visibility::Hidden,
        GhostMarker { mat: ghost_mat },
        DespawnOnExit(Screen::Game),
    ));

    // Snowfall volume around the camera focus. Phones get far fewer flakes —
    // each is a translucent draw and mobile GPUs are fill-rate bound.
    let flake_count = if *quality == Quality::Low { 60 } else { 240 };
    let mut rng = Rng::new(0x5005_7EA1);
    for _ in 0..flake_count {
        let x = rng.range(-24, 24) as f32 + rng.below(100) as f32 * 0.01;
        let z = rng.range(-24, 24) as f32 + rng.below(100) as f32 * 0.01;
        let y = rng.below(140) as f32 * 0.1;
        let s = 0.03 + rng.below(30) as f32 * 0.002;
        commands.spawn((
            Mesh3d(assets.cube.clone()),
            MeshMaterial3d(assets.snow_mat.clone()),
            Transform::from_xyz(x, y, z).with_scale(Vec3::splat(s)),
            Snowflake {
                fall: 1.4 + rng.below(22) as f32 * 0.1,
                drift: 0.2 + rng.below(10) as f32 * 0.06,
                phase: rng.below(628) as f32 * 0.01,
            },
            DespawnOnExit(Screen::Game),
        ));
    }
}
