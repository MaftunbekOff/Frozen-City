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
/// Tunnel migrant wait/accept/leave cycle; V8: `Stockpile::fur`/`cloth`,
/// `GameState::wildlife` — the Tailor Shop / deer-rabbit wildlife resource
/// system; V9: `Survivor::thirst`/`bury_target`, `Stockpile::water`,
/// `GameState::corpses`/`graves` — births, death/burial and the thirst/Well
/// system; V10: `GameState::livestock` — the Farmhouse / cow-sheep livestock
/// resource system; V11: `Stockpile::gold`, `GameState::pending_caravan` —
/// the Tunnel trade caravan; V12: `GameState::guest_perm` REMOVED — the
/// tiered guest-permission system was retired, every guest now has full
/// authority). Bincode is positional, so without a header there'd be no way
/// to tell one version's file from another's — and misreading one version as
/// another would "collapse to None" in `load_at` and silently wipe a
/// production world on the first deploy after a format change. Files WITHOUT
/// any recognized prefix are decoded through the frozen V1 mirror
/// (`legacy.rs`) and migrated all the way to V12; files with
/// `MAGIC_V2`/`MAGIC_V3`/`MAGIC_V4`/`MAGIC_V5`/`MAGIC_V6`/`MAGIC_V7`/
/// `MAGIC_V8`/`MAGIC_V9`/`MAGIC_V10`/`MAGIC_V11` decode through the matching
/// mirror and migrate the rest of the way.
const MAGIC_V2: &[u8; 8] = b"FCWORLD2";
const MAGIC_V3: &[u8; 8] = b"FCWORLD3";
const MAGIC_V4: &[u8; 8] = b"FCWORLD4";
const MAGIC_V5: &[u8; 8] = b"FCWORLD5";
const MAGIC_V6: &[u8; 8] = b"FCWORLD6";
const MAGIC_V7: &[u8; 8] = b"FCWORLD7";
const MAGIC_V8: &[u8; 8] = b"FCWORLD8";
const MAGIC_V9: &[u8; 8] = b"FCWORLD9";
// One byte longer than every prior header — "FCWORLD" + "10"/"11"/"12" is 9
// bytes, not 8. `strip_prefix` works fine against a differently-sized slice;
// only the const's own array-length annotation needs to match.
const MAGIC_V10: &[u8; 9] = b"FCWORLD10";
const MAGIC_V11: &[u8; 9] = b"FCWORLD11";
const MAGIC_V12: &[u8; 9] = b"FCWORLD12";

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
    let mut bytes = Vec::with_capacity(MAGIC_V12.len() + body.len());
    bytes.extend_from_slice(MAGIC_V12);
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
    let mut state = load_at_raw(path)?;
    // See `GameState::repair_mislabeled_tunnel`'s doc comment — a real
    // 2026-07-14 production bug, safe/idempotent to run on every load.
    state.repair_mislabeled_tunnel();
    Some(state)
}

