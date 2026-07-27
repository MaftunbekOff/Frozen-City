use bevy::prelude::*;

use frozen_city::game::rng::Rng;
use frozen_city::game::sim;
use frozen_city::game::types::{xp_level, Profession, TUNNEL_X, TUNNEL_Y};

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

/// V0.16: small per-survivor offset so every survivor `sim::survivor_is_at_meal`
/// reports as eating doesn't render stacked on the exact single gather point
/// `sim::gather_points` sends them all to — spreads them around the dining
/// cluster's stools (`render::buildings`'s `BuildingKind::Kitchen` arm; the
/// stool-local offsets there must stay in sync with these). Purely a render
/// nicety: `Wander::sim_pos` is a render-layer value, so sim's authoritative
/// `Survivor.x/y` (and therefore click hit-testing, which reads that field
/// directly — see `input.rs`'s `resolve_world_click`) is untouched. All
/// offsets here stay inside `input::SURVIVOR_PICK_RADIUS` (0.6) of the gather
/// point, so a seated/standing survivor is still clickable at their drawn spot.
const MEAL_SEAT_OFFSETS: [(f32, f32); 3] = [(-0.26, -0.03), (0.26, -0.03), (0.0, 0.29)];

/// Standing spots for the fourth-and-later hungry survivor, who has no free
/// stool — a small fan just behind the fire so they neither sink into a seated
/// pose over thin air nor stack on an occupied stool (`id % 3` used to do the
/// latter once four+ survivors ate at once). Cycles if even these fill up.
const MEAL_STAND_OFFSETS: [(f32, f32); 4] =
    [(-0.45, -0.30), (0.45, -0.30), (-0.20, -0.48), (0.20, -0.48)];

/// Render placement for a hungry survivor of arrival `rank` (0 = first to
/// claim a seat; see `sync_survivors`'s stable lowest-id ordering) at the
/// Kitchen dining cluster: the first three sit on the three stools, the rest
/// stand in the fan behind them. Returns the world-space offset from the
/// shared gather point and whether to fold into the seated leg pose.
fn meal_slot(rank: usize) -> (Vec3, bool) {
    if rank < MEAL_SEAT_OFFSETS.len() {
        let (dx, dz) = MEAL_SEAT_OFFSETS[rank];
        (Vec3::new(dx, 0.0, dz), true)
    } else {
        let (dx, dz) = MEAL_STAND_OFFSETS[(rank - MEAL_SEAT_OFFSETS.len()) % MEAL_STAND_OFFSETS.len()];
        (Vec3::new(dx, 0.0, dz), false)
    }
}

