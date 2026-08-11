use std::time::Duration;

use bevy::gltf::GltfMaterialName;
use bevy::prelude::*;

use frozen_city::game::sim;
use frozen_city::game::types::{xp_level, BuildingKind, Profession, TUNNEL_X, TUNNEL_Y};

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

/// The individual details a survivor is born with: skin tone, scarf color and
/// a slight difference in build. All three are derived from the sim id and
/// nothing else, so they survive a reconnect, agree between every client in a
/// co-op session, and never need storing or syncing.
///
/// Only these three vary. Coat and hat belong to the trade and hair/faces
/// aren't in the models, so without this every worker of a profession is a
/// pixel-perfect copy of the next — the single thing that most made a colony
/// read as clones rather than as people.
fn survivor_look(id: u32) -> (usize, usize, f32) {
    // One multiply-shift mix: consecutive ids must not land on neighbouring
    // tones, and survivors are handed out with sequential ids.
    let h = id.wrapping_mul(2_654_435_761) >> 8;
    let skin = (h % 4) as usize;
    let scarf = ((h >> 4) % 4) as usize;
    // ±6% of height. Enough to break up a crowd's silhouette, small enough
    // that nobody reads as a child or a giant.
    let build = 0.94 + ((h >> 8) % 13) as f32 * 0.01;
    (skin, scarf, build)
}

