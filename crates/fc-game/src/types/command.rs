//! The commands a player may issue. The server validates every one of them
//! through [`super::GameState::can_issue`].

use serde::{Deserialize, Serialize};

use super::{BuildingKind, Tech, TradeGood};

/// Commands a player may issue; the server validates every one of them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum PlayerCommand {
    Place { kind: BuildingKind, x: u8, y: u8 },
    Demolish { building: u32 },
    AdjustWorkers { building: u32, delta: i8 },
    /// Assign (or, with `building: None`, unassign) one named survivor to
    /// work at a specific building. Distinct from `AdjustWorkers`: this
    /// targets an identity, not a headcount.
    AssignSurvivor { survivor: u32, building: Option<u32> },
    SetFurnaceLevel { level: u8 },
    /// Contribute resources toward excavating the Tunnel (once unlocked).
    InvestTunnel,
    /// Spend resources to permanently unlock a technology.
    Research { tech: Tech },
    /// Answer a pending event choice (e.g. a refugee caravan).
    RespondEvent { accept: bool },
    /// V0.7: walk a named survivor to a tile under player control. Unassigns
    /// them from work (a manually walked survivor becomes idle) — see the
    /// `move_target` field doc on `Survivor`.
    /// (This and everything below it: appended in order, never reordered —
    /// bincode enum indices are positional.)
    MoveSurvivor { survivor: u32, x: u8, y: u8 },
    /// V0.7: appoint a survivor as the city's leader. Owner-only in
    /// owned worlds; refused outright in the central world (no single
    /// leader for the shared city — see `GameState::leader`).
    SetLeader { survivor: u32 },
    /// V0.8: raise a finished building one level (2..=`BUILDING_MAX_LEVEL`).
    /// Wood is charged up front (`BuildingKind::upgrade_cost_wood`), then the
    /// building becomes a construction site again (`Building::build_left`)
    /// and produces nothing until its crew finishes the work.
    UpgradeBuilding { building: u32 },
    /// Manually send a named survivor to chop a specific forest tile —
    /// unassigns them from work exactly like `MoveSurvivor` (a manual
    /// command always overrides the standing job), then walks them there
    /// and chops it on arrival (`Survivor::chop_target`, `tick.rs`'s
    /// chop/carry block). Unlike the Furnace-building errand that same
    /// field also drives, a manual chop credits the wood to the stockpile
    /// immediately on the chop — there's no assigned building to carry it
    /// back to.
    ChopTile { survivor: u32, x: u8, y: u8 },
    /// V0.11: send a named (living) survivor to bury a corpse — unassigns
    /// them from work exactly like `MoveSurvivor`/`ChopTile`, walks them to
    /// the body, then performs a timed action on arrival (see
    /// `Survivor::bury_target`, `tick.rs`'s burial block).
    Bury { survivor: u32, corpse: u32 },
    /// V0.13: send a caravan through the Tunnel to the Global World — either
    /// to sell `amount` of `good` for gold, or (if `selling` is false) to
    /// buy `amount` of it with banked gold. See `GameState.pending_caravan`.
    DispatchTradeCaravan { good: TradeGood, amount: u32, selling: bool },
    /// V0.14: move a finished, buildable building to a new tile — free
    /// (no wood), but re-enters `Building::build_left`/`under_construction`
    /// at a discounted duration (see `RELOCATE_WORKDAYS_FACTOR`), same
    /// mechanism `UpgradeBuilding` reuses. `level` and any named worker
    /// assignments carry over unchanged; only `x`/`y` move.
    RelocateBuilding { building: u32, x: u8, y: u8 },
}
