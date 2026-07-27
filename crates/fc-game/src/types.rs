//! Shared, serializable game data. This is the single source of truth that the
//! server simulates and broadcasts to every client each tick.
//!
//! The data model is split across `types/` submodules by domain — the map
//! ([`world`]), buildings ([`building`]), survivors and players ([`people`]),
//! resources ([`economy`]), progression ([`progression`]), transient events
//! ([`events`]), player commands ([`command`]) and the [`GameState`] aggregate
//! ([`state`]) — but every item is re-exported here, so the whole codebase
//! keeps referring to them as `game::types::X`. The shared balance constants
//! and the two spatial free functions live in this root module.

mod building;
mod command;
mod economy;
mod events;
mod expedition;
mod furnishing;
mod law;
mod people;
mod progression;
mod state;
mod world;

pub use building::*;
pub use command::*;
pub use economy::*;
pub use events::*;
pub use expedition::*;
pub use furnishing::*;
pub use law::*;
pub use people::*;
pub use progression::*;
pub use state::*;
pub use world::*;

pub const MAP_W: usize = 64;
pub const MAP_H: usize = 64;

/// Simulation rate: 5 ticks per second.
pub const TICK_MS: u64 = 200;
/// One in-game day lasts 150 real seconds.
pub const TICKS_PER_DAY: u64 = 750;
/// Tick-of-day at which newcomers may arrive (morning, t = 0.3).
pub const ARRIVAL_TICK: u64 = 225;

pub const FURNACE_X: u8 = 31;
pub const FURNACE_Y: u8 = 31;

pub const DEFAULT_WIN_DAYS: u32 = 12;
/// Day-based victory must never fire before the longest `SurviveDays` mission
/// can complete, or the Tunnel could be permanently unreachable on that world.
pub const MIN_WIN_DAYS: u32 = 4;
pub const MAX_POPULATION: i32 = 60;
/// How many survivors one account may bring through the Tunnel into the
/// central world, total. Re-entering tops the group back up to this cap (from
/// the personal world's population), so repeat visits can't flood the shared
/// map with one account's settlers.
pub const CENTRAL_MIGRANTS_PER_ACCOUNT: usize = 5;

pub const FOOD_PER_SURVIVOR_DAY: f32 = 1.2;
pub const FURNACE_COAL_PER_DAY_PER_LEVEL: f32 = 12.0;
/// Extra HP per in-game day each staffed hospital worker restores to survivors.
pub const HOSPITAL_CARE_PER_WORKER_DAY: f32 = 2.0;
/// Food-consumption multiplier while any kitchen is staffed (lower = thriftier).
pub const KITCHEN_FOOD_EFFICIENCY: f32 = 0.75;
/// Construction wood-cost multiplier while any warehouse is staffed — salvaged
/// and well-organized materials mean less goes to waste on a new build.
pub const WAREHOUSE_BUILD_DISCOUNT: f32 = 0.80;
/// Wood burns less efficiently than coal.
pub const WOOD_FUEL_PENALTY: f32 = 1.5;
pub const DEMOLISH_REFUND: f32 = 0.4;
pub const TENT_CAPACITY: usize = 4;

// --- V0.8: bino qurilishi va darajalar (construction & upgrades) ---

/// Binolar 1 dan shu darajagacha, bittalab yangilanadi.
pub const BUILDING_MAX_LEVEL: u8 = 10;
/// Bitta qurilish maydonchasi (yangi bino yoki yangilash) sig'diradigan
/// ustalar soni — `AdjustWorkers` qurilish paytida shu bilan cheklanadi.
pub const CONSTRUCTION_CREW_MAX: u8 = 3;
/// 1-daraja binoni tiklash uchun kerak usta-kunlar, bazaviy yog'och narxining
/// har birligiga (Chodir 15y ≈ 0.3 usta-kun; Issiqxona 35y ≈ 0.7).
pub const BUILD_WORKDAYS_PER_WOOD: f32 = 0.02;
/// How many logs the leader (or any survivor assigned to it) must chop and
/// carry to the site to finish the boshlang'ich Furnace — each one is a
/// real walk-there-chop-walk-back trip (`Survivor::chop_target`/
/// `carrying_wood`, `tick.rs`'s furnace-construction block), not an
/// abstract worker-days tick-down like every other building's construction.
pub const FURNACE_LOGS_NEEDED: u32 = 6;
/// Chebyshev radius (tiles) a furnace-building survivor searches for the
/// nearest choppable forest tile — generous because mapgen deliberately
/// keeps the area right around the furnace clear (forest starts ~6-8 tiles
/// out), so a small radius could come up empty on a sparsely-forested map.
pub const FURNACE_CHOP_RADIUS: i32 = 25;
/// 1-darajadan yuqori har bir daraja uchun ishlab-chiqarish/effekt bonusi
/// (+12%/daraja, L10 ≈ +108%).
pub const LEVEL_PRODUCTION_BONUS: f32 = 0.12;
/// Chodir 1-darajadan yuqori har bir darajada qo'shimcha joy oladi.
pub const TENT_CAPACITY_PER_LEVEL: usize = 1;

