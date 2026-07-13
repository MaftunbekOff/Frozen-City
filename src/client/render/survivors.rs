use std::time::Duration;

use bevy::prelude::*;

use frozen_city::game::rng::Rng;
use frozen_city::game::types::{xp_level, Profession};

use super::*;
use crate::client::*;

// ---------------------------------------------------------------- survivors

/// Survivor world position from the sim's authoritative tile coordinates
/// (`Survivor.x/y` — see `types.rs`'s V0.7 doc comment). The server already
/// walks survivors toward their `move_target` or assigned-building goal every
/// tick, so the client no longer picks its own idle/work position — it just
/// renders where the sim says they are.
fn survivor_sim_world(s: &frozen_city::game::types::Survivor) -> Vec3 {
    tilef_to_world((s.x, s.y))
}

pub fn sync_survivors(
    mut commands: Commands,
    view: Res<GameView>,
    assets: Res<GameAssets>,
    models: Res<SurvivorModels>,
    mut viz: ResMut<SurvivorViz>,
    mut dots: Query<(&SurvivorDot, &mut Wander, &mut SurvivorRig)>,
    mut gear: Query<(&SurvivorGear, &mut Visibility)>,
    mut seen: Local<u64>,
) {
    let Some(state) = view.ready() else { return };
    if *seen == view.version {
        return;
    }
    *seen = view.version;

    for s in &state.survivors {
        if viz.0.contains_key(&s.id) {
            continue;
        }
        let pos = survivor_sim_world(s);
        // Kasbiga mos 3D odam modeli (Quaternius, skelet + idle/yurish/yuk
        // animatsiyalari): har aholi o'z SceneRoot nusxasini oladi,
        // animatsiyani `setup_survivor_animations`/`drive_survivor_animations`
        // boshqaradi. Root'ning o'zida mesh yo'q — u pozitsiya/burilish
        // tashuvchisi.
        let variant = Profession::ALL
            .iter()
            .position(|&p| p == s.profession)
            .unwrap_or(0);
        let e = commands
            .spawn((
                Transform::from_translation(pos + Vec3::Y * 0.24),
                Visibility::Inherited,
                SurvivorDot { id: s.id },
                SurvivorRig {
                    variant,
                    carrying: s.assigned_building.is_some(),
                },
                Wander {
                    sim_pos: pos,
                    shuffle_target: pos,
                    speed: 0.9 + (s.id % 7) as f32 * 0.1,
                },
                DespawnOnExit(Screen::Game),
            ))
            .with_children(|p| {
                // Model ~2 birlik bo'yli, tagligi oyoqda — o'yin
                // masshtabiga ~0.5 birlikka keltiramiz; root y=0.24 da
                // turgani uchun sahna -0.24 ga tushiriladi (oyoq yerda).
                // (`WorldAssetRoot` — Bevy 0.19 dagi eski `SceneRoot`.)
                p.spawn((
                    WorldAssetRoot(models.variants[variant].scene.clone()),
                    Transform::from_xyz(0.0, -0.24, 0.0).with_scale(Vec3::splat(0.26)),
                ));
                // XP-daraja anjomlari (yashirin tug'iladi; pastdagi gear-sikl
                // haqiqiy darajaga qarab ochadi): L1 peshona tasmasi,
                // L2 charm qalpoq, L3 oltin ko'krak nishoni.
                p.spawn((
                    Mesh3d(assets.cylinder.clone()),
                    MeshMaterial3d(assets.gear_band_mat.clone()),
                    Transform::from_xyz(0.0, 0.27, 0.0).with_scale(Vec3::new(0.17, 0.035, 0.17)),
                    Visibility::Hidden,
                    SurvivorGear { id: s.id, level: 1 },
                ));
                p.spawn((
                    Mesh3d(assets.cone.clone()),
                    MeshMaterial3d(assets.gear_cap_mat.clone()),
                    Transform::from_xyz(0.0, 0.34, 0.0).with_scale(Vec3::new(0.20, 0.12, 0.20)),
                    Visibility::Hidden,
                    SurvivorGear { id: s.id, level: 2 },
                ));
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(assets.tier_flag_mats[2].clone()),
                    Transform::from_xyz(0.0, 0.12, 0.09).with_scale(Vec3::new(0.07, 0.07, 0.03)),
                    Visibility::Hidden,
                    SurvivorGear { id: s.id, level: 3 },
                ));
            })
            .id();
        viz.0.insert(s.id, e);
    }

    // XP-daraja anjomlarining ko'rinishi — daraja oshgan sari qo'shilib
    // boradi (kumulyativ), o'lgan/ketgan aholiniki root bilan yo'qoladi.
    for (g, mut vis) in &mut gear {
        let lvl = state
            .survivors
            .iter()
            .find(|s| s.id == g.id)
            .map(|s| xp_level(s.xp))
            .unwrap_or(0);
        let want = if lvl >= g.level {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != want {
            *vis = want;
        }
    }

    let gone: Vec<u32> = viz
        .0
        .keys()
        .filter(|id| !state.survivors.iter().any(|s| s.id == **id))
        .copied()
        .collect();
    for id in gone {
        if let Some(e) = viz.0.remove(&id) {
            commands.entity(e).despawn();
        }
    }

    // Refresh each entity's sim-position goal and hauling state.
    for (dot, mut wander, mut rig) in &mut dots {
        if let Some(s) = state.survivors.iter().find(|s| s.id == dot.id) {
            let pos = survivor_sim_world(s);
            if wander.sim_pos.distance(pos) > 0.001 {
                wander.sim_pos = pos;
            }
            let carrying = s.assigned_building.is_some();
            if rig.carrying != carrying {
                rig.carrying = carrying;
            }
        }
    }
}

