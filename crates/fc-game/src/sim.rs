//! Pure, deterministic simulation. No Bevy, no I/O — the server thread calls
//! `tick` / `apply_command`, tests call them directly.

mod mapgen;
mod text;
mod players;
mod command;
mod tick;

pub use mapgen::*;
pub use text::*;
pub use players::*;
pub use command::*;
pub use tick::*;