// --- V0.7: survivor management (positions/movement, leader, professions,
// XP/levels, morale) ---

/// Constant walking speed for server-authoritative survivor movement, in
/// tiles per real second. The map is open (no obstacles), so movement is a
/// straight line toward the current goal — no pathfinding needed.
pub const SURVIVOR_SPEED_TILES_PER_SEC: f32 = 2.5;
/// Distance covered per tick at [`SURVIVOR_SPEED_TILES_PER_SEC`], derived
/// from [`TICK_MS`] so the two constants can never drift apart.
pub const SURVIVOR_SPEED_PER_TICK: f32 = SURVIVOR_SPEED_TILES_PER_SEC * (TICK_MS as f32 / 1000.0);
/// How close (in tiles) a survivor must get to their goal to snap to it and
/// stop, so they don't perpetually overshoot-and-correct in a tiny jitter.
pub const ARRIVAL_EPSILON: f32 = SURVIVOR_SPEED_PER_TICK;

/// Colony-wide production multiplier while a leader is alive.
pub const LEADER_PRODUCTION_BONUS: f32 = 1.08;
/// Colony-wide production multiplier while the city mourns a dead leader.
pub const MOURNING_PRODUCTION_PENALTY: f32 = 0.85;
/// A dead leader's city mourns for one full in-game day before command
/// authority over the caravan choice reverts to "nobody's here to decide".
pub const MOURNING_DURATION_TICKS: u64 = TICKS_PER_DAY;

/// Production bonus when a survivor's profession matches the building kind
/// they're working (e.g. a Lumberjack in a Sawmill).
pub const PROFESSION_MATCH_BONUS: f32 = 1.25;

/// In-game days of work at the same building kind to reach each XP level.
/// Level N requires the CUMULATIVE total below (not N's slice alone).
pub const XP_DAYS_LEVEL_1: f32 = 1.0;
pub const XP_DAYS_LEVEL_2: f32 = 3.0;
pub const XP_DAYS_LEVEL_3: f32 = 6.0;
/// Per-level contribution multiplier bonus (level 1 -> +5%, 2 -> +10%, 3 -> +15%).
pub const XP_LEVEL_BONUS_PER_LEVEL: f32 = 0.05;
pub const XP_MAX_LEVEL: u8 = 3;

/// XP level (0..=XP_MAX_LEVEL) from accrued in-game work-days, thresholded by
/// `XP_DAYS_LEVEL_*` (cumulative totals, not per-level slices). Shared by the
/// sim's contribution math and the client's roster/appearance tiers, so the
/// two can never disagree about a survivor's level.
pub fn xp_level(xp: f32) -> u8 {
    if xp >= XP_DAYS_LEVEL_3 {
        3
    } else if xp >= XP_DAYS_LEVEL_2 {
        2
    } else if xp >= XP_DAYS_LEVEL_1 {
        1
    } else {
        0
    }
}

/// Morale starts here on a fresh world and is what every existing balance
/// test implicitly assumes (multiplier 1.0 — see `morale_multiplier`), so a
/// new world's production math is unchanged by this feature.
pub const MORALE_START: f32 = 70.0;
/// Morale drifts toward this baseline (±1/day) absent any other adjustment.
pub const MORALE_BASELINE: f32 = 60.0;
pub const MORALE_DEATH_PENALTY: f32 = 10.0;
pub const MORALE_STARVATION_PER_DAY: f32 = 3.0;
pub const MORALE_BLIZZARD_PER_DAY: f32 = 2.0;
pub const MORALE_KITCHEN_PER_DAY: f32 = 2.0;
pub const MORALE_HOSPITAL_PER_DAY: f32 = 2.0;
pub const MORALE_LEADER_PER_DAY: f32 = 1.0;
pub const MORALE_DRIFT_PER_DAY: f32 = 1.0;

/// Rolling chat log length kept in the snapshot.
pub const MAX_CHAT: usize = 40;
/// Longest chat message accepted (characters, after sanitizing).
pub const MAX_CHAT_LEN: usize = 200;
/// Longest player display name accepted (characters, after sanitizing).
pub const MAX_NAME_LEN: usize = 24;
/// Maximum simultaneous map pings kept in the snapshot (global cap).
pub const MAX_PINGS: usize = 12;
/// Maximum live pings a single player may hold, so one spammer can't evict
/// everyone else's markers before they naturally expire.
pub const MAX_PINGS_PER_PLAYER: usize = 4;
/// A map ping fades and is dropped after this many ticks (~5 s at 5 Hz).
pub const PING_TTL_TICKS: u64 = 25;