pub fn sync_survivors(
    mut commands: Commands,
    view: Res<GameView>,
    assets: Res<GameAssets>,
    mut viz: ResMut<SurvivorViz>,
    mut dots: Query<(&SurvivorDot, &mut Wander, &mut SurvivorRig)>,
    mut gear: Query<(&SurvivorGear, &mut Visibility), Without<SurvivorCarry>>,
    mut carry: Query<(&SurvivorCarry, &mut Visibility), Without<SurvivorGear>>,
    mut heads: Query<(&SurvivorHead, &mut MeshMaterial3d<StandardMaterial>)>,
    mut seen: Local<u64>,
) {
    let Some(state) = view.ready() else { return };
    if *seen == view.version {
        return;
    }
    *seen = view.version;

    // Once per snapshot, not once per survivor — every survivor below is
    // asked the same `survivor_is_at_meal` question against the same
    // buildings (see `sim::survivor_is_at_meal_with`). The sorted id list of
    // everyone currently at the Kitchen doubles as a stable seat assignment:
    // a survivor's `binary_search` rank (0,1,2,…) picks its stool/stand spot
    // (`meal_slot`), so seating stays consistent frame-to-frame and no two
    // share a stool.
    let gather = sim::gather_points(state);
    let mut hungry: Vec<u32> = state
        .survivors
        .iter()
        .filter(|s| sim::survivor_is_at_meal_with(state, &gather, s))
        .map(|s| s.id)
        .collect();
    hungry.sort_unstable();
    // V0.17: same once-per-snapshot snapshot trick for the sleeping-in-a-bunk
    // query (`sim::survivor_is_resting_with`) — every survivor asks it
    // against the same bunk-holder set instead of each recomputing
    // `GameState::bunked_ids` (an O(buildings) scan) themselves.
    let bunked = state.bunked_ids();

    for s in &state.survivors {
        let is_leader = state.leader == Some(s.id);
        if let Some(entry) = viz.0.get(&s.id) {
            if entry.is_leader == is_leader {
                continue;
            }
            // Leadership just changed hands (or this survivor just lost/
            // gained the seat) — the trade-vs-leader look swaps whole
            // meshes, so rebuild rather than try to patch materials in place.
            commands.entity(entry.entity).despawn();
            viz.0.remove(&s.id);
        }
        let (meal_off, sitting) = match hungry.binary_search(&s.id) {
            Ok(rank) => meal_slot(rank),
            Err(_) => (Vec3::ZERO, false),
        };
        let sleeping = sim::survivor_is_resting_with(state, &gather, &bunked, s);
        let pos = survivor_sim_world(s) + meal_off;
        let variant = Profession::ALL
            .iter()
            .position(|&p| p == s.profession)
            .unwrap_or(0);
        // Root sits at ground level (feet), unlike the old single-capsule
        // dot which stored its own center height — every body part below
        // is positioned as an absolute height from the ground instead.
        let e = commands
            .spawn((
                Transform::from_translation(pos),
                Visibility::Inherited,
                SurvivorDot { id: s.id },
                SurvivorRig {
                    carrying: s.assigned_building.is_some(),
                    sitting,
                    sleeping,
                },
                Wander {
                    sim_pos: pos,
                    shuffle_target: pos,
                    speed: 0.9 + (s.id % 7) as f32 * 0.1,
                    moving: false,
                },
                DespawnOnExit(Screen::Game),
            ))
            .with_children(|p| {
                spawn_survivor_body(p, &assets, s.profession, variant, s.id, is_leader);

                // XP-daraja anjomlari (yashirin tug'iladi; pastdagi gear-sikl
                // haqiqiy darajaga qarab ochadi): L1 peshona tasmasi, L2
                // qalpoq halqasi, L3 oltin ko'krak nishoni.
                p.spawn((
                    Mesh3d(assets.cylinder.clone()),
                    MeshMaterial3d(assets.gear_band_mat.clone()),
                    Transform::from_xyz(0.0, 0.50, 0.0).with_scale(Vec3::new(0.13, 0.03, 0.13)),
                    Visibility::Hidden,
                    SurvivorGear { id: s.id, level: 1 },
                ));
                p.spawn((
                    Mesh3d(assets.cylinder.clone()),
                    MeshMaterial3d(assets.gear_cap_mat.clone()),
                    Transform::from_xyz(0.0, 0.565, 0.0).with_scale(Vec3::new(0.20, 0.05, 0.20)),
                    Visibility::Hidden,
                    SurvivorGear { id: s.id, level: 2 },
                ));
                p.spawn((
                    Mesh3d(assets.cube.clone()),
                    MeshMaterial3d(assets.tier_flag_mats[2].clone()),
                    Transform::from_xyz(-0.09, 0.34, 0.10).with_scale(Vec3::new(0.07, 0.07, 0.03)),
                    Visibility::Hidden,
                    SurvivorGear { id: s.id, level: 3 },
                ));
            })
            .id();
        viz.0.insert(s.id, SurvivorVizEntry { entity: e, is_leader });
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

    // Carried-resource prop — visible only while assigned to a building
    // (mirrors the XP-gear loop above, computed straight from sim state so
    // it never races the `rig.carrying` refresh below).
    for (c, mut vis) in &mut carry {
        let carrying = state
            .survivors
            .iter()
            .find(|s| s.id == c.id)
            .map(|s| s.assigned_building.is_some())
            .unwrap_or(false);
        let want = if carrying {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != want {
            *vis = want;
        }
    }

    // V0.17: sick tint — swap the shared skin material handle (never a new
    // `StandardMaterial` per survivor/frame; see `GameAssets::
    // survivor_skin_sick_mat`'s doc comment), same id-keyed lookup the gear/
    // carry loops above use.
    for (h, mut mat) in &mut heads {
        let sick = state
            .survivors
            .iter()
            .find(|s| s.id == h.id)
            .map(|s| s.is_sick())
            .unwrap_or(false);
        let want = if sick {
            assets.survivor_skin_sick_mat.clone()
        } else {
            assets.survivor_skin_mat.clone()
        };
        if mat.0 != want {
            mat.0 = want;
        }
    }

    let gone: Vec<u32> = viz
        .0
        .keys()
        .filter(|id| !state.survivors.iter().any(|s| s.id == **id))
        .copied()
        .collect();
    for id in gone {
        if let Some(entry) = viz.0.remove(&id) {
            commands.entity(entry.entity).despawn();
        }
    }

    // Refresh each entity's sim-position goal and hauling/sitting state.
    for (dot, mut wander, mut rig) in &mut dots {
        if let Some(s) = state.survivors.iter().find(|s| s.id == dot.id) {
            let (meal_off, sitting) = match hungry.binary_search(&s.id) {
                Ok(rank) => meal_slot(rank),
                Err(_) => (Vec3::ZERO, false),
            };
            let pos = survivor_sim_world(s) + meal_off;
            if wander.sim_pos.distance(pos) > 0.001 {
                wander.sim_pos = pos;
            }
            let carrying = s.assigned_building.is_some();
            if rig.carrying != carrying {
                rig.carrying = carrying;
            }
            if rig.sitting != sitting {
                rig.sitting = sitting;
            }
            let sleeping = sim::survivor_is_resting_with(state, &gather, &bunked, s);
            if rig.sleeping != sleeping {
                rig.sleeping = sleeping;
            }
        }
    }
}

/// Traveler figures standing at the Tunnel mouth while `pending_migrant` is
/// set — cosmetic only (see `MigrantViz`'s doc comment), rebuilt whenever the
/// pending batch's identity changes and cleared once it resolves (absorbed
/// into the colony or turned back), both handled in `tick.rs`.
pub fn sync_migrants(
    mut commands: Commands,
    view: Res<GameView>,
    assets: Res<GameAssets>,
    mut viz: ResMut<MigrantViz>,
    mut seen: Local<u64>,
) {
    let Some(state) = view.ready() else { return };
    if *seen == view.version {
        return;
    }
    *seen = view.version;

    let key = state.pending_migrant.map(|m| (m.count, m.expires));
    if key == viz.key {
        return;
    }
    viz.key = key;
    for e in viz.entities.drain(..) {
        commands.entity(e).despawn();
    }
    let Some(m) = state.pending_migrant else { return };
    // A little in front of the tunnel mouth (TUNNEL is a 2x2 footprint), so
    // they read as "just stepped out" rather than standing inside it.
    let base = tilef_to_world((TUNNEL_X as f32 + 1.0, TUNNEL_Y as f32 + 2.3));
    for i in 0..m.count {
        let offset = (i as f32 - (m.count as f32 - 1.0) / 2.0) * 0.5;
        let pos = base + Vec3::new(offset, 0.0, 0.0);
        let variant = (i as usize + m.expires as usize) % Profession::ALL.len();
        let profession = Profession::ALL[variant];
        let e = commands
            .spawn((
                Transform::from_translation(pos),
                Visibility::Inherited,
                DespawnOnExit(Screen::Game),
            ))
            .with_children(|p| {
                spawn_survivor_body(p, &assets, profession, variant, 0, false);
            })
            .id();
        viz.entities.push(e);
    }
}

/// Builds one procedural, low-poly winter survivor as children of the
/// just-spawned `SurvivorDot` root: two swinging legs, two static arms, a
/// torso + head in the trade's coat color, and profession-specific
/// headwear + tool (`Profession::ALL` order, matching `variant`). Everyone
/// is bundled up against the cold — the headwear shape (hood/hardhat/toque)
/// is what makes a trade readable from its silhouette, colors reinforce it.
/// Tool materials deliberately reuse a workplace building's own material
/// (e.g. the lumberjack's axe blade is `sawmill_blade_mat`) so a survivor
/// visually echoes the building they work.
///
/// While `is_leader` (`GameState.leader == Some(id)`), the trade look is
/// swapped for a distinct one — royal coat, bare head under a crown, a
/// ceremonial staff instead of their trade's tool — echoing
/// `survivor_contribution`'s sim-side rule that the leader is a generalist
/// for as long as they hold the seat, not stuck looking like whatever they
/// did before. The crown is spawned right here as a plain child (not a
/// separately tracked/reparented entity — `sync_survivors` already fully
/// despawns/respawns a survivor's whole body on a leadership change, so
/// there's nothing to gain from keeping the crown a separate entity, and
/// doing so previously caused a panic: reparenting a `Local`-cached crown
/// handle onto a new leader after the old leader's body — and the crown as
/// its child — had already been despawned this same frame).
fn spawn_survivor_body(
    p: &mut ChildSpawnerCommands,
    assets: &GameAssets,
    profession: Profession,
    variant: usize,
    id: u32,
    is_leader: bool,
) {
    let coat = if is_leader {
        assets.leader_coat_mat.clone()
    } else {
        assets.survivor_coat_mats[variant].clone()
    };
    let head_mat = assets.survivor_head_mats[variant].clone();

    // Legs — direct children of the root so `animate_survivor_legs` can walk
    // one `ChildOf` hop up to read the root's `Wander::moving`.
    p.spawn((
        Mesh3d(assets.capsule.clone()),
        MeshMaterial3d(coat.clone()),
        Transform::from_xyz(-0.055, 0.13, 0.0).with_scale(Vec3::new(0.42, 0.60, 0.42)),
        SurvivorLeg { phase: 0.0 },
    ));
    p.spawn((
        Mesh3d(assets.capsule.clone()),
        MeshMaterial3d(coat.clone()),
        Transform::from_xyz(0.055, 0.13, 0.0).with_scale(Vec3::new(0.42, 0.60, 0.42)),
        SurvivorLeg {
            phase: std::f32::consts::PI,
        },
    ));
    // Arms — static, just for silhouette fill; the legs carry the gait.
    for dx in [-0.145f32, 0.145] {
        p.spawn((
            Mesh3d(assets.capsule.clone()),
            MeshMaterial3d(coat.clone()),
            Transform::from_xyz(dx, 0.32, 0.0).with_scale(Vec3::new(0.28, 0.52, 0.28)),
        ));
    }
    // Torso.
    p.spawn((
        Mesh3d(assets.capsule.clone()),
        MeshMaterial3d(coat),
        Transform::from_xyz(0.0, 0.34, 0.0).with_scale(Vec3::splat(0.62)),
    ));
    // Head. `SurvivorHead` lets `sync_survivors` retint it while sick,
    // without a full rebuild of the rest of the body.
    p.spawn((
        Mesh3d(assets.sphere.clone()),
        MeshMaterial3d(assets.survivor_skin_mat.clone()),
        Transform::from_xyz(0.0, 0.52, 0.0).with_scale(Vec3::splat(0.19)),
        SurvivorHead { id },
    ));

    if is_leader {
        // Bare head (the crown sits directly on it) and a ceremonial staff
        // — wood handle reused from the same material every trade's tool
        // handle uses, gold orb reusing the XP-tier gold so it reads as
        // "distinguished" without a new material.
        p.spawn((
            Mesh3d(assets.cylinder.clone()),
            MeshMaterial3d(assets.sawmill_roof_mat.clone()),
            Transform::from_xyz(0.16, 0.30, 0.0).with_scale(Vec3::new(0.035, 0.50, 0.035)),
        ));
        p.spawn((
            Mesh3d(assets.sphere.clone()),
            MeshMaterial3d(assets.tier_flag_mats[2].clone()),
            Transform::from_xyz(0.16, 0.56, 0.0).with_scale(Vec3::splat(0.07)),
        ));
        // Crown — tip-up (the mesh's default orientation) so it reads as
        // sitting on the survivor's bare head.
        p.spawn((
            Mesh3d(assets.cone.clone()),
            MeshMaterial3d(assets.leader_crown_mat.clone()),
            Transform::from_xyz(0.0, 0.72, 0.0).with_scale(Vec3::new(0.15, 0.15, 0.15)),
            LeaderCrown,
        ));
    } else {
    match profession {
        Profession::Lumberjack => {
            p.spawn((
                Mesh3d(assets.cone.clone()),
                MeshMaterial3d(head_mat),
                Transform::from_xyz(0.0, 0.60, 0.0).with_scale(Vec3::new(0.22, 0.15, 0.22)),
            ));
            // Axe slung on the back: wood handle + the sawmill's own blade.
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(assets.sawmill_roof_mat.clone()),
                Transform::from_xyz(0.0, 0.36, -0.10)
                    .with_rotation(Quat::from_rotation_x(0.55))
                    .with_scale(Vec3::new(0.05, 0.34, 0.05)),
            ));
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.sawmill_blade_mat.clone()),
                Transform::from_xyz(0.0, 0.50, -0.16)
                    .with_rotation(Quat::from_rotation_x(0.55))
                    .with_scale(Vec3::new(0.14, 0.12, 0.03)),
            ));
        }
        Profession::Miner => {
            // Safety hardhat — a flat cylinder instead of a hood.
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(head_mat),
                Transform::from_xyz(0.0, 0.60, 0.0).with_scale(Vec3::new(0.21, 0.10, 0.21)),
            ));
            // Pickaxe: wood handle + dark iron head.
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(assets.sawmill_roof_mat.clone()),
                Transform::from_xyz(0.0, 0.36, -0.10)
                    .with_rotation(Quat::from_rotation_x(0.55))
                    .with_scale(Vec3::new(0.045, 0.34, 0.045)),
            ));
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.furnace_stone_mat.clone()),
                Transform::from_xyz(0.0, 0.50, -0.16)
                    .with_rotation(Quat::from_rotation_x(0.55))
                    .with_scale(Vec3::new(0.18, 0.05, 0.05)),
            ));
        }
        Profession::Hunter => {
            p.spawn((
                Mesh3d(assets.cone.clone()),
                MeshMaterial3d(head_mat),
                Transform::from_xyz(0.0, 0.60, 0.0).with_scale(Vec3::new(0.22, 0.15, 0.22)),
            ));
            // Rifle slung diagonally across the back.
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(assets.sawmill_roof_mat.clone()),
                Transform::from_xyz(0.0, 0.40, -0.10)
                    .with_rotation(Quat::from_rotation_x(0.6))
                    .with_scale(Vec3::new(0.04, 0.46, 0.04)),
            ));
        }
        Profession::Farmer => {
            p.spawn((
                Mesh3d(assets.cone.clone()),
                MeshMaterial3d(head_mat),
                Transform::from_xyz(0.0, 0.60, 0.0).with_scale(Vec3::new(0.22, 0.15, 0.22)),
            ));
            // Wicker basket on the back with a greenhouse-green sprig.
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(assets.warehouse_plank_mat.clone()),
                Transform::from_xyz(0.0, 0.34, -0.14).with_scale(Vec3::new(0.16, 0.14, 0.16)),
            ));
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.greenhouse_glass_mat.clone()),
                Transform::from_xyz(0.0, 0.43, -0.14).with_scale(Vec3::new(0.08, 0.06, 0.08)),
            ));
        }
        Profession::Medic => {
            p.spawn((
                Mesh3d(assets.cone.clone()),
                MeshMaterial3d(head_mat),
                Transform::from_xyz(0.0, 0.60, 0.0).with_scale(Vec3::new(0.22, 0.15, 0.22)),
            ));
            // Red-cross chest badge — reuses the Hospital building's material.
            let cross = assets.hospital_cross_mat.clone();
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(cross.clone()),
                Transform::from_xyz(0.0, 0.36, 0.10).with_scale(Vec3::new(0.11, 0.03, 0.02)),
            ));
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(cross),
                Transform::from_xyz(0.0, 0.36, 0.10).with_scale(Vec3::new(0.03, 0.11, 0.02)),
            ));
        }
        Profession::Cook => {
            // White toque — a tall, narrow cylinder.
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(head_mat),
                Transform::from_xyz(0.0, 0.63, 0.0).with_scale(Vec3::new(0.16, 0.22, 0.16)),
            ));
            // Ladle at the hip: handle + bowl.
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(assets.sawmill_roof_mat.clone()),
                Transform::from_xyz(0.13, 0.28, 0.0)
                    .with_rotation(Quat::from_rotation_z(0.3))
                    .with_scale(Vec3::new(0.035, 0.22, 0.035)),
            ));
            p.spawn((
                Mesh3d(assets.sphere.clone()),
                MeshMaterial3d(assets.sawmill_blade_mat.clone()),
                Transform::from_xyz(0.16, 0.19, 0.0).with_scale(Vec3::splat(0.06)),
            ));
        }
        Profession::Tailor => {
            // Soft rounded cap (sphere, not the cone/cylinder/toque every
            // other trade uses) — reads as fabric, distinct silhouette.
            p.spawn((
                Mesh3d(assets.sphere.clone()),
                MeshMaterial3d(head_mat),
                Transform::from_xyz(0.0, 0.60, 0.0).with_scale(Vec3::new(0.20, 0.14, 0.20)),
            ));
            // Spool of thread at the hip: a short cylinder (spool body, the
            // Tailor Shop's own cloth material) crossed by a thin cylinder
            // (the needle, reusing the shared wood-tone handle material).
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(assets.tailor_cloth_mat.clone()),
                Transform::from_xyz(0.13, 0.26, 0.0).with_scale(Vec3::new(0.07, 0.10, 0.07)),
            ));
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(assets.sawmill_roof_mat.clone()),
                Transform::from_xyz(0.13, 0.26, 0.0)
                    .with_rotation(Quat::from_rotation_z(1.2))
                    .with_scale(Vec3::new(0.015, 0.20, 0.015)),
            ));
        }
    }
    }

    // Carried-resource prop (a hauled plank/log) — hidden until `sync_survivors`
    // shows it while the survivor is assigned to a building.
    p.spawn((
        Mesh3d(assets.cube.clone()),
        MeshMaterial3d(assets.warehouse_plank_mat.clone()),
        Transform::from_xyz(0.0, 0.40, 0.12).with_scale(Vec3::new(0.10, 0.06, 0.06)),
        Visibility::Hidden,
        SurvivorCarry { id },
    ));
}

