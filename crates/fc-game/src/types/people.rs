//! Survivors and connected players: professions, the settler model, player
//! identity, and co-op roles.

use serde::{Deserialize, Serialize};

use super::BuildingKind;

/// A survivor's trade. Assigned once at spawn (deterministically, from the
/// sim RNG) and never changes. Grants [`super::PROFESSION_MATCH_BONUS`]
/// production when the survivor works at the matching `BuildingKind`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Profession {
    Lumberjack,
    Miner,
    Hunter,
    Farmer,
    Medic,
    Cook,
}

impl Profession {
    pub const ALL: [Profession; 6] = [
        Profession::Lumberjack,
        Profession::Miner,
        Profession::Hunter,
        Profession::Farmer,
        Profession::Medic,
        Profession::Cook,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Profession::Lumberjack => "Lumberjack",
            Profession::Miner => "Miner",
            Profession::Hunter => "Hunter",
            Profession::Farmer => "Farmer",
            Profession::Medic => "Medic",
            Profession::Cook => "Cook",
        }
    }

    /// The building kind this profession is a specialist at, if any.
    pub fn matching_building(self) -> BuildingKind {
        match self {
            Profession::Lumberjack => BuildingKind::Sawmill,
            Profession::Miner => BuildingKind::CoalMine,
            Profession::Hunter => BuildingKind::HunterHut,
            Profession::Farmer => BuildingKind::Greenhouse,
            Profession::Medic => BuildingKind::Hospital,
            Profession::Cook => BuildingKind::Kitchen,
        }
    }

    /// Deterministic profession from a survivor id, used for migrated saves
    /// (V3 -> V4) that predate professions: no RNG stream to draw from, so
    /// the id itself is hashed instead. Kept separate from the spawn-time RNG
    /// path so a save's migration result never depends on load order.
    pub fn from_id_hash(id: u32) -> Profession {
        // Same SplitMix64 finalizer `rng::Rng` uses, applied once to the id —
        // cheap, well-distributed, and needs no RNG state of its own.
        let mut z = id as u64 ^ 0x9E37_79B9_7F4A_7C15;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        Profession::ALL[(z as usize) % Profession::ALL.len()]
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Survivor {
    pub id: u32,
    pub name: String,
    /// 0..=100; death at 0.
    pub hp: f32,
    /// 0..=120; starvation damage above 80.
    pub hunger: f32,
    /// Building this survivor is individually assigned to work at, if any.
    /// `None` means they're part of the anonymous pool `AdjustWorkers`
    /// counts but doesn't track by identity. Cleared automatically when the
    /// survivor dies or their building is demolished.
    pub assigned_building: Option<u32>,
    /// Account id (see `net::accounts`) this survivor belongs to. `None` in
    /// personal/shared worlds; `Some` only for settlers brought through the
    /// Tunnel into the central world, where only their owner may command them.
    pub owner: Option<i64>,
    /// Server-authoritative position in tile coordinates (V0.7). The client
    /// renders survivors here instead of picking its own idle position.
    pub x: f32,
    pub y: f32,
    /// A player-issued walk destination (`MoveSurvivor`). Takes priority over
    /// the survivor's assigned-building goal every tick until they arrive,
    /// at which point it's cleared and they stand idle there. Issuing this
    /// command also unassigns the survivor from work — see `MoveSurvivor`.
    pub move_target: Option<(u8, u8)>,
    /// Trade assigned once at spawn (or derived deterministically for
    /// migrated saves); grants `PROFESSION_MATCH_BONUS` at the matching
    /// building kind.
    pub profession: Profession,
    /// Accrued experience at `trained_kind`. Resets to 0 whenever the
    /// survivor is (re)assigned to a DIFFERENT building kind than the one
    /// they were accruing at.
    pub xp: f32,
    /// The building kind `xp` is being accrued toward, if any (`None` when
    /// never assigned, or immediately after unassignment — xp is only reset
    /// on a kind CHANGE, not on a plain unassign, so a temporarily idled
    /// survivor doesn't lose progress).
    pub trained_kind: Option<BuildingKind>,
    /// The forest tile this survivor is walking to (or standing at, about to
    /// chop) — either a Furnace-building errand (auto-picked while
    /// `assigned_building` points at a still-under-construction Furnace,
    /// each chopped log carried home toward `FURNACE_LOGS_NEEDED`) or a
    /// one-shot manual chop (`PlayerCommand::ChopTile`, which clears
    /// `assigned_building` first — chopping credits the stockpile directly
    /// since there's nowhere assigned to carry it). See `tick.rs`'s
    /// chop/carry block. Overrides the assigned-building walk goal but
    /// never `move_target`, which a player-issued walk always cancels this
    /// (and `carrying_wood`) via.
    pub chop_target: Option<(u8, u8)>,
    /// Set on arrival at `chop_target` (which is cleared at that point) once
    /// a Furnace-errand tile's been chopped — a manual chop never sets this,
    /// it credits the stockpile immediately instead. Cleared again on
    /// arrival back at the Furnace, when it counts as one delivered log
    /// toward `FURNACE_LOGS_NEEDED`. While true, the survivor's walk goal is
    /// their assigned building (the Furnace) instead of a tree.
    pub carrying_wood: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlayerInfo {
    pub id: u64,
    pub name: String,
    /// Index into the client-side player palette.
    pub color: u8,
    /// Last known cursor position in world tile coordinates.
    pub cursor: Option<(f32, f32)>,
    /// Buildings this player has placed (kept across reconnects).
    pub built: u32,
    /// Buildings this player has demolished (kept across reconnects).
    pub demolished: u32,
    /// Authority in this world (owner vs guest); kept across reconnects.
    pub role: Role,
    /// Account id this player signed in with, `None` for guests. In the
    /// central world it links a player to the settlers they own (see
    /// `Survivor::owner`); the stable identity across sessions is the
    /// account, not the per-world player id.
    pub account: Option<i64>,
}

/// A player's authority in the world. The first to join owns it; everyone else
/// is a guest, bounded by the world's [`GuestPermission`].
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Owner,
    Guest,
}

/// What guests are allowed to do, chosen by the owner. The owner always has
/// full authority; a world with no connected owner behaves as `Full`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuestPermission {
    /// Chat, ping and move the cursor only — no world changes.
    ViewOnly,
    /// Place buildings, assign workers, and demolish their OWN buildings.
    Build,
    /// Everything the owner can do (except owner-only admin: set policy, kick).
    Full,
}