// --- V0.3: the Tunnel (see `progression::TunnelState`) ---
pub const TUNNEL_STAGES: u8 = 3;
/// How many `InvestTunnel` contributions complete one stage.
pub const TUNNEL_INVESTS_PER_STAGE: u32 = 5;
pub const TUNNEL_INVEST_WOOD: f32 = 15.0;
pub const TUNNEL_INVEST_COAL: f32 = 12.0;
/// Map position of the always-present `BuildingKind::Tunnel` fixture, a few
/// tiles from the furnace (mirroring `FURNACE_X`/`FURNACE_Y`) so mapgen never
/// needs to search for a spot for it.
pub const TUNNEL_X: u8 = FURNACE_X + 4;
pub const TUNNEL_Y: u8 = FURNACE_Y;

// --- V0.9: Tunnel migrants (see `events::TunnelMigrant`) ---
/// Per-day chance a traveler group emerges from an unlocked Tunnel.
pub const TUNNEL_MIGRANT_CHANCE: f32 = 0.35;
/// How long travelers wait at the Tunnel for room before giving up.
pub const TUNNEL_MIGRANT_WAIT_TICKS: u64 = TICKS_PER_DAY;

// --- V0.3: technology-tree effects (see `progression::Tech`) ---
/// Flat extra warmth for every survivor once Insulation is researched.
pub const TECH_INSULATION_WARMTH: f32 = 4.0;
/// Furnace fuel-need multiplier with Efficient Furnace.
pub const TECH_FURNACE_EFFICIENCY: f32 = 0.75;
/// Production multiplier with Better Tools.
pub const TECH_TOOLS_PRODUCTION: f32 = 1.25;
/// Food-consumption multiplier with Rationing.
pub const TECH_RATIONING_FOOD: f32 = 0.85;
/// Hospital-care multiplier with Medicine.
pub const TECH_MEDICINE_CARE: f32 = 1.5;

// --- V0.3: dynamic events (see `events::CaravanOffer` / `events::GameEvent`) ---

/// Events never fire before this in-game day, so a fresh colony (and short
/// deterministic tests) is never disrupted while finding its feet.
pub const EVENT_GRACE_DAY: u32 = 3;

/// Per-day chance a sickness breaks out, and how long/hard it hits.
pub const DISEASE_CHANCE: f32 = 0.16;
pub const DISEASE_TICKS: u64 = 2 * TICKS_PER_DAY;
/// Retired in V0.17: an outbreak used to drain this much HP per day from
/// EVERY survivor at once. Illness is now individual — `disease_until` only
/// opens the daily infection roll, and the HP cost lands on whoever actually
/// caught it (`SICK_HP_PER_DAY`). Kept as a named constant so the old balance
/// figure stays greppable next to its replacement; nothing reads it.
pub const DISEASE_HP_PER_DAY: f32 = 6.0;

/// Per-day chance of a blizzard, its duration, and the extra cold it brings.
pub const BLIZZARD_CHANCE: f32 = 0.14;
pub const BLIZZARD_TICKS: u64 = TICKS_PER_DAY;
pub const BLIZZARD_COLD: f32 = -8.0;

/// Per-day chance a refugee caravan arrives, and the offer's shape.
pub const CARAVAN_CHANCE: f32 = 0.22;
pub const CARAVAN_FOOD_PER_PERSON: u32 = 4;
/// How long the player has to decide (half an in-game day).
pub const CARAVAN_EXPIRE_TICKS: u64 = TICKS_PER_DAY / 2;

// --- V0.10: Tailor Shop & wildlife (deer/rabbit -> fur -> cloth) ---

/// Starting deer/rabbit population on a fresh world, and the migration
/// default for saves that predate this feature (see `fc-net`'s V7->V8 hop) —
/// deliberately NOT zero, so a `HunterHut` can hunt immediately rather than
/// waiting for population to regrow from nothing (logistic growth from 0
/// never leaves 0).
pub const DEER_START: f32 = 40.0;
pub const RABBIT_START: f32 = 80.0;
/// Population ceiling each species' logistic regen approaches.
pub const DEER_CAP: f32 = 60.0;
pub const RABBIT_CAP: f32 = 120.0;
/// Logistic regen rate (fraction of current population regrown per day at
/// the inflection point) — deer regenerate slower than rabbits, the classic
/// K-strategist/r-strategist split.
pub const DEER_REGEN_PER_DAY: f32 = 0.18;
pub const RABBIT_REGEN_PER_DAY: f32 = 0.45;
/// Population consumed per hunt-unit (see the `HunterHut` production arm in
/// `sim::tick`) — deer cost more population per unit hunted than rabbits,
/// reflecting a smaller, slower-growing source.
pub const DEER_HUNT_PER_UNIT: f32 = 0.35;
pub const RABBIT_HUNT_PER_UNIT: f32 = 0.65;
/// Food + fur yielded per unit of deer/rabbit hunted.
pub const DEER_FOOD_PER_UNIT: f32 = 1.6;
pub const DEER_FUR_PER_UNIT: f32 = 0.9;
pub const RABBIT_FOOD_PER_UNIT: f32 = 0.6;
pub const RABBIT_FUR_PER_UNIT: f32 = 0.15;
/// Fur consumed per cloth produced by a staffed Tailor Shop.
pub const FUR_PER_CLOTH: f32 = 2.0;
/// Extra warmth from a staffed Tailor Shop while cloth lasts — stacks
/// additively with `TECH_INSULATION_WARMTH` rather than replacing it.
pub const TAILORING_WARMTH: f32 = 3.0;
/// Cloth consumed per in-game day to sustain `TAILORING_WARMTH` (garments
/// wearing out) — a flat colony-wide drain while any Tailor Shop is staffed
/// and cloth remains, same "staffed flag" granularity as Kitchen's
/// efficiency bonus.
pub const CLOTH_UPKEEP_PER_DAY: f32 = 1.0;

