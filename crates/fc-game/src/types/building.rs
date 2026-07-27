//! Buildings: the kinds players can place and a placed building instance.

use serde::{Deserialize, Serialize};

use super::{
    required_furnishing_level, FurnishingKind, ROOM_SIZE, BUILDING_MAX_LEVEL, BUILD_WORKDAYS_PER_WOOD,
    FURNISHING_MAX_LEVEL, LEVEL_PRODUCTION_BONUS, TENT_CAPACITY, TENT_CAPACITY_PER_LEVEL,
    WELL_WATER_PER_WORKER_DAY,
};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuildingKind {
    Furnace,
    Tent,
    Sawmill,
    CoalMine,
    HunterHut,
    Greenhouse,
    Hospital,
    Kitchen,
    Warehouse,
    /// V0.9: the Tunnel to the Global World — a single always-present,
    /// non-buildable fixture (same convention as `Furnace`), placed once by
    /// `mapgen`. Its excavation state lives in `GameState.tunnel`
    /// (`TunnelState`), not on this `Building` (`level`/`build_left` are
    /// unused/inert for it — see `render/buildings.rs`).
    ///
    /// IMPORTANT: bincode serializes this enum positionally (by declaration
    /// order) — every variant ADDED AFTER THIS ONE must stay appended at the
    /// very end (never inserted before `Tunnel`), or every already-persisted
    /// world's `Tunnel` building silently decodes as the wrong kind. `Tunnel`
    /// itself must never move either.
    Tunnel,
    /// V0.10: converts hunted `fur` into `cloth` (see the wildlife/hunting
    /// system on `GameState.wildlife` and `Stockpile.fur`/`cloth`). A
    /// fur-capped producer like `CoalMine` is deposit-capped, not a flat
    /// staffed-effect building like `Kitchen`/`Warehouse`.
    TailorShop,
    /// V0.11: a purely decorative/organizational perimeter tile — this game
    /// has no threat/combat mechanic and no movement-collision model (see
    /// `types.rs`'s "no obstacles, no pathfinding" doc comment), so a Wall
    /// cannot mechanically block anything; it only occupies its tile (via
    /// the normal `can_place` check every building already gets) to mark a
    /// boundary. Cheap by design (see `cost_wood`) since it buys no
    /// production/housing value.
    Wall,
    /// V0.11: the "opening" companion to `Wall` — mechanically identical
    /// (decorative, occupies a tile), distinguished only by name/visual so
    /// a perimeter reads as having a marked entrance.
    Gate,
    /// V0.11: draws `water` (flat, uncapped producer, mirrors `Greenhouse`'s
    /// shape) — a real survival need on par with food (see
    /// `Survivor::thirst`), not an optional side-economy.
    Well,
    /// V0.12: raises cow/sheep `Livestock` for food — same deposit-capped
    /// producer shape as `HunterHut`/`Wildlife`, just domesticated instead
    /// of hunted (see `GameState.livestock`). `Profession::Farmer` matches
    /// both this and `Greenhouse` (see `Profession::matches_building`).
    Farmhouse,
    /// V0.19: staffed, it keeps the snow down within `SNOW_CREW_RADIUS` of
    /// itself (see `Tile::snow`) — the standing answer to a road network that
    /// would otherwise need re-clearing by hand after every blizzard. Produces
    /// no resource: like `Kitchen`/`Warehouse`, its output is an effect
    /// applied in `sim::tick`, not a credit to the stockpile.
    SnowCrew,
}

