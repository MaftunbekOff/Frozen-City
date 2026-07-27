//! The map: terrain kinds and tiles.

use serde::{Deserialize, Serialize};

use super::{ROAD_SNOW_PENALTY, SNOW_MAX, SNOW_SLOWEST_FACTOR, TILE_ROAD_SPEED};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terrain {
    Snow,
    Forest,
    Coal,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Tile {
    pub terrain: Terrain,
    /// Remaining harvestable units (wood for Forest, coal for Coal).
    pub deposit: u16,
    /// V0.19: a laid road surface. Roads never block anything and produce
    /// nothing — what they buy is SPEED (see [`Tile::speed_factor`]): walking
    /// a cleared road is far quicker than wading through drift. A road that
    /// fills with snow stops helping until someone clears it, which is what
    /// "the roads are closed" means here.
    ///
    /// IMPORTANT: bincode serializes this struct positionally, and there are
    /// `MAP_W * MAP_H` of them in every snapshot — new fields stay APPENDED at
    /// the end (same rule as `Survivor`/`GameState`), and old saves load
    /// through `legacy::TileV15`.
    pub road: bool,
    /// V0.19: how deep the snow lies here, 0..=[`SNOW_MAX`]. Stored as a byte
    /// rather than an f32 because there is one per tile and the whole grid
    /// rides along in periodic snapshots — a `u8` keeps that cheap, and the
    /// resolution (one unit of depth) is finer than anything the player can
    /// perceive.
    pub snow: u8,
}

impl Tile {
    /// V0.19: how fast a survivor crosses this tile, as a multiple of
    /// `SURVIVOR_SPEED_TILES_PER_SEC`.
    ///
    /// Snow **never blocks** — it only costs. That is a deliberate invariant,
    /// not an accident: the map has no pathfinder (see the "no obstacles"
    /// note in `types.rs`), so an impassable tile could strand a survivor
    /// between their bunk and their post with no way for the player to see
    /// why. Cost is always finite, so every tile is always reachable and a
    /// blizzard makes the colony slow and miserable rather than broken.
    pub fn speed_factor(&self) -> f32 {
        // A road buried past `ROAD_SNOW_PENALTY` has stopped being a road —
        // it degrades toward the same crawl as open ground, and clearing it
        // is what brings it back.
        let depth = self.snow as f32 / SNOW_MAX as f32;
        let open = 1.0 - depth * (1.0 - SNOW_SLOWEST_FACTOR);
        if self.road {
            let buried = (self.snow as f32 / ROAD_SNOW_PENALTY as f32).clamp(0.0, 1.0);
            // Full speed on a clear road, sliding back toward plain open
            // ground as it fills in.
            open.max(TILE_ROAD_SPEED - buried * (TILE_ROAD_SPEED - open))
        } else {
            open
        }
    }

    /// V0.19: relative cost of stepping onto this tile — the reciprocal of
    /// [`Self::speed_factor`], which is what `sim::tick`'s road-following step
    /// compares between neighbours. Never infinite (see above).
    pub fn move_cost(&self) -> f32 {
        1.0 / self.speed_factor().max(0.01)
    }

    /// V0.19: true once this tile's snow has reached the depth at which a
    /// road under it stops helping — what the client paints as "closed".
    pub fn road_is_buried(&self) -> bool {
        self.road && self.snow >= ROAD_SNOW_PENALTY
    }
}
