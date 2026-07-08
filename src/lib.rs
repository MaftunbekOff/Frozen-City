//! Frozen City — core library: pure simulation + networking.
//! Deliberately free of any Bevy dependency so it can be unit-tested,
//! run as a dedicated server, and later ported to WASM.

pub mod game;
pub mod net;
