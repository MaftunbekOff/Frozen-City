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
/// `Building::owner_account`, `GameState::central_ledger`; V4: survivor
/// position/movement, leader/mourning, professions, xp/levels, morale — see
/// the V0.7 "survivor management" fields on `Survivor`/`GameState`; V5:
/// `Building::level`/`build_left` — V0.8 building levels & construction;
/// V6: `Survivor::chop_target`/`carrying_wood` — the Furnace's opening
/// chop-and-carry construction cycle; V7: `GameState::pending_migrant` — the
/// Tunnel migrant wait/accept/leave cycle). Bincode is positional, so
/// without a header there'd be no way to tell one version's file from
/// another's — and misreading one version as another would "collapse to
/// None" in `load_at` and silently wipe a production world on the first
/// deploy after a format change. Files WITHOUT any recognized prefix are
/// decoded through the frozen V1 mirror (`legacy.rs`) and migrated all the
/// way to V7; files with `MAGIC_V2`/`MAGIC_V3`/`MAGIC_V4`/`MAGIC_V5`/
/// `MAGIC_V6` decode through the matching mirror and migrate the rest of the
/// way.
const MAGIC_V2: &[u8; 8] = b"FCWORLD2";
const MAGIC_V3: &[u8; 8] = b"FCWORLD3";
const MAGIC_V4: &[u8; 8] = b"FCWORLD4";
const MAGIC_V5: &[u8; 8] = b"FCWORLD5";
const MAGIC_V6: &[u8; 8] = b"FCWORLD6";
const MAGIC_V7: &[u8; 8] = b"FCWORLD7";

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
    let mut bytes = Vec::with_capacity(MAGIC_V7.len() + body.len());
    bytes.extend_from_slice(MAGIC_V7);
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
    if let Some(body) = bytes.strip_prefix(MAGIC_V7.as_slice()) {
        return bincode::deserialize(body).ok();
    }
    if let Some(body) = bytes.strip_prefix(MAGIC_V6.as_slice()) {
        // V6 (pre-Tunnel-migrants): decode through the frozen V6 mirror and
        // migrate one hop to V7. The next autosave rewrites it as V7.
        return bincode::deserialize::<crate::legacy::GameStateV6>(body)
            .ok()
            .map(GameState::from);
    }
    if let Some(body) = bytes.strip_prefix(MAGIC_V5.as_slice()) {
        // V5 (pre-chop/carry): decode through the frozen V5 mirror and
        // migrate two hops to V7. The next autosave rewrites it as V7.
        return bincode::deserialize::<crate::legacy::GameStateV5>(body)
            .ok()
            .map(GameState::from);
    }
    if let Some(body) = bytes.strip_prefix(MAGIC_V4.as_slice()) {
        // V4 (pre-building-levels): decode through the frozen V4 mirror and
        // migrate three hops to V7. The next autosave rewrites it as V7.
        return bincode::deserialize::<crate::legacy::GameStateV4>(body)
            .ok()
            .map(GameState::from);
    }
    if let Some(body) = bytes.strip_prefix(MAGIC_V3.as_slice()) {
        // V3 (pre-survivor-management): decode through the frozen V3 mirror
        // and migrate four hops to V7. The next autosave rewrites it as V7.
        return bincode::deserialize::<crate::legacy::GameStateV3>(body)
            .ok()
            .map(GameState::from);
    }
    if let Some(body) = bytes.strip_prefix(MAGIC_V2.as_slice()) {
        // V2 (pre-account-ownership/contribution-ledger): decode through the
        // frozen V2 mirror and migrate five hops to V7. The next autosave
        // rewrites it as V7.
        return bincode::deserialize::<crate::legacy::GameStateV2>(body)
            .ok()
            .map(GameState::from);
    }
    // No recognized header: a save written before versioning existed at
    // all — decode it through the frozen V1 mirror and migrate all the way
    // to V7. The next autosave rewrites it as V7.
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

        // And once re-saved (V7, with header), it round-trips as-is.
        save_at(&loaded, &path).unwrap();
        assert_eq!(load_at(&path).expect("V7 reload"), loaded);
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
            survivors: modern.survivors.iter().map(crate::legacy::SurvivorV3::from).collect(),
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

        // And once re-saved (V7, with header), it round-trips as-is.
        save_at(&loaded, &path).unwrap();
        assert_eq!(load_at(&path).expect("V7 reload"), loaded);
        fs::remove_file(&path).ok();
    }

    /// A world saved under `FCWORLD3` (pre-survivor-management: no
    /// positions/movement, no leader/mourning, no professions/xp, no morale)
    /// must load and migrate one hop to V4 with the defaults the brief
    /// specifies: furnace-adjacent spawn position, no walk target, profession
    /// from the id hash, xp 0 / no trained kind, no leader, no mourning in
    /// progress, and morale starting fresh — same non-negotiable "never
    /// collapses to None" guarantee as V1/V2.
    #[test]
    fn v3_save_migrates() {
        use crate::legacy::{GameStateV3, SurvivorV3};

        let path = throwaway_path("v3-migrate");
        let mut modern = sim::new_game(31, 12);
        sim::player_joined(&mut modern, 4, "Dilnoza");
        modern.graduated = true;
        let v3 = GameStateV3 {
            tick: modern.tick,
            win_days: modern.win_days,
            tiles: modern.tiles.clone(),
            buildings: modern.buildings.iter().map(crate::legacy::BuildingV4::from).collect(),
            survivors: modern.survivors.iter().map(SurvivorV3::from).collect(),
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
            central_ledger: modern.central_ledger.clone(),
        };
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC_V3);
        bytes.extend_from_slice(&bincode::serialize(&v3).unwrap());
        fs::write(&path, bytes).unwrap();

        let loaded = load_at(&path).expect("V3 save must migrate, never wipe");
        assert_eq!(loaded.survivors.len(), modern.survivors.len());
        assert_eq!(loaded.players, modern.players);
        assert!(loaded.graduated, "graduation must survive migration");
        assert_eq!(loaded.tiles, modern.tiles);
        assert_eq!(loaded.stock, modern.stock);
        assert_eq!(loaded.buildings, modern.buildings, "V3 already has owner_account");

        assert_eq!(loaded.leader, None, "V3 predates leadership");
        assert_eq!(loaded.mourning_until, 0, "V3 predates mourning");
        assert_eq!(loaded.morale, fc_game::types::MORALE_START, "V3 predates morale");
        for s in &loaded.survivors {
            assert_eq!(s.move_target, None, "V3 predates movement");
            assert_eq!(s.xp, 0.0, "V3 predates xp");
            assert_eq!(s.trained_kind, None, "V3 predates training");
            assert_eq!(
                s.profession,
                fc_game::types::Profession::from_id_hash(s.id),
                "migrated professions must be deterministic from the id"
            );
            let (fx, fy) = GameState::furnace_center();
            let d = ((s.x - fx).powi(2) + (s.y - fy).powi(2)).sqrt();
            assert!(d < 10.0, "V3 survivors should migrate to a furnace-adjacent spawn, got ({}, {})", s.x, s.y);
        }

        // And once re-saved (V7, with header), it round-trips as-is.
        save_at(&loaded, &path).unwrap();
        assert_eq!(load_at(&path).expect("V7 reload"), loaded);
        fs::remove_file(&path).ok();
    }

    /// A world saved under `FCWORLD4` (pre-building-levels: no
    /// `Building::level`/`build_left`) must load and migrate two hops to V6
    /// with every old building as a FINISHED level-1 building — same
    /// non-negotiable "never collapses to None" guarantee as V1/V2/V3.
    #[test]
    fn v4_save_migrates() {
        use crate::legacy::{BuildingV4, GameStateV4, SurvivorV5};

        let path = throwaway_path("v4-migrate");
        let mut modern = sim::new_game(41, 12);
        sim::player_joined(&mut modern, 6, "Yulduz");
        modern.graduated = true;
        let v4 = GameStateV4 {
            tick: modern.tick,
            win_days: modern.win_days,
            tiles: modern.tiles.clone(),
            buildings: modern.buildings.iter().map(BuildingV4::from).collect(),
            survivors: modern.survivors.iter().map(SurvivorV5::from).collect(),
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
            central_ledger: modern.central_ledger.clone(),
            leader: modern.leader,
            mourning_until: modern.mourning_until,
            morale: modern.morale,
        };
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC_V4);
        bytes.extend_from_slice(&bincode::serialize(&v4).unwrap());
        fs::write(&path, bytes).unwrap();

        let loaded = load_at(&path).expect("V4 save must migrate, never wipe");
        assert_eq!(
            loaded.survivors, modern.survivors,
            "V4 predates chop/carry, but a fresh survivor's default (None/false) matches anyway"
        );
        assert_eq!(loaded.players, modern.players);
        assert!(loaded.graduated, "graduation must survive migration");
        assert_eq!(loaded.stock, modern.stock);
        for b in &loaded.buildings {
            assert_eq!(b.level, 1, "V4 buildings migrate as level 1");
            assert_eq!(b.build_left, 0.0, "V4 buildings migrate as FINISHED");
        }
        // A fresh world's buildings are level-1/finished too, so the whole
        // set must match field-for-field after migration.
        assert_eq!(loaded.buildings, modern.buildings);

        // And once re-saved (V7, with header), it round-trips as-is.
        save_at(&loaded, &path).unwrap();
        assert_eq!(load_at(&path).expect("V7 reload"), loaded);
        fs::remove_file(&path).ok();
    }

    /// A world saved under `FCWORLD5` (pre-chop/carry: no
    /// `Survivor::chop_target`/`carrying_wood`) must load and migrate one
    /// hop to V6 with every old survivor defaulting to no errand in
    /// progress — same non-negotiable "never collapses to None" guarantee
    /// as V1/V2/V3/V4. `Building` is unchanged since V5, so it's cloned
    /// straight from `modern` (no `BuildingV*::from` remap needed, unlike
    /// the V4 test).
    #[test]
    fn v5_save_migrates() {
        use crate::legacy::{GameStateV5, SurvivorV5};

        let path = throwaway_path("v5-migrate");
        let mut modern = sim::new_game(51, 12);
        sim::player_joined(&mut modern, 8, "Bekzod");
        modern.graduated = true;
        let v5 = GameStateV5 {
            tick: modern.tick,
            win_days: modern.win_days,
            tiles: modern.tiles.clone(),
            buildings: modern.buildings.clone(),
            survivors: modern.survivors.iter().map(SurvivorV5::from).collect(),
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
            central_ledger: modern.central_ledger.clone(),
            leader: modern.leader,
            mourning_until: modern.mourning_until,
            morale: modern.morale,
        };
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC_V5);
        bytes.extend_from_slice(&bincode::serialize(&v5).unwrap());
        fs::write(&path, bytes).unwrap();

        let loaded = load_at(&path).expect("V5 save must migrate, never wipe");
        assert_eq!(
            loaded.survivors, modern.survivors,
            "V5 predates chop/carry, but a fresh survivor's default (None/false) matches anyway"
        );
        for s in &loaded.survivors {
            assert_eq!(s.chop_target, None, "V5 predates the chop errand");
            assert!(!s.carrying_wood, "V5 predates carrying wood");
        }
        assert_eq!(loaded.players, modern.players);
        assert!(loaded.graduated, "graduation must survive migration");
        assert_eq!(loaded.stock, modern.stock);
        assert_eq!(loaded.buildings, modern.buildings, "V5 already has level/build_left");

        // And once re-saved (V7, with header), it round-trips as-is.
        save_at(&loaded, &path).unwrap();
        assert_eq!(load_at(&path).expect("V7 reload"), loaded);
        fs::remove_file(&path).ok();
    }

    /// A world saved under `FCWORLD6` (pre-Tunnel-migrants: no
    /// `GameState::pending_migrant`) must load and migrate one hop to V7
    /// with no travelers mid-wait at the tunnel — same non-negotiable
    /// "never collapses to None" guarantee as V1..V5. `Building` and
    /// `Survivor` are both unchanged since V6, so they're cloned straight
    /// from `modern` (no `*V*::from` remap needed, unlike the V4/V5 tests).
    #[test]
    fn v6_save_migrates() {
        use crate::legacy::GameStateV6;

        let path = throwaway_path("v6-migrate");
        let mut modern = sim::new_game(61, 12);
        sim::player_joined(&mut modern, 9, "Nodira");
        modern.graduated = true;
        let v6 = GameStateV6 {
            tick: modern.tick,
            win_days: modern.win_days,
            tiles: modern.tiles.clone(),
            buildings: modern.buildings.clone(),
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
            central_ledger: modern.central_ledger.clone(),
            leader: modern.leader,
            mourning_until: modern.mourning_until,
            morale: modern.morale,
        };
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC_V6);
        bytes.extend_from_slice(&bincode::serialize(&v6).unwrap());
        fs::write(&path, bytes).unwrap();

        let loaded = load_at(&path).expect("V6 save must migrate, never wipe");
        assert_eq!(loaded.pending_migrant, None, "V6 predates Tunnel migrants");
        assert_eq!(loaded.survivors, modern.survivors);
        assert_eq!(loaded.players, modern.players);
        assert!(loaded.graduated, "graduation must survive migration");
        assert_eq!(loaded.stock, modern.stock);
        assert_eq!(loaded.buildings, modern.buildings, "V6 already has chop/carry");

        // And once re-saved (V7, with header), it round-trips as-is.
        save_at(&loaded, &path).unwrap();
        assert_eq!(load_at(&path).expect("V7 reload"), loaded);
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