pub fn sync_survivors(
    mut commands: Commands,
    view: Res<GameView>,
    assets: Res<GameAssets>,
    models: Res<SurvivorModels>,
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
        // No gender field in the sim — split deterministically on id parity
        // (same trick `Wander::speed` uses below for per-survivor variance)
        // so a given survivor keeps the same model across snapshots.
        let gender = (s.id % 2) as usize;
        let coat = if is_leader {
            assets.leader_coat_mat.clone()
        } else {
            assets.survivor_coat_mats[variant].clone()
        };
        let hood = assets.survivor_head_mats[variant].clone();
        let (tone, scarf, build) = survivor_look(s.id);
        // Root sits at ground level (feet), unlike the old single-capsule
        // dot which stored its own center height — every body part below
        // is positioned as an absolute height from the ground instead.
        let e = commands
            .spawn((
                Transform::from_translation(pos),
                Visibility::Inherited,
                SurvivorDot { id: s.id },
                SurvivorRig {
                    gender,
                    carrying: s.assigned_building.is_some(),
                    sitting,
                    sleeping,
                },
                SurvivorSkin {
                    coat,
                    hood,
                    skin: assets.survivor_skin_mats[tone].clone(),
                    scarf: assets.survivor_scarf_mats[scarf].clone(),
                },
                SurvivorProps {
                    profession: s.profession,
                    variant,
                    gender,
                    is_leader,
                },
                Wander {
                    sim_pos: pos,
                    ground_speed: 0.0,
                    moving: false,
                },
                DespawnOnExit(Screen::Game),
            ))
            .with_children(|p| {
                spawn_survivor_body(p, &assets, &models, gender, build, s.id);

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
            h.healthy.clone()
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
    models: Res<SurvivorModels>,
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
        let gender = (i as usize + m.expires as usize) % 2;
        // Travellers have no sim id to key off, so seed the look from the
        // batch's own identity — stable for as long as the batch is pending,
        // which is all these cosmetic figures need.
        let (tone, scarf, build) = survivor_look(i.wrapping_mul(31) ^ m.expires as u32);
        let e = commands
            .spawn((
                Transform::from_translation(pos),
                Visibility::Inherited,
                // No `SurvivorDot`/`Wander` — these are static placeholders,
                // not simulated survivors — but they still need `SurvivorRig`
                // (gender, for the animation graph) and `SurvivorSkin` (coat/
                // hood tint) since `setup_survivor_animations`/
                // `fixup_survivor_materials` look for those on the ancestor
                // chain the same way real survivors' bodies do.
                SurvivorRig {
                    gender,
                    carrying: false,
                    sitting: false,
                    sleeping: false,
                },
                SurvivorSkin {
                    coat: assets.survivor_coat_mats[variant].clone(),
                    hood: assets.survivor_head_mats[variant].clone(),
                    skin: assets.survivor_skin_mats[tone].clone(),
                    scarf: assets.survivor_scarf_mats[scarf].clone(),
                },
                SurvivorProps {
                    profession,
                    variant,
                    gender,
                    is_leader: false,
                },
                DespawnOnExit(Screen::Game),
            ))
            .with_children(|p| {
                spawn_survivor_body(p, &assets, &models, gender, build, 0);
            })
            .id();
        viz.entities.push(e);
    }
}

/// Uniform scale on each model's `WorldAssetRoot` bringing standing height to
/// ~0.62 world units — the old procedural rig's height (head top at y≈0.615),
/// so the root-anchored props still tuned for that rig (XP gear, carry plank)
/// keep landing where they did, and camera/building framing needs no retuning.
///
/// Height is the glb's raw mesh bbox (male 0.678, female 0.627 — the female
/// is proportionally shorter in the source art, hence the larger multiplier).
/// Note the `*_Rig` node's own 0.377 scale does NOT compound here: a skinned
/// mesh is drawn through `joint_global * inverse_bind`, and since the export's
/// inverse binds were taken from those same globals, everything the rig node
/// contributes cancels — vertices land at their authored coordinates, scaled
/// only by whatever sits ABOVE the glTF scene. Getting this wrong is why the
/// first pass drew survivors ~2.7× oversized.
const MALE_MODEL_SCALE: f32 = 0.915;
const FEMALE_MODEL_SCALE: f32 = 0.988;

/// The `*_Rig` node's baked scale. It cancels out for the drawn mesh (above)
/// but NOT for the bone entities themselves — those are ordinary children in
/// the transform hierarchy, so a bone's world scale really is
/// `model_scale * RIG_NODE_SCALE`. [`Socket`] divides it back out.
const RIG_NODE_SCALE: f32 = 0.376_710_7;

/// A bone-anchored attachment point. Spawned as a child of a skeleton bone
/// with a transform that cancels that bone's rest orientation and inherited
/// scale, so the frame it hands to ITS children is world-axis-aligned and
/// measured in plain world units — letting prop geometry be written with the
/// same numbers the old root-anchored rig used, while still riding the bone
/// through every animation (a tool in `hand_r` swings with the arm).
///
/// `offset` is in bone-local units and `unrotate` is the inverse of the
/// bone's rest-pose world rotation; both were read straight out of the two
/// glb files (see the `head`/`hand_r` rows below) rather than dialled in by
/// eye, so they hold exactly at bind pose.
struct Socket {
    offset: Vec3,
    unrotate: Quat,
}

impl Socket {
    /// The bone-child transform: land on the socket point, undo the bone's
    /// rest rotation, and undo its inherited scale.
    fn transform(&self, model_scale: f32) -> Transform {
        Transform {
            translation: self.offset,
            rotation: self.unrotate,
            scale: Vec3::splat(1.0 / (model_scale * RIG_NODE_SCALE)),
        }
    }
}

/// `head` socket, sitting just clear of the top of the skull — hats hang off
/// this, so `y = 0` there is the crown of the head.
const MALE_HEAD_SOCKET: Socket = Socket {
    offset: Vec3::new(0.0, 0.2255, 0.0095),
    unrotate: Quat::from_xyzw(0.0210, 0.0, 0.0, 0.9998),
};
const FEMALE_HEAD_SOCKET: Socket = Socket {
    offset: Vec3::new(0.0, 0.2198, 0.0115),
    unrotate: Quat::from_xyzw(0.0262, 0.0, 0.0, 0.9997),
};
/// `hand_r` socket, right at the grip — `y = 0` is the hand itself, so a
/// tool's geometry runs upward out of the fist.
const MALE_HAND_SOCKET: Socket = Socket {
    offset: Vec3::ZERO,
    unrotate: Quat::from_xyzw(0.5096, 0.2763, 0.6864, -0.4391),
};
const FEMALE_HAND_SOCKET: Socket = Socket {
    offset: Vec3::ZERO,
    unrotate: Quat::from_xyzw(0.4840, 0.2683, 0.7278, -0.4050),
};

/// How far up the ancestor chain the per-instance lookups walk. A bone sits
/// as deep as `head` -> neck -> 3×spine -> pelvis -> Root -> rig -> glTF
/// scene root -> `WorldAssetRoot` -> survivor root (ten hops), so this is
/// deliberately well clear of that rather than exactly it.
const RIG_ANCESTOR_HOPS: usize = 24;

/// Blend time between clips in `drive_survivor_animations`. Long enough that
/// an arriving survivor eases out of the walk cycle rather than snapping to
/// a stand; the walk/idle flip itself is kept rare by the hysteresis in
/// `animate_survivors`, since restarting a crossfade every few frames reads
/// as a stutter no matter how smooth one blend is.
const CLIP_CROSSFADE: Duration = Duration::from_millis(320);

/// Ground speed, in world units per second, that the `Walk` clip is authored
/// for. Derived from the rig rather than guessed: the clip is a 1.0 s cycle
/// and the thigh swings ±28°, which on this ~0.32-unit leg covers about
/// 0.30 units per step, so ~0.60 units per two-step cycle.
/// `drive_survivor_animations` scales playback by the ratio of real speed to
/// this, which is what keeps the feet planted.
///
/// Note how far apart the two numbers are: the sim walks survivors at
/// `SURVIVOR_SPEED_TILES_PER_SEC` (2.5), so a body barely 0.62 units tall
/// crosses four of its own lengths every second and the clip has to run at
/// roughly 4× to keep up. That is why the playback clamp reaches as high as
/// it does — anything lower and the feet skate no matter how smooth the
/// interpolation is.
const WALK_CLIP_STRIDE_SPEED: f32 = 0.60;

/// Seated pose, applied to the leg bones by `pose_resting_survivors`. Both
/// joints turn about their own local X — the only axis the Walk clip uses
/// for them — and the signs come from walking the rig forward-kinematically:
/// −80° at the thigh swings the foot forward (+z, the direction the models
/// face), so the hip flexes rather than hyper-extending backwards.
const SIT_THIGH: f32 = -1.31; // ~75° of hip flexion: thigh out level
const SIT_CALF: f32 = 1.40; // ~80° back under it, so the shin hangs down
/// How far the root drops so the folded legs still reach the ground: the
/// pelvis sits at 0.318 in the standing pose and about stool height when sat.
const SIT_DROP: f32 = 0.15;

/// Builds one skinned winter survivor as children of the just-spawned
/// `SurvivorDot` root: the gender's glTF body (`SurvivorModels::male`/
/// `female`, picked by `gender`) and the hauled-plank prop.
///
/// Nothing a survivor *wears or holds* is spawned here. Hats and tools hang
/// off skeleton bones instead, so they ride the animation rather than
/// floating beside a walking body — and those bone entities don't exist yet
/// on this frame, since a `WorldAssetRoot` populates asynchronously. See
/// [`attach_survivor_props`], which owns both the timing and the per-trade
/// table. The coat/hood tint has the same "wait for the instance" problem
/// and is handled by [`fixup_survivor_materials`].
fn spawn_survivor_body(
    p: &mut ChildSpawnerCommands,
    assets: &GameAssets,
    models: &SurvivorModels,
    gender: usize,
    build: f32,
    id: u32,
) {
    let (model, scale) = if gender == 0 {
        (&models.male, MALE_MODEL_SCALE)
    } else {
        (&models.female, FEMALE_MODEL_SCALE)
    };
    // `build` is this survivor's own ±6% (see `survivor_look`). It scales the
    // body only; the hat and tool still ride bone sockets sized off the
    // gender's BASE scale, so they inherit the difference automatically —
    // a taller survivor gets a proportionally larger hat rather than the
    // same hat perched oddly on a bigger head.
    p.spawn((
        WorldAssetRoot(model.scene.clone()),
        Transform::from_scale(Vec3::splat(scale * build)),
    ));

    // Carried-resource prop (a hauled plank/log), held out at chest height in
    // both hands — hidden until `sync_survivors` shows it while the survivor
    // is assigned to a building. Anchored to the root rather than a hand
    // bone precisely because it reads as two-handed: pinning it to one fist
    // would swing a plank through the body on every arm swing.
    p.spawn((
        Mesh3d(assets.cube.clone()),
        MeshMaterial3d(assets.warehouse_plank_mat.clone()),
        Transform::from_xyz(0.0, 0.40, 0.12).with_scale(Vec3::new(0.10, 0.06, 0.06)),
        Visibility::Hidden,
        SurvivorCarry { id },
    ));
}

/// Which bone [`attach_survivor_props`] just found, and therefore which half
/// of the per-trade kit to hang off it.
#[derive(Clone, Copy)]
enum PropSlot {
    /// `head` — the trade's hat.
    Head,
    /// `hand_r` — the trade's tool.
    Hand,
}

/// Hangs each survivor's hat and tool off the matching skeleton bone as soon
/// as the glTF scene instance spawns it. Bone entities are named after their
/// glTF nodes (`head`, `hand_r`), so `Added<Name>` is the arrival signal;
/// survivors are the only animated scene this client instantiates, so the
/// filter needs no further narrowing. The bone doesn't carry the survivor's
/// identity, so a bounded walk up `ChildOf` finds the ancestor
/// [`SurvivorProps`] — same convention as `setup_survivor_animations` and
/// `fixup_survivor_materials`, just with a taller hop budget
/// ([`RIG_ANCESTOR_HOPS`]) because bones sit deep in the skeleton.
///
/// Each prop goes under a [`Socket`] child rather than straight onto the
/// bone: that hands the geometry below a world-aligned, world-scaled frame,
/// so the numbers here stay in plain world units instead of being expressed
/// in some bone's rest orientation.
pub fn attach_survivor_props(
    mut commands: Commands,
    assets: Res<GameAssets>,
    parents: Query<&ChildOf>,
    roots: Query<&SurvivorProps>,
    bones: Query<(Entity, &Name, &Transform), Added<Name>>,
) {
    for (bone, name, bone_tr) in &bones {
        // The leg joints carry no props — they are tagged for
        // `pose_resting_survivors`, which needs their authored rest rotation
        // and a direct line back to the survivor root. Snapshotting the
        // rotation here is safe precisely because Idle never moves them.
        let joint = match name.as_str() {
            "thigh_l" | "thigh_r" => Some(RestJoint::Thigh),
            "calf_l" | "calf_r" => Some(RestJoint::Calf),
            _ => None,
        };
        let slot = match name.as_str() {
            "head" => Some(PropSlot::Head),
            "hand_r" => Some(PropSlot::Hand),
            _ => None,
        };
        if joint.is_none() && slot.is_none() {
            continue;
        }
        let mut cur = bone;
        let mut found = None;
        for _ in 0..RIG_ANCESTOR_HOPS {
            let Ok(child_of) = parents.get(cur) else { break };
            cur = child_of.parent();
            if let Ok(props) = roots.get(cur) {
                found = Some((*props, cur));
                break;
            }
        }
        let Some((props, root)) = found else { continue };
        if let Some(joint) = joint {
            commands.entity(bone).insert(RestBone {
                root,
                rest: bone_tr.rotation,
                joint,
            });
            continue;
        }
        let Some(slot) = slot else { continue };
        let (socket, model_scale) = match (slot, props.gender) {
            (PropSlot::Head, 0) => (&MALE_HEAD_SOCKET, MALE_MODEL_SCALE),
            (PropSlot::Head, _) => (&FEMALE_HEAD_SOCKET, FEMALE_MODEL_SCALE),
            (PropSlot::Hand, 0) => (&MALE_HAND_SOCKET, MALE_MODEL_SCALE),
            (PropSlot::Hand, _) => (&FEMALE_HAND_SOCKET, FEMALE_MODEL_SCALE),
        };
        commands.entity(bone).with_children(|p| {
            p.spawn((socket.transform(model_scale), Visibility::Inherited))
                .with_children(|p| match slot {
                    PropSlot::Head => spawn_headwear(p, &assets, props),
                    PropSlot::Hand => spawn_tool(p, &assets, props),
                });
        });
    }
}

/// The trade's headwear, in the `head` [`Socket`]'s frame: `y = 0` is the
/// crown of the skull, `+y` is straight up in world space. Everyone is
/// bundled up against the cold, and the hat's silhouette is what makes a
/// trade readable at this camera distance — the coat tint only reinforces
/// it. Colors come from `profession_head_color` via
/// `GameAssets::survivor_head_mats`, shared so same-trade hats still batch.
///
/// Sizes are calibrated against the HEAD, not the body: the skull is only
/// ~0.095 units across (the model's `fc_hood` bbox at
/// [`MALE_MODEL_SCALE`]), so a brim reads as a brim at roughly 0.17–0.22 and
/// anything near the 0.62 body height swallows the character whole.
fn spawn_headwear(p: &mut ChildSpawnerCommands, assets: &GameAssets, props: SurvivorProps) {
    if props.is_leader {
        // A crown instead of the trade's hat, echoing `survivor_contribution`'s
        // sim-side rule that the leader is a generalist while they hold the
        // seat rather than still being whatever they were before.
        p.spawn((
            Mesh3d(assets.cylinder.clone()),
            MeshMaterial3d(assets.leader_crown_mat.clone()),
            Transform::from_xyz(0.0, 0.020, 0.0).with_scale(Vec3::new(0.125, 0.045, 0.125)),
            LeaderCrown,
        ));
        return;
    }
    let head_mat = assets.survivor_head_mats[props.variant].clone();
    match props.profession {
        Profession::Lumberjack => {
            // Fur-trimmed ushanka: a thick band under a rounded crown.
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(head_mat.clone()),
                Transform::from_xyz(0.0, 0.008, 0.0).with_scale(Vec3::new(0.140, 0.032, 0.140)),
            ));
            p.spawn((
                Mesh3d(assets.sphere.clone()),
                MeshMaterial3d(head_mat),
                Transform::from_xyz(0.0, 0.034, 0.0).with_scale(Vec3::new(0.128, 0.090, 0.128)),
            ));
        }
        Profession::Miner => {
            // Safety hardhat: a low dome with a short brim over the eyes.
            p.spawn((
                Mesh3d(assets.sphere.clone()),
                MeshMaterial3d(head_mat.clone()),
                Transform::from_xyz(0.0, 0.012, 0.0).with_scale(Vec3::new(0.125, 0.085, 0.125)),
            ));
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(head_mat),
                Transform::from_xyz(0.0, 0.006, 0.052).with_scale(Vec3::new(0.115, 0.014, 0.060)),
            ));
        }
        Profession::Hunter => {
            // Full brimmed hat — the widest silhouette of the seven.
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(head_mat.clone()),
                Transform::from_xyz(0.0, 0.006, 0.0).with_scale(Vec3::new(0.200, 0.014, 0.200)),
            ));
            p.spawn((
                Mesh3d(assets.cone.clone()),
                MeshMaterial3d(head_mat),
                Transform::from_xyz(0.0, 0.048, 0.0).with_scale(Vec3::new(0.120, 0.090, 0.120)),
            ));
        }
        Profession::Farmer => {
            // Straw hat: wider brim than the hunter's, flatter crown.
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(head_mat.clone()),
                Transform::from_xyz(0.0, 0.005, 0.0).with_scale(Vec3::new(0.220, 0.012, 0.220)),
            ));
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(head_mat),
                Transform::from_xyz(0.0, 0.030, 0.0).with_scale(Vec3::new(0.115, 0.055, 0.115)),
            ));
        }
        Profession::Medic => {
            // Close pale cap, no brim — reads as clinical next to the
            // weather-beaten trades.
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(head_mat),
                Transform::from_xyz(0.0, 0.020, 0.0).with_scale(Vec3::new(0.120, 0.050, 0.120)),
            ));
        }
        Profession::Cook => {
            // Toque: a band at the brow with a puffed crown above it. Taller
            // than the other hats, but wider than it is tall — a narrow
            // column the width of the skull just read as a stovepipe.
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(head_mat.clone()),
                Transform::from_xyz(0.0, 0.010, 0.0).with_scale(Vec3::new(0.118, 0.026, 0.118)),
            ));
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(head_mat),
                Transform::from_xyz(0.0, 0.058, 0.0).with_scale(Vec3::new(0.132, 0.082, 0.132)),
            ));
        }
        Profession::Tailor => {
            // Soft rounded cap (a squashed sphere, not the cone/cylinder
            // every other trade uses) — reads as fabric.
            p.spawn((
                Mesh3d(assets.sphere.clone()),
                MeshMaterial3d(head_mat),
                Transform::from_xyz(0.0, 0.020, 0.0).with_scale(Vec3::new(0.125, 0.090, 0.125)),
            ));
        }
    }
}

