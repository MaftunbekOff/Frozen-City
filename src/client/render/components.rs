use std::collections::HashMap;

use bevy::prelude::*;

use crate::client::*;

#[derive(Resource, Default)]
pub struct TerrainViz {
    pub ground: Option<Entity>,
    pub trees: Option<Entity>,
    pub rocks: Option<Entity>,
    pub cache: Vec<Tile>,
    pub seen_tiles_version: u64,
}

/// Bitta binoning sahnadagi ifodasi + qaysi (daraja, qurilish-holati)
/// juftligi uchun qurilgani. V0.8: daraja yoki qurilish holati o'zgarsa
/// `sync_buildings` eski entity'ni buzib, yangisini quradi — bino ko'rinishi
/// darajaga qarab o'zgaradi.
pub struct BuildingVizEntry {
    pub entity: Entity,
    pub level: u8,
    pub under_construction: bool,
    /// `GameState.furnace_level` at spawn time — only meaningful for the
    /// Furnace (0 for every other kind), which regrows its log-teepee
    /// structure a tier at a time as this rises (see `spawn_building`'s
    /// `BuildingKind::Furnace` arm). Tracked here so `sync_buildings`
    /// rebuilds the furnace when the player changes its burn level, even
    /// though `level`/`under_construction` didn't change.
    pub furnace_level: u8,
    /// `GameState.tunnel` at spawn time, encoded as 0 = not yet unlocked
    /// (sealed mound) or `1 + stage` while unlocked (0..=3 excavation
    /// stages) — only meaningful for the Tunnel (0 for every other kind).
    /// Same rebuild-on-change trick as `furnace_level`.
    pub tunnel_stage: u8,
}

#[derive(Resource, Default)]
pub struct BuildingViz(pub HashMap<u32, BuildingVizEntry>);

#[derive(Resource, Default)]
pub struct SurvivorViz(pub HashMap<u32, SurvivorVizEntry>);

/// Traveler figures waiting at the Tunnel (`GameState.pending_migrant`) —
/// purely cosmetic placeholders, not simulated `Survivor`s (no id in
/// `state.survivors`, no XP/carry props). `sync_migrants` rebuilds the whole
/// set whenever the pending batch's identity (`count`, `expires`) changes,
/// the same trigger-on-change trick `BuildingViz` uses.
#[derive(Resource, Default)]
pub struct MigrantViz {
    pub entities: Vec<Entity>,
    pub key: Option<(u32, u64)>,
}

/// `sync_survivors` rebuilds a survivor's whole body when `is_leader`
/// changes (appointment, succession, or losing the seat) — the trade-vs-
/// leader look swaps entire meshes (coat color, headwear, tool vs.
/// ceremonial staff), not just a material tweak, so it's simplest to
/// despawn and respawn like `BuildingVizEntry` does for the Furnace.
pub struct SurvivorVizEntry {
    pub entity: Entity,
    pub is_leader: bool,
}

/// Aholining XP-daraja anjomi (V0.8 ko'rinish darajalari): `level` — shu
/// anjom ko'rinishi uchun zarur minimal `xp_level` (1 = peshona tasmasi,
/// 2 = qalpoq, 3 = oltin ko'krak nishoni). `sync_survivors` har snapshotda
/// ko'rinishini yangilaydi; anjomlar kumulyativ ko'rinadi.
#[derive(Component)]
pub struct SurvivorGear {
    pub id: u32,
    pub level: u8,
}


/// Remote player cursors: world marker + screen-space name label.
#[derive(Resource, Default)]
pub struct CursorViz(pub HashMap<u64, (Entity, Entity)>);

/// Live map pings, keyed by (player id, creation tick, x-bits, y-bits) so two
/// pings a player drops within the same tick stay distinct.
#[derive(Resource, Default)]
pub struct PingViz(pub HashMap<(u64, u64, u32, u32), Entity>);

/// Central-world light avatars: one body + one screen-space name label per
/// connected player (including the local player — everyone sees everyone,
/// unlike `CursorViz` which skips "me"). Only populated while
/// `GameState.central` is true; `sync_player_cursors` keeps its normal
/// marker-cursor behavior in every other world unchanged.
#[derive(Resource, Default)]
pub struct AvatarViz(pub HashMap<u64, (Entity, Entity)>);

// --------------------------------------------------------------- components

#[derive(Component)]
pub struct SunLight;

#[derive(Component)]
pub struct BuildingMarker;

/// Small cube on a building's roof; visible while `index < workers`.
#[derive(Component)]
pub struct WorkerCube {
    pub building: u32,
    pub index: u8,
}

