//! Saves/loads the authoritative `GameState` to disk so a dedicated,
//! persistent-mode server keeps players' cities across a restart — the
//! periodic systemd restart (`RuntimeMaxSec`) and a `deploy.sh` swap
//! (`systemctl stop`/`start`) would otherwise wipe the whole world every
//! time. Server-side only: the wasm client never touches a filesystem.

use std::fs;
use std::io;
use std::path::Path;

use fc_game::types::GameState;

/// Where the dedicated server's world is saved between restarts. Overridable
/// via `FC_WORLD_SAVE` (same variable name pattern as `accounts::DEFAULT_DB_PATH`),
/// mainly so tests can point at a throwaway path instead of the real one.
pub const DEFAULT_SAVE_PATH: &str = "/var/lib/frozen-city/world.bin";

/// Version header every save is written with since the format grew fields
/// (V2: `Survivor::owner`, `PlayerInfo::account`, `GameState::central`; V3:
/// `Building::owner_account`, `GameState::central_ledger`). Bincode is
/// positional, so without a header there'd be no way to tell one version's
/// file from another's — and misreading one version as another would
/// "collapse to None" in `load_at` and silently wipe a production world on
/// the first deploy after a format change. Files WITHOUT any recognized
/// prefix are decoded through the frozen V1 mirror (`legacy.rs`) and
/// migrated all the way to V3; files with `MAGIC_V2` decode through the V2
/// mirror and migrate one hop to V3.
const MAGIC_V2: &[u8; 8] = b"FCWORLD2";
const MAGIC_V3: &[u8; 8] = b"FCWORLD3";

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