// --- V0.11: births, death/burial, profession-gating, thirst & the Well ---

/// Tick-of-day births are checked (offset from `ARRIVAL_TICK` so a birth
/// event never lands in the same tick's log as a morning-arrival one).
pub const BIRTH_TICK: u64 = ARRIVAL_TICK + 50;
/// Later than `EVENT_GRACE_DAY`/arrivals' `day >= 2` — a birth should read
/// as "the colony has settled", not day-1 luck.
pub const BIRTH_GRACE_DAY: u32 = 5;
/// Per-day chance, deliberately far below arrivals (0.55) or even caravans
/// (0.22) — a birth is a slow background trickle on top of external growth,
/// not a replacement for it.
pub const BIRTH_CHANCE: f32 = 0.12;
/// A colony must be doing reasonably well (not thriving, just not actively
/// suffering) for a birth to fire.
pub const BIRTH_MIN_MORALE: f32 = 55.0;
/// A few days' food buffer at `FOOD_PER_SURVIVOR_DAY`, so a birth is never
/// itself the tipping point into starvation the same day.
pub const BIRTH_MIN_FOOD_STOCK: f32 = 10.0;

/// How long an unburied corpse stays before it fades into a `Grave` on its
/// own — never a permanent map obstruction, matching "vaqtinchalik iz".
pub const CORPSE_DECAY_TICKS: u64 = 2 * TICKS_PER_DAY;
/// How long a `Grave` (from burial or decay) lingers before it too fades.
pub const GRAVE_FADE_TICKS: u64 = 3 * TICKS_PER_DAY;
/// How long a survivor sent to bury a corpse stands there performing the
/// timed action — small but non-zero, roughly one Furnace chop-carry trip.
pub const BURY_DURATION_TICKS: u64 = TICKS_PER_DAY / 20;

/// A mismatched survivor at a "skilled" building (see
/// `Profession::is_skilled_at`) produces almost nothing — specialized trades
/// aren't picked up informally the way general labor is. Every other
/// mismatch keeps today's 1.0x baseline; only these specific pairs replace
/// it with this harsher penalty.
pub const SKILLED_MISMATCH_PENALTY: f32 = 0.1;

/// Water needed per survivor per in-game day — set above `FOOD_PER_SURVIVOR_
/// DAY` (1.2) since thirst is calibrated to become critical sooner than
/// hunger (see the drink/HP-drain thresholds on `Survivor::thirst`).
pub const WATER_PER_SURVIVOR_DAY: f32 = 1.6;
/// Water-consumption multiplier while any Kitchen is staffed — mirrors
/// `KITCHEN_FOOD_EFFICIENCY` exactly (a kitchen serves both food and drink).
pub const KITCHEN_WATER_EFFICIENCY: f32 = 0.75;
/// Thirst-accrual multiplier with Rationing — mirrors `TECH_RATIONING_FOOD`
/// (the same tech now covers both: "the city eats and drinks carefully").
pub const TECH_RATIONING_WATER: f32 = 0.85;
/// Colony-wide morale penalty per day while anyone is critically thirsty —
/// mirrors `MORALE_STARVATION_PER_DAY` exactly, stacking additively with it.
pub const MORALE_THIRST_PER_DAY: f32 = 3.0;
/// Water units produced per Well worker per in-game day — flat and
/// uncapped (no map-tile deposit to exhaust, mirroring Greenhouse's simplest-
/// producer shape), calibrated so ~2 Wells comfortably cover a maxed
/// (`MAX_POPULATION`) colony's daily thirst, the same "not a hard
/// bottleneck" design intent food production already has.
pub const WELL_WATER_PER_WORKER_DAY: f32 = 22.0;

// --- V0.12: Farmhouse & livestock (cow/sheep -> food) ---

