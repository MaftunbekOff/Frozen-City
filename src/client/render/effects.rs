use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;

use frozen_city::game::rng::Rng;

use super::*;
use crate::client::*;

// -------------------------------------------------------------- environment

/// Sun, ambient light, fog and sky color track the in-game time of day.
pub fn animate_environment(
    time: Res<Time>,
    view: Res<GameView>,
    assets: Res<GameAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut clear: ResMut<ClearColor>,
    mut sun: Query<(&mut DirectionalLight, &mut Transform), With<SunLight>>,
    mut cam_fx: Query<(&mut DistanceFog, &mut AmbientLight)>,
) {
    let Some(state) = view.state.as_ref() else { return };
    let t = state.time_of_day();
    let daylight = (1.0 - (std::f32::consts::TAU * t).cos()) / 2.0;
    let cold = state.cold_snap && state.is_night();
    let blizzard = state.blizzard_active();

    // Windows glow as the light fades.
    let glow = (1.0 - daylight).powf(2.0);
    if let Some(mut m) = materials.get_mut(&assets.window_mat) {
        m.emissive = LinearRgba::rgb(3.2 * glow + 0.02, 1.9 * glow + 0.015, 0.55 * glow);
    }

    let sky_night = Vec3::new(0.012, 0.022, 0.052);
    let sky_day = if cold {
        Vec3::new(0.30, 0.36, 0.48)
    } else {
        Vec3::new(0.42, 0.50, 0.62)
    };
    let mut sky = sky_night.lerp(sky_day, daylight.powf(1.2));

    // Aurora: a faint green/violet shimmer high in the deep-night sky.
    let night = (1.0 - daylight * 4.0).clamp(0.0, 1.0);
    if night > 0.0 {
        let e = time.elapsed_secs();
        let g = 0.030 * (e * 0.23).sin().max(0.0) * night;
        let v = 0.020 * (e * 0.17 + 1.3).sin().max(0.0) * night;
        sky.x += v * 0.6;
        sky.y += g;
        sky.z += v;
    }
    // Blizzard whiteout: pull the sky toward pale cold gray.
    if blizzard {
        sky = sky.lerp(Vec3::new(0.55, 0.60, 0.68), 0.6);
    }
    clear.0 = Color::srgb(sky.x, sky.y, sky.z);

    // During a blizzard visibility collapses (fog closes right in).
    let vis = if blizzard { 0.45 } else { 1.0 };
    for (mut f, mut ambient) in &mut cam_fx {
        f.color = Color::srgb(sky.x, sky.y, sky.z);
        let start = (24.0 + 46.0 * daylight) * vis;
        f.falloff = FogFalloff::Linear {
            start,
            end: start + (60.0 + 40.0 * daylight) * vis,
        };
        ambient.brightness = 45.0 + 300.0 * daylight;
        ambient.color = if cold || blizzard {
            Color::srgb(0.55, 0.68, 1.0)
        } else {
            Color::srgb(0.70, 0.78, 0.95)
        };
    }

    if let Ok((mut light, mut tr)) = sun.single_mut() {
        light.illuminance = 250.0 + 10_500.0 * daylight.powf(1.3);
        // Low sun is warm, midday is neutral, night is moon-blue.
        light.color = if daylight > 0.05 {
            let warm = (1.0 - daylight).powf(1.5);
            Color::srgb(1.0, 0.96 - 0.25 * warm, 0.90 - 0.42 * warm)
        } else {
            Color::srgb(0.65, 0.72, 1.0)
        };
        // The sun never grazes the horizon, however "dawn" the clock says it
        // is. At the old floor (0.18 rad, about 10 degrees) every object threw
        // a shadow roughly six times its own height, and the shadow map — two
        // cascades over 70 units, see `assets.rs` — has nowhere near the
        // precision to resolve one at that angle: the whole valley filled with
        // long thin dark streaks radiating from every survivor and building.
        // It read as corrupted geometry, not as sunrise.
        //
        // 0.45 rad (~26 degrees) keeps shadows to about twice an object's
        // height, which the cascades resolve cleanly, and still leaves a
        // visible low-sun rake at dawn and dusk. The warm light colour above
        // is what actually sells the time of day; the angle was only ever
        // meant to support it.
        let elev = 0.45 + 0.85 * daylight;
        let az = 2.35 + (t - 0.5) * 0.9;
        let sun_dir = -Vec3::new(az.cos() * elev.cos(), elev.sin(), az.sin() * elev.cos());
        *tr = Transform::default().looking_to(sun_dir, Vec3::Y);
    }

}