fn load_at_raw(path: &str) -> Option<GameState> {
    let bytes = fs::read(path).ok()?;
    if let Some(body) = bytes.strip_prefix(MAGIC_V12.as_slice()) {
        return bincode::deserialize(body).ok();
    }
    if let Some(body) = bytes.strip_prefix(MAGIC_V11.as_slice()) {
        // V11 (pre-guest-permission-removal): decode through the frozen V11
        // mirror and migrate one hop to V12. The next autosave rewrites it
        // as V12.
        return bincode::deserialize::<crate::legacy::GameStateV11>(body)
            .ok()
            .map(GameState::from);
    }
    if let Some(body) = bytes.strip_prefix(MAGIC_V10.as_slice()) {
        // V10 (pre-trade-caravan): decode through the frozen V10 mirror and
        // migrate two hops to V12. The next autosave rewrites it as V12.
        return bincode::deserialize::<crate::legacy::GameStateV10>(body)
            .ok()
            .map(GameState::from);
    }
    if let Some(body) = bytes.strip_prefix(MAGIC_V9.as_slice()) {
        // V9 (pre-livestock): decode through the frozen V9 mirror and
        // migrate three hops to V12. The next autosave rewrites it as V12.
        return bincode::deserialize::<crate::legacy::GameStateV9>(body)
            .ok()
            .map(GameState::from);
    }
    if let Some(body) = bytes.strip_prefix(MAGIC_V8.as_slice()) {
        // V8 (pre-death/burial/thirst): decode through the frozen V8 mirror
        // and migrate four hops to V12. The next autosave rewrites it as V12.
        return bincode::deserialize::<crate::legacy::GameStateV8>(body)
            .ok()
            .map(GameState::from);
    }
    if let Some(body) = bytes.strip_prefix(MAGIC_V7.as_slice()) {
        // V7 (pre-wildlife/Tailor-Shop): decode through the frozen V7 mirror
        // and migrate five hops to V12. The next autosave rewrites it as V12.
        return bincode::deserialize::<crate::legacy::GameStateV7>(body)
            .ok()
            .map(GameState::from);
    }
    if let Some(body) = bytes.strip_prefix(MAGIC_V6.as_slice()) {
        // V6 (pre-Tunnel-migrants): decode through the frozen V6 mirror and
        // migrate six hops to V12. The next autosave rewrites it as V12.
        return bincode::deserialize::<crate::legacy::GameStateV6>(body)
            .ok()
            .map(GameState::from);
    }
    if let Some(body) = bytes.strip_prefix(MAGIC_V5.as_slice()) {
        // V5 (pre-chop/carry): decode through the frozen V5 mirror and
        // migrate seven hops to V12. The next autosave rewrites it as V12.
        return bincode::deserialize::<crate::legacy::GameStateV5>(body)
            .ok()
            .map(GameState::from);
    }
    if let Some(body) = bytes.strip_prefix(MAGIC_V4.as_slice()) {
        // V4 (pre-building-levels): decode through the frozen V4 mirror and
        // migrate eight hops to V12. The next autosave rewrites it as V12.
        return bincode::deserialize::<crate::legacy::GameStateV4>(body)
            .ok()
            .map(GameState::from);
    }
    if let Some(body) = bytes.strip_prefix(MAGIC_V3.as_slice()) {
        // V3 (pre-survivor-management): decode through the frozen V3 mirror
        // and migrate nine hops to V12. The next autosave rewrites it as V12.
        return bincode::deserialize::<crate::legacy::GameStateV3>(body)
            .ok()
            .map(GameState::from);
    }
    if let Some(body) = bytes.strip_prefix(MAGIC_V2.as_slice()) {
        // V2 (pre-account-ownership/contribution-ledger): decode through the
        // frozen V2 mirror and migrate ten hops to V12. The next autosave
        // rewrites it as V12.
        return bincode::deserialize::<crate::legacy::GameStateV2>(body)
            .ok()
            .map(GameState::from);
    }
    // No recognized header: a save written before versioning existed at
    // all — decode it through the frozen V1 mirror and migrate all the way
    // to V12. The next autosave rewrites it as V12.
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

    /// Regression test for the 2026-07-14 production incident: a save whose
    /// Tunnel building was mislabeled as some other kind (by the
    /// `BuildingKind` wire-index bug) must come back correctly as `Tunnel`
    /// on load — see `GameState::repair_mislabeled_tunnel`.
    #[test]
    fn load_at_repairs_a_mislabeled_tunnel() {
        let path = throwaway_path("mislabeled-tunnel");
        let mut state = sim::new_game(9, 12);
        {
            let tunnel = state
                .buildings
                .iter_mut()
                .find(|b| b.kind == fc_game::types::BuildingKind::Tunnel)
                .expect("new_game always places a Tunnel");
            tunnel.kind = fc_game::types::BuildingKind::TailorShop;
        }
        assert!(
            !state.buildings.iter().any(|b| b.kind == fc_game::types::BuildingKind::Tunnel),
            "test setup: no building should claim to be the Tunnel anymore"
        );
        save_at(&state, &path).unwrap();

        let loaded = load_at(&path).expect("loads back");
        assert!(
            loaded.buildings.iter().any(|b| b.kind == fc_game::types::BuildingKind::Tunnel),
            "load_at should repair the mislabeled building back to Tunnel"
        );
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
            stock: crate::legacy::StockpileV7 {
                wood: modern.stock.wood,
                coal: modern.stock.coal,
                food: modern.stock.food,
            },
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
            guest_perm: fc_game::types::GuestPermission::Build,
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
        // V0.11: a fresh `modern` world now starts with a real water buffer
        // (`START_WATER`), but a migrated V1 save predates water entirely —
        // same reasoning as the fur/cloth/thirst fields below, just one
        // level up (whole-stockpile comparison instead of per-survivor).
        assert_eq!(loaded.stock.wood, modern.stock.wood);
        assert_eq!(loaded.stock.coal, modern.stock.coal);
        assert_eq!(loaded.stock.food, modern.stock.food);
        assert_eq!(loaded.stock.fur, modern.stock.fur);
        assert_eq!(loaded.stock.cloth, modern.stock.cloth);
        assert_eq!(loaded.stock.water, 0.0, "V1 predates water");
        assert!(
            loaded.buildings.iter().all(|b| b.owner_account.is_none()),
            "V1 predates account-based building ownership"
        );
        assert!(loaded.central_ledger.is_empty(), "V1 predates the contribution ledger");

        // And once re-saved (V11, with header), it round-trips as-is.
        save_at(&loaded, &path).unwrap();
        assert_eq!(load_at(&path).expect("V11 reload"), loaded);
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
            stock: crate::legacy::StockpileV7 {
                wood: modern.stock.wood,
                coal: modern.stock.coal,
                food: modern.stock.food,
            },
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
            guest_perm: fc_game::types::GuestPermission::Build,
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
        assert_eq!(loaded.stock.wood, modern.stock.wood);
        assert_eq!(loaded.stock.coal, modern.stock.coal);
        assert_eq!(loaded.stock.food, modern.stock.food);
        assert_eq!(loaded.stock.fur, modern.stock.fur);
        assert_eq!(loaded.stock.cloth, modern.stock.cloth);
        assert_eq!(loaded.stock.water, 0.0, "V2 predates water");
        assert!(
            loaded.buildings.iter().all(|b| b.owner_account.is_none()),
            "V2 predates account-based building ownership"
        );
        assert!(loaded.central_ledger.is_empty(), "V2 predates the contribution ledger");

        // And once re-saved (V11, with header), it round-trips as-is.
        save_at(&loaded, &path).unwrap();
        assert_eq!(load_at(&path).expect("V11 reload"), loaded);
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
        // V3 predates the furnace's chop-and-carry opening construction —
        // every migrated building (including the furnace) lands FINISHED
        // (see the V4->V5 migration's `build_left: 0.0`), whereas a truly
        // fresh `new_game` furnace now starts unbuilt. Finish it here so the
        // `buildings` comparison below is against the same "already built"
        // state migration actually produces.
        sim::finish_all_construction(&mut modern);
        let v3 = GameStateV3 {
            tick: modern.tick,
            win_days: modern.win_days,
            tiles: modern.tiles.clone(),
            buildings: modern.buildings.iter().map(crate::legacy::BuildingV4::from).collect(),
            survivors: modern.survivors.iter().map(SurvivorV3::from).collect(),
            stock: crate::legacy::StockpileV7 {
                wood: modern.stock.wood,
                coal: modern.stock.coal,
                food: modern.stock.food,
            },
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
            guest_perm: fc_game::types::GuestPermission::Build,
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
        assert_eq!(loaded.stock.wood, modern.stock.wood);
        assert_eq!(loaded.stock.coal, modern.stock.coal);
        assert_eq!(loaded.stock.food, modern.stock.food);
        assert_eq!(loaded.stock.fur, modern.stock.fur);
        assert_eq!(loaded.stock.cloth, modern.stock.cloth);
        assert_eq!(loaded.stock.water, 0.0, "V3 predates water");
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

        // And once re-saved (V11, with header), it round-trips as-is.
        save_at(&loaded, &path).unwrap();
        assert_eq!(load_at(&path).expect("V11 reload"), loaded);
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
        // V4 predates the furnace's chop-and-carry opening construction —
        // every migrated building (including the furnace) lands FINISHED,
        // whereas a truly fresh `new_game` furnace now starts unbuilt.
        // Finish it here so the `buildings` comparison below is against the
        // same "already built" state migration actually produces.
        sim::finish_all_construction(&mut modern);
        let v4 = GameStateV4 {
            tick: modern.tick,
            win_days: modern.win_days,
            tiles: modern.tiles.clone(),
            buildings: modern.buildings.iter().map(BuildingV4::from).collect(),
            survivors: modern.survivors.iter().map(SurvivorV5::from).collect(),
            stock: crate::legacy::StockpileV7 {
                wood: modern.stock.wood,
                coal: modern.stock.coal,
                food: modern.stock.food,
            },
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
            guest_perm: fc_game::types::GuestPermission::Build,
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
        assert_eq!(loaded.survivors.len(), modern.survivors.len());
        for s in &loaded.survivors {
            assert_eq!(s.chop_target, None, "V4 predates the chop errand");
            assert!(!s.carrying_wood, "V4 predates carrying wood");
            assert_eq!(s.thirst, 0.0, "V4 predates thirst");
            assert_eq!(s.bury_target, None, "V4 predates burial");
        }
        assert_eq!(loaded.players, modern.players);
        assert!(loaded.graduated, "graduation must survive migration");
        assert_eq!(loaded.stock.wood, modern.stock.wood);
        assert_eq!(loaded.stock.coal, modern.stock.coal);
        assert_eq!(loaded.stock.food, modern.stock.food);
        assert_eq!(loaded.stock.fur, modern.stock.fur);
        assert_eq!(loaded.stock.cloth, modern.stock.cloth);
        assert_eq!(loaded.stock.water, 0.0, "V4 predates water");
        for b in &loaded.buildings {
            assert_eq!(b.level, 1, "V4 buildings migrate as level 1");
            assert_eq!(b.build_left, 0.0, "V4 buildings migrate as FINISHED");
        }
        // A fresh world's buildings are level-1/finished too, so the whole
        // set must match field-for-field after migration.
        assert_eq!(loaded.buildings, modern.buildings);
        assert!(loaded.corpses.is_empty(), "V4 predates death/burial");
        assert!(loaded.graves.is_empty(), "V4 predates death/burial");

        // And once re-saved (V11, with header), it round-trips as-is.
        save_at(&loaded, &path).unwrap();
        assert_eq!(load_at(&path).expect("V11 reload"), loaded);
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
            stock: crate::legacy::StockpileV7 {
                wood: modern.stock.wood,
                coal: modern.stock.coal,
                food: modern.stock.food,
            },
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
            guest_perm: fc_game::types::GuestPermission::Build,
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
        assert_eq!(loaded.survivors.len(), modern.survivors.len());
        for s in &loaded.survivors {
            assert_eq!(s.chop_target, None, "V5 predates the chop errand");
            assert!(!s.carrying_wood, "V5 predates carrying wood");
            assert_eq!(s.thirst, 0.0, "V5 predates thirst");
            assert_eq!(s.bury_target, None, "V5 predates burial");
        }
        assert_eq!(loaded.players, modern.players);
        assert!(loaded.graduated, "graduation must survive migration");
        assert_eq!(loaded.stock.wood, modern.stock.wood);
        assert_eq!(loaded.stock.coal, modern.stock.coal);
        assert_eq!(loaded.stock.food, modern.stock.food);
        assert_eq!(loaded.stock.fur, modern.stock.fur);
        assert_eq!(loaded.stock.cloth, modern.stock.cloth);
        assert_eq!(loaded.stock.water, 0.0, "V5 predates water");
        assert_eq!(loaded.buildings, modern.buildings, "V5 already has level/build_left");
        assert!(loaded.corpses.is_empty(), "V5 predates death/burial");
        assert!(loaded.graves.is_empty(), "V5 predates death/burial");

        // And once re-saved (V11, with header), it round-trips as-is.
        save_at(&loaded, &path).unwrap();
        assert_eq!(load_at(&path).expect("V11 reload"), loaded);
        fs::remove_file(&path).ok();
    }

    /// A world saved under `FCWORLD6` (pre-Tunnel-migrants: no
    /// `GameState::pending_migrant`) must load and migrate two hops to V8
    /// with no travelers mid-wait at the tunnel — same non-negotiable
    /// "never collapses to None" guarantee as V1..V5. `Building` and
    /// `Survivor` are both unchanged since V6, so they're cloned straight
    /// from `modern` (no `*V*::from` remap needed, unlike the V4/V5 tests).
    #[test]
    fn v6_save_migrates() {
        use crate::legacy::{GameStateV6, SurvivorV8};

        let path = throwaway_path("v6-migrate");
        let mut modern = sim::new_game(61, 12);
        sim::player_joined(&mut modern, 9, "Nodira");
        modern.graduated = true;
        let v6 = GameStateV6 {
            tick: modern.tick,
            win_days: modern.win_days,
            tiles: modern.tiles.clone(),
            buildings: modern.buildings.clone(),
            survivors: modern.survivors.iter().map(SurvivorV8::from).collect(),
            stock: crate::legacy::StockpileV7 {
                wood: modern.stock.wood,
                coal: modern.stock.coal,
                food: modern.stock.food,
            },
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
            guest_perm: fc_game::types::GuestPermission::Build,
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
        assert_eq!(loaded.survivors.len(), modern.survivors.len());
        for s in &loaded.survivors {
            assert_eq!(s.thirst, 0.0, "V6 predates thirst");
            assert_eq!(s.bury_target, None, "V6 predates burial");
        }
        assert_eq!(loaded.players, modern.players);
        assert!(loaded.graduated, "graduation must survive migration");
        assert_eq!(loaded.stock.wood, modern.stock.wood);
        assert_eq!(loaded.stock.coal, modern.stock.coal);
        assert_eq!(loaded.stock.food, modern.stock.food);
        assert_eq!(loaded.stock.fur, modern.stock.fur);
        assert_eq!(loaded.stock.cloth, modern.stock.cloth);
        assert_eq!(loaded.stock.water, 0.0, "V6 predates water");
        assert_eq!(loaded.buildings, modern.buildings, "V6 already has chop/carry");
        assert!(loaded.corpses.is_empty(), "V6 predates death/burial");
        assert!(loaded.graves.is_empty(), "V6 predates death/burial");

        // And once re-saved (V11, with header), it round-trips as-is.
        save_at(&loaded, &path).unwrap();
        assert_eq!(load_at(&path).expect("V11 reload"), loaded);
        fs::remove_file(&path).ok();
    }

    /// A world saved under `FCWORLD7` (pre-wildlife/Tailor-Shop: no
    /// `Stockpile::fur`/`cloth`, no `GameState::wildlife`) must load and
    /// migrate two hops to V9 with the defaults the feature specifies: a
    /// fresh-world-level deer/rabbit population (NOT zero, which would
    /// permanently strand every migrated `HunterHut`) and an empty fur/cloth
    /// stockpile — same non-negotiable "never collapses to None" guarantee
    /// as V1..V6. `Building` is unchanged, so it's cloned straight from
    /// `modern`; `Survivor` is mirrored as `SurvivorV8`.
    #[test]
    fn v7_save_migrates() {
        use crate::legacy::{GameStateV7, SurvivorV8};

        let path = throwaway_path("v7-migrate");
        let mut modern = sim::new_game(71, 12);
        sim::player_joined(&mut modern, 10, "Shahnoza");
        modern.graduated = true;
        let v7 = GameStateV7 {
            tick: modern.tick,
            win_days: modern.win_days,
            tiles: modern.tiles.clone(),
            buildings: modern.buildings.clone(),
            survivors: modern.survivors.iter().map(SurvivorV8::from).collect(),
            stock: crate::legacy::StockpileV7 {
                wood: modern.stock.wood,
                coal: modern.stock.coal,
                food: modern.stock.food,
            },
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
            guest_perm: fc_game::types::GuestPermission::Build,
            owner_id: modern.owner_id,
            next_id: modern.next_id,
            rng: modern.rng,
            central_ledger: modern.central_ledger.clone(),
            leader: modern.leader,
            mourning_until: modern.mourning_until,
            morale: modern.morale,
            pending_migrant: modern.pending_migrant,
        };
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC_V7);
        bytes.extend_from_slice(&bincode::serialize(&v7).unwrap());
        fs::write(&path, bytes).unwrap();

        let loaded = load_at(&path).expect("V7 save must migrate, never wipe");
        assert_eq!(
            loaded.wildlife,
            fc_game::types::Wildlife {
                deer: fc_game::types::DEER_START,
                rabbit: fc_game::types::RABBIT_START,
            },
            "V7 predates wildlife — a migrated HunterHut must be able to hunt immediately"
        );
        assert_eq!(loaded.stock.fur, 0.0, "V7 predates fur");
        assert_eq!(loaded.stock.cloth, 0.0, "V7 predates cloth");
        assert_eq!(loaded.stock.water, 0.0, "V7 predates water");
        assert_eq!(loaded.survivors.len(), modern.survivors.len());
        for s in &loaded.survivors {
            assert_eq!(s.thirst, 0.0, "V7 predates thirst");
            assert_eq!(s.bury_target, None, "V7 predates burial");
        }
        assert_eq!(loaded.players, modern.players);
        assert!(loaded.graduated, "graduation must survive migration");
        assert_eq!(loaded.buildings, modern.buildings, "V7 already has chop/carry");
        assert!(loaded.corpses.is_empty(), "V7 predates death/burial");
        assert!(loaded.graves.is_empty(), "V7 predates death/burial");

        // And once re-saved (V11, with header), it round-trips as-is.
        save_at(&loaded, &path).unwrap();
        assert_eq!(load_at(&path).expect("V11 reload"), loaded);
        fs::remove_file(&path).ok();
    }

    /// A world saved under `FCWORLD8` (pre-death/burial/thirst: no
    /// `Survivor::thirst`/`bury_target`, no `Stockpile::water`, no
    /// `GameState::corpses`/`graves`) must load and migrate one hop to V9
    /// with the defaults this feature specifies: thirst starts at 0 (a safe
    /// "fresh start" default, not a stranded-mechanic risk, since it's a
    /// consumption meter not a production source), water starts at 0 (an
    /// empty stockpile for a resource that didn't exist yet), and no
    /// corpses/graves are pending (nobody was mid-decay when this world
    /// predated the concept entirely) — same non-negotiable "never
    /// collapses to None" guarantee as V1..V7. `Building` is unchanged, so
    /// it's cloned straight from `modern`; `Survivor` is mirrored as
    /// `SurvivorV8`, `Stockpile` as `StockpileV8`.
    #[test]
    fn v8_save_migrates() {
        use crate::legacy::{GameStateV8, StockpileV8, SurvivorV8};

        let path = throwaway_path("v8-migrate");
        let mut modern = sim::new_game(81, 12);
        sim::player_joined(&mut modern, 11, "Bekzod");
        modern.graduated = true;
        let v8 = GameStateV8 {
            tick: modern.tick,
            win_days: modern.win_days,
            tiles: modern.tiles.clone(),
            buildings: modern.buildings.clone(),
            survivors: modern.survivors.iter().map(SurvivorV8::from).collect(),
            stock: StockpileV8 {
                wood: modern.stock.wood,
                coal: modern.stock.coal,
                food: modern.stock.food,
                fur: modern.stock.fur,
                cloth: modern.stock.cloth,
            },
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
            guest_perm: fc_game::types::GuestPermission::Build,
            owner_id: modern.owner_id,
            next_id: modern.next_id,
            rng: modern.rng,
            central_ledger: modern.central_ledger.clone(),
            leader: modern.leader,
            mourning_until: modern.mourning_until,
            morale: modern.morale,
            pending_migrant: modern.pending_migrant,
            wildlife: modern.wildlife,
        };
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC_V8);
        bytes.extend_from_slice(&bincode::serialize(&v8).unwrap());
        fs::write(&path, bytes).unwrap();

        let loaded = load_at(&path).expect("V8 save must migrate, never wipe");
        assert_eq!(loaded.stock.water, 0.0, "V8 predates water");
        assert_eq!(loaded.survivors.len(), modern.survivors.len());
        for s in &loaded.survivors {
            assert_eq!(s.thirst, 0.0, "V8 predates thirst");
            assert_eq!(s.bury_target, None, "V8 predates burial");
        }
        assert_eq!(loaded.players, modern.players);
        assert!(loaded.graduated, "graduation must survive migration");
        assert_eq!(loaded.buildings, modern.buildings, "V8 already has chop/carry");
        assert_eq!(loaded.wildlife, modern.wildlife, "V8 already has wildlife");
        assert!(loaded.corpses.is_empty(), "V8 predates death/burial");
        assert!(loaded.graves.is_empty(), "V8 predates death/burial");

        // And once re-saved (V11, with header), it round-trips as-is.
        save_at(&loaded, &path).unwrap();
        assert_eq!(load_at(&path).expect("V11 reload"), loaded);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn v9_save_migrates() {
        use crate::legacy::{GameStateV9, StockpileV10};

        let path = throwaway_path("v9-migrate");
        let mut modern = sim::new_game(91, 12);
        sim::player_joined(&mut modern, 12, "Kamila");
        modern.graduated = true;
        // Neither `Building` nor `Survivor` changed shape in this hop (only
        // `GameState` itself grew `livestock`, and `Stockpile` predates
        // `gold` — see `StockpileV10`), so `Building`/`Survivor` reuse
        // straight from `modern` with no frozen mirror needed.
        let v9 = GameStateV9 {
            tick: modern.tick,
            win_days: modern.win_days,
            tiles: modern.tiles.clone(),
            buildings: modern.buildings.clone(),
            survivors: modern.survivors.clone(),
            stock: StockpileV10 {
                wood: modern.stock.wood,
                coal: modern.stock.coal,
                food: modern.stock.food,
                fur: modern.stock.fur,
                cloth: modern.stock.cloth,
                water: modern.stock.water,
            },
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
            guest_perm: fc_game::types::GuestPermission::Build,
            owner_id: modern.owner_id,
            next_id: modern.next_id,
            rng: modern.rng,
            central_ledger: modern.central_ledger.clone(),
            leader: modern.leader,
            mourning_until: modern.mourning_until,
            morale: modern.morale,
            pending_migrant: modern.pending_migrant,
            wildlife: modern.wildlife,
            corpses: modern.corpses.clone(),
            graves: modern.graves.clone(),
        };
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC_V9);
        bytes.extend_from_slice(&bincode::serialize(&v9).unwrap());
        fs::write(&path, bytes).unwrap();

        let loaded = load_at(&path).expect("V9 save must migrate, never wipe");
        assert_eq!(loaded.livestock.cow, fc_game::types::COW_START, "V9 predates livestock");
        assert_eq!(loaded.livestock.sheep, fc_game::types::SHEEP_START, "V9 predates livestock");
        assert_eq!(loaded.stock.gold, 0.0, "V9 predates the trade caravan");
        assert!(loaded.pending_caravan.is_none(), "V9 predates the trade caravan");
        assert_eq!(loaded.survivors, modern.survivors);
        assert_eq!(loaded.players, modern.players);
        assert!(loaded.graduated, "graduation must survive migration");
        assert_eq!(loaded.stock, modern.stock);
        assert_eq!(loaded.buildings, modern.buildings, "V9 already has chop/carry");
        assert_eq!(loaded.wildlife, modern.wildlife, "V9 already has wildlife");
        assert_eq!(loaded.corpses, modern.corpses, "V9 already has death/burial");
        assert_eq!(loaded.graves, modern.graves, "V9 already has death/burial");

        // And once re-saved (V11, with header), it round-trips as-is.
        save_at(&loaded, &path).unwrap();
        assert_eq!(load_at(&path).expect("V11 reload"), loaded);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn v10_save_migrates() {
        use crate::legacy::{GameStateV10, StockpileV10};

        let path = throwaway_path("v10-migrate");
        let mut modern = sim::new_game(101, 12);
        sim::player_joined(&mut modern, 13, "Elyor");
        modern.graduated = true;
        let v10 = GameStateV10 {
            tick: modern.tick,
            win_days: modern.win_days,
            tiles: modern.tiles.clone(),
            buildings: modern.buildings.clone(),
            survivors: modern.survivors.clone(),
            stock: StockpileV10 {
                wood: modern.stock.wood,
                coal: modern.stock.coal,
                food: modern.stock.food,
                fur: modern.stock.fur,
                cloth: modern.stock.cloth,
                water: modern.stock.water,
            },
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
            guest_perm: fc_game::types::GuestPermission::Build,
            owner_id: modern.owner_id,
            next_id: modern.next_id,
            rng: modern.rng,
            central_ledger: modern.central_ledger.clone(),
            leader: modern.leader,
            mourning_until: modern.mourning_until,
            morale: modern.morale,
            pending_migrant: modern.pending_migrant,
            wildlife: modern.wildlife,
            corpses: modern.corpses.clone(),
            graves: modern.graves.clone(),
            livestock: modern.livestock,
        };
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC_V10);
        bytes.extend_from_slice(&bincode::serialize(&v10).unwrap());
        fs::write(&path, bytes).unwrap();

        let loaded = load_at(&path).expect("V10 save must migrate, never wipe");
        assert_eq!(loaded.stock.gold, 0.0, "V10 predates the trade caravan");
        assert!(loaded.pending_caravan.is_none(), "V10 predates the trade caravan");
        assert_eq!(loaded.survivors, modern.survivors);
        assert_eq!(loaded.players, modern.players);
        assert!(loaded.graduated, "graduation must survive migration");
        assert_eq!(loaded.stock, modern.stock);
        assert_eq!(loaded.buildings, modern.buildings, "V10 already has chop/carry");
        assert_eq!(loaded.wildlife, modern.wildlife, "V10 already has wildlife");
        assert_eq!(loaded.livestock, modern.livestock, "V10 already has livestock");
        assert_eq!(loaded.corpses, modern.corpses, "V10 already has death/burial");
        assert_eq!(loaded.graves, modern.graves, "V10 already has death/burial");

        // And once re-saved (V11, with header), it round-trips as-is.
        save_at(&loaded, &path).unwrap();
        assert_eq!(load_at(&path).expect("V11 reload"), loaded);
        fs::remove_file(&path).ok();
    }

    /// A world saved under `FCWORLD11` (pre-guest-permission-removal: still
    /// carries `guest_perm`, the last field V12 dropped) must load and
    /// migrate one hop to V12 — same non-negotiable "never collapses to
    /// None" guarantee as V1..V10. Deliberately uses a `guest_perm` value
    /// (`ViewOnly`) that doesn't match any default, to prove the field is
    /// genuinely dropped rather than coincidentally ignored.
    #[test]
    fn v11_save_migrates() {
        use crate::legacy::GameStateV11;

        let path = throwaway_path("v11-migrate");
        let mut modern = sim::new_game(111, 12);
        sim::player_joined(&mut modern, 14, "Sardor");
        modern.graduated = true;
        let v11 = GameStateV11 {
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
            guest_perm: fc_game::types::GuestPermission::ViewOnly,
            owner_id: modern.owner_id,
            next_id: modern.next_id,
            rng: modern.rng,
            central_ledger: modern.central_ledger.clone(),
            leader: modern.leader,
            mourning_until: modern.mourning_until,
            morale: modern.morale,
            pending_migrant: modern.pending_migrant,
            wildlife: modern.wildlife,
            corpses: modern.corpses.clone(),
            graves: modern.graves.clone(),
            livestock: modern.livestock,
            pending_caravan: modern.pending_caravan.clone(),
        };
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC_V11);
        bytes.extend_from_slice(&bincode::serialize(&v11).unwrap());
        fs::write(&path, bytes).unwrap();

        let loaded = load_at(&path).expect("V11 save must migrate, never wipe");
        assert_eq!(loaded.survivors, modern.survivors);
        assert_eq!(loaded.players, modern.players);
        assert!(loaded.graduated, "graduation must survive migration");
        assert_eq!(loaded.stock, modern.stock);
        assert_eq!(loaded.buildings, modern.buildings);
        assert_eq!(loaded.wildlife, modern.wildlife);
        assert_eq!(loaded.livestock, modern.livestock);
        assert_eq!(loaded.corpses, modern.corpses);
        assert_eq!(loaded.graves, modern.graves);
        assert_eq!(
            loaded.pending_caravan, modern.pending_caravan,
            "V11 already has the trade caravan"
        );

        // And once re-saved (V12, with header), it round-trips as-is.
        save_at(&loaded, &path).unwrap();
        assert_eq!(load_at(&path).expect("V12 reload"), loaded);
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