impl BuildingKind {
    pub const BUILDABLE: [BuildingKind; 14] = [
        BuildingKind::Tent,
        BuildingKind::Sawmill,
        BuildingKind::CoalMine,
        BuildingKind::HunterHut,
        BuildingKind::Greenhouse,
        BuildingKind::Hospital,
        BuildingKind::Kitchen,
        BuildingKind::Warehouse,
        BuildingKind::TailorShop,
        BuildingKind::Wall,
        BuildingKind::Gate,
        BuildingKind::Well,
        BuildingKind::Farmhouse,
        BuildingKind::SnowCrew,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BuildingKind::Furnace => "Furnace",
            BuildingKind::Tent => "Tent",
            BuildingKind::Sawmill => "Sawmill",
            BuildingKind::CoalMine => "Coal Mine",
            BuildingKind::HunterHut => "Hunter's Hut",
            BuildingKind::Greenhouse => "Greenhouse",
            BuildingKind::Hospital => "Hospital",
            BuildingKind::Kitchen => "Kitchen",
            BuildingKind::Warehouse => "Warehouse",
            BuildingKind::TailorShop => "Tailor Shop",
            BuildingKind::Wall => "Wall",
            BuildingKind::Gate => "Gate",
            BuildingKind::Well => "Well",
            BuildingKind::Farmhouse => "Farmhouse",
            BuildingKind::SnowCrew => "Snow Crew",
            BuildingKind::Tunnel => "Tunnel",
        }
    }

    pub fn letter(self) -> &'static str {
        match self {
            BuildingKind::Furnace => "F",
            BuildingKind::Tent => "T",
            BuildingKind::Sawmill => "S",
            BuildingKind::CoalMine => "C",
            BuildingKind::HunterHut => "H",
            BuildingKind::Greenhouse => "G",
            BuildingKind::Hospital => "+",
            BuildingKind::Kitchen => "K",
            BuildingKind::Warehouse => "W",
            BuildingKind::TailorShop => "Y",
            BuildingKind::Wall => "L",
            BuildingKind::Gate => "D",
            BuildingKind::Well => "A",
            BuildingKind::Farmhouse => "M",
            BuildingKind::SnowCrew => "N",
            BuildingKind::Tunnel => "U",
        }
    }

    pub fn cost_wood(self) -> u32 {
        match self {
            // Never actually spent on `Place` (neither kind is `buildable`),
            // but the Furnace's value here still matters: it's the base
            // `upgrade_cost_wood`/`upgrade_workdays` scale for its (V0.9)
            // level 1-10 upgrade path (see `upgradeable`). The Tunnel has no
            // such path, so it stays 0.
            BuildingKind::Furnace => 50,
            BuildingKind::Tunnel => 0,
            BuildingKind::Tent => 15,
            BuildingKind::Sawmill => 25,
            BuildingKind::CoalMine => 30,
            BuildingKind::HunterHut => 25,
            BuildingKind::Greenhouse => 35,
            BuildingKind::Hospital => 35,
            BuildingKind::Kitchen => 25,
            BuildingKind::Warehouse => 30,
            // Second stage of the hunting->fur->cloth chain: needs a working
            // Hunter's Hut supply line to be useful at all, so it's priced
            // above every existing producer (see V0.10 balancing notes).
            BuildingKind::TailorShop => 45,
            // V0.11: buys no production/housing value at all (purely
            // decorative/organizational — see the `Wall` doc comment), so
            // it's priced far below every other buildable kind, cheap
            // enough to place many of in a row.
            BuildingKind::Wall => 4,
            BuildingKind::Gate => 5,
            // A real early-game survival need from day one (unlike the
            // optional Tailor Shop chain), so priced to be affordable before
            // much wood is banked — below Sawmill/HunterHut/CoalMine since it
            // needs no supporting infrastructure (no forest/deposit siting).
            BuildingKind::Well => 20,
            // Same tier as Greenhouse (its fellow Farmer building) — a
            // second, deposit-capped food source rather than a strict
            // upgrade to either it or HunterHut.
            BuildingKind::Farmhouse => 35,
            // Priced with the other staffed-effect buildings (Kitchen 25,
            // Warehouse 30): it produces nothing, it keeps something else
            // working — here, the road network.
            BuildingKind::SnowCrew => 28,
        }
    }

    pub fn max_workers(self) -> u8 {
        match self {
            BuildingKind::Furnace
            | BuildingKind::Tent
            | BuildingKind::Tunnel
            | BuildingKind::Wall
            | BuildingKind::Gate => 0,
            BuildingKind::Sawmill => 2,
            BuildingKind::CoalMine => 3,
            BuildingKind::HunterHut => 2,
            BuildingKind::Greenhouse => 2,
            BuildingKind::Hospital => 2,
            BuildingKind::Kitchen => 1,
            BuildingKind::Warehouse => 1,
            BuildingKind::TailorShop => 2,
            BuildingKind::Well => 2,
            BuildingKind::Farmhouse => 2,
            BuildingKind::SnowCrew => 2,
        }
    }

    /// Units produced per worker per in-game day (resource producers only;
    /// Hospital/Kitchen provide effects handled in the tick, not raw output).
    pub fn production_per_worker_day(self) -> f32 {
        match self {
            BuildingKind::Sawmill => 12.0,
            BuildingKind::CoalMine => 15.0,
            BuildingKind::HunterHut => 10.0,
            // Pricier than the Hunter's Hut but higher output per worker — a
            // sustained late-game food source rather than a strict upgrade.
            BuildingKind::Greenhouse => 13.0,
            // Nominal cloth units/worker/day before fur-capping (see the
            // TailorShop arm in `sim::tick`, which mirrors CoalMine's
            // deposit-capping style rather than crediting this flat).
            BuildingKind::TailorShop => 4.0,
            BuildingKind::Well => WELL_WATER_PER_WORKER_DAY,
            // Nominal food units/worker/day before livestock-capping (see the
            // Farmhouse arm in `sim::tick`, which mirrors HunterHut's
            // deposit-capping style rather than crediting this flat).
            BuildingKind::Farmhouse => 11.0,
            _ => 0.0,
        }
    }

    /// Footprint in tiles.
    ///
    /// V0.22: any building with an INTERIOR is a room — `ROOM_SIZE` on a side
    /// — because that is what it takes to draw one: a floor, a wall frame,
    /// fittings standing on the floor with space between them, and workers at
    /// their stations rather than piled on a single tile. A one-tile shed has
    /// nowhere to put a stove and a dining table.
    ///
    /// Everything without an interior keeps the footprint it had. A Wall or a
    /// Gate is a line drawn on the ground, not a room, and blowing it up to
    /// 3x3 would make a perimeter impossible to trace. The Furnace and the
    /// Tunnel stay 2x2 fixtures: both are hand-placed by `mapgen` at fixed
    /// coordinates, and both have bespoke models that a room frame would
    /// fight rather than flatter.
    pub fn size(self) -> (u8, u8) {
        match self {
            BuildingKind::Furnace | BuildingKind::Tunnel => (2, 2),
            _ if !self.furnishings().is_empty() => (ROOM_SIZE, ROOM_SIZE),
            // Tent: shelter, not a workplace — no fittings, and small enough
            // that a colony can still ring the furnace with them.
            BuildingKind::Tent => (2, 2),
            _ => (1, 1),
        }
    }

    pub fn buildable(self) -> bool {
        !matches!(self, BuildingKind::Furnace | BuildingKind::Tunnel)
    }

    /// V0.9: whether `UpgradeBuilding` accepts this kind — distinct from
    /// `buildable` (which gates `Place`/`Demolish`). The Furnace is the one
    /// exception: never placeable or demolishable (a single permanent
    /// fixture), but still growable — level 1-6 stays a modest "gulxan"
    /// (campfire), 7-10 becomes an established "Pech" (see
    /// `render/buildings.rs`'s two-tier model). The Tunnel has no such path.
    pub fn upgradeable(self) -> bool {
        self.buildable() || self == BuildingKind::Furnace
    }

    /// V0.8: worker-days of construction to erect this building at level 1.
    pub fn build_workdays(self) -> f32 {
        self.cost_wood() as f32 * BUILD_WORKDAYS_PER_WOOD
    }

    /// V0.8: wood price of upgrading TO `next_level` (2..=BUILDING_MAX_LEVEL).
    /// Scales with the target level so late levels are a real wood sink.
    pub fn upgrade_cost_wood(self, next_level: u8) -> u32 {
        (self.cost_wood() * next_level as u32).div_ceil(2)
    }

    /// V0.8: worker-days of construction an upgrade TO `next_level` takes.
    pub fn upgrade_workdays(self, next_level: u8) -> f32 {
        self.build_workdays() * next_level as f32 * 0.5
    }

    /// V0.11: true for buildings where being staffed by the WRONG profession
    /// is a `SKILLED_MISMATCH_PENALTY`, not just the ordinary 1.0x baseline
    /// every other mismatch keeps — currently just Hospital (see
    /// `Profession::is_skilled_at`, which answers the per-profession half of
    /// this: whether a GIVEN profession is the one `kind` needs). Checked
    /// independently of any specific survivor so `survivor_contribution` can
    /// tell "this building needs a specialist" apart from "this survivor
    /// happens to be one" — the two are different questions.
    pub fn requires_specialist(self) -> bool {
        matches!(self, BuildingKind::Hospital)
    }

    /// V0.20: the fittings this building takes, in panel order. Only rooms
    /// where somebody actually WORKS have an interior — a Tent is somewhere
    /// to sleep, and a Wall is a line on the ground; neither has a workplace
    /// to furnish. The Furnace and the Tunnel are fixtures, not rooms.
    ///
    /// Each building takes a SUBSET, chosen for what that room plausibly
    /// contains, which is where the variety lives (see the module doc on
    /// `furnishing`): a Coal Mine gets tools and a stove, not a dining set;
    /// a Kitchen is the one room that is mostly seating.
    pub fn furnishings(self) -> &'static [FurnishingKind] {
        use FurnishingKind::*;
        match self {
            BuildingKind::Kitchen => &[Seating, Workbench, Heater],
            BuildingKind::Hospital => &[Workbench, Shelving, Heater],
            BuildingKind::Sawmill => &[Workbench, Shelving],
            BuildingKind::CoalMine => &[Workbench, Heater],
            BuildingKind::HunterHut => &[Workbench, Heater],
            BuildingKind::Greenhouse => &[Workbench, Heater],
            BuildingKind::Farmhouse => &[Workbench, Shelving],
            BuildingKind::TailorShop => &[Workbench, Shelving, Seating],
            BuildingKind::Warehouse => &[Shelving, Workbench],
            BuildingKind::Well => &[Workbench],
            BuildingKind::SnowCrew => &[Workbench, Heater, Seating],
            // No workplace inside: nothing to furnish.
            BuildingKind::Tent
            | BuildingKind::Wall
            | BuildingKind::Gate
            | BuildingKind::Furnace
            | BuildingKind::Tunnel => &[],
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            BuildingKind::Furnace => "Keep it burning. Consumes coal (or wood).",
            BuildingKind::Tent => "Houses 4 people. Place inside the heat radius.",
            BuildingKind::Sawmill => "Workers harvest nearby forest for wood.",
            BuildingKind::CoalMine => "Must be placed on a coal deposit.",
            BuildingKind::HunterHut => "Workers hunt for food.",
            BuildingKind::Greenhouse => "A high-output indoor farm (more food/worker).",
            BuildingKind::Hospital => "Staffed: heals survivors faster.",
            BuildingKind::Kitchen => "Staffed: the city eats and drinks more efficiently.",
            BuildingKind::Warehouse => "Staffed: new construction wastes less wood.",
            BuildingKind::TailorShop => "Turns fur into cloth. Workers turn hides into warmth.",
            BuildingKind::Wall => "Marks a boundary. Decorative.",
            BuildingKind::Gate => "An opening in a wall line. Decorative.",
            BuildingKind::Well => "Workers draw water. The city needs it to survive.",
            BuildingKind::Farmhouse => "Workers raise cattle and sheep for food.",
            BuildingKind::SnowCrew => "Staffed: keeps the snow down nearby, so the roads stay open.",
            BuildingKind::Tunnel => "The way to the Global World. Travelers pass through once it's open.",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Building {
    pub id: u32,
    pub kind: BuildingKind,
    pub x: u8,
    pub y: u8,
    pub workers: u8,
    /// Fractional extraction progress (sawmill / coal mine).
    pub progress: f32,
    /// Player id that placed this building; `None` for the starting furnace.
    pub owner: Option<u64>,
    /// Account id that placed this building, set only when it was placed in
    /// the CENTRAL world by an account-authenticated connection; `None`
    /// everywhere else (personal/guest worlds keep using `owner`/session-id
    /// authority) and for legacy central buildings placed before this field
    /// existed (migration default `None` — they stay demolishable by anyone,
    /// same as today, rather than becoming permanently stuck).
    pub owner_account: Option<i64>,
    /// V0.8: building level, 1..=`BUILDING_MAX_LEVEL`. Upgrades go one level
    /// at a time (`UpgradeBuilding`); migration default for old saves is 1.
    pub level: u8,
    /// V0.8: remaining construction work in worker-days; > 0 while this is an
    /// unfinished site (a fresh placement or an in-progress upgrade). The
    /// assigned crew works it down each tick and the building produces
    /// nothing / houses nobody until it reaches 0.
    pub build_left: f32,
    /// V0.20: level of each fitting this building has, parallel to
    /// `kind.furnishings()` — index 0 is that slice's first kind, and 0 means
    /// "not bought yet". Stored as a plain `Vec<u8>` rather than a map keyed
    /// by `FurnishingKind`: the slot list is a property of the KIND and never
    /// changes at runtime, so an index is the whole identity, and every read
    /// goes through `furnishing_level`, which treats a short (or empty) vec
    /// as all-zero. That is what lets a migrated pre-V0.20 building arrive
    /// with an empty vec and behave correctly instead of panicking.
    ///
    /// IMPORTANT: bincode serializes this struct positionally — new fields
    /// stay APPENDED at the end. Old saves load through `legacy::BuildingV16`.
    pub furnishings: Vec<u8>,
    /// V0.16: player-chosen orientation, 0..=3 = a quarter-turn each
    /// (0 = default/south-facing). Purely visual — the footprint is a single
    /// tile regardless, so `can_place`/`can_relocate` and every gameplay rule
    /// ignore it; only `render::buildings` reads it (spins the building root a
    /// multiple of 90°). Placed via the CoC-style confirm flow's rotate
    /// button; migration default for pre-V0.16 saves is 0 (south-facing, how
    /// every building already looked). See `PlayerCommand::Place`.
    pub facing: u8,
}

impl Building {
    /// True while this is an unfinished construction site (new or upgrading).
    pub fn under_construction(&self) -> bool {
        self.build_left > 0.0
    }

    /// V0.22: real-time seconds until this site is finished at its CURRENT
    /// crew size — what the scaffold's floating countdown shows. `None` when
    /// nothing is being built, or when nobody is working it (an unstaffed
    /// site never finishes, and a bar counting down to a time that will never
    /// arrive is worse than no bar).
    ///
    /// Mirrors `sim::tick`'s construction step exactly (`build_left` falls by
    /// `crew / TICKS_PER_DAY` each tick), so the number on screen is the one
    /// the sim will actually take.
    pub fn build_eta_secs(&self) -> Option<f32> {
        if !self.under_construction() || self.workers == 0 {
            return None;
        }
        let days = self.build_left / self.workers as f32;
        Some(days * super::TICKS_PER_DAY as f32 * super::TICK_MS as f32 / 1000.0)
    }

    /// V0.22: 0.0..=1.0 build progress, for the scaffold's bar. Uses the work
    /// this site started from, so an upgrade's bar fills over the upgrade's
    /// own (discounted) duration rather than the original build's.
    pub fn build_progress(&self) -> f32 {
        let total = if self.level > 1 {
            self.kind.upgrade_workdays(self.level)
        } else {
            self.kind.build_workdays()
        };
        if total <= 0.0 {
            return 1.0;
        }
        (1.0 - self.build_left / total).clamp(0.0, 1.0)
    }

    /// Level-scaled output/effect multiplier: 1.0 at level 1,
    /// +`LEVEL_PRODUCTION_BONUS` per level above it.
    pub fn level_factor(&self) -> f32 {
        1.0 + LEVEL_PRODUCTION_BONUS * self.level.saturating_sub(1) as f32
    }

    /// Housing slots this building contributes (Tents only; a site under
    /// construction shelters nobody). Grows with level.
    pub fn housing_slots(&self) -> usize {
        if self.kind != BuildingKind::Tent || self.under_construction() {
            return 0;
        }
        TENT_CAPACITY + TENT_CAPACITY_PER_LEVEL * self.level.saturating_sub(1) as usize
    }

    /// Sanity guard used by upgrade validation.
    pub fn at_max_level(&self) -> bool {
        self.level >= BUILDING_MAX_LEVEL
    }

    // --- V0.20: interiors ---

    /// Level of the fitting in `slot` (an index into `kind.furnishings()`),
    /// or 0 if it hasn't been bought — or if this building predates interiors
    /// entirely and carries an empty vec. Every read goes through here so a
    /// short vec is never an out-of-bounds panic.
    pub fn furnishing_level(&self, slot: usize) -> u8 {
        self.furnishings.get(slot).copied().unwrap_or(0)
    }

    /// Level of a fitting by KIND (0 when this building doesn't take one, or
    /// hasn't bought it) — what `sim::tick` asks, since it cares about
    /// "does this room have a heater" and not about slot order.
    pub fn furnishing_of(&self, kind: FurnishingKind) -> u8 {
        self.kind
            .furnishings()
            .iter()
            .position(|k| *k == kind)
            .map_or(0, |slot| self.furnishing_level(slot))
    }

    /// The multiplier `kind`'s fitting contributes here — `1.0 + per_level *
    /// level`, so an unfurnished (or unfurnishable) building is exactly 1.0
    /// and every pre-V0.20 balance expectation is untouched. Only meaningful
    /// for the kinds whose effect IS a multiplier (`Workbench`, `Shelving`);
    /// `Heater` is a reduction and `Seating` an additive morale term, so
    /// their call sites read `furnishing_of` directly.
    pub fn furnishing_factor(&self, kind: FurnishingKind) -> f32 {
        1.0 + kind.per_level() * self.furnishing_of(kind) as f32
    }

    /// V0.20: are this building's fittings keeping pace with its level? The
    /// gate on `UpgradeBuilding` — see `required_furnishing_level` for why it
    /// stops mattering once the fittings are maxed. Vacuously true for a
    /// building with no interior (a Tent, a Wall), which is what keeps those
    /// upgrading exactly as they always did.
    pub fn furnishings_keep_pace(&self) -> bool {
        let want = required_furnishing_level(self.level);
        self.kind
            .furnishings()
            .iter()
            .enumerate()
            .all(|(slot, _)| self.furnishing_level(slot) >= want)
    }

    /// V0.22: where the `slot`-th fitting stands inside this building's room,
    /// in TILE coordinates (the same space `Survivor::x`/`y` uses), or `None`
    /// when the slot doesn't exist.
    ///
    /// Positions are laid out along the room's back and side walls, leaving
    /// the middle clear — the reference layout every workshop in this genre
    /// uses, and the reason the floor reads as a room rather than a crate
    /// pile. Deterministic from the slot index alone, so the client's model
    /// and the sim's walk target can never disagree about where the stove is.
    pub fn station(&self, slot: usize) -> Option<(f32, f32)> {
        let (w, h) = self.kind.size();
        if slot >= self.kind.furnishings().len() {
            return None;
        }
        // Inset from the wall by a third of a tile so a model at the station
        // sits against the wall rather than inside it.
        let inset = 0.62f32;
        let (w, h) = (w as f32, h as f32);
        let (ox, oy) = (self.x as f32, self.y as f32);
        // Back wall first (left to right), then the left wall going down —
        // enough distinct spots for the four fittings any kind takes.
        let local = match slot {
            0 => (inset, inset),
            1 => (w - inset, inset),
            2 => (inset, h - inset),
            _ => (w - inset, h - inset),
        };
        Some((ox + local.0, oy + local.1))
    }

    /// V0.22: where the `i`-th worker of this building stands. Workers take
    /// the fittings' stations in order — somebody is AT the stove, somebody
    /// is AT the workbench — and anyone past the last fitting stands in the
    /// middle of the floor rather than outside the wall.
    pub fn worker_station(&self, i: usize) -> (f32, f32) {
        if let Some(p) = self.station(i) {
            return p;
        }
        let (w, h) = self.kind.size();
        (self.x as f32 + w as f32 / 2.0, self.y as f32 + h as f32 / 2.0)
    }

    /// V0.21: this building's live production cycle — the one its producing
    /// fitting defines (see [`FurnishingKind::cycle`]). `None` means it makes
    /// nothing right now: either it yields no resource at all (a Kitchen's
    /// value is an effect, not a stockpile credit) or its workbench hasn't
    /// been bought yet.
    pub fn production_cycle(&self) -> Option<super::FurnishingCycle> {
        FurnishingKind::Workbench.cycle(self.kind, self.furnishing_of(FurnishingKind::Workbench))
    }

    /// The cycle `slot`'s fitting WOULD run at `level` — what the panel needs
    /// to show an upgrade's before/after ("Time 7.8s −0.2s").
    pub fn cycle_at(&self, slot: usize, level: u8) -> Option<super::FurnishingCycle> {
        let kind = *self.kind.furnishings().get(slot)?;
        kind.cycle(self.kind, level)
    }

    /// Wood to bring `slot`'s fitting up one level, and the level it would
    /// reach — `None` when the slot doesn't exist or is already maxed.
    pub fn next_furnishing_step(&self, slot: usize) -> Option<(u8, f32)> {
        let kind = *self.kind.furnishings().get(slot)?;
        let next = self.furnishing_level(slot) + 1;
        (next <= FURNISHING_MAX_LEVEL).then(|| (next, kind.cost_wood(next)))
    }
}
