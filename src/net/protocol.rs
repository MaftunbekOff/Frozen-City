//! Wire protocol: bincode messages behind a 4-byte little-endian length prefix,
//! carried over TCP (or in-memory channels for the local player).

use std::io::{self, Read, Write};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::game::types::{GameState, PlayerCommand};

/// Hard cap on a single frame; a full snapshot is ~40 KB, so this is generous.
pub const MAX_FRAME: u32 = 8 * 1024 * 1024;

/// How often full tile data rides along with the snapshot (every Nth tick).
/// Part of the protocol contract: every server implementation (threaded native
/// server, in-browser local sim) follows the same cadence.
pub const TILES_EVERY_N_TICKS: u64 = 5;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ClientMsg {
    /// Must be the first message on every connection.
    Hello { name: String },
    Cmd(PlayerCommand),
    /// Cursor position in world tile coordinates, for co-op presence.
    Cursor { x: f32, y: f32 },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ServerMsg {
    Welcome {
        player_id: u64,
        state: GameState,
    },
    State {
        state: GameState,
        /// When false, `state.tiles` is empty and the client should keep the
        /// tile grid from the last snapshot that included it.
        tiles_included: bool,
    },
}

pub fn write_frame<W: Write, T: Serialize>(w: &mut W, msg: &T) -> io::Result<()> {
    let bytes = bincode::serialize(msg)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    if bytes.len() as u64 > MAX_FRAME as u64 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
    }
    w.write_all(&(bytes.len() as u32).to_le_bytes())?;
    w.write_all(&bytes)?;
    w.flush()
}

pub fn read_frame<R: Read, T: DeserializeOwned>(r: &mut R) -> io::Result<T> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf);
    if len > MAX_FRAME {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "frame too large"));
    }
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf)?;
    bincode::deserialize(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