/// The trade's tool, in the `hand_r` [`Socket`]'s frame: `y = 0` is the fist
/// itself and `+y` is straight up in world space, so a tool's shaft runs up
/// out of the grip and swings with the arm. Tool materials deliberately
/// reuse a workplace building's own material (the lumberjack's axe blade is
/// `sawmill_blade_mat`, the tailor's spool is `tailor_cloth_mat`) so a
/// survivor visually echoes the building they work at.
fn spawn_tool(p: &mut ChildSpawnerCommands, assets: &GameAssets, props: SurvivorProps) {
    // Shared wood tone behind every tool handle in the colony.
    let handle = assets.sawmill_roof_mat.clone();
    if props.is_leader {
        // Ceremonial staff, long enough to plant on the ground, capped with
        // the XP-tier gold so it reads as "distinguished" without a new
        // material.
        p.spawn((
            Mesh3d(assets.cylinder.clone()),
            MeshMaterial3d(handle),
            Transform::from_xyz(0.0, -0.05, 0.0).with_scale(Vec3::new(0.035, 0.62, 0.035)),
        ));
        p.spawn((
            Mesh3d(assets.sphere.clone()),
            MeshMaterial3d(assets.tier_flag_mats[2].clone()),
            Transform::from_xyz(0.0, 0.275, 0.0).with_scale(Vec3::splat(0.07)),
        ));
        return;
    }
    match props.profession {
        Profession::Lumberjack => {
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(handle),
                Transform::from_xyz(0.0, 0.05, 0.0).with_scale(Vec3::new(0.04, 0.34, 0.04)),
            ));
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.sawmill_blade_mat.clone()),
                Transform::from_xyz(0.0, 0.20, 0.03).with_scale(Vec3::new(0.045, 0.13, 0.10)),
            ));
        }
        Profession::Miner => {
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(handle),
                Transform::from_xyz(0.0, 0.05, 0.0).with_scale(Vec3::new(0.04, 0.34, 0.04)),
            ));
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.furnace_stone_mat.clone()),
                Transform::from_xyz(0.0, 0.21, 0.0).with_scale(Vec3::new(0.04, 0.05, 0.20)),
            ));
        }
        Profession::Hunter => {
            // Rifle carried muzzle-up, angled off the shoulder line.
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(handle.clone()),
                Transform::from_xyz(0.0, 0.10, -0.04)
                    .with_rotation(Quat::from_rotation_x(0.35))
                    .with_scale(Vec3::new(0.036, 0.50, 0.036)),
            ));
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.furnace_stone_mat.clone()),
                Transform::from_xyz(0.0, -0.06, 0.01)
                    .with_rotation(Quat::from_rotation_x(0.35))
                    .with_scale(Vec3::new(0.045, 0.14, 0.06)),
            ));
        }
        Profession::Farmer => {
            // Wicker basket carried at the hip, a greenhouse-green sprig
            // poking out of it.
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(assets.warehouse_plank_mat.clone()),
                Transform::from_xyz(0.0, -0.02, 0.02).with_scale(Vec3::new(0.16, 0.13, 0.16)),
            ));
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.greenhouse_glass_mat.clone()),
                Transform::from_xyz(0.0, 0.06, 0.02).with_scale(Vec3::new(0.08, 0.06, 0.08)),
            ));
        }
        Profession::Medic => {
            // Medical case with the Hospital's own red cross on its lid.
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(assets.warehouse_plank_mat.clone()),
                Transform::from_xyz(0.0, -0.05, 0.02).with_scale(Vec3::new(0.13, 0.10, 0.08)),
            ));
            let cross = assets.hospital_cross_mat.clone();
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(cross.clone()),
                Transform::from_xyz(0.0, -0.05, 0.061).with_scale(Vec3::new(0.08, 0.025, 0.02)),
            ));
            p.spawn((
                Mesh3d(assets.cube.clone()),
                MeshMaterial3d(cross),
                Transform::from_xyz(0.0, -0.05, 0.061).with_scale(Vec3::new(0.025, 0.07, 0.02)),
            ));
        }
        Profession::Cook => {
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(handle),
                Transform::from_xyz(0.0, 0.04, 0.0).with_scale(Vec3::new(0.03, 0.24, 0.03)),
            ));
            p.spawn((
                Mesh3d(assets.sphere.clone()),
                MeshMaterial3d(assets.sawmill_blade_mat.clone()),
                Transform::from_xyz(0.0, 0.17, 0.0).with_scale(Vec3::splat(0.07)),
            ));
        }
        Profession::Tailor => {
            // Spool of dyed wool crossed by a needle.
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(assets.tailor_cloth_mat.clone()),
                Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::new(0.08, 0.11, 0.08)),
            ));
            p.spawn((
                Mesh3d(assets.cylinder.clone()),
                MeshMaterial3d(handle),
                Transform::from_xyz(0.0, 0.0, 0.0)
                    .with_rotation(Quat::from_rotation_z(1.2))
                    .with_scale(Vec3::new(0.016, 0.22, 0.016)),
            ));
        }
    }
}

