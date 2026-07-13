//! The map: terrain kinds and tiles.

use serde::{Deserialize, Serialize};

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
}
