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
    /// V0.17: RNG stream for the once-daily infection roll (see
    /// `ILLNESS_TICK`). Deliberately its own stream rather than a few extra
    /// draws from `rng`/`event_rng`: illness rolls happen on days those two
    /// streams already drive arrivals, births and weather, and sharing one
    /// would silently re-sequence every existing deterministic outcome.
    ///
    /// IMPORTANT: bincode serializes this struct positionally — new fields
    /// stay APPENDED here at the end (same rule as `Survivor`).
    pub illness_rng: u64,
    /// V0.18: parties currently away from the valley. Their members are NOT
    /// in `survivors` while they're gone — see the [`Expedition`] module doc
    /// for why they leave the roster entirely instead of carrying an "away"
    /// flag through every per-tick system.
    pub expeditions: Vec<Expedition>,
    /// V0.18: RNG stream for expedition hazard/haul rolls. Its own stream for
    /// the same reason as `illness_rng`: an expedition can be launched on any
    /// tick of any day, and sharing `rng`/`event_rng` would silently
    /// re-sequence arrivals, births and weather from that point on.
    pub expedition_rng: u64,
    /// V0.18: the standing laws (see [`Law`]). Capped at `MAX_ACTIVE_LAWS`.
    pub laws: Vec<Law>,
    /// V0.18: tick until which the law book is closed after an enact/repeal
    /// (`LAW_COOLDOWN_TICKS`), so policy is a decision rather than a dial.
    pub law_cooldown_until: u64,
    /// V0.18: RNG stream for the daily aging/pairing/old-age rolls — its own
    /// stream, same reasoning as `illness_rng`/`expedition_rng`.
    pub lifecycle_rng: u64,
    /// V0.18: how many EXTRA mission cycles have been issued beyond the
    /// original one (0 on a fresh world). The Tunnel's unlock latch
    /// (`tunnel.unlocked`) is set once and never cleared, so re-issuing
    /// missions can never re-lock it.
    pub mission_cycle: u32,
    /// V0.19: tiles a survivor has been sent to shovel clear
    /// (`PlayerCommand::ClearSnow`). Its own vec rather than a field on
    /// `Survivor` for the same reason `corpses` is one: the order is about a
    /// PLACE, it outlives whoever was walking to it (they can die on the way),
    /// and keeping it here means no `Survivor` field to migrate.
    pub clear_orders: Vec<ClearOrder>,
}