/// Starting cow/sheep population on a fresh world, and the migration default
/// for saves that predate this feature (see `fc-net`'s V9->V10 hop) —
/// deliberately NOT zero, same "logistic growth from 0 never leaves 0"
/// reasoning as `DEER_START`/`RABBIT_START`.
pub const COW_START: f32 = 25.0;
pub const SHEEP_START: f32 = 50.0;
/// Population ceiling each species' logistic regen approaches.
pub const COW_CAP: f32 = 40.0;
pub const SHEEP_CAP: f32 = 90.0;
/// Logistic regen rate (fraction of current population regrown per day at
/// the inflection point) — cows regenerate slower than sheep, same
/// K-strategist/r-strategist split as deer/rabbit.
pub const COW_REGEN_PER_DAY: f32 = 0.16;
pub const SHEEP_REGEN_PER_DAY: f32 = 0.40;
/// Population consumed per farm-unit (see the `Farmhouse` production arm in
/// `sim::tick`) — cows cost more population per unit raised than sheep,
/// reflecting a smaller, slower-growing herd.
pub const COW_HARVEST_PER_UNIT: f32 = 0.30;
pub const SHEEP_HARVEST_PER_UNIT: f32 = 0.55;
/// Food yielded per unit of cow/sheep raised.
pub const COW_FOOD_PER_UNIT: f32 = 2.0;
pub const SHEEP_FOOD_PER_UNIT: f32 = 0.8;

// --- V0.13: Tunnel trade caravan (see `economy::TradeCaravan`) ---

/// Available the moment the Tunnel is unlocked (all missions done) — same
/// gate as `TunnelMigrant`, deliberately NOT tied to `InvestTunnel`'s full
/// excavation/graduation, since graduating ends the personal world's normal
/// tick loop entirely (see `GamePhase::Won`). Trading is an ongoing back-
/// and-forth with the Global World while the colony is still very much
/// alive, not a one-time departure.
pub const CARAVAN_TRIP_TICKS: u64 = TICKS_PER_DAY + TICKS_PER_DAY / 2;
/// Only one caravan can be on the road at a time (see
/// `GameState::pending_caravan`) — same single-slot convention as
/// `pending_migrant`/`pending_event`.
pub const CARAVAN_MAX_AMOUNT: u32 = 200;

// --- V0.14: building relocation (see `PlayerCommand::RelocateBuilding`) ---

/// Relocating reuses `Building::build_left`/`under_construction` exactly
/// like a level upgrade does, just at a discount vs `build_workdays()` —
/// the foundation and materials are already there, only reassembly is
/// needed. No wood is charged either way (see `GameState::can_relocate`).
pub const RELOCATE_WORKDAYS_FACTOR: f32 = 0.4;

// --- V0.15: daily routine (see `sim::tick::routine_goal`) ---

/// `GameState::time_of_day()` windows an otherwise-idle survivor (no
/// `move_target`/`chop_target`/`bury_target`/`assigned_building` — truly
/// nobody telling them where to be) heads to the Kitchen for, if one's
/// finished. Two short windows, not the whole day: a real "people gather to
/// eat, then disperse" rhythm rather than a permanent loitering crowd.
/// Purely cosmetic (see the doc comment on `routine_goal`) — no stockpile
/// effect of its own; the real food/water consumption already happens
/// unconditionally elsewhere in `tick`, wherever a survivor happens to be.
pub const BREAKFAST_WINDOW: (f32, f32) = (0.27, 0.31);
pub const LUNCH_WINDOW: (f32, f32) = (0.48, 0.52);

// --- V0.17: aholi hayoti — charchoq/uyqu va shaxsiy kasallik ---

/// Fatigue gained per in-game day by a survivor assigned to a building
/// (working a post) versus one with nothing to do. Both only accrue while
/// awake — see the night branch below.
pub const FATIGUE_WORK_PER_DAY: f32 = 70.0;
pub const FATIGUE_IDLE_PER_DAY: f32 = 26.0;
/// Fatigue shed per in-game day while asleep in a Tent bunk, versus sleeping
/// rough (colony housing ran out, so this survivor has no bunk to walk to and
/// dozes wherever they are). Night is exactly half the day
/// (`GameState::is_night`), so a bunked worker nets roughly -40/day while an
/// unbunked one nets about +20/day and drifts into the tired band within a
/// few days: Tents now buy throughput, not just population headroom.
pub const FATIGUE_TENT_REST_PER_DAY: f32 = 110.0;
pub const FATIGUE_ROUGH_REST_PER_DAY: f32 = 30.0;
/// Fatigue below this costs nothing; above it, production falls linearly to
/// `FATIGUE_MIN_FACTOR` at 100 (see `Survivor::fatigue_factor`).
pub const FATIGUE_TIRED: f32 = 60.0;
pub const FATIGUE_MIN_FACTOR: f32 = 0.55;
/// Fatigue at which a survivor reads as "exhausted" — the roster/HUD tag,
/// the colony-wide morale penalty below, and the raised illness risk in
/// `sim::tick`'s infection roll all key off this one threshold.
pub const FATIGUE_EXHAUSTED: f32 = 90.0;
/// Colony-wide morale penalty per day while anyone is exhausted — same shape
/// and magnitude as `MORALE_BLIZZARD_PER_DAY` (a grinding background misery,
/// milder than starvation/thirst).
pub const MORALE_EXHAUSTION_PER_DAY: f32 = 2.0;
/// Colony-wide morale penalty per day while anyone is sick.
pub const MORALE_SICK_PER_DAY: f32 = 2.0;