/// Exposed to `world_manager` so it can save/load each account's world at
/// its own path, independent of the single shared-world path above — and to
/// e2e tests, which pre-seed world files (e.g. a graduated personal world)
/// before starting a server against them.
pub fn save_at(state: &GameState, path: &str) -> io::Result<()> {
    if let Some(dir) = Path::new(path).parent() {
        fs::create_dir_all(dir)?;
    }
    let body =
        bincode::serialize(state).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut bytes = Vec::with_capacity(MAGIC_V3.len() + body.len());
    bytes.extend_from_slice(MAGIC_V3);
    bytes.extend_from_slice(&body);
    // Write to a sibling temp file and rename over the target: a kill mid-write
    // (the exact moment this feature exists to survive) leaves the previous,
    // still-valid save in place instead of a truncated, unloadable one — a
    // same-filesystem rename is atomic.
    let tmp = format!("{path}.tmp");
    fs::write(&tmp, &bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

pub fn load_at(path: &str) -> Option<GameState> {
    let bytes = fs::read(path).ok()?;
    if let Some(body) = bytes.strip_prefix(MAGIC_V3.as_slice()) {
        return bincode::deserialize(body).ok();
    }
    if let Some(body) = bytes.strip_prefix(MAGIC_V2.as_slice()) {
        // V2 (pre-account-ownership/contribution-ledger): decode through the
        // frozen V2 mirror and migrate one hop to V3. The next autosave
        // rewrites it as V3.
        return bincode::deserialize::<crate::legacy::GameStateV2>(body)
            .ok()
            .map(GameState::from);
    }
    // No recognized header: a save written before versioning existed at
    // all — decode it through the frozen V1 mirror and migrate all the way
    // to V3. The next autosave rewrites it as V3.
    bincode::deserialize::<crate::legacy::GameStateV1>(&bytes)
        .ok()
        .map(GameState::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_game::sim;

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

    /// A world saved by a pre-versioning binary (raw V1 bincode, no magic
    /// header) must load and migrate, not collapse to None — None here means
    /// a production city silently wiped on the first post-format-change boot.
    #[test]
    fn v1_save_without_header_migrates() {
        use crate::legacy::{BuildingV2, GameStateV1, PlayerInfoV1, SurvivorV1};

        let path = throwaway_path("v1-migrate");
        // Fabricate V1 bytes exactly the way an old binary wrote them: a
        // plain bincode `GameState` (V1 layout), no header. Build the V1
        // struct from a current state so every shared field is realistic.
        let mut modern = sim::new_game(11, 12);
        sim::player_joined(&mut modern, 7, "Aziz");
        modern.graduated = true;
        let v1 = GameStateV1 {
            tick: modern.tick,
            win_days: modern.win_days,
            tiles: modern.tiles.clone(),
            buildings: modern
                .buildings
                .iter()
                .map(|b| BuildingV2 {
                    id: b.id,
                    kind: b.kind,
                    x: b.x,
                    y: b.y,
                    workers: b.workers,
                    progress: b.progress,
                    owner: b.owner,
                })
                .collect(),
            survivors: modern
                .survivors
                .iter()
                .map(|s| SurvivorV1 {
                    id: s.id,
                    name: s.name.clone(),
                    hp: s.hp,
                    hunger: s.hunger,
                    assigned_building: s.assigned_building,
                })
                .collect(),
            stock: modern.stock,
            furnace_level: modern.furnace_level,
            furnace_lit: modern.furnace_lit,
            cold_snap: modern.cold_snap,
            players: modern
                .players
                .iter()
                .map(|p| PlayerInfoV1 {
                    id: p.id,
                    name: p.name.clone(),
                    color: p.color,
                    cursor: p.cursor,
                    built: p.built,
                    demolished: p.demolished,
                    role: p.role,
                })
                .collect(),
            phase: modern.phase,
            events: modern.events.clone(),
            total_events: modern.total_events,
            chat: modern.chat.clone(),
            total_chat: modern.total_chat,
            pings: modern.pings.clone(),
            missions: modern.missions.clone(),
            tunnel: modern.tunnel,
            graduated: modern.graduated,
            techs: modern.techs.clone(),
            disease_until: modern.disease_until,
            blizzard_until: modern.blizzard_until,
            pending_event: modern.pending_event,
            event_rng: modern.event_rng,
            guest_perm: modern.guest_perm,
            owner_id: modern.owner_id,
            next_id: modern.next_id,
            rng: modern.rng,
        };
        fs::write(&path, bincode::serialize(&v1).unwrap()).unwrap();

        let loaded = load_at(&path).expect("V1 save must migrate, never wipe");
        assert_eq!(loaded.survivors.len(), modern.survivors.len());
        assert!(loaded.survivors.iter().all(|s| s.owner.is_none()));
        assert!(loaded.players.iter().all(|p| p.account.is_none()));
        assert!(!loaded.central);
        assert!(loaded.graduated, "graduation must survive migration");
        assert_eq!(loaded.tiles, modern.tiles);
        assert_eq!(loaded.stock, modern.stock);
        assert!(
            loaded.buildings.iter().all(|b| b.owner_account.is_none()),
            "V1 predates account-based building ownership"
        );
        assert!(loaded.central_ledger.is_empty(), "V1 predates the contribution ledger");

        // And once re-saved (V3, with header), it round-trips as-is.
        save_at(&loaded, &path).unwrap();
        assert_eq!(load_at(&path).expect("V3 reload"), loaded);
        fs::remove_file(&path).ok();
    }

    /// A world saved under `FCWORLD2` (post-central-world, pre-account-owned
    /// central buildings/contribution ledger) must load and migrate one hop
    /// to V3, same non-negotiable "never collapses to None" guarantee as V1.
    #[test]
    fn v2_save_migrates() {
        use crate::legacy::{BuildingV2, GameStateV2};

        let path = throwaway_path("v2-migrate");
        let mut modern = sim::new_game(23, 12);
        sim::player_joined(&mut modern, 3, "Vali");
        modern.graduated = true;
        let v2 = GameStateV2 {
            tick: modern.tick,
            win_days: modern.win_days,
            tiles: modern.tiles.clone(),
            buildings: modern
                .buildings
                .iter()
                .map(|b| BuildingV2 {
                    id: b.id,
                    kind: b.kind,
                    x: b.x,
                    y: b.y,
                    workers: b.workers,
                    progress: b.progress,
                    owner: b.owner,
                })
                .collect(),
            survivors: modern.survivors.clone(),
            stock: modern.stock,
            furnace_level: modern.furnace_level,
            furnace_lit: modern.furnace_lit,
            cold_snap: modern.cold_snap,
            players: modern.players.clone(),
            phase: modern.phase,
            events: modern.events.clone(),
            total_events: modern.total_events,
            chat: modern.chat.clone(),
            total_chat: modern.total_chat,
            pings: modern.pings.clone(),
            missions: modern.missions.clone(),
            tunnel: modern.tunnel,
            graduated: modern.graduated,
            central: modern.central,
            techs: modern.techs.clone(),
            disease_until: modern.disease_until,
            blizzard_until: modern.blizzard_until,
            pending_event: modern.pending_event,
            event_rng: modern.event_rng,
            guest_perm: modern.guest_perm,
            owner_id: modern.owner_id,
            next_id: modern.next_id,
            rng: modern.rng,
        };
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC_V2);
        bytes.extend_from_slice(&bincode::serialize(&v2).unwrap());
        fs::write(&path, bytes).unwrap();

        let loaded = load_at(&path).expect("V2 save must migrate, never wipe");
        assert_eq!(loaded.survivors.len(), modern.survivors.len());
        assert_eq!(loaded.players, modern.players);
        assert!(loaded.graduated, "graduation must survive migration");
        assert_eq!(loaded.tiles, modern.tiles);
        assert_eq!(loaded.stock, modern.stock);
        assert!(
            loaded.buildings.iter().all(|b| b.owner_account.is_none()),
            "V2 predates account-based building ownership"
        );
        assert!(loaded.central_ledger.is_empty(), "V2 predates the contribution ledger");

        // And once re-saved (V3, with header), it round-trips as-is.
        save_at(&loaded, &path).unwrap();
        assert_eq!(load_at(&path).expect("V3 reload"), loaded);
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