/// On the furnace root: handle to the animated fire material.
#[derive(Component)]
pub struct FurnaceGlow {
    pub fire_mat: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct FurnaceLight;

/// V0.16: on a Kitchen's dining-cluster campfire, handle to the animated
/// flame material — same idea as `FurnaceGlow`, but a distinct type so its
/// flicker (`animate_meal_fire`) is independent of the player's Furnace
/// on/off state (`animate_effects`'s `FurnaceGlow` loop).
#[derive(Component)]
pub struct MealFireGlow {
    pub fire_mat: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct MealFireLight;

#[derive(Component)]
pub struct SurvivorDot {
    pub id: u32,
}

/// Sim facts the procedural body needs after spawn. Lives on the same root
/// entity as [`SurvivorDot`]. There's no profession field — a survivor's
/// trade never changes after spawn (assigned once from the sim RNG), so the
/// coat/headwear/tool are picked once in `spawn_survivor_body` and never
/// need to be looked up again.
#[derive(Component)]
pub struct SurvivorRig {
    /// Assigned to a building — shows the carried-resource prop
    /// (`SurvivorCarry`) while walking instead of strolling empty-handed.
    pub carrying: bool,
    /// V0.16: arrived at the Kitchen's dining cluster during a meal window
    /// (`fc_game::sim::survivor_is_at_meal`) — `animate_survivor_legs` folds
    /// this into a seated pose instead of the idle/walk cycle. Render-only;
    /// never affects sim state.
    pub sitting: bool,
}

/// One of a survivor's two legs — a direct child of the [`SurvivorDot`]
/// root, rotated by `animate_survivor_legs` into a walk-cycle swing while
/// `Wander::moving` is true. `phase` offsets the two legs by half a cycle so
/// they alternate instead of moving in lockstep.
#[derive(Component)]
pub struct SurvivorLeg {
    pub phase: f32,
}

/// The small carried-resource prop shown while the owning survivor's
/// `SurvivorRig::carrying` is true (mirrors [`SurvivorGear`]'s
/// id-keyed visibility toggle, driven from `sync_survivors`).
#[derive(Component)]
pub struct SurvivorCarry {
    pub id: u32,
}

/// Drives a survivor entity toward the sim-authoritative position
/// (`Survivor.x/y`, see `types.rs`'s V0.7 doc comment), smoothed with an
/// exponential lerp — the same pattern `sync_player_cursors`/`sync_avatars`
/// use for remote cursors/avatars. Once within `shuffle_target` of that sim
/// goal (i.e. the survivor is standing still, not actively walking), a tiny
/// ±0.3-tile shuffle keeps them from looking frozen — purely cosmetic, never
/// fights the authoritative position.
#[derive(Component)]
pub struct Wander {
    /// Latest sim position (world space), refreshed every snapshot.
    pub sim_pos: Vec3,
    /// Small cosmetic shuffle offset from `sim_pos`, active only once the
    /// entity has visually caught up to `sim_pos`.
    pub shuffle_target: Vec3,
    pub speed: f32,
    /// Actively walking toward `sim_pos` this frame (as opposed to the idle
    /// shuffle) — read by `animate_survivor_legs` to gate the walk-cycle
    /// swing so standing survivors don't shuffle their feet.
    pub moving: bool,
}

#[derive(Component)]
pub struct CursorMarker {
    pub player: u64,
}

/// Tags the root of a co-op map ping (identity is tracked in [`PingViz`]).
#[derive(Component)]
pub struct PingMarker;

/// UI text node that follows a remote player's cursor on screen.
#[derive(Component)]
pub struct CursorLabel {
    pub player: u64,
}

/// Central-world light-avatar body: walks (lerps) toward the owning
/// player's synced cursor tile. Distinct from `CursorMarker` (which floats a
/// marker cone above the cursor in every other world) and from `Wander`
/// (survivor idle wandering around a fixed home) — an avatar has neither a
/// fixed home nor autonomous wandering, it only ever chases the live cursor.
#[derive(Component)]
pub struct AvatarWalk {
    pub player: u64,
    pub target: Vec3,
}

/// UI text node that follows a central-world avatar's name on screen —
/// same projection idea as `CursorLabel`, kept as a separate type so the two
/// systems' queries stay disjoint (Bevy 0.19 ECS safety: no query ever needs
/// `Without<CursorLabel>` just to also see `AvatarLabel`, they're unrelated).
#[derive(Component)]
pub struct AvatarLabel {
    pub player: u64,
}

#[derive(Component)]
pub struct HeatRing {
    pub mat: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct SelectionRing;

/// Ring hovering under the selected survivor (`roster::SurvivorSelection`),
/// distinct from `SelectionRing` (buildings) so the two never fight over the
/// same component in a query — see the module's "one query conflict per new
/// system" rule.
#[derive(Component)]
pub struct SurvivorSelectionRing;

/// Small low-poly crown worn by the appointed leader (`GameState.leader`).
/// One instance is spawned as a child of the leader's `SurvivorDot` entity
/// and moved to whichever survivor is currently leader on change.
#[derive(Component)]
pub struct LeaderCrown;

/// A brief expanding ring dropped at a `MoveSurvivor` destination as visual
/// confirmation the command was sent — purely cosmetic, self-despawns after
/// `MOVE_PING_LIFETIME` seconds.
#[derive(Component)]
pub struct MoveOrderPing {
    pub age: f32,
}

pub const MOVE_PING_LIFETIME: f32 = 0.6;

#[derive(Component)]
pub struct GhostMarker {
    pub mat: Handle<StandardMaterial>,
}

#[derive(Component)]
pub struct Snowflake {
    pub fall: f32,
    pub drift: f32,
    pub phase: f32,
}

/// A looping puff rising from the furnace chimney while it burns.
#[derive(Component)]
pub struct Smoke {
    pub phase: f32,
}

/// Newly-spawned buildings scale up from nothing for tactile placement feedback.
#[derive(Component)]
pub struct SpawnGrow {
    pub age: f32,
}

/// Full-screen cold haze that fades in during a blizzard.
#[derive(Component)]
pub struct BlizzardOverlay;
