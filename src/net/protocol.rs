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
    /// Must be the first message on every connection. `token` carries a session
    /// token from a previous `Welcome` to resume the same identity (reconnect);
    /// `None` for a fresh join.
    Hello { name: String, token: Option<u64> },
    Cmd(PlayerCommand),
    /// Cursor position in world tile coordinates, for co-op presence.
    Cursor { x: f32, y: f32 },
    /// A line of co-op text chat.
    Chat { text: String },
    /// Drop a transient map marker at world tile coordinates (Alt+click).
    Ping { x: f32, y: f32 },
    /// Owner-only: change what guests are allowed to do. Ignored from guests.
    SetGuestPermission { perm: crate::game::types::GuestPermission },
    /// Owner-only: remove a guest from the world by player id. Ignored from guests.
    Kick { target: u64 },
    /// Alternative to `Hello` as the first message on a connection: sign in
    /// with a login/password minted by the registration bot instead of a
    /// free-text guest name. `token` carries a session token from a previous
    /// `Welcome`, same meaning as on `Hello`.
    Login {
        login: String,
        password: String,
        token: Option<u64>,
    },
    /// Third possible first message: sign in and enter the CENTRAL world (the
    /// Global World through the Tunnel) instead of this account's personal
    /// world. Only graduated accounts are admitted; on first entry the server
    /// migrates a group of survivors out of the personal world to be this
    /// account's settlers. `token` reconnects an interrupted central-world
    /// session, same meaning as on `Hello`/`Login`.
    /// (This and everything below it: appended in order, never reordered —
    /// bincode enum indices are positional.)
    EnterCentral {
        login: String,
        password: String,
        token: Option<u64>,
    },
    /// Fourth possible first message: create an account right from the client
    /// (no Telegram bot needed) and, on success, sign straight into the new
    /// account's personal world. Refused (as `AuthFailed`, with the reason)
    /// when the login/display name is taken or fails validation.
    Register {
        login: String,
        password: String,
        /// Desired display name (unique, like the bot enforces).
        name: String,
    },
    /// Fifth possible first message: sign in and join a FRIEND's personal
    /// world as a guest. Admitted only while a fresh invite from `host`
    /// (`ClientMsg::Invite`, issued in the central world) is standing AND the
    /// host is currently online in their world. `token` reconnects an
    /// interrupted visit.
    VisitFriend {
        login: String,
        password: String,
        /// Account id of the world's owner (from `ServerMsg::Invited`).
        host: i64,
        token: Option<u64>,
    },
    /// Nearby chat: delivered as a transient `ServerMsg::Bubble` to players
    /// whose presence is within earshot, instead of the persistent world-wide
    /// chat log. The speaker's position is their last cursor/avatar position.
    ChatLocal { text: String },
    /// Add `name` (a display name) to the sender's friends list. Answered
    /// with a refreshed `ServerMsg::Social`.
    AddFriend { name: String },
    /// Remove an account from the sender's friends list. Answered with a
    /// refreshed `ServerMsg::Social`.
    RemoveFriend { account: i64 },
    /// Ask for a fresh `ServerMsg::Social` (e.g. to poll who's online).
    RefreshSocial,
    /// Central world only: invite another account (who must be online in the
    /// central world right now) to visit the sender's personal world. The
    /// target receives `ServerMsg::Invited`; the standing invite then admits
    /// their `VisitFriend` for a while.
    Invite { account: i64 },
}

/// One friend row in `ServerMsg::Social`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FriendInfo {
    pub account: i64,
    pub name: String,
    /// True when this friend is connected to the central world right now.
    /// Only meaningful when the receiving client is in the central world
    /// itself (elsewhere the server reports `false` — it has no global view).
    pub online_central: bool,
}

/// Which of `GameState`'s pricier collection fields actually ride along on a
/// given `State` message. A field flagged `false` is empty on the wire (same
/// trick the original tiles throttle used) and the client keeps its last
/// known copy instead. Only collections that are genuinely quiet most ticks
/// are worth this — `buildings`/`survivors`/`stock` drift essentially every
/// tick from continuous production/hunger/decay, so skip-when-unchanged
/// would almost never trigger for them and isn't applied there.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub struct Included {
    pub tiles: bool,
    pub events: bool,
    pub chat: bool,
    pub pings: bool,
    pub missions: bool,
    pub techs: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ServerMsg {
    Welcome {
        player_id: u64,
        /// Session token to send back in a future `Hello` to reconnect as the
        /// same player (preserving id, color and contribution stats).
        token: u64,
        state: GameState,
    },
    State { state: GameState, included: Included },
    /// Sent instead of `Welcome` when a `Login` first message had a bad
    /// login/password; the connection is closed right after.
    AuthFailed { reason: String },
    /// Transient nearby-chat line (see `ClientMsg::ChatLocal`): rendered as a
    /// short-lived speech bubble / tagged chat line, never stored in the
    /// world snapshot. (Appended: bincode enum indices are positional.)
    Bubble {
        player_id: u64,
        name: String,
        color: u8,
        text: String,
    },
    /// The sender's friends list, refreshed after every Add/Remove/Refresh
    /// and once on join for account sessions.
    Social { friends: Vec<FriendInfo> },
    /// Someone in the central world invited this client to visit their
    /// personal world — follow up with `VisitFriend { host, .. }`.
    Invited { host: i64, host_name: String },
}

/// Deflate level: fast enough to run every tick on a shared box, still gets
/// most of the win on the repetitive tile/struct data that dominates a
/// snapshot (see `encode`'s doc comment). Levels above ~6 buy little extra
/// ratio here for a lot more CPU.
const COMPRESSION_LEVEL: u8 = 6;

/// Bincode-serializes then deflates `msg`. Snapshots are dominated by
/// repetitive data (the tile grid, near-identical `Building`/`Survivor`
/// entries) that deflate shrinks a lot; tiny messages (a `Cmd`, a cursor
/// update) still round-trip correctly, just with a few bytes of fixed
/// deflate overhead.
pub fn encode<T: Serialize>(msg: &T) -> io::Result<Vec<u8>> {
    let bytes =
        bincode::serialize(msg).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(miniz_oxide::deflate::compress_to_vec(&bytes, COMPRESSION_LEVEL))
}

/// Inverse of [`encode`]. `max_decompressed` bounds the inflated size so a
/// malicious/corrupt peer can't zip-bomb the other side into exhausting memory.
pub fn decode<T: DeserializeOwned>(bytes: &[u8], max_decompressed: usize) -> io::Result<T> {
    let raw = miniz_oxide::inflate::decompress_to_vec_with_limit(bytes, max_decompressed)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "decompress failed"))?;
    bincode::deserialize(&raw).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn write_frame<W: Write, T: Serialize>(w: &mut W, msg: &T) -> io::Result<()> {
    let bytes = encode(msg)?;
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
    // Decompressed size is unbounded by the wire length (that's the whole
    // point of compression), so bound it independently at MAX_FRAME — a
    // legitimate snapshot decompresses to well under that.
    decode(&buf, MAX_FRAME as usize)
}
