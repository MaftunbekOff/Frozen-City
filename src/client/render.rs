//! 3D world rendering: vertex-colored terrain, low-poly trees and rocks,
//! block-out buildings, day/night sun + fog, furnace light, snowfall and
//! co-op player cursors. All geometry is procedural — no asset files.
//!
//! The camera looks down at a tilt (2.5D feel) but the scene is true 3D;
//! the rig in `input.rs` allows rotation and zoom.

mod assets;
mod buildings;
mod components;
mod cursors;
mod effects;
mod meshes;
mod scene;
mod survivors;
mod terrain;

pub use assets::*;
pub use buildings::*;
pub use components::*;
pub use cursors::*;
pub use effects::*;
pub(crate) use meshes::*;
pub use scene::*;
pub use survivors::*;
pub use terrain::*;