/// V0.19: one standing "shovel this tile" order — mirrors `Corpse`'s
/// remaining-work shape (`bury_left`) exactly, because it is the same kind of
/// thing: walk there, then spend time.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ClearOrder {
    pub x: u8,
    pub y: u8,
    /// Who is walking to it. Cleared (and the order dropped) if they die.
    pub survivor: u32,
    /// Remaining work in ticks, counting down only while the assigned
    /// survivor is actually standing on the tile.
    pub work_left: f32,
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
        tools * self.leader_multiplier() * self.morale_multiplier() * self.law_production_multiplier()
    }

    // --- V0.18: the law book. Every effect a law has on the sim is read
    // through one of these composed queries, so `sim::tick` never matches on
    // a specific `Law` variant and a new law is a data change, not a new
    // branch scattered across the tick. With no law enacted every multiplier
    // below is exactly 1.0 (and the morale/funeral sums exactly 0.0), so a
    // world that never opens the book behaves precisely as it did pre-V0.18.

    pub fn has_law(&self, law: Law) -> bool {
        self.laws.contains(&law)
    }

    /// Composes `f` over every enacted law multiplicatively.
    fn law_product(&self, f: impl Fn(Law) -> f32) -> f32 {
        self.laws.iter().map(|l| f(*l)).product()
    }

    pub fn law_production_multiplier(&self) -> f32 {
        self.law_product(Law::production_factor)
    }

    pub fn law_food_multiplier(&self) -> f32 {
        self.law_product(Law::food_factor)
    }

    pub fn law_fatigue_multiplier(&self) -> f32 {
        self.law_product(Law::fatigue_factor)
    }

    pub fn law_rest_multiplier(&self) -> f32 {
        self.law_product(Law::rest_factor)
    }

    pub fn law_contagion_multiplier(&self) -> f32 {
        self.law_product(Law::contagion_factor)
    }

    pub fn law_xp_multiplier(&self) -> f32 {
        self.law_product(Law::xp_factor)
    }

    pub fn law_death_morale_multiplier(&self) -> f32 {
        self.law_product(Law::death_morale_factor)
    }

    /// Sum (not product) — these are additive per-day morale adjustments,
    /// exactly like the Kitchen/Hospital/leader terms they join in `tick`.
    pub fn law_morale_per_day(&self) -> f32 {
        self.laws.iter().map(|l| l.morale_per_day()).sum()
    }

    /// Wood a single funeral costs under the current book (0.0 with no
    /// funeral law enacted).
    pub fn law_funeral_wood(&self) -> f32 {
        self.laws.iter().map(|l| l.funeral_wood()).sum()
    }

    /// Shared validation for `PlayerCommand::EnactLaw` — client greying and
    /// server authority read the same answer, the `can_place` convention.
    pub fn can_enact_law(&self, law: Law) -> Result<(), &'static str> {
        if self.central {
            return Err("The Global World has no lawbook");
        }
        if self.day() < LAW_MIN_DAY {
            return Err("Too early to pass laws");
        }
        if self.tick < self.law_cooldown_until {
            return Err("The council is still deliberating");
        }
        if self.has_law(law) {
            return Err("Already in force");
        }
        if self.laws.len() >= MAX_ACTIVE_LAWS {
            return Err("Too many laws already stand");
        }
        Ok(())
    }

    /// Shared validation for `PlayerCommand::RepealLaw`.
    pub fn can_repeal_law(&self, law: Law) -> Result<(), &'static str> {
        if !self.has_law(law) {
            return Err("Not in force");
        }
        if self.tick < self.law_cooldown_until {
            return Err("The council is still deliberating");
        }
        Ok(())
    }

    // --- V0.18: expeditions ---

    /// How many people are away on expeditions right now. They are NOT in
    /// `survivors`, so every population figure the colony reports (housing,
    /// food, missions, defeat check) already excludes them by construction;
    /// this is purely for display and for the "don't empty the city" guard.
    pub fn people_away(&self) -> usize {
        self.expeditions.iter().map(|e| e.party.len()).sum()
    }

    /// Shared validation for `PlayerCommand::LaunchExpedition`. `members` are
    /// survivor ids; duplicates, unknown ids, children, the sick and anyone
    /// mid-errand are all refused here rather than silently dropped, so the
    /// client can explain exactly why the button is dead.
    pub fn can_launch_expedition(
        &self,
        _site: ExpeditionSite,
        members: &[u32],
    ) -> Result<(), &'static str> {
        if self.central {
            return Err("No expeditions leave the Global World");
        }
        if self.day() < EXPEDITION_MIN_DAY {
            return Err("The colony is not ready to send anyone out");
        }
        if self.expeditions.len() >= EXPEDITION_MAX_ACTIVE {
            return Err("A party is already out there");
        }
        if members.len() < EXPEDITION_MIN_PARTY {
            return Err("Too few for a party");
        }
        if members.len() > EXPEDITION_MAX_PARTY {
            return Err("Too many for one party");
        }
        for (i, id) in members.iter().enumerate() {
            if members[..i].contains(id) {
                return Err("Someone is listed twice");
            }
            let Some(s) = self.survivors.iter().find(|s| s.id == *id) else {
                return Err("Someone is no longer here");
            };
            if s.stage() == LifeStage::Child {
                return Err("Children do not travel");
            }
            if s.is_sick() {
                return Err("The sick cannot travel");
            }
        }
        if self.survivors.len().saturating_sub(members.len()) < EXPEDITION_MIN_STAY_HOME {
            return Err("Too few would be left at home");
        }
        Ok(())
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
    /// Outside the central world (see below), every connected player — owner
    /// or guest alike — has full command authority; the only admin actions
    /// that stay owner-only (`Kick`) aren't `PlayerCommand`s at all and are
    /// checked separately via `is_owner` at the call site.
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
                // only whoever placed the building improves it. V0.20's
                // interior fittings are an improvement to that same building,
                // so they share the arm.
                PlayerCommand::UpgradeBuilding { building }
                | PlayerCommand::UpgradeFurnishing { building, .. } => {
                    self.find_building(*building).is_some_and(|b| match b.owner_account {
                        Some(owner_acc) => account == Some(owner_acc),
                        None => b.owner == Some(pid),
                    })
                }
                // Relocating is as much a structural change as upgrading —
                // same ownership rule. V0.18's `RelocateFacing` is the same
                // action with a heading attached, so it shares the arm.
                PlayerCommand::RelocateBuilding { building, .. }
                | PlayerCommand::RelocateFacing { building, .. } => {
                    self.find_building(*building).is_some_and(|b| match b.owner_account {
                        Some(owner_acc) => account == Some(owner_acc),
                        None => b.owner == Some(pid),
                    })
                }
                // Rotating re-squares the structure just like relocating —
                // same ownership rule.
                PlayerCommand::RotateBuilding { building } => {
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
                // V0.18: expeditions leave from a colony, and laws govern
                // one. The Global World is neither — it's a meeting place
                // with no hunger, no weather and no council, so both are
                // personal-world-only, exactly like `Research`/`InvestTunnel`
                // above. (The way OUT of the Global World is the return
                // migration, not a `PlayerCommand` — see `ClientMsg::ReturnHome`.)
                PlayerCommand::LaunchExpedition { .. }
                | PlayerCommand::RecallExpedition { .. }
                | PlayerCommand::EnactLaw { .. }
                | PlayerCommand::RepealLaw { .. } => false,
                // V0.19: no weather runs in the Global World (`tick` returns
                // before the blizzard/event block for `central`), so no snow
                // ever falls there and clearing it is meaningless. Roads are
                // refused for a different reason: a road is a tile property
                // with no owner, and the shared map's whole authority model
                // is "you may only undo what you placed" — one account could
                // otherwise pave over, or tear up, another's approach.
                PlayerCommand::BuildRoad { .. }
                | PlayerCommand::RemoveRoad { .. }
                | PlayerCommand::ClearSnow { .. } => false,
            };
        }
        // Personal/shared-guest worlds have no per-command permission tiering
        // — any connected player, owner or guest, may issue any command.
        true
    }

    pub fn total_workers(&self) -> u32 {
        self.buildings.iter().map(|b| b.workers as u32).sum()
    }

    pub fn idle_workers(&self) -> u32 {
        (self.survivors.len() as u32).saturating_sub(self.total_workers())
    }

    /// V0.17: how many survivors are currently ill.
    pub fn sick_count(&self) -> usize {
        self.survivors.iter().filter(|s| s.is_sick()).count()
    }

    /// V0.17: how many survivors are at or past `FATIGUE_EXHAUSTED`.
    pub fn exhausted_count(&self) -> usize {
        self.survivors.iter().filter(|s| s.fatigue >= FATIGUE_EXHAUSTED).count()
    }

    /// V0.17: ids of the survivors who have a Tent bunk tonight — the first
    /// `housing_capacity()` of them in roster order, which is stable across
    /// ticks (ids only ever grow, and the roster is never reordered), so a
    /// survivor doesn't lose and regain their bed from one tick to the next.
    /// Everyone else sleeps rough where they stand: the sim uses this to pick
    /// the rest rate, and `routine_goal` uses it to decide who even walks to
    /// a Tent, so the picture and the mechanic can never disagree.
    pub fn bunked_ids(&self) -> std::collections::HashSet<u32> {
        let cap = self.housing_capacity();
        self.survivors.iter().take(cap).map(|s| s.id).collect()
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
        self.footprint_is_clear(kind, x, y, None)?;
        if self.stock.wood < kind.cost_wood() as f32 {
            return Err("Not enough wood");
        }
        Ok(())
    }

    /// Shared footprint check for [`Self::can_place`] and
    /// [`Self::can_relocate`] — `ignore` is the building vacating the spot
    /// (relocation) so it never collides with itself.
    ///
    /// V0.22: a mine's rule changed with the room footprint. It used to
    /// demand that EVERY tile it covered be a coal deposit, which was easy at
    /// 1x1 and is nearly impossible at 3x3 — deposit blobs are ragged, so the
    /// building simply became unplaceable. What actually matters is that the
    /// mine SITS ON a seam: at least one tile of coal under it, and no forest
    /// in the way of the rest. Extraction follows the same rule (see the
    /// `CoalMine` arm in `sim::tick`, which draws from the richest covered
    /// tile rather than assuming the corner one).
    fn footprint_is_clear(
        &self,
        kind: BuildingKind,
        x: u8,
        y: u8,
        ignore: Option<u32>,
    ) -> Result<(), &'static str> {
        let (w, h) = kind.size();
        let mut coal_under = false;
        for dy in 0..h {
            for dx in 0..w {
                let (tx, ty) = (x + dx, y + dy);
                if let Some(occupant) = self.building_at(tx, ty) {
                    if Some(occupant.id) != ignore {
                        return Err("Space is occupied");
                    }
                }
                let Some(tile) = self.tile(tx, ty) else {
                    return Err("Out of bounds");
                };
                match kind {
                    BuildingKind::CoalMine => {
                        // Forest would have to be cleared first; bare snow and
                        // the seam itself are both fine to build over.
                        if tile.terrain == Terrain::Forest {
                            return Err("Ground must be clear");
                        }
                        if tile.terrain == Terrain::Coal && tile.deposit > 0 {
                            coal_under = true;
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
        if kind == BuildingKind::CoalMine && !coal_under {
            return Err("Needs a coal deposit");
        }
        Ok(())
    }

    /// V0.22: the tile a Coal Mine at `(x, y)` should draw from — the richest
    /// seam under its footprint. `None` when every covered tile is spent,
    /// which is what stops extraction.
    pub fn richest_coal_under(&self, kind: BuildingKind, x: u8, y: u8) -> Option<usize> {
        let (w, h) = kind.size();
        let mut best: Option<(u16, usize)> = None;
        for dy in 0..h {
            for dx in 0..w {
                let (tx, ty) = (x + dx, y + dy);
                let Some(tile) = self.tile(tx, ty) else { continue };
                if tile.terrain == Terrain::Coal && tile.deposit > 0 {
                    let idx = tile_index(tx, ty);
                    if best.is_none_or(|(d, _)| tile.deposit > d) {
                        best = Some((tile.deposit, idx));
                    }
                }
            }
        }
        best.map(|(_, idx)| idx)
    }

    /// V0.16: validation for `PlayerCommand::RotateBuilding` — a finished,
    /// buildable building can be turned in place. Rotation never moves the
    /// footprint, so there are no coordinates to check: it's just the "is this
    /// a rotatable building right now" half of [`Self::can_relocate`].
    pub fn can_rotate(&self, building: u32) -> Result<(), &'static str> {
        let Some(b) = self.find_building(building) else {
            return Err("No such building");
        };
        if !b.kind.buildable() {
            return Err("That cannot be rotated");
        }
        if b.under_construction() {
            return Err("Still under construction");
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
        self.footprint_is_clear(kind, x, y, Some(building))
    }

    // --- V0.19: roads and snow ---

    /// Shared validation for one tile of `PlayerCommand::BuildRoad`. A road
    /// goes on open ground only: not on forest or a coal deposit (both are
    /// harvestable terrain a road would pave over), and not under a building
    /// (which occupies the tile outright). Deliberately does NOT check wood —
    /// a drag is priced as a whole and paid for tile by tile until it runs
    /// out, so affordability is the caller's business.
    pub fn can_lay_road(&self, x: u8, y: u8) -> Result<(), &'static str> {
        let Some(tile) = self.tile(x, y) else {
            return Err("Out of bounds");
        };
        if tile.road {
            return Err("Already a road");
        }
        if tile.terrain != Terrain::Snow {
            return Err("Ground must be clear");
        }
        if self.building_at(x, y).is_some() {
            return Err("Space is occupied");
        }
        Ok(())
    }

    /// V0.19: the tile a survivor standing at `(x, y)` is on, if any. Their
    /// position is a float; this is the one place it is rounded to a tile, so
    /// movement speed and snow trampling can never disagree about where
    /// somebody is.
    pub fn tile_under(&self, x: f32, y: f32) -> Option<&Tile> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let (tx, ty) = (x.floor(), y.floor());
        if !in_bounds(tx as i32, ty as i32) {
            return None;
        }
        self.tile(tx as u8, ty as u8)
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