/// Natural course of an individual illness, in ticks — how long
/// `Survivor::sick_left` takes to burn down with no hospital care at all.
pub const SICKNESS_TICKS: f32 = 2.0 * TICKS_PER_DAY as f32;
/// HP a sick survivor loses per in-game day. Above V0.3's colony-wide
/// `DISEASE_HP_PER_DAY` (which it replaces) because it now hits only the
/// people who actually caught it, not everyone at once.
pub const SICK_HP_PER_DAY: f32 = 10.0;
/// Production scale for a sick survivor — they stay at their post but get
/// little done. Composes multiplicatively with `fatigue_factor`.
pub const SICK_WORK_FACTOR: f32 = 0.35;
/// Extra illness ticks a staffed Hospital burns down per hospital worker-unit
/// per in-game day, on top of the natural 1-tick-per-tick course. Measured in
/// the same profession/XP-boosted "units" `survivor_contribution` returns, so
/// a trained Medic cures visibly faster than a conscript.
pub const HOSPITAL_RECOVERY_PER_UNIT_DAY: f32 = 600.0;
/// Tick-of-day the once-daily infection roll happens (see `ILLNESS_RNG_SEED`
/// on the isolated stream it draws from). Late morning, after arrivals.
pub const ILLNESS_TICK: u64 = ARRIVAL_TICK + 120;
/// Per-day chance a healthy survivor catches the bug while an outbreak
/// (`GameState.disease_until`) is running.
pub const OUTBREAK_INFECT_CHANCE_PER_DAY: f32 = 0.30;
/// Per-day chance each already-sick survivor passes it to a given healthy
/// one — the outbreak window can end while the illness keeps circulating.
pub const CONTAGION_CHANCE_PER_SICK_PER_DAY: f32 = 0.04;
/// Infection-chance multiplier for an exhausted survivor
/// (`fatigue >= FATIGUE_EXHAUSTED`) — the fatigue and illness systems feed
/// each other rather than running side by side.
pub const EXHAUSTION_SICK_MULTIPLIER: f32 = 2.0;
/// Ceiling on the composed per-day infection chance, so a big outbreak in a
/// big colony can never round up to "everyone falls ill tonight".
pub const MAX_INFECT_CHANCE_PER_DAY: f32 = 0.75;
/// Seed offset for `GameState.illness_rng` — a stream of its own so the
/// infection rolls can never shift the existing `rng`/`event_rng` sequences
/// (and so every pre-V0.17 save migrates to a deterministic, non-zero seed).
pub const ILLNESS_RNG_SEED: u64 = 0x5EED_1117;

// --- V0.18: ekspeditsiyalar (see `expedition::Expedition`) ---

/// Party-size band. Below the minimum a "party" is one person alone in the
/// wastes (and the colony learns nothing from losing them); above the maximum
/// an expedition would just be a way to empty the city on purpose.
pub const EXPEDITION_MIN_PARTY: usize = 2;
pub const EXPEDITION_MAX_PARTY: usize = 4;
/// A colony must keep at least this many people at home — the same
/// "a personal world is never emptied" invariant `extract_migrants` enforces
/// for the Tunnel, applied to the other way out of the valley.
pub const EXPEDITION_MIN_STAY_HOME: usize = 2;
/// Only one party may be on the road at a time (same single-slot convention
/// as `pending_caravan`/`pending_migrant`, just expressed as a cap on the
/// `expeditions` vec so the type doesn't have to change if that ever grows).
pub const EXPEDITION_MAX_ACTIVE: usize = 1;
/// Expeditions are gated on the same "the colony has found its feet" signal
/// the rest of the mid-game uses.
pub const EXPEDITION_MIN_DAY: u32 = 3;
/// HP a hazard costs the unlucky member (never lethal on its own: the floor
/// below keeps a party from silently wiping, which would read as a bug).
pub const EXPEDITION_HAZARD_HP: f32 = 22.0;
/// Nobody comes home below this HP — a hazard can maim, only the colony's own
/// cold and hunger can kill.
pub const EXPEDITION_MIN_RETURN_HP: f32 = 8.0;
/// Chance a hazard also sends that member home ill (on top of the HP loss).
pub const EXPEDITION_HAZARD_SICK_CHANCE: f32 = 0.35;
/// Fatigue a member accrues per day on the road, regardless of law/tent
/// state at home — travelling is tiring by definition.
pub const EXPEDITION_FATIGUE_PER_DAY: f32 = 34.0;
/// XP a member banks per day away, credited to their `trained_kind` on
/// return (an expedition teaches generally useful work).
pub const EXPEDITION_XP_PER_DAY: f32 = 0.5;
/// Morale the colony gains when a party comes home whole, and loses when it
/// comes home hurt.
pub const MORALE_EXPEDITION_RETURN: f32 = 4.0;
pub const MORALE_EXPEDITION_HURT: f32 = 3.0;
/// Seed offset for `GameState.expedition_rng` — its own stream, so hazard
/// rolls never re-sequence `rng`/`event_rng`/`illness_rng` (same reasoning as
/// `ILLNESS_RNG_SEED`).
pub const EXPEDITION_RNG_SEED: u64 = 0x0E_7BED_1180;

