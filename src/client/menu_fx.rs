//! Menyu atmosferasi: to'liq-ekran gradient fon + sekin yog'ayotgan qor.
//!
//! Menyu ilgari yalang'och qora bo'shliq ustida turardi; bu modul unga o'yin
//! kayfiyatini beradi. Hammasi UI-qatlamda (3D sahna talab qilmaydi) va
//! `Screen::Menu`dan chiqilganda avtomatik yo'qoladi. Qor parchalari
//! indeksdan olingan deterministik pseudo-tasodifga ega — `rand`ga
//! bog'liqlik yo'q.

use bevy::prelude::*;
use bevy::ui::{BackgroundGradient, ColorStop, Gradient, LinearGradient};

use super::Screen;

const FLAKES: usize = 40;

/// Bitta qor parchasi: tushish tezligi (foiz/soniya) va gorizontal
/// tebranish fazasi.
#[derive(Component)]
pub struct SnowFlake {
    fall_speed: f32,
    sway_phase: f32,
    base_left: f32,
}

/// Indeksdan barqaror [0,1) qiymat — kadrlar aro o'zgarmas, seed talab qilmas.
fn hash01(i: usize, salt: u32) -> f32 {
    let mut x = (i as u32).wrapping_mul(2654435761).wrapping_add(salt.wrapping_mul(40503));
    x ^= x >> 13;
    x = x.wrapping_mul(1274126177);
    x ^= x >> 16;
    (x % 10_000) as f32 / 10_000.0
}

pub fn spawn_menu_fx(mut commands: Commands) {
    // Chuqur muzli tun gradienti: tepada qorong'i, markazda biroz ko'tarilgan
    // ko'k (ufq nuri), pastda yana qorong'i.
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            ..default()
        },
        BackgroundGradient(vec![Gradient::Linear(LinearGradient::new(
            std::f32::consts::PI,
            vec![
                ColorStop::new(Color::srgb(0.016, 0.028, 0.055), Val::Percent(0.0)),
                ColorStop::new(Color::srgb(0.042, 0.075, 0.135), Val::Percent(42.0)),
                ColorStop::new(Color::srgb(0.055, 0.100, 0.165), Val::Percent(62.0)),
                ColorStop::new(Color::srgb(0.014, 0.024, 0.048), Val::Percent(100.0)),
            ],
        ))]),
        GlobalZIndex(-2),
        DespawnOnExit(Screen::Menu),
    ));

    // Qor parchalari: turli o'lcham/shaffoflik/tezlik — uch "chuqurlik qatlami"
    // taassurotini beradi. UI-tugunlar menyu kartalari ORQASIDA (GlobalZIndex -1).
    for i in 0..FLAKES {
        let size = 2.0 + hash01(i, 1) * 4.0;
        let left = hash01(i, 2) * 100.0;
        let top = hash01(i, 3) * 100.0;
        let alpha = 0.08 + hash01(i, 4) * 0.22;
        commands.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(left),
                top: Val::Percent(top),
                width: Val::Px(size),
                height: Val::Px(size),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            BackgroundColor(Color::srgba(0.85, 0.92, 1.0, alpha)),
            SnowFlake {
                // Kattaroq parcha — yaqinroq — tezroq tushadi (parallaks).
                fall_speed: 2.5 + (size - 2.0) * 1.8 + hash01(i, 5) * 2.0,
                sway_phase: hash01(i, 6) * std::f32::consts::TAU,
                base_left: left,
            },
            GlobalZIndex(-1),
            DespawnOnExit(Screen::Menu),
        ));
    }
}

/// Qorni sekin tushirib, yengil chayqatib turadi; pastga yetganda tepadan
/// qayta boshlaydi.
pub fn snow_fall(time: Res<Time>, mut flakes: Query<(&mut Node, &SnowFlake)>) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    for (mut node, flake) in &mut flakes {
        let Val::Percent(top) = node.top else { continue };
        let next = top + flake.fall_speed * dt;
        node.top = Val::Percent(if next > 102.0 { -2.0 } else { next });
        let sway = (t * 0.45 + flake.sway_phase).sin() * 1.1;
        node.left = Val::Percent((flake.base_left + sway).rem_euclid(100.0));
    }
}