/// Swings each `SurvivorLeg` into a walk cycle while its root survivor is
/// actively walking (`Wander::moving`), settling back to neutral otherwise —
/// the one bit of procedural "gait" standing in for the skeletal animation a
/// glTF rig would have carried. Legs are direct children of the
/// `SurvivorDot` root, so a single `ChildOf` hop finds `Wander`/`SurvivorRig`.
///
/// V0.16: while `SurvivorRig::sitting` (arrived at the Kitchen's dining
/// cluster during a meal window), both legs instead settle into a fixed
/// forward tilt — a low-poly "seated at the table" cue, in keeping with how
/// abstract the rest of this rig already is. Deliberately a partial tilt
/// (`SIT_TILT`, well short of the 90° a literal bent knee would need) since
/// each leg capsule rotates around its own center, not a hip joint — a full
/// right angle would swing half the capsule below the floor.
///
/// V0.17: while `SurvivorRig::sleeping` (asleep in a Tent bunk — see
/// `fc_game::sim::survivor_is_resting_with`), the same trick tilts further
/// still (`SLEEP_TILT`) than the seated pose, reading as reclined rather
/// than sitting up, checked ahead of `sitting` since the two never overlap
/// in practice (meal windows are daytime, bunks are night-only) but a lying
/// pose should win if they ever did.
pub fn animate_survivor_legs(
    time: Res<Time>,
    parents: Query<&ChildOf>,
    walkers: Query<(&Wander, &SurvivorRig), With<SurvivorDot>>,
    mut legs: Query<(Entity, &mut Transform, &SurvivorLeg)>,
) {
    const SWING_SPEED: f32 = 9.0;
    const MAX_SWING: f32 = 0.55;
    const SIT_TILT: f32 = 0.85;
    const SLEEP_TILT: f32 = 1.2;
    let t = time.elapsed_secs();
    let blend = 1.0 - (-10.0 * time.delta_secs()).exp();
    for (e, mut tr, leg) in &mut legs {
        let (moving, sitting, sleeping) = parents
            .get(e)
            .and_then(|co| walkers.get(co.parent()))
            .map(|(w, rig)| (w.moving, rig.sitting, rig.sleeping))
            .unwrap_or((false, false, false));
        let target = if sleeping {
            Quat::from_rotation_x(SLEEP_TILT)
        } else if sitting {
            Quat::from_rotation_x(SIT_TILT)
        } else if moving {
            Quat::from_rotation_x((t * SWING_SPEED + leg.phase).sin() * MAX_SWING)
        } else {
            Quat::IDENTITY
        };
        tr.rotation = tr.rotation.slerp(target, blend);
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
            w.moving = false;
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
            w.moving = true;
            w.shuffle_target = w.sim_pos;
            let np = pos.lerp(w.sim_pos, blend);
            t.translation.x = np.x;
            t.translation.z = np.z;
            // Model yurish yo'nalishiga yuzlanadi — silliq burilish, keskin
            // sakramaydi.
            let dir = w.sim_pos - pos;
            if dir.length() > 0.05 {
                let yaw = dir.x.atan2(dir.z);
                t.rotation = t.rotation.slerp(Quat::from_rotation_y(yaw), blend);
            }
        }
        // A tiny walking bob on the root — the legs carry most of the
        // motion now, this just keeps the body from looking perfectly rigid.
        t.translation.y = if w.moving {
            (time.elapsed_secs() * 9.0 + t.translation.x * 3.0).sin().abs() * 0.025
        } else {
            0.0
        };
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
