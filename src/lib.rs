//! Frozen City — core library: pure simulation + networking.
//! Re-exports the `fc-game` (pure simulation) and `fc-net` (protocol/server/
//! persistence) workspace crates as `game`/`net`, so every existing
//! `frozen_city::game::...` / `frozen_city::net::...` path keeps working
//! unchanged. Both crates are deliberately free of any Bevy dependency so
//! they can be unit-tested, run as a dedicated server, and later ported to
//! WASM.

pub use fc_game as game;
pub use fc_net as net;