/// Furnace fire, heat ring and the selection ring.
pub fn animate_effects(
    time: Res<Time>,
    view: Res<GameView>,
    selection: Res<Selection>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    furnaces: Query<&FurnaceGlow>,
    mut lights: Query<&mut PointLight, With<FurnaceLight>>,
    mut heat: Query<(&HeatRing, &mut Transform, &mut Visibility), Without<SelectionRing>>,
    mut sel_ring: Query<(&mut Transform, &mut Visibility), With<SelectionRing>>,
    mut glow_r: Local<f32>,
) {
    let Some(state) = view.state.as_ref() else { return };
    let pulse = (time.elapsed_secs() * 6.0).sin();

    // Heat radius ring.
    let target = if state.furnace_lit {
        state.heat_radius() * TILE
    } else {
        0.0
    };
    *glow_r += (target - *glow_r) * (4.0 * time.delta_secs()).min(1.0);
    for (ring, mut tr, mut vis) in &mut heat {
        if *glow_r < 0.5 {
            *vis = Visibility::Hidden;
        } else {
            *vis = Visibility::Visible;
            tr.scale = Vec3::splat(*glow_r);
            if let Some(mut m) = materials.get_mut(&ring.mat) {
                m.base_color = Color::srgba(
                    1.0,
                    0.55,
                    0.18,
                    0.22 + 0.05 * state.furnace_level as f32 + 0.03 * pulse,
                );
            }
        }
    }

    // Fire glow + light pulse. Both scaled well down from their original
    // values: they were tuned for the furnace's old, much larger physical
    // size (before several rounds of shrinking it down to a proportionate
    // campfire/Pech) and never rescaled alongside it — left at the old
    // brightness, bloom around the now-small structure was blowing it out
    // to a shapeless glow, hiding the model inside its own light.
    for glow in &furnaces {
        if let Some(mut m) = materials.get_mut(&glow.fire_mat) {
            m.emissive = if state.furnace_lit {
                let k = (1.0 + 0.18 * pulse) * (0.7 + 0.3 * state.furnace_level as f32);
                LinearRgba::rgb(1.3 * k, 0.5 * k, 0.11 * k)
            } else {
                LinearRgba::rgb(0.08, 0.05, 0.04)
            };
        }
    }
    for mut light in &mut lights {
        light.intensity = if state.furnace_lit {
            (150_000.0 + 120_000.0 * state.furnace_level as f32) * (1.0 + 0.12 * pulse)
        } else {
            0.0
        };
        light.range = 3.5 + state.heat_radius() * 0.6;
    }

    // Selection ring.
    for (mut tr, mut vis) in &mut sel_ring {
        let sel = selection.0.and_then(|id| state.find_building(id));
        if let Some(b) = sel {
            let (w, h) = b.kind.size();
            *vis = Visibility::Visible;
            let pos = building_center_world(b);
            tr.translation.x = pos.x;
            tr.translation.z = pos.z;
            tr.scale = Vec3::splat(w.max(h) as f32 * 0.8);
        } else {
            *vis = Visibility::Hidden;
        }
    }
}

/// V0.16: the Kitchen dining-cluster campfire's flicker — same emissive-pulse
/// idea as `animate_effects`'s furnace loop, but unconditionally "lit" (a
/// Kitchen is always cooking, independent of `state.furnace_lit`), so it's
/// a separate small system rather than a branch inside `animate_effects`.
pub fn animate_meal_fire(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    fires: Query<&MealFireGlow>,
    mut lights: Query<&mut PointLight, With<MealFireLight>>,
) {
    let pulse = (time.elapsed_secs() * 6.0).sin();
    for glow in &fires {
        if let Some(mut m) = materials.get_mut(&glow.fire_mat) {
            let k = 1.0 + 0.18 * pulse;
            m.emissive = LinearRgba::rgb(0.9 * k, 0.32 * k, 0.07 * k);
        }
    }
    for mut light in &mut lights {
        light.intensity = 60_000.0 * (1.0 + 0.12 * pulse);
    }
}

