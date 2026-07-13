//! Authoritative game server. Runs the simulation on its own thread at a fixed
//! 5 Hz tick and hands the local (in-process) player a plain channel pair —
//! one unified code path for singleplayer, host and join.
//!
//! A single TCP port speaks three protocols, told apart by the first bytes of
//! each connection: the native length-prefixed frame protocol, browser
//! WebSockets ("GET " + `Upgrade: websocket`), and plain HTTP GET for the
//! static web build (index.html + wasm) so a dedicated server is also the web
//! host.

mod config;
mod listener;
mod messages;
mod native;
mod ratelimit;
mod simloop;
mod util;
mod web;

pub use config::{connect_local, start, start_with_accounts, ServerConfig, ServerHandle, ToServer};
pub(crate) use config::join;
pub(crate) use simloop::sim_loop;

// Internal-only re-exports (module-private, i.e. visible within this
// `server` tree but not outside it, matching each item's original
// file-private visibility) so every submodule's `use super::*;` can reach
// its siblings, exactly as `crate::types`'s submodules do.
use config::*;
use listener::*;
use messages::*;
use native::*;
use ratelimit::*;
use util::*;
use web::*;