// --- V0.18: aholi hayoti — yosh, juftlik, farzand, qarilik ---

/// Age is measured in DAYS LIVED, and the whole scale is calibrated against
/// the length of an actual run — not against a human lifespan. A world ends
/// at `DEFAULT_WIN_DAYS` (12) and then resets, so thresholds on a human scale
/// (adult at 14, elder at 90) would make this entire system invisible: no
/// child would ever grow up and no elder would ever exist. These numbers are
/// chosen so the arc is visible inside one run — a child born early is
/// working before the run ends, and a colony that survives well past its
/// victory day starts to age.
///
/// A survivor becomes a working adult at this age. Children below it are fed
/// and housed but contribute no work.
pub const ADULT_AGE_DAYS: f32 = 6.0;
/// Past this age a survivor is an elder: less output, more fragile. Reachable
/// within a long-lived world, and reached almost immediately by the oldest
/// arrivals (see `ARRIVAL_AGE_MAX_DAYS`, deliberately just short of it), so
/// elders are part of the colony from early on rather than a late-game
/// curiosity nobody ever sees.
pub const ELDER_AGE_DAYS: f32 = 40.0;
/// Work multiplier for a child (0.0 — they don't work) and an elder.
pub const CHILD_WORK_FACTOR: f32 = 0.0;
pub const ELDER_WORK_FACTOR: f32 = 0.7;
/// Food a child eats, as a fraction of an adult's `FOOD_PER_SURVIVOR_DAY`.
pub const CHILD_FOOD_FACTOR: f32 = 0.6;
/// Age band a newly arriving (non-born) survivor is drawn from — grown (well
/// past `ADULT_AGE_DAYS`), with a life already behind them, and at the top of
/// the band only a few days short of `ELDER_AGE_DAYS`. The band is wide on
/// purpose: a founding population drawn from a narrow one would cross into
/// old age all together, in a single cliff, instead of ageing out gradually.
pub const ARRIVAL_AGE_MIN_DAYS: f32 = 8.0;
pub const ARRIVAL_AGE_MAX_DAYS: f32 = 34.0;
/// Per-day chance two unpartnered adults pair up (checked once a day, one
/// pairing at most — courtship is not a batch job).
pub const PARTNER_CHANCE_PER_DAY: f32 = 0.22;
/// Morale a new pairing gives the colony.
pub const MORALE_PARTNER_BONUS: f32 = 3.0;
/// Birth (see `BIRTH_CHANCE`) is multiplied by this when the colony has at
/// least one partnered couple — V0.11's birth roll stays as the floor so a
/// colony with no couples yet is never sterile, just slower.
pub const BIRTH_COUPLE_MULTIPLIER: f32 = 2.5;
/// Per-day chance an elder dies of old age, scaled by how far past
/// `ELDER_AGE_DAYS` they are (see `sim::lifecycle`).
pub const OLD_AGE_DEATH_PER_DAY: f32 = 0.02;
/// Illness-chance multiplier for children and elders — the fragile ends of
/// the colony, mirroring `EXHAUSTION_SICK_MULTIPLIER`'s shape.
pub const FRAIL_SICK_MULTIPLIER: f32 = 1.6;
/// Seed offset for `GameState.lifecycle_rng`.
pub const LIFECYCLE_RNG_SEED: u64 = 0x11FE_C7C1;

// --- V0.18: qonunlar kitobi (see `law::Law`) ---

/// How long after enacting or repealing a law before the book can be touched
/// again — a policy is a decision the city lives with, not a dial to spin
/// every tick.
pub const LAW_COOLDOWN_TICKS: u64 = TICKS_PER_DAY / 2;
/// Most laws that may stand at once, so a colony can't simply enact the whole
/// book and keep only the upsides that happen to matter today.
pub const MAX_ACTIVE_LAWS: usize = 3;
/// Laws unlock once the colony is past its opening crisis.
pub const LAW_MIN_DAY: u32 = 2;

// --- V0.18: takroriy missiya tsikllari ---

/// Once every mission in the current cycle is done, a fresh, harder cycle is
/// issued — the Tunnel unlock still keys off cycle 0 (`all_missions_done`
/// stays true the moment the FIRST cycle completes, see `tunnel_unlocked`),
/// so this only adds content after the gate rather than moving it.
pub const MISSION_CYCLE_TARGET_GROWTH: f32 = 1.6;
pub const MISSION_CYCLE_REWARD_GROWTH: f32 = 1.5;
/// Cycles stop being issued past this many, so a very long-lived world
/// doesn't end up with absurd six-digit targets.
pub const MAX_MISSION_CYCLES: u32 = 8;

// --- V0.19: yo'llar va qor (see `world::Tile`) ---

