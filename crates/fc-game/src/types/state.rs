//! The world's phase and the `GameState` aggregate — the single source of
//! truth the server simulates and every client renders. Its inherent methods
//! (temperature, heat radius, placement validation, command authority, …) are
//! the shared, deterministic queries used by the sim, the server and the
//! client alike.

use serde::{Deserialize, Serialize};

use super::*;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GamePhase {
    Running,
    Won,
    Lost,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GameState {
    pub tick: u64,
    pub win_days: u32,
    /// Row-major MAP_W x MAP_H. May be empty on the wire (tiles are only
    /// included in periodic snapshots); clients keep their last copy.
    pub tiles: Vec<Tile>,
    pub buildings: Vec<Building>,
    pub survivors: Vec<Survivor>,
    pub stock: Stockpile,
    /// 0 = off, 1..=3 heat output level.
    pub furnace_level: u8,
    pub furnace_lit: bool,
    pub cold_snap: bool,
    pub players: Vec<PlayerInfo>,
    pub phase: GamePhase,
    /// Rolling log of the most recent events.
    pub events: Vec<GameEvent>,
    /// Monotonic counter of all events ever pushed (events itself is capped).
    pub total_events: u64,
    /// Rolling co-op chat log (capped at [`MAX_CHAT`]).
    pub chat: Vec<ChatLine>,
    /// Monotonic counter of all chat lines ever pushed (chat itself is capped).
    pub total_chat: u64,
    /// Active map pings; expired each tick after [`PING_TTL_TICKS`].
    pub pings: Vec<Ping>,
    /// Ordered personal-world missions; completing all unlocks the Tunnel.
    pub missions: Vec<Mission>,
    /// The Tunnel megaproject (graduation to the Global World).
    pub tunnel: TunnelState,
    /// Set when the win was earned by completing the Tunnel (graduating to
    /// the Global World), as opposed to surviving to the day-count victory.
    /// A permanent achievement: the server carries it across the post-game
    /// world reset, so graduation keeps unlocking the central world forever.
    pub graduated: bool,
    /// True for the one shared central world (the Global World reached
    /// through the Tunnel): survivors there are account-owned settlers who
    /// never hunger or die, there is no win/lose/events/arrivals, and command
    /// authority follows settler ownership instead of owner/guest roles.
    pub central: bool,
    /// Permanently unlocked technologies.
    pub techs: Vec<Tech>,
    /// Tick until which a disease is active (0 = none).
    pub disease_until: u64,
    /// Tick until which a blizzard is active (0 = none).
    pub blizzard_until: u64,
    /// An unanswered event choice (refugee caravan), if any.
    pub pending_event: Option<CaravanOffer>,
    /// Private RNG stream for events, kept separate from the main sim RNG so
    /// adding events never perturbs mapgen/cold-snap/arrival determinism.
    pub event_rng: u64,
    /// What guests may do in this world, set by the owner.
    pub guest_perm: GuestPermission,
    /// The player id that owns this world. Set once when the very first player
    /// joins and never cleared automatically, so a momentarily-empty roster
    /// (owner mid-reconnect) can't let a stranger seize ownership.
    pub owner_id: Option<u64>,
    pub next_id: u32,
    /// SplitMix64 RNG state.
    pub rng: u64,
    /// Central world only: per-account contribution ledger (see
    /// [`ContributionTotals`]). Always empty in personal/guest worlds.
    pub central_ledger: Vec<LedgerEntry>,
    /// The city's leader (V0.7), by survivor id. `None` in the central world
    /// (the shared city has no single leader) and in any world that hasn't
    /// appointed one. While alive: `LEADER_PRODUCTION_BONUS`.
    pub leader: Option<u32>,
    /// Tick until which the city mourns a dead leader (0 = none / not
    /// mourning). During mourning: `MOURNING_PRODUCTION_PENALTY`, and the
    /// caravan choice auto-resolves to reject instead of accepting input.
    pub mourning_until: u64,
    /// 0..=100 colony morale (V0.7), starting at `MORALE_START`. Feeds
    /// `morale_multiplier` into the same production math as every other
    /// colony-wide multiplier.
    pub morale: f32,
    /// Traveler(s) currently waiting at the Tunnel to join the colony, if
    /// any (V0.9). See `events::TunnelMigrant` and `tick.rs`'s tunnel-migrant
    /// block for the spawn/absorb/expire lifecycle.
    pub pending_migrant: Option<TunnelMigrant>,
    /// V0.10: abstract deer/rabbit populations the `HunterHut` hunts from
    /// (see [`Wildlife`]).
    pub wildlife: Wildlife,
    /// V0.11: dead survivors awaiting burial (or natural decay into a
    /// `Grave`) — see [`Corpse`], `PlayerCommand::Bury`.
    pub corpses: Vec<Corpse>,
    /// V0.11: faded traces left by burial or corpse decay — see [`Grave`].
    pub graves: Vec<Grave>,
    /// V0.12: abstract cow/sheep populations the `Farmhouse` raises from
    /// (see [`Livestock`]).
    pub livestock: Livestock,
    /// V0.13: a trade caravan currently on the road through the Tunnel, if
    /// any — see [`TradeCaravan`], `PlayerCommand::DispatchTradeCaravan`.
    pub pending_caravan: Option<TradeCaravan>,
}

impl GameState {
    /// One-time, idempotent, self-healing repair for a real production bug
    /// (2026-07-14): a `BuildingKind` enum reorder briefly shipped with
    /// `Tunnel` not last, silently mis-decoding some worlds' Tunnel building
    /// as a different kind — which kind depends on exactly when that world's
    /// Tunnel was (re)created relative to the buggy deploy window, so a
    /// single wire-index fix can't correct every affected save by itself.
    /// The Tunnel is always a fixed, singular, always-present, non-buildable
    /// fixture at `(TUNNEL_X, TUNNEL_Y)` (see `mapgen`) — nothing else is
    /// ever placed there, so if no building currently has
    /// `BuildingKind::Tunnel` but one sits exactly at that position, it can
    /// only ever be this mislabeling and is safe to relabel back. Called on
    /// every load (`persist::load_at`); a no-op once a world's Tunnel
    /// already decodes correctly, so safe to keep calling indefinitely.
    pub fn repair_mislabeled_tunnel(&mut self) {
        if self.buildings.iter().any(|b| b.kind == BuildingKind::Tunnel) {
            return;
        }
        if let Some(b) = self.buildings.iter_mut().find(|b| b.x == TUNNEL_X && b.y == TUNNEL_Y) {
            b.kind = BuildingKind::Tunnel;
        }
    }

    pub fn day(&self) -> u32 {
        (self.tick / TICKS_PER_DAY) as u32 + 1
    }

    /// 0.0 = midnight, 0.5 = midday.
    pub fn time_of_day(&self) -> f32 {
        (self.tick % TICKS_PER_DAY) as f32 / TICKS_PER_DAY as f32
    }

    pub fn is_night(&self) -> bool {
        let t = self.time_of_day();
        !(0.25..0.75).contains(&t)
    }

    /// Outside temperature in degrees Celsius.
    pub fn temperature(&self) -> f32 {
        let day = self.day() as f32;
        let base = -4.0 - 1.2 * (day - 1.0);
        let t = self.time_of_day();
        let diurnal = -6.0 * (std::f32::consts::TAU * t).cos();
        let snap = if self.cold_snap && t >= 0.7 { -10.0 } else { 0.0 };
        let blizzard = if self.blizzard_active() { BLIZZARD_COLD } else { 0.0 };
        base + diurnal + snap + blizzard
    }

    /// Heat radius in tiles around the furnace center; 0 when unlit. Scaled
    /// to match the furnace's own (small, `render/buildings.rs`) physical
    /// footprint. V0.9: a rough "gulxan" (`Building.level` 1-6) still grows
    /// its reach a little each level it climbs — same "keeps morphing, not
    /// just one jump" shape the model itself follows — even though its
    /// burn-intensity dial stays locked at 1 the whole time (see
    /// `SetFurnaceLevel`'s `too_young` gate). Only once it's upgraded into
    /// an established "Pech" (level 7+) does the full player-controlled
    /// 2-14 tile range unlock.
    pub fn heat_radius(&self) -> f32 {
        if !(self.furnace_lit && self.furnace_level > 0) {
            return 0.0;
        }
        let struct_level = self
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Furnace)
            .map_or(1, |b| b.level);
        if struct_level >= 7 {
            2.0 + 4.0 * self.furnace_level as f32
        } else {
            1.5 + 0.3 * struct_level.saturating_sub(1) as f32
        }
    }

    /// Center of the 2x2 furnace in tile coordinates.
    pub fn furnace_center() -> (f32, f32) {
        (FURNACE_X as f32 + 1.0, FURNACE_Y as f32 + 1.0)
    }

    /// The tile at `(x, y)`, or `None` when the coordinates are out of
    /// bounds — or when `tiles` itself is short/empty, which is what a raw
    /// protocol consumer holds on a delta snapshot (`Included { tiles: false }`
    /// ships `tiles: Vec::new()`; the client's `GameView` merge refills it,
    /// but nothing forces other consumers to). Indexing here used to panic
    /// on exactly that case.
    pub fn tile(&self, x: u8, y: u8) -> Option<&Tile> {
        self.tiles.get(tile_index(x, y))
    }

    pub fn building_at(&self, x: u8, y: u8) -> Option<&Building> {
        self.buildings.iter().find(|b| {
            let (w, h) = b.kind.size();
            x >= b.x && x < b.x + w && y >= b.y && y < b.y + h
        })
    }

    pub fn find_building(&self, id: u32) -> Option<&Building> {
        self.buildings.iter().find(|b| b.id == id)
    }

    /// Look up a connected player by id.
    pub fn player(&self, id: u64) -> Option<&PlayerInfo> {
        self.players.iter().find(|p| p.id == id)
    }

    pub fn is_owner(&self, id: u64) -> bool {
        self.player(id).map(|p| p.role == Role::Owner).unwrap_or(false)
    }

    /// Live progress value for a mission, compared against `kind.target()`.
    pub fn mission_current(&self, kind: MissionKind) -> u32 {
        match kind {
            // Only FINISHED buildings count toward build missions — a site
            // under construction isn't a tent/sawmill yet (V0.8).
            MissionKind::BuildTents(_) => self
                .buildings
                .iter()
                .filter(|b| b.kind == BuildingKind::Tent && !b.under_construction())
                .count() as u32,
            MissionKind::Population(_) => self.survivors.len() as u32,
            MissionKind::Sawmills(_) => self
                .buildings
                .iter()
                .filter(|b| b.kind == BuildingKind::Sawmill && !b.under_construction())
                .count() as u32,
            MissionKind::StockpileCoal(_) => self.stock.coal.max(0.0) as u32,
            MissionKind::SurviveDays(_) => self.day(),
        }
    }

    pub fn all_missions_done(&self) -> bool {
        !self.missions.is_empty() && self.missions.iter().all(|m| m.done)
    }

    pub fn has_tech(&self, t: Tech) -> bool {
        self.techs.contains(&t)
    }

    pub fn disease_active(&self) -> bool {
        self.tick < self.disease_until
    }

    pub fn blizzard_active(&self) -> bool {
        self.tick < self.blizzard_until
    }

    /// Whether the appointed leader is still among the living. `false` for
    /// the central world (no leader is ever set there) and for any world
    /// with no leader appointed.
    pub fn leader_alive(&self) -> bool {
        self.leader.is_some_and(|id| self.survivors.iter().any(|s| s.id == id))
    }

    pub fn mourning_active(&self) -> bool {
        self.tick < self.mourning_until
    }

    /// Colony-wide production multiplier from leadership: a bonus while the
    /// leader lives, a penalty while the city mourns a dead one, identity
    /// otherwise. Mutually exclusive by construction: `mourning_until` is
    /// only ever set at the moment `leader` is cleared, and `SetLeader`
    /// clears `mourning_until` back to 0 the moment a new leader is
    /// appointed — so a living leader and active mourning never coexist.
    pub fn leader_multiplier(&self) -> f32 {
        if self.leader_alive() {
            LEADER_PRODUCTION_BONUS
        } else if self.mourning_active() {
            MOURNING_PRODUCTION_PENALTY
        } else {
            1.0
        }
    }

    /// Colony-wide production multiplier from morale (banded, see
    /// `MORALE_START`'s doc comment for why a fresh world's multiplier must
    /// be exactly 1.0).
    pub fn morale_multiplier(&self) -> f32 {
        if self.morale < 25.0 {
            0.8
        } else if self.morale < 50.0 {
            0.9
        } else if self.morale <= 75.0 {
            1.0
        } else {
            1.1
        }
    }

    /// Every colony-wide (not per-survivor) production multiplier, composed
    /// in one place so new ones can't be forgotten at a call site. Order
    /// doesn't matter mathematically (plain multiplication), but is fixed
    /// here as tools -> leader -> morale to match the brief.
    pub fn colony_production_multiplier(&self) -> f32 {
        let tools = if self.has_tech(Tech::Tools) { TECH_TOOLS_PRODUCTION } else { 1.0 };
        tools * self.leader_multiplier() * self.morale_multiplier()
    }

    /// Deterministic per-id spawn offset near the furnace, used both for
    /// brand-new survivors and for migrated saves that predate positions
    /// (V3 -> V4): small, stable, and collision-free enough to just spread
    /// people out without needing real pathfinding-aware placement.
    pub fn spawn_position(id: u32) -> (f32, f32) {
        let (fx, fy) = Self::furnace_center();
        // Same SplitMix64 finalizer as `Profession::from_id_hash`, salted
        // differently so the two derived values aren't correlated.
        let mut z = (id as u64 ^ 0xA5A5_5A5A_1234_ABCD) ^ 0x9E37_79B9_7F4A_7C15;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        let angle = (z as u32 % 360) as f32 * std::f32::consts::PI / 180.0;
        let radius = 1.5 + ((z >> 9) as u32 % 100) as f32 / 100.0 * 2.5; // 1.5..=4.0
        let (x, y) = (fx + angle.cos() * radius, fy + angle.sin() * radius);
        (x.clamp(0.0, MAP_W as f32 - 0.01), y.clamp(0.0, MAP_H as f32 - 0.01))
    }

    /// Whether any connected player currently owns the world.
    pub fn owner_present(&self) -> bool {
        self.players.iter().any(|p| p.role == Role::Owner)
    }

    /// The account `pid` signed in with, if they're connected and logged in.
    pub fn player_account(&self, pid: u64) -> Option<i64> {
        self.player(pid).and_then(|p| p.account)
    }

    /// How many central-world settlers `account` owns.
    pub fn owned_settlers(&self, account: i64) -> usize {
        self.survivors
            .iter()
            .filter(|s| s.owner == Some(account))
            .count()
    }

    /// Mutable access to `account`'s central-world ledger row, creating it
    /// (zeroed) on first use. Callers apply their own delta via the closure —
    /// keeps every call site a one-liner instead of a find-or-insert dance.
    pub fn credit_ledger(&mut self, account: i64, f: impl FnOnce(&mut ContributionTotals)) {
        match self.central_ledger.iter_mut().find(|e| e.account == account) {
            Some(entry) => f(&mut entry.totals),
            None => {
                let mut totals = ContributionTotals::default();
                f(&mut totals);
                self.central_ledger.push(LedgerEntry { account, totals });
            }
        }
    }

    /// `account`'s central-world contribution totals, or zero if they have
    /// none recorded yet (never having placed/staffed anything there).
    pub fn ledger_for(&self, account: i64) -> ContributionTotals {
        self.central_ledger
            .iter()
            .find(|e| e.account == account)
            .map(|e| e.totals)
            .unwrap_or_default()
    }

    /// The single source of truth for command authority, shared by the server
    /// (enforcement), the sim (`apply_command`) and the client (UI greying).
    /// Unknown players (unit tests, trusted local calls) and owner-less worlds
    /// are unrestricted so co-op never dead-ends when the owner leaves.
    pub fn can_issue(&self, pid: u64, cmd: &PlayerCommand) -> bool {
        // The central world has no owner/guest hierarchy at all — authority
        // follows settler ownership. This branch must come before the
        // unknown-player and no-owner fallbacks below: both are unrestricted,
        // and an owner-less shared map full of strangers is exactly where
        // "anyone commands anyone's settlers" must never happen.
        if self.central {
            let account = self.player_account(pid);
            return match cmd {
                // Anyone may add to the shared city; tearing down is limited
                // to whoever placed it. Account-owned central buildings (set
                // from the builder's account at placement time) require that
                // SAME account — any of its connections, any session — while
                // legacy central buildings from before account ownership
                // existed (`owner_account: None`, migration default) stay
                // demolishable by the placing session, as they always were.
                PlayerCommand::Place { .. } => true,
                PlayerCommand::Demolish { building } => {
                    self.find_building(*building).is_some_and(|b| match b.owner_account {
                        Some(owner_acc) => account == Some(owner_acc),
                        None => b.owner == Some(pid),
                    })
                }
                // Upgrading follows the same ownership rule as tearing down:
                // only whoever placed the building improves it.
                PlayerCommand::UpgradeBuilding { building } => {
                    self.find_building(*building).is_some_and(|b| match b.owner_account {
                        Some(owner_acc) => account == Some(owner_acc),
                        None => b.owner == Some(pid),
                    })
                }
                // Relocating is as much a structural change as upgrading —
                // same ownership rule.
                PlayerCommand::RelocateBuilding { building, .. } => {
                    self.find_building(*building).is_some_and(|b| match b.owner_account {
                        Some(owner_acc) => account == Some(owner_acc),
                        None => b.owner == Some(pid),
                    })
                }
                // Only your own settlers, by account identity.
                PlayerCommand::AssignSurvivor { survivor, .. } => {
                    account.is_some()
                        && self
                            .survivors
                            .iter()
                            .find(|s| s.id == *survivor)
                            .is_some_and(|s| s.owner == account)
                }
                // Mirrors `AssignSurvivor` exactly: walking a settler is just
                // as much "commanding your own settler" as staffing them is.
                PlayerCommand::MoveSurvivor { survivor, .. }
                | PlayerCommand::ChopTile { survivor, .. } => {
                    account.is_some()
                        && self
                            .survivors
                            .iter()
                            .find(|s| s.id == *survivor)
                            .is_some_and(|s| s.owner == account)
                }
                // The anonymous +/- pool can't tell whose settler it would
                // move, so it has no meaning where every settler is owned.
                PlayerCommand::AdjustWorkers { .. } => false,
                // The shared city has no single leader — see `GameState::leader`.
                PlayerCommand::SetLeader { .. } => false,
                // Communal fixtures and personal-world progression have no
                // per-player authority in the central world.
                PlayerCommand::SetFurnaceLevel { .. }
                | PlayerCommand::InvestTunnel
                | PlayerCommand::Research { .. }
                | PlayerCommand::RespondEvent { .. } => false,
                // Central-world settlers never die (see `GameState.central`'s
                // doc comment) — no corpse can ever exist there, so this is
                // never meaningfully issuable in this branch.
                PlayerCommand::Bury { .. } => false,
                // The Tunnel trade caravan is personal-world progression
                // (mirrors `InvestTunnel`) — there's no "Global World" to
                // trade with from inside the Global World itself.
                PlayerCommand::DispatchTradeCaravan { .. } => false,
            };
        }
        let Some(p) = self.player(pid) else {
            return true;
        };
        if p.role == Role::Owner || !self.owner_present() {
            return true;
        }
        match self.guest_perm {
            GuestPermission::Full => true,
            GuestPermission::ViewOnly => false,
            GuestPermission::Build => match cmd {
                // Building (incl. upgrades), worker assignment and the shared
                // Tunnel/research goals are the cooperative core, so guests
                // may all pitch in.
                PlayerCommand::Place { .. }
                | PlayerCommand::UpgradeBuilding { .. }
                | PlayerCommand::AdjustWorkers { .. }
                | PlayerCommand::AssignSurvivor { .. }
                | PlayerCommand::MoveSurvivor { .. }
                | PlayerCommand::ChopTile { .. }
                | PlayerCommand::Bury { .. }
                | PlayerCommand::InvestTunnel
                | PlayerCommand::Research { .. }
                | PlayerCommand::RespondEvent { .. }
                | PlayerCommand::DispatchTradeCaravan { .. }
                | PlayerCommand::RelocateBuilding { .. } => true,
                // Guests may only tear down their own buildings, never touch the
                // furnace, under the Build policy.
                PlayerCommand::Demolish { building } => self
                    .find_building(*building)
                    .map(|b| b.owner == Some(pid))
                    .unwrap_or(false),
                // Owner-only admin, same tier as `SetFurnaceLevel`: appointing
                // a leader is a whole-city decision, not routine co-op upkeep.
                PlayerCommand::SetFurnaceLevel { .. } | PlayerCommand::SetLeader { .. } => false,
            },
        }
    }

    pub fn total_workers(&self) -> u32 {
        self.buildings.iter().map(|b| b.workers as u32).sum()
    }

    pub fn idle_workers(&self) -> u32 {
        (self.survivors.len() as u32).saturating_sub(self.total_workers())
    }

    pub fn housing_capacity(&self) -> usize {
        // Level-aware and construction-aware (an unfinished Tent site
        // shelters nobody) — see `Building::housing_slots`.
        self.buildings.iter().map(|b| b.housing_slots()).sum()
    }

    /// Chebyshev distance from a tile's center to the furnace center.
    pub fn dist_to_furnace(x: u8, y: u8) -> f32 {
        let (fx, fy) = Self::furnace_center();
        let dx = (x as f32 + 0.5 - fx).abs();
        let dy = (y as f32 + 0.5 - fy).abs();
        dx.max(dy)
    }

    /// Full placement validation, shared by client preview and server authority.
    pub fn can_place(&self, kind: BuildingKind, x: u8, y: u8) -> Result<(), &'static str> {
        if !kind.buildable() {
            return Err("That cannot be built");
        }
        let (w, h) = kind.size();
        if x as usize + w as usize > MAP_W || y as usize + h as usize > MAP_H {
            return Err("Out of bounds");
        }
        for dy in 0..h {
            for dx in 0..w {
                let (tx, ty) = (x + dx, y + dy);
                if self.building_at(tx, ty).is_some() {
                    return Err("Space is occupied");
                }
                let Some(tile) = self.tile(tx, ty) else {
                    return Err("Out of bounds");
                };
                match kind {
                    BuildingKind::CoalMine => {
                        if tile.terrain != Terrain::Coal || tile.deposit == 0 {
                            return Err("Needs a coal deposit");
                        }
                    }
                    _ => {
                        if tile.terrain != Terrain::Snow {
                            return Err("Ground must be clear");
                        }
                    }
                }
            }
        }
        if self.stock.wood < kind.cost_wood() as f32 {
            return Err("Not enough wood");
        }
        Ok(())
    }

    /// V0.14: full placement validation for `PlayerCommand::RelocateBuilding`
    /// — shared by client preview and server authority, same convention as
    /// `can_place`. Mirrors it closely, with two differences: the building's
    /// OWN current footprint is excluded from the occupancy check (it's
    /// vacating that spot, not colliding with itself), and there's no wood
    /// cost (relocating is free).
    pub fn can_relocate(&self, building: u32, x: u8, y: u8) -> Result<(), &'static str> {
        let Some(b) = self.find_building(building) else {
            return Err("No such building");
        };
        if !b.kind.buildable() {
            return Err("That cannot be relocated");
        }
        if b.under_construction() {
            return Err("Still under construction");
        }
        let kind = b.kind;
        let (w, h) = kind.size();
        if x as usize + w as usize > MAP_W || y as usize + h as usize > MAP_H {
            return Err("Out of bounds");
        }
        for dy in 0..h {
            for dx in 0..w {
                let (tx, ty) = (x + dx, y + dy);
                if let Some(occupant) = self.building_at(tx, ty) {
                    if occupant.id != building {
                        return Err("Space is occupied");
                    }
                }
                let Some(tile) = self.tile(tx, ty) else {
                    return Err("Out of bounds");
                };
                match kind {
                    BuildingKind::CoalMine => {
                        if tile.terrain != Terrain::Coal || tile.deposit == 0 {
                            return Err("Needs a coal deposit");
                        }
                    }
                    _ => {
                        if tile.terrain != Terrain::Snow {
                            return Err("Ground must be clear");
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// How many forest units are harvestable within `r` tiles of (x, y) —
    /// used by the client to warn about badly placed sawmills.
    pub fn forest_near(&self, x: u8, y: u8, r: i32) -> u32 {
        let mut total = 0u32;
        for dy in -r..=r {
            for dx in -r..=r {
                let (tx, ty) = (x as i32 + dx, y as i32 + dy);
                if in_bounds(tx, ty) {
                    let t = &self.tiles[tile_index(tx as u8, ty as u8)];
                    if t.terrain == Terrain::Forest {
                        total += t.deposit as u32;
                    }
                }
            }
        }
        total
    }
}
