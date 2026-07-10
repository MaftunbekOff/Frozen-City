//! Saves/loads the authoritative `GameState` to disk so a dedicated,
//! persistent-mode server keeps players' cities across a restart — the
//! periodic systemd restart (`RuntimeMaxSec`) and a `deploy.sh` swap
//! (`systemctl stop`/`start`) would otherwise wipe the whole world every
//! time. Server-side only: the wasm client never touches a filesystem.

use std::fs;
use std::io;
use std::path::Path;

use crate::game::types::GameState;

/// Where the dedicated server's world is saved between restarts. Overridable
/// via `FC_WORLD_SAVE` (same variable name pattern as `accounts::DEFAULT_DB_PATH`),
/// mainly so tests can point at a throwaway path instead of the real one.
pub const DEFAULT_SAVE_PATH: &str = "/var/lib/frozen-city/world.bin";

fn resolve_path() -> String {
    std::env::var("FC_WORLD_SAVE").unwrap_or_else(|_| DEFAULT_SAVE_PATH.to_string())
}

/// Serializes `state` and writes it to the save path. Best-effort by design:
/// a failure here never means the live game is lost, since the in-memory
/// `state` the caller holds is untouched either way — callers just log it.
pub fn save(state: &GameState) -> io::Result<()> {
    save_at(state, &resolve_path())
}

/// Loads a previously saved `GameState`. Every failure mode — missing file,
/// truncated/corrupt data — collapses to `None`, so a fresh boot and a
/// corrupted save both fall back the same way: a new `sim::new_game()`.
pub fn load() -> Option<GameState> {
    load_at(&resolve_path())
}

fn save_at(state: &GameState, path: &str) -> io::Result<()> {
    if let Some(dir) = Path::new(path).parent() {
        fs::create_dir_all(dir)?;
    }
    let bytes =
        bincode::serialize(state).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    // Write to a sibling temp file and rename over the target: a kill mid-write
    // (the exact moment this feature exists to survive) leaves the previous,
    // still-valid save in place instead of a truncated, unloadable one — a
    // same-filesystem rename is atomic.
    let tmp = format!("{path}.tmp");
    fs::write(&tmp, &bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn load_at(path: &str) -> Option<GameState> {
    let bytes = fs::read(path).ok()?;
    bincode::deserialize(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::sim;

    fn throwaway_path(name: &str) -> String {
        std::env::temp_dir()
            .join(format!("fc-persist-test-{name}.bin"))
            .to_str()
            .unwrap()
            .to_string()
    }

    #[test]
    fn roundtrip_preserves_state() {
        let path = throwaway_path("roundtrip");
        let mut state = sim::new_game(7, 12);
        sim::player_joined(&mut state, 1, "Aziz");
        save_at(&state, &path).unwrap();
        let loaded = load_at(&path).expect("loads back");
        assert_eq!(state, loaded);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn missing_file_returns_none() {
        assert!(load_at("/nonexistent/frozen-city-test-dir/world.bin").is_none());
    }

    #[test]
    fn corrupt_file_returns_none() {
        let path = throwaway_path("corrupt");
        fs::write(&path, b"not a valid bincode payload").unwrap();
        assert!(load_at(&path).is_none());
        fs::remove_file(&path).ok();
    }

    #[test]
    fn save_creates_missing_parent_directory() {
        let dir = std::env::temp_dir().join("fc-persist-test-newdir");
        fs::remove_dir_all(&dir).ok();
        let path = dir.join("world.bin");
        let state = sim::new_game(1, 5);
        save_at(&state, path.to_str().unwrap()).unwrap();
        assert!(path.exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_save_leaves_no_tmp_file_behind() {
        let path = throwaway_path("no-tmp-leftover");
        let state = sim::new_game(2, 5);
        save_at(&state, &path).unwrap();
        assert!(!Path::new(&format!("{path}.tmp")).exists());
        fs::remove_file(&path).ok();
    }
}