/// Wires a freshly-spawned `AnimationPlayer` (buried a few nodes deep inside
/// a survivor's just-instantiated `WorldAssetRoot` scene) to its gender's
/// animation graph and starts it on `Idle`. A global `Added<AnimationPlayer>`
/// filter is enough — survivors/migrants are the only thing in this client
/// that spawns an animated scene — but the player itself doesn't carry
/// `SurvivorRig`, so a bounded walk up `ChildOf` finds the ancestor that
/// does (same trick `drive_survivor_animations` and
/// `fixup_survivor_materials` use for their own per-instance lookups).
pub fn setup_survivor_animations(
    mut commands: Commands,
    models: Res<SurvivorModels>,
    rigs: Query<&SurvivorRig>,
    parents: Query<&ChildOf>,
    mut players: Query<(Entity, &mut AnimationPlayer), Added<AnimationPlayer>>,
) {
    for (e, mut player) in &mut players {
        let mut cur = e;
        let mut gender = None;
        for _ in 0..8 {
            let Ok(co) = parents.get(cur) else { break };
            cur = co.parent();
            if let Ok(rig) = rigs.get(cur) {
                gender = Some(rig.gender);
                break;
            }
        }
        let Some(gender) = gender else { continue };
        let model = if gender == 0 { &models.male } else { &models.female };
        let mut transitions = AnimationTransitions::new();
        transitions.play(&mut player, model.idle, Duration::ZERO).repeat();
        commands
            .entity(e)
            .insert((AnimationGraphHandle(model.graph.clone()), transitions));
    }
}