/// Chimney smoke rises, drifts and grows while the furnace burns.
pub fn animate_smoke(
    time: Res<Time>,
    view: Res<GameView>,
    mut q: Query<(&Smoke, &mut Transform, &mut Visibility)>,
) {
    let lit = view
        .state
        .as_ref()
        .map(|s| s.furnace_lit)
        .unwrap_or(false);
    let elapsed = time.elapsed_secs();
    for (smoke, mut tr, mut vis) in &mut q {
        if !lit {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Inherited;
        let t = (elapsed * 0.22 + smoke.phase).fract();
        let sway = (smoke.phase * 37.0 + elapsed * 0.6).sin();
        tr.translation = Vec3::new(
            sway * 0.35 * t,
            2.15 + t * 3.4,
            (smoke.phase * 53.0 + elapsed * 0.45).cos() * 0.3 * t,
        );
        // Puffs grow as they rise, then pop back to the chimney.
        tr.scale = Vec3::splat(0.08 + t * 0.42);
    }
}

/// Grow newly-placed buildings from almost nothing over a short beat.
pub fn animate_spawn(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Transform, &mut SpawnGrow)>,
) {
    for (e, mut tr, mut grow) in &mut q {
        grow.age += time.delta_secs();
        let t = (grow.age / 0.35).clamp(0.0, 1.0);
        // Smoothstep from a tiny seed to full size.
        let s = 0.08 + 0.92 * (t * t * (3.0 - 2.0 * t));
        tr.scale = Vec3::splat(s);
        if t >= 1.0 {
            tr.scale = Vec3::ONE;
            commands.entity(e).remove::<SpawnGrow>();
        }
    }
}

/// Fade the full-screen cold haze in/out with the blizzard, plus a faint pulse.
pub fn animate_blizzard_overlay(
    time: Res<Time>,
    view: Res<GameView>,
    mut q: Query<&mut BackgroundColor, With<BlizzardOverlay>>,
    mut alpha: Local<f32>,
) {
    let active = view
        .state
        .as_ref()
        .map(|s| s.blizzard_active())
        .unwrap_or(false);
    let target = if active { 0.20 } else { 0.0 };
    *alpha += (target - *alpha) * (2.0 * time.delta_secs()).min(1.0);
    let pulse = if active {
        0.03 * (time.elapsed_secs() * 1.7).sin()
    } else {
        0.0
    };
    for mut bg in &mut q {
        bg.0 = Color::srgba(0.80, 0.86, 0.95, (*alpha + pulse).max(0.0));
    }
}

// --------------------------------------------------------------------- snow

pub fn snow_fall(
    time: Res<Time>,
    rig: Res<crate::client::input::CamRig>,
    view: Res<GameView>,
    mut flakes: Query<(&mut Transform, &Snowflake)>,
    mut rng: Local<Rng>,
) {
    let half = 24.0;
    let top = 14.0;
    let focus = rig.focus;
    let dt = time.delta_secs();
    let elapsed = time.elapsed_secs();
    // A blizzard drives the snow harder and more sideways.
    let blizzard = view
        .state
        .as_ref()
        .map(|s| s.blizzard_active())
        .unwrap_or(false);
    let fall_k = if blizzard { 2.0 } else { 1.0 };
    let drift_k = if blizzard { 2.6 } else { 1.0 };

    for (mut t, flake) in &mut flakes {
        t.translation.y -= flake.fall * fall_k * dt;
        t.translation.x += (elapsed * 0.8 + flake.phase).sin() * flake.drift * drift_k * dt;
        t.translation.z += (elapsed * 0.63 + flake.phase * 1.7).cos() * flake.drift * 0.6 * dt;
        if t.translation.y < 0.0 {
            t.translation.y = top;
            t.translation.x = focus.x + rng.range(-(half as i32), half as i32) as f32;
            t.translation.z = focus.z + rng.range(-(half as i32), half as i32) as f32;
        }
        if (t.translation.x - focus.x).abs() > half * 1.6
            || (t.translation.z - focus.z).abs() > half * 1.6
        {
            t.translation.x = focus.x + rng.range(-(half as i32), half as i32) as f32;
            t.translation.z = focus.z + rng.range(-(half as i32), half as i32) as f32;
        }
    }
}
