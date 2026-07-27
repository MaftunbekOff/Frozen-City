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
    /// V0.10: specialist at the `TailorShop` (fur -> cloth).
    Tailor,
}

impl Profession {
    pub const ALL: [Profession; 7] = [
        Profession::Lumberjack,
        Profession::Miner,
        Profession::Hunter,
        Profession::Farmer,
        Profession::Medic,
        Profession::Cook,
        Profession::Tailor,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Profession::Lumberjack => "Lumberjack",
            Profession::Miner => "Miner",
            Profession::Hunter => "Hunter",
            Profession::Farmer => "Farmer",
            Profession::Medic => "Medic",
            Profession::Cook => "Cook",
            Profession::Tailor => "Tailor",
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
            Profession::Tailor => BuildingKind::TailorShop,
        }
    }

    /// V0.12: true if this profession earns `PROFESSION_MATCH_BONUS` at
    /// `kind` — almost always identical to `matching_building() == kind`,
    /// except `Farmer` matches BOTH `Greenhouse` (crop farming) and
    /// `Farmhouse` (livestock), the one profession with two trades instead
    /// of one. `matching_building` still returns a single kind (used where a
    /// UI needs ONE "primary" building to name); this is the general
    /// bonus-eligibility check `survivor_contribution` and the roster's
    /// off-trade-level tag should use instead.
    pub fn matches_building(self, kind: BuildingKind) -> bool {
        if self == Profession::Farmer && kind == BuildingKind::Farmhouse {
            return true;
        }
        kind == self.matching_building()
    }

    /// V0.11: true if this profession's specialized training is required to
    /// be even minimally useful at `kind` — a mismatched survivor here gets
    /// `SKILLED_MISMATCH_PENALTY` instead of the normal 1.0x baseline every
    /// other mismatch keeps. Only specific (profession, building) pairs are
    /// gated (currently just Medic/Hospital — healing plausibly needs real
    /// training in a way chopping wood or cooking a meal doesn't); everyone
    /// else can informally staff any building at the ordinary rate.
    pub fn is_skilled_at(self, kind: BuildingKind) -> bool {
        matches!((self, kind), (Profession::Medic, BuildingKind::Hospital))
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
    /// 0..=120; dehydration damage above 65 — a tighter, faster-escalating
    /// band than `hunger`'s 80 (water is calibrated to become critical
    /// sooner, see `WATER_PER_SURVIVOR_DAY`'s doc comment), same clamp/
    /// two-threshold shape otherwise. V0.11.
    pub thirst: f32,
    /// V0.11: the corpse this (living) survivor is walking to/performing the
    /// timed bury action on (`PlayerCommand::Bury`) — mirrors `chop_target`'s
    /// role as a walk-then-act errand. Overrides the assigned-building walk
    /// goal but never `move_target`.
    pub bury_target: Option<u32>,
    /// V0.17: 0..=100 tiredness. Climbs while awake (faster while assigned to
    /// a building than while idle) and falls back overnight — fast in a Tent
    /// bunk, slowly for anyone sleeping rough because housing ran out (see
    /// `FATIGUE_*` in `types.rs` and the fatigue block in `sim::tick`).
    /// Above `FATIGUE_TIRED` it scales this survivor's production share down
    /// (see `survivor_contribution`), so Tents now buy work throughput on top
    /// of the population cap they always gated.
    ///
    /// IMPORTANT: bincode serializes this struct positionally — every field
    /// added from here on must stay APPENDED at the very end, or every
    /// already-persisted world decodes its survivors as garbage. Old saves
    /// load through `legacy::SurvivorV13` (the frozen pre-V0.17 mirror).
    pub fatigue: f32,
    /// V0.17: remaining ticks of an individual illness, 0 when healthy —
    /// "remaining work" shape, same as `Building.build_left`, so a staffed
    /// Hospital can burn it down faster than the natural course (see
    /// `SICKNESS_TICKS`, `HOSPITAL_RECOVERY_PER_UNIT_DAY`). Replaces V0.3's
    /// colony-wide flat HP drain: an outbreak (`GameState.disease_until`)
    /// now infects individuals, who then carry and spread it themselves.
    pub sick_left: f32,
    /// V0.18: in-game days lived — counted from birth for anyone born here,
    /// and from a drawn starting age (`ARRIVAL_AGE_MIN_DAYS`..
    /// `ARRIVAL_AGE_MAX_DAYS`) for anyone who walked in already grown. Drives
    /// [`Survivor::stage`]: a child doesn't work, an elder works less and
    /// eventually dies of old age. Pre-V0.18 saves migrate to a drawn adult
    /// age rather than 0 — a whole city of newborns would be a very literal
    /// reading of "no data".
    pub age_days: f32,
    /// V0.18: this survivor's partner, by id, if they've paired up. Symmetric
    /// by construction (`sim::lifecycle` always writes both sides) and cleared
    /// on either side's death, since a dangling partner id would outlive the
    /// person it names. Couples raise the colony's birth chance
    /// (`BIRTH_COUPLE_MULTIPLIER`).
    pub partner: Option<u32>,
}