/// Picks the clip each survivor's `AnimationPlayer` should be playing —
/// `Walk` while `Wander::moving`, `Work` while `SurvivorRig::sitting` (V0.16:
/// arrived at the Kitchen's dining cluster; the rig has no dedicated sit
/// clip, so `Work`'s hands-busy loop is the closest stand-in), `Idle`
/// otherwise (including while `sleeping` — see `animate_survivors`'s root
/// tilt for that case, there's no lying-down clip either) — and crossfades
/// to it when it changes. `AnimationPlayer`/`AnimationTransitions` sit deep
/// inside the glTF scene instance, so a bounded walk up `ChildOf` finds the
/// `SurvivorRig` that has the facts (same convention as
/// `setup_survivor_animations`); migrants have no `Wander` (they never
/// walk/sit), hence `Option<&Wander>`.
pub fn drive_survivor_animations(
    parents: Query<&ChildOf>,
    roots: Query<(Option<&Wander>, &SurvivorRig)>,
    models: Res<SurvivorModels>,
    mut players: Query<(Entity, &mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    for (e, mut player, mut transitions) in &mut players {
        let mut cur = e;
        let mut found = None;
        for _ in 0..8 {
            let Ok(co) = parents.get(cur) else { break };
            cur = co.parent();
            if let Ok((w, rig)) = roots.get(cur) {
                found = Some((w, rig));
                break;
            }
        }
        let Some((w, rig)) = found else { continue };
        let model = if rig.gender == 0 { &models.male } else { &models.female };
        let moving = w.map(|w| w.moving).unwrap_or(false);
        // Seated survivors keep Idle playing under the pose
        // `pose_resting_survivors` folds onto their legs — Idle leaves the
        // legs completely static, so the two never fight and the upper body
        // keeps breathing instead of freezing solid. Someone standing at
        // their post inside a workshop gets the Work loop; you only see it
        // with that building selected (`hide_indoor_survivors`), which is
        // exactly when it's worth having.
        let want = if moving {
            model.walk
        } else if rig.sitting || rig.sleeping {
            model.idle
        } else if rig.carrying {
            model.work
        } else {
            model.idle
        };
        if transitions.get_main_animation() != Some(want) {
            transitions
                .play(&mut player, want, CLIP_CROSSFADE)
                .repeat();
        }
        // Match the stride to the ground actually covered. Without this the
        // clip runs at its authored cadence no matter how fast the body is
        // travelling, which is what made survivors look like they were
        // skating — slow deliberate steps under a fast-sliding body.
        if let Some(active) = player.animation_mut(want) {
            let speed = if moving {
                let measured = w.map(|w| w.ground_speed).unwrap_or(0.0);
                (measured / WALK_CLIP_STRIDE_SPEED).clamp(0.55, 4.5)
            } else {
                1.0
            };
            active.set_speed(speed);
        }
    }
}

/// A survivor's glTF scene instance populates its mesh-primitive children
/// asynchronously — they don't exist the same frame `WorldAssetRoot` is
/// spawned — so retinting the model's coat/hood (per profession/leadership,
/// see [`SurvivorSkin`]) has to happen here, once each primitive actually
/// shows up. `bevy_gltf` tags every primitive entity with `GltfMaterialName`
/// (the glTF material's authored name) the frame it's spawned, which is all
/// the identification needed: `fc_coat`/`fc_hood` get retargeted to the
/// ancestor `SurvivorSkin`'s handles, and `fc_skin` gets tagged
/// `SurvivorHead` so `sync_survivors`'s existing sick-tint loop picks it up
/// exactly like it did the old rig's separate head sphere — no changes
/// needed there. `fc_leather`/`fc_scarf` are left as authored (accent
/// colors every survivor shares). Same bounded-parent-walk convention as
/// `setup_survivor_animations`/`drive_survivor_animations`.
pub fn fixup_survivor_materials(
    mut commands: Commands,
    parents: Query<&ChildOf>,
    skins: Query<&SurvivorSkin>,
    dots: Query<&SurvivorDot>,
    mut new_parts: Query<
        (Entity, &GltfMaterialName, &mut MeshMaterial3d<StandardMaterial>),
        Added<GltfMaterialName>,
    >,
) {
    for (e, name, mut mat) in &mut new_parts {
        let mut cur = e;
        let mut skin = None;
        let mut id = None;
        for _ in 0..8 {
            let Ok(co) = parents.get(cur) else { break };
            cur = co.parent();
            if skin.is_none() {
                skin = skins.get(cur).ok();
            }
            if id.is_none() {
                id = dots.get(cur).ok().map(|d| d.id);
            }
            if skin.is_some() && id.is_some() {
                break;
            }
        }
        let Some(skin) = skin else { continue };
        match name.0.as_str() {
            "fc_coat" => mat.0 = skin.coat.clone(),
            "fc_hood" => mat.0 = skin.hood.clone(),
            "fc_scarf" => mat.0 = skin.scarf.clone(),
            "fc_skin" => {
                mat.0 = skin.skin.clone();
                if let Some(id) = id {
                    commands.entity(e).insert(SurvivorHead {
                        id,
                        healthy: skin.skin.clone(),
                    });
                }
            }
            _ => {}
        }
    }
}

/// Drives each survivor's root: walk toward the sim's authoritative position,
/// face the way they are going, and settle into a resting pose once there.
///
/// There is deliberately no idle wander any more. The old rig drifted a
/// standing survivor around a ±0.3-tile circle so they would not look frozen,
/// but under a real skeleton that reads as pacing on the spot — the body
/// slides while the feet do nothing. Standing still and letting Idle's
/// breathing carry the life is calmer and more honest about what the sim says
/// is happening; the genuine rest states (a meal, a bunk) get real poses
/// instead — the lie-down below, and the seated fold in
/// [`pose_resting_survivors`].
pub fn animate_survivors(
    time: Res<Time>,
    mut q: Query<(&mut Transform, &mut Wander, Option<&SurvivorRig>)>,
) {
    // Hysteresis around the arrival radius. `Wander::moving` picks the
    // Walk/Idle clip, and a survivor loitering right at one threshold would
    // otherwise flip every few frames, restarting a `CLIP_CROSSFADE` each
    // time — visibly juddery. Start walking past the outer edge, keep walking
    // until well inside the inner one.
    const START_WALK: f32 = 0.30;
    const STOP_WALK: f32 = 0.10;
    let dt = time.delta_secs();
    let blend = 1.0 - (-6.0 * dt).exp();
    for (mut t, mut w, rig) in &mut q {
        let pos = Vec3::new(t.translation.x, 0.0, t.translation.z);
        let goal_dist = pos.distance(w.sim_pos);
        w.moving = if w.moving { goal_dist > STOP_WALK } else { goal_dist > START_WALK };

        // Same exponential smoothing `sync_player_cursors`/`sync_avatars` use
        // for remote cursors. It keeps closing on the goal even once stopped,
        // so a survivor settles exactly where the sim says rather than near it.
        let np = pos.lerp(w.sim_pos, blend);
        t.translation.x = np.x;
        t.translation.z = np.z;

        // Measure what the body actually covered this frame and low-pass it so
        // `drive_survivor_animations` can pace the stride to match. Taken from
        // the real translation rather than from `sim_pos`, because the lerp
        // above makes the two differ exactly when it matters most: right after
        // a snapshot moves the goal.
        let instant = if dt > 0.0 { np.distance(pos) / dt } else { 0.0 };
        w.ground_speed += (instant - w.ground_speed) * (1.0 - (-8.0 * dt).exp());

        if w.moving {
            let dir = w.sim_pos - pos;
            if dir.length() > 0.05 {
                let yaw = dir.x.atan2(dir.z);
                t.rotation = t.rotation.slerp(Quat::from_rotation_y(yaw), blend);
            }
        }

        // Resting. Sleeping lays the whole body over: the rig has no
        // lying-down clip, and tipping the root reads far better than anything
        // the three shipped clips could fake. Sitting only drops the hips to
        // stool height here — the legs are folded after the animation systems
        // have run, since they would otherwise overwrite the pose.
        let (sitting, sleeping) = rig.map(|r| (r.sitting, r.sleeping)).unwrap_or((false, false));
        let yaw = Quat::from_rotation_y(t.rotation.to_euler(EulerRot::YXZ).0);
        if sleeping && !w.moving {
            // Tipping about the feet lays the body flat; lift it by half its
            // own thickness so it rests ON the bunk rather than half inside it.
            t.rotation = yaw * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
            t.translation.y = 0.07;
        } else if sitting && !w.moving {
            t.rotation = yaw;
            t.translation.y = -SIT_DROP;
        } else {
            t.rotation = yaw;
            // The old rig faked a walking bob here; the real Walk clip carries
            // its own vertical rhythm at the bone level, so the root stays put.
            t.translation.y = 0.0;
        }
    }
}

/// Folds a resting survivor's legs into a seated pose, on top of whatever the
/// Idle clip just wrote. Runs in `PostUpdate` after Bevy's `AnimationSystems`
/// and before transform propagation — the only window where a bone's local
/// rotation can be overridden and still reach the screen this frame.
///
/// Only the legs are touched, and only while seated: Idle leaves them
/// completely static (0° of travel across its whole loop), so nothing is
/// being fought over, and the upper body keeps breathing normally. A
/// standing or walking survivor is left entirely to the clip.
pub fn pose_resting_survivors(
    rigs: Query<(&SurvivorRig, &Wander)>,
    mut bones: Query<(&RestBone, &mut Transform)>,
) {
    for (bone, mut tr) in &mut bones {
        let Ok((rig, wander)) = rigs.get(bone.root) else { continue };
        if !rig.sitting || wander.moving {
            continue;
        }
        let bend = match bone.joint {
            RestJoint::Thigh => SIT_THIGH,
            RestJoint::Calf => SIT_CALF,
        };
        tr.rotation = bone.rest * Quat::from_rotation_x(bend);
    }
}

/// Hides survivors who have walked inside a roofed building, and shows them
/// again on the way out.
///
/// Interiors are drawn as a "look inside" view that only appears while the
/// building is selected (`render::buildings`' `BuildingRoof`/
/// `BuildingInterior` pair), so an unselected workshop is a closed box — and
/// a survivor standing at their station inside it pushed their head, hat and
/// tool straight through the roof. Rather than clip them, drop them: they are
/// indoors, so not seeing them is correct. Selecting the building lifts its
/// roof AND brings its workers back, which is what makes the dollhouse view
/// worth opening.
///
/// Only kinds that actually have a roof count. The Furnace and Tunnel are
/// open fixtures, and Wall/Gate/Well are single tiles with nothing to stand
/// inside, so a survivor next to any of them stays visible.
pub fn hide_indoor_survivors(
    view: Res<GameView>,
    selection: Res<Selection>,
    mut q: Query<(&SurvivorDot, &SurvivorRig, &Wander, &mut Visibility)>,
) {
    let Some(state) = view.ready() else { return };
    for (dot, rig, wander, mut vis) in &mut q {
        let Some(s) = state.survivors.iter().find(|s| s.id == dot.id) else { continue };
        // Standing on a tile is not the same as being indoors. Someone
        // crossing a plot, or hauling timber to a half-built one, is out in
        // the open and must stay on screen — only a survivor who has settled
        // into the building disappears: asleep in a bunk, sat down to a meal,
        // or arrived and working their post.
        let settled = rig.sleeping
            || rig.sitting
            || (!wander.moving && s.assigned_building.is_some());
        let indoors = settled
            && state.buildings.iter().any(|b| {
                // A scaffold has no roof to hide behind, and a selected
                // building has had its roof lifted for the interior view —
                // in both cases the player is meant to see who's in there.
                if selection.0 == Some(b.id)
                    || b.under_construction()
                    || !building_has_roof(b.kind)
                {
                    return false;
                }
                let (w, h) = b.kind.size();
                s.x >= b.x as f32
                    && s.x < (b.x + w) as f32
                    && s.y >= b.y as f32
                    && s.y < (b.y + h) as f32
            });
        let want = if indoors { Visibility::Hidden } else { Visibility::Inherited };
        if *vis != want {
            *vis = want;
        }
    }
}

/// Whether this kind is drawn as a closed, roofed volume you can stand inside
/// — every room (anything with fittings) plus the Tent, whose solid prism has
/// no interior at all.
fn building_has_roof(kind: BuildingKind) -> bool {
    kind == BuildingKind::Tent || !kind.furnishings().is_empty()
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
