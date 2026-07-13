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
}

#[derive(Resource, Default)]
pub struct BuildingViz(pub HashMap<u32, BuildingVizEntry>);

#[derive(Resource, Default)]
pub struct SurvivorViz(pub HashMap<u32, Entity>);

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

#[derive(Component)]
pub struct SurvivorDot {
    pub id: u32,
}

/// Which profession model variant this survivor root spawned with, plus the
/// sim facts the animation driver needs (`assets::SurvivorModels` is indexed
/// by `variant`). Lives on the same root entity as [`SurvivorDot`]; the
/// spawned `AnimationPlayer`s sit deep inside the glTF scene instance, so
/// `setup/drive_survivor_animations` walk up the parent chain to read this.
#[derive(Component)]
pub struct SurvivorRig {
    /// Index into `SurvivorModels::variants` (`Profession::ALL` order).
    pub variant: usize,
    /// Assigned to a building — walking survivors haul supplies
    /// (`Walk_Carry`) instead of strolling empty-handed.
    pub carrying: bool,
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