/// Deepest a tile's snow ever gets (`Tile::snow` is a byte; this is the top of
/// its meaningful range, not 255, so the depth-to-speed curve has a defined
/// end).
pub const SNOW_MAX: u8 = 100;
/// Speed multiplier at `SNOW_MAX` on open ground — deep drift is a crawl, but
/// never a wall (see `Tile::speed_factor`'s doc for why nothing ever blocks).
pub const SNOW_SLOWEST_FACTOR: f32 = 0.35;
/// Speed multiplier on a fully cleared road. The whole point of a road: it is
/// worth walking around for, which is what makes `sim::tick`'s step chooser
/// follow one instead of cutting straight across.
pub const TILE_ROAD_SPEED: f32 = 1.6;
/// Snow depth at which a road has stopped being a road (`road_is_buried`) and
/// crossing it is no better than crossing open ground. Well below `SNOW_MAX`:
/// a road should close long before the map itself bogs down, or clearing
/// would never feel urgent.
pub const ROAD_SNOW_PENALTY: u8 = 45;

/// Snow that falls on every tile per in-game day in ordinary weather, and
/// during a blizzard.
///
/// The blizzard rate is calibrated to do ONE specific thing: a single storm
/// must close a road outright. A blizzard lasts exactly `BLIZZARD_TICKS`
/// (one in-game day), so this has to clear `ROAD_SNOW_PENALTY` on its own —
/// at the original 34 it fell 11 short, and since a road people actually walk
/// on sheds `SNOW_TRAMPLE_PER_DAY` (6) against fair weather's 3, a used road
/// simply never closed. Measured over 40 days across five seeds: roads were
/// buried on 0% of blizzard days. The whole point of the mechanic — a storm
/// shuts the network and somebody has to go dig it out — never fired once.
pub const SNOW_FALL_PER_DAY: f32 = 3.0;
pub const SNOW_FALL_BLIZZARD_PER_DAY: f32 = 52.0;
/// Snow packed down per in-game day on a tile a survivor is standing on —
/// paths worn by ordinary traffic stay a little clearer than untouched
/// ground, without the player doing anything.
pub const SNOW_TRAMPLE_PER_DAY: f32 = 6.0;

/// Wood per tile of road. Cheap — a road buys no production, no housing and
/// no capacity, only movement, and a useful road is dozens of tiles long.
/// Priced near `Wall` (4) for the same reason that one is cheap.
pub const ROAD_COST_WOOD: f32 = 3.0;
/// Wood returned per tile when a road is torn up.
pub const ROAD_REFUND: f32 = 0.4;
/// Most tiles one `BuildRoad` command may lay at once — a drag across the
/// whole map should not be a single unbounded allocation from the wire.
pub const MAX_ROAD_TILES_PER_COMMAND: usize = 256;

/// Worker-days one survivor spends clearing one tile by hand
/// (`PlayerCommand::ClearSnow`) — short, since the point of hand-clearing is
/// reopening one blocked stretch, not maintaining the whole network.
pub const CLEAR_SNOW_WORKDAYS: f32 = 0.06;
/// Snow a staffed Snow Crew removes per worker-unit per in-game day, spread
/// over the tiles inside its radius. Measured in the same profession/XP-
/// boosted units `survivor_contribution` returns, so a trained crew clears
/// visibly faster.
pub const SNOW_CREW_CLEAR_PER_UNIT_DAY: f32 = 900.0;
/// Chebyshev radius a Snow Crew keeps clear around itself.
pub const SNOW_CREW_RADIUS: i32 = 7;

// --- V0.22: bino xonalari (see `BuildingKind::size`) ---

/// Side length, in tiles, of a building that has an interior. Three is the
/// smallest size that reads as a ROOM once a wall frame is drawn around it:
/// a 2x2 leaves no floor between opposite walls, and anything larger starts
/// eating the valley (a 64x64 map still holds well over a hundred of these,
/// far past what a colony ever builds).
pub const ROOM_SIZE: u8 = 3;

// --- V0.20: bino ichki jihozlari (see `furnishing::FurnishingKind`) ---

/// Highest a single fitting goes. Deliberately far below
/// `BUILDING_MAX_LEVEL` (10): interiors are meant to shape the EARLY life of
/// a building — the first few upgrades — and then stop being a gate, not to
/// become a second ten-step ladder shadowing the first.
pub const FURNISHING_MAX_LEVEL: u8 = 3;

/// The building level a fitting must keep pace with before the building
/// itself may be upgraded again: every fitting the building takes must be at
/// least `min(building.level, FURNISHING_MAX_LEVEL)`. So a bare room can
/// never climb past level 1, a half-furnished one stalls, and once the
/// fittings are maxed the gate opens for good and ordinary wood-and-time
/// upgrades carry on to 10.
pub fn required_furnishing_level(building_level: u8) -> u8 {
    building_level.min(FURNISHING_MAX_LEVEL)
}

pub fn tile_index(x: u8, y: u8) -> usize {
    y as usize * MAP_W + x as usize
}

pub fn in_bounds(x: i32, y: i32) -> bool {
    x >= 0 && y >= 0 && (x as usize) < MAP_W && (y as usize) < MAP_H
}
