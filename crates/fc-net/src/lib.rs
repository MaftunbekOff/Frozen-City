#[cfg(not(target_arch = "wasm32"))]
pub mod accounts;
pub mod client;
#[cfg(not(target_arch = "wasm32"))]
pub mod legacy;
/// V0.18 global market — SQLite-backed, so native server only (the in-browser
/// local sim has no accounts DB and no other players to trade with).
#[cfg(not(target_arch = "wasm32"))]
pub mod market;
#[cfg(not(target_arch = "wasm32"))]
pub mod persist;
pub mod protocol;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;
#[cfg(not(target_arch = "wasm32"))]
pub mod telemetry;
#[cfg(target_arch = "wasm32")]
pub mod ws;
#[cfg(not(target_arch = "wasm32"))]
pub mod world_manager;