/// Sahna nusxasi ichida tug'ilgan har yangi `AnimationPlayer`ga o'z
/// kasb-variantining grafini ulab, idle klipdan boshlaydi (o'yinda sahna
/// instansiyalaydigan yagona model turi — aholi, shuning uchun global
/// `Added` filtri yetarli). Variantni ota zanjiridagi `SurvivorRig`dan
/// o'qiydi — player sahna skeleti ichida, root esa bir necha pog'ona tepada.
pub fn setup_survivor_animations(
    mut commands: Commands,
    models: Res<SurvivorModels>,
    rigs: Query<&SurvivorRig>,
    parents: Query<&ChildOf>,
    mut players: Query<(Entity, &mut AnimationPlayer), Added<AnimationPlayer>>,
) {
    for (e, mut player) in &mut players {
        let mut cur = e;
        let mut variant = None;
        for _ in 0..8 {
            let Ok(co) = parents.get(cur) else { break };
            cur = co.parent();
            if let Ok(rig) = rigs.get(cur) {
                variant = Some(rig.variant);
                break;
            }
        }
        let Some(v) = variant else { continue };
        let v = &models.variants[v];
        let mut transitions = AnimationTransitions::new();
        transitions.play(&mut player, v.idle, Duration::ZERO).repeat();
        commands
            .entity(e)
            .insert((AnimationGraphHandle(v.graph.clone()), transitions));
    }
}

