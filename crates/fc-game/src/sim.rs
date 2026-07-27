//! Pure, deterministic simulation. No Bevy, no I/O — the server thread calls
//! `tick` / `apply_command`, tests call them directly.

mod mapgen;
mod text;
mod players;
mod command;
mod tick;
// V0.18: each new mechanic owns its own module rather than growing `tick.rs`
// further — `tick` calls into them at a single marked hook point each.
pub mod expedition;
pub mod lawbook;
pub mod lifecycle;
pub mod missions;
pub mod snow;

pub use mapgen::*;
pub use text::*;
pub use players::*;
pub use command::*;
pub use tick::*;
