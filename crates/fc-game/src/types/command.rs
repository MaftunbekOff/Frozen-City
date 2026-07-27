//! The commands a player may issue. The server validates every one of them
//! through [`super::GameState::can_issue`].

use serde::{Deserialize, Serialize};

use super::{BuildingKind, ExpeditionSite, Law, Tech, TradeGood};

/// Commands a player may issue; the server validates every one of them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum PlayerCommand {
    /// Place a new building. `facing` (0..=3, quarter-turns) is the CoC-style
    /// confirm flow's chosen orientation — purely visual (see `Building::facing`);
    /// the server clamps it defensively but never lets it affect placement rules.
    Place { kind: BuildingKind, x: u8, y: u8, facing: u8 },
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
    /// V0.16: turn a finished, buildable building a quarter-turn in place
    /// (`Building::facing` += 1 mod 4). Purely visual, but modelled on
    /// `RelocateBuilding`: free of wood yet it re-enters
    /// `Building::build_left`/`under_construction` at the same discounted
    /// duration (`RELOCATE_WORKDAYS_FACTOR`) — the crew has to re-square the
    /// structure on its new heading. `level`/worker assignments carry over.
    RotateBuilding { building: u32 },
    /// V0.18: send a party of named survivors out to `site` for several days.
    /// They leave `GameState.survivors` entirely for the trip (see
    /// `Expedition`), so the colony genuinely loses their hands while they're
    /// gone. Validated by `GameState::can_launch_expedition`; provisions are
    /// charged up front.
    LaunchExpedition { site: ExpeditionSite, members: Vec<u32> },
    /// V0.18: order a party home early. They keep the haul earned so far and
    /// arrive after the same fraction of road they've already walked (see
    /// `sim::expedition`), so a recall is a real decision rather than a free
    /// undo.
    RecallExpedition { expedition: u32 },
    /// V0.18: pass a standing law (`GameState::can_enact_law`).
    EnactLaw { law: Law },
    /// V0.18: repeal a standing law (`GameState::can_repeal_law`).
    RepealLaw { law: Law },
    /// V0.18: move a finished building AND set its heading in one commit —
    /// what the CoC-style confirm bar sends when the player relocates
    /// (`ui::placement`). This has to be ONE command rather than a
    /// `RelocateBuilding` followed by a `RotateBuilding`: relocating puts the
    /// building back under construction, and `can_rotate` refuses a
    /// construction site, so the follow-up rotate would always be dropped.
    /// Validation, cost and rebuild timer are `RelocateBuilding`'s exactly
    /// (`can_relocate`, free of wood, `RELOCATE_WORKDAYS_FACTOR`); `facing`
    /// stays purely visual and is clamped defensively, like `Place`'s.
    ///
    /// `RelocateBuilding`/`RotateBuilding` remain — they are still valid,
    /// still tested, and a client that only wants to move (or only to turn)
    /// a building may keep using them; the confirm-bar flow simply always has
    /// both answers at once and so always sends this.
    RelocateFacing { building: u32, x: u8, y: u8, facing: u8 },
    /// V0.19: lay road on every listed tile at once — one command per drag,
    /// not one per tile, because a useful road is dozens of tiles and the
    /// player draws it in a single gesture. Charges `ROAD_COST_WOOD` per tile
    /// actually laid (tiles that are already road, or that can't take one,
    /// are skipped and cost nothing), and the colony can't go into debt: if
    /// the wood runs out partway the rest of the drag is simply not laid.
    /// Capped at `MAX_ROAD_TILES_PER_COMMAND` so a crafted client can't send
    /// an unbounded list.
    BuildRoad { tiles: Vec<(u8, u8)> },
    /// V0.19: tear road up again, refunding `ROAD_REFUND` of its cost. Same
    /// batch shape as `BuildRoad` (the same drag gesture, in reverse).
    RemoveRoad { tiles: Vec<(u8, u8)> },
    /// V0.19: send a named survivor to shovel one tile clear — a
    /// walk-there-then-work errand in the exact shape of `Bury`
    /// (`GameState.clear_orders`, `ClearOrder::work_left`), and like it,
    /// unassigns them from their post first. The standing alternative is a
    /// staffed `BuildingKind::SnowCrew`, which keeps its whole radius clear
    /// without being told; this is the manual answer for reopening one
    /// stretch right now.
    ClearSnow { survivor: u32, x: u8, y: u8 },
    /// V0.20: buy or improve one fitting inside a building — `slot` indexes
    /// `BuildingKind::furnishings()`. Buying and upgrading are the same
    /// command because they are the same act from the player's side: spend
    /// wood, that fitting gets better. Unlike `UpgradeBuilding` this does NOT
    /// put the building back under construction — furnishing a room is
    /// carrying a table in, not rebuilding the walls.
    UpgradeFurnishing { building: u32, slot: u8 },
}