/// Holatga mos klipni tanlaydi va silliq krossfeyd bilan almashtiradi:
/// sim-maqsad sari ketayotganda yurish (biriktirilgan aholi yuk ko'tarib —
/// `Walk_Carry`), joyida turganda idle. AnimationPlayer sahna skeleti
/// ichida — ota zanjiridan `SurvivorDot` root'ini topamiz (zanjir qisqa,
/// aholi soni ≤ 60 — arzon).
pub fn drive_survivor_animations(
    roots: Query<(&Transform, &Wander, &SurvivorRig), With<SurvivorDot>>,
    parents: Query<&ChildOf>,
    models: Res<SurvivorModels>,
    mut players: Query<(Entity, &mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    for (e, mut player, mut transitions) in &mut players {
        let mut cur = e;
        let mut found = None;
        for _ in 0..8 {
            let Ok(co) = parents.get(cur) else { break };
            cur = co.parent();
            if let Ok(r) = roots.get(cur) {
                found = Some(r);
                break;
            }
        }
        let Some((tr, w, rig)) = found else { continue };
        let pos = Vec3::new(tr.translation.x, 0.0, tr.translation.z);
        let moving = pos.distance(w.sim_pos) > 0.34;
        let v = &models.variants[rig.variant];
        let want = if moving {
            if rig.carrying {
                v.carry
            } else {
                v.walk
            }
        } else {
            v.idle
        };
        if transitions.get_main_animation() != Some(want) {
            transitions
                .play(&mut player, want, Duration::from_millis(220))
                .repeat();
        }
    }
}

/// Lerp each survivor toward the sim's authoritative position; once caught
/// up, a tiny ±0.3-tile shuffle keeps a stationary survivor from looking
/// frozen. The shuffle is purely cosmetic and re-centers on `sim_pos` — it
/// never accumulates drift away from the authoritative location.
pub fn animate_survivors(
    time: Res<Time>,
    mut q: Query<(&mut Transform, &mut Wander)>,
    mut rng: Local<Rng>,
) {
    const SHUFFLE_RADIUS: f32 = 0.3;
    let dt = time.delta_secs();
    let blend = 1.0 - (-6.0 * dt).exp();
    for (mut t, mut w) in &mut q {
        let pos = Vec3::new(t.translation.x, 0.0, t.translation.z);
        // Caught up to the sim goal (within the shuffle radius): idle-shuffle
        // around it instead of chasing a now-static point exactly.
        if pos.distance(w.sim_pos) < SHUFFLE_RADIUS + 0.05 {
            let to = w.shuffle_target - pos;
            let dist = to.length();
            if dist < 0.08 {
                let off = Vec3::new(
                    rng.range(-100, 100) as f32 * 0.01 * SHUFFLE_RADIUS,
                    0.0,
                    rng.range(-100, 100) as f32 * 0.01 * SHUFFLE_RADIUS,
                );
                w.shuffle_target = w.sim_pos + off;
            } else {
                let step = (w.speed * 0.3 * dt).min(dist);
                let np = pos + to / dist * step;
                t.translation.x = np.x;
                t.translation.z = np.z;
            }
        } else {
            // Actively walking toward the sim goal: reset the shuffle so it
            // doesn't fight the real movement once arrived, and lerp there —
            // same exponential smoothing `sync_player_cursors`/`sync_avatars`
            // use for remote cursors/avatars.
            w.shuffle_target = w.sim_pos;
            let np = pos.lerp(w.sim_pos, blend);
            t.translation.x = np.x;
            t.translation.z = np.z;
            // Model yurish yo'nalishiga yuzlanadi (glTF modellari +Z ga
            // qaraydi) — silliq burilish, keskin sakramaydi.
            let dir = w.sim_pos - pos;
            if dir.length() > 0.05 {
                let yaw = dir.x.atan2(dir.z);
                t.rotation = t.rotation.slerp(Quat::from_rotation_y(yaw), blend);
            }
        }
        // Qadam ritmi endi yurish klipining o'zida — sun'iy bob kerak emas.
        t.translation.y = 0.24;
    }
}

/// Track the survivor-selection ring under whichever survivor is currently
/// selected in the roster panel (`roster::SurvivorSelection`). Reads
/// `SurvivorDot` transforms to follow the selected survivor as they walk.
pub fn animate_survivor_selection(
    time: Res<Time>,
    survivor_sel: Res<crate::client::roster::SurvivorSelection>,
    dots: Query<(&SurvivorDot, &Transform), Without<SurvivorSelectionRing>>,
    mut ring: Query<(&mut Transform, &mut Visibility), With<SurvivorSelectionRing>>,
) {
    let Ok((mut tr, mut vis)) = ring.single_mut() else { return };
    let Some(id) = survivor_sel.0 else {
        *vis = Visibility::Hidden;
        return;
    };
    let Some((_, dot_tr)) = dots.iter().find(|(d, _)| d.id == id) else {
        *vis = Visibility::Hidden;
        return;
    };
    *vis = Visibility::Visible;
    tr.translation.x = dot_tr.translation.x;
    tr.translation.z = dot_tr.translation.z;
    let pulse = 1.0 + 0.08 * (time.elapsed_secs() * 5.0).sin();
    tr.scale = Vec3::splat(0.55 * pulse);
}

/// Keep one crown mesh as a child of whichever `SurvivorDot` entity is the
/// current `GameState.leader`, (re)parenting it when leadership changes
/// (appointment, succession, or death — `leader` cleared with no replacement
/// just hides it). A single shared crown entity, not one per survivor, since
/// there is at most one leader at a time.
pub fn sync_leader_crown(
    mut commands: Commands,
    view: Res<GameView>,
    assets: Res<GameAssets>,
    viz: Res<SurvivorViz>,
    mut crown: Local<Option<Entity>>,
    mut crown_mat: Local<Option<Handle<StandardMaterial>>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut seen_leader: Local<Option<u32>>,
) {
    let Some(state) = view.state.as_ref() else { return };
    if *seen_leader == state.leader {
        return;
    }
    *seen_leader = state.leader;

    let mat = crown_mat
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.82, 0.20),
                emissive: LinearRgba::rgb(0.35, 0.28, 0.03),
                metallic: 0.4,
                perceptual_roughness: 0.3,
                ..default()
            })
        })
        .clone();

    match (state.leader.and_then(|id| viz.0.get(&id)), *crown) {
        (Some(&parent), Some(existing)) => {
            commands.entity(existing).insert(ChildOf(parent));
        }
        (Some(&parent), None) => {
            let e = commands
                .spawn((
                    Mesh3d(assets.cone.clone()),
                    MeshMaterial3d(mat),
                    // Tip-up (the mesh's default orientation) so it reads as
                    // a crown sitting on the survivor's head, unlike the
                    // downward-pointing cones used for cursor/ping markers.
                    // (V0.8: bosh 0.30 da — toj undan yuqorida turadi.)
                    Transform::from_xyz(0.0, 0.46, 0.0).with_scale(Vec3::new(0.16, 0.16, 0.16)),
                    LeaderCrown,
                    ChildOf(parent),
                ))
                .id();
            *crown = Some(e);
        }
        (None, Some(existing)) => {
            commands.entity(existing).despawn();
            *crown = None;
        }
        (None, None) => {}
    }
}