/// V0.18: which stage of life a survivor is in. The single place the
/// `ADULT_AGE_DAYS`/`ELDER_AGE_DAYS` thresholds are interpreted — shared by
/// the sim (work output, food share, frailty) and the client (roster tag), so
/// the two can never disagree about who counts as a child.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifeStage {
    Child,
    Adult,
    Elder,
}

impl LifeStage {
    pub fn name(self) -> &'static str {
        match self {
            LifeStage::Child => "Child",
            LifeStage::Adult => "Adult",
            LifeStage::Elder => "Elder",
        }
    }
}

impl Survivor {
    /// V0.17: true while this survivor is carrying an illness.
    pub fn is_sick(&self) -> bool {
        self.sick_left > 0.0
    }

    /// V0.17: production scale from tiredness — 1.0 up to `FATIGUE_TIRED`,
    /// then falling linearly to `FATIGUE_MIN_FACTOR` at a full 100. Shared by
    /// `survivor_contribution` (the sim) and the roster card (the client), so
    /// the number a player reads is the number the sim applies.
    pub fn fatigue_factor(&self) -> f32 {
        if self.fatigue <= super::FATIGUE_TIRED {
            return 1.0;
        }
        let t = ((self.fatigue - super::FATIGUE_TIRED) / (100.0 - super::FATIGUE_TIRED)).clamp(0.0, 1.0);
        1.0 - t * (1.0 - super::FATIGUE_MIN_FACTOR)
    }

    /// V0.18: this survivor's stage of life, from `age_days`.
    pub fn stage(&self) -> LifeStage {
        if self.age_days < super::ADULT_AGE_DAYS {
            LifeStage::Child
        } else if self.age_days >= super::ELDER_AGE_DAYS {
            LifeStage::Elder
        } else {
            LifeStage::Adult
        }
    }

    /// V0.18: production scale from age — children contribute nothing (they
    /// still eat and still need a bunk), elders contribute less. Composes
    /// multiplicatively with `fatigue_factor` and `SICK_WORK_FACTOR` in
    /// `survivor_contribution`, so an exhausted, sick elder is scaled by all
    /// three rather than by whichever one the call site remembered.
    pub fn age_work_factor(&self) -> f32 {
        match self.stage() {
            LifeStage::Child => super::CHILD_WORK_FACTOR,
            LifeStage::Adult => 1.0,
            LifeStage::Elder => super::ELDER_WORK_FACTOR,
        }
    }

    /// V0.18: this survivor's share of `FOOD_PER_SURVIVOR_DAY` — a child eats
    /// less than an adult. Water is deliberately NOT scaled: thirst doesn't
    /// care how old you are.
    pub fn food_factor(&self) -> f32 {
        match self.stage() {
            LifeStage::Child => super::CHILD_FOOD_FACTOR,
            _ => 1.0,
        }
    }

    /// V0.18: true at the fragile ends of life (child or elder), where the
    /// daily infection roll is multiplied by `FRAIL_SICK_MULTIPLIER`.
    pub fn is_frail(&self) -> bool {
        !matches!(self.stage(), LifeStage::Adult)
    }
}

/// V0.11: a dead survivor's remains, left at the death location until a
/// player issues `PlayerCommand::Bury`, or until `CORPSE_DECAY_TICKS`
/// passes and it fades into a `Grave` on its own. Categorically not a
/// `Survivor` — never workable, assignable, or counted in any population
/// figure — so it lives in its own `GameState.corpses` vec.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Corpse {
    /// The dead survivor's former id — reused only for display (e.g. the
    /// event log), never looked up against `Vec<Survivor>` again.
    pub id: u32,
    pub name: String,
    pub x: f32,
    pub y: f32,
    /// Tick this survivor died — drives the decay-to-`Grave` timer.
    pub died_tick: u64,
    /// Set once a survivor arrives and starts performing the timed bury
    /// action, so a second player can't send another survivor to the same
    /// corpse. Counts down to 0 to complete the action (mirrors
    /// `Building.build_left`'s "remaining work" shape).
    pub bury_left: f32,
    /// Who's currently burying this corpse, if anyone.
    pub being_buried_by: Option<u32>,
}

/// V0.11: a faded, purely decorative trace left where a survivor was buried
/// or where an unburied corpse finally decayed away ("vaqtinchalik iz") —
/// itself expires after `GRAVE_FADE_TICKS`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Grave {
    pub x: f32,
    pub y: f32,
    pub created_tick: u64,
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

/// A player's authority in the world. The first to join owns it; everyone
/// else is a guest — guests have full command authority alongside the owner
/// (see `GameState::can_issue`), `Role` only gates the owner-only admin
/// actions (`Kick`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Owner,
    Guest,
}

/// Retired (2026-07-16): guest permission tiering (view-only/build/full) was
/// removed — every guest now has full authority, same as `Role::Owner`. This
/// type is kept ONLY so `legacy.rs`'s frozen `GameStateV11` mirror (and
/// earlier) can still deserialize old on-disk saves that carry this field;
/// nothing live reads or writes it anymore.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuestPermission {
    ViewOnly,
    Build,
    Full,
}