/// Spawn a brief expanding ring at a `MoveSurvivor` destination — visual
/// confirmation the walk command was actually sent, since the survivor
/// itself won't visibly react for a tick or two over the network. Drains
/// `MoveOrderQueue`, a small inbox `input.rs`/`touch.rs` push into right
/// after sending the command (same "resource inbox" shape as
/// `SocialState::bubbles`, chosen over a Bevy `Message` type since nothing
/// else in this client defines a custom one).
pub fn spawn_move_ping(
    mut commands: Commands,
    assets: Res<GameAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut queue: ResMut<crate::client::MoveOrderQueue>,
) {
    for (x, y) in queue.0.drain(..) {
        let mat = materials.add(StandardMaterial {
            base_color: Color::srgba(0.45, 0.85, 1.0, 0.8),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        });
        commands.spawn((
            Mesh3d(assets.ring.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(tilef_to_world((x as f32 + 0.5, y as f32 + 0.5)))
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                .with_scale(Vec3::splat(0.1)),
            MoveOrderPing { age: 0.0 },
            DespawnOnExit(Screen::Game),
        ));
    }
}

pub fn animate_move_pings(
    time: Res<Time>,
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(
        Entity,
        &mut MoveOrderPing,
        &mut Transform,
        &MeshMaterial3d<StandardMaterial>,
    )>,
) {
    for (e, mut ping, mut tr, mat) in &mut q {
        ping.age += time.delta_secs();
        let t = (ping.age / MOVE_PING_LIFETIME).clamp(0.0, 1.0);
        tr.scale = Vec3::splat(0.1 + 0.9 * t);
        if let Some(mut m) = materials.get_mut(&mat.0) {
            m.base_color = m.base_color.with_alpha(0.8 * (1.0 - t));
        }
        if ping.age >= MOVE_PING_LIFETIME {
            commands.entity(e).despawn();
        }
    }
}
