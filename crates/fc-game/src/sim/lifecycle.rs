//! V0.18: the survivor life cycle — aging, pairing, childhood and old age.
//!
//! V0.11 already gave the colony births; what it did not give anyone was a
//! LIFE: a newborn was a full worker the instant it appeared, and nobody ever
//! grew old. This module adds the arc — [`Survivor::age_days`] ticks up for
//! everyone, `Survivor::stage` reads it, and the sim's existing per-survivor
//! multipliers (`age_work_factor`, `food_factor`, `is_frail`) pick it up
//! wherever work, food and illness are already computed. Pairing
//! ([`Survivor::partner`]) is the social half: couples make births much more
//! likely without ever being required for one.
//!
//! Draws from `GameState.lifecycle_rng` — an isolated stream, so the daily
//! pairing/old-age rolls can never re-sequence arrivals, weather or infection.

use crate::rng::Rng;
use crate::types::*;

use super::push_event;

/// A starting age for someone who walks into the colony already grown (a
/// morning arrival, a Tunnel traveller, a rescued stranger) — mapped into
/// `ARRIVAL_AGE_MIN_DAYS..ARRIVAL_AGE_MAX_DAYS`, never 0: only a birth
/// produces a child.
///
/// Derived deterministically from the survivor's own id, NOT drawn from the
/// sim rng: `mapgen::new_survivor` calls this for every spawn, and a
/// shared-stream draw here would consume one extra roll per survivor,
/// shifting every later name/hp/hunger/profession draw for that survivor —
/// and every survivor spawned after them — re-sequencing arrivals, births
/// and weather out from under every seed-based expectation elsewhere (tests,
/// balance tools). Same SplitMix64 finalizer as `Profession::from_id_hash` /
/// `GameState::spawn_position`, salted differently (and differently again
/// from `legacy::migrated_age`'s own id-hash for pre-V0.18 saves) so none of
/// these derived values correlate with each other or collide on the same id.
pub fn starting_age(id: u32) -> f32 {
    let mut z = (id as u64 ^ 0x5EED_ADE1_7A9E_0001) ^ 0x9E37_79B9_7F4A_7C15;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    let span = ARRIVAL_AGE_MAX_DAYS - ARRIVAL_AGE_MIN_DAYS;
    ARRIVAL_AGE_MIN_DAYS + (z % span.max(1.0) as u64) as f32
}

/// Multiplier applied to `BIRTH_CHANCE`: a colony with at least one couple is
/// markedly more fertile, one with none still isn't sterile (V0.11's roll
/// stays as the floor).
pub fn birth_chance_multiplier(state: &GameState) -> f32 {
    if state.survivors.iter().any(|s| s.partner.is_some()) {
        BIRTH_COUPLE_MULTIPLIER
    } else {
        1.0
    }
}

/// Clear both sides of any partnership involving `dead`. Called from the
/// death path: a dangling `partner` id would outlive the person it names, and
/// every later lookup would silently resolve to whoever inherits that id.
pub fn clear_partner_links(state: &mut GameState, dead: &[u32]) {
    for s in state.survivors.iter_mut() {
        if s.partner.is_some_and(|p| dead.contains(&p)) {
            s.partner = None;
        }
    }
}

/// Per-tick life-cycle upkeep, called from `sim::tick`.
///
/// Aging is continuous (a fraction of a day per tick, so a survivor's age is
/// always exactly "ticks lived / TICKS_PER_DAY"); pairing and old-age death
/// are once-daily rolls, checked at `BIRTH_TICK` so they share the day's
/// "colony business" moment with the birth check rather than scattering
/// events across the clock.
pub fn tick_lifecycle(state: &mut GameState) {
    // The central world has no hunger, no death and no births (see
    // `GameState.central`) — settlers there are visitors, not a population.
    if state.central {
        return;
    }
    let per_tick = 1.0 / TICKS_PER_DAY as f32;
    for s in state.survivors.iter_mut() {
        s.age_days += per_tick;
    }
    if state.tick % TICKS_PER_DAY != BIRTH_TICK {
        return;
    }
    let mut rng = Rng(state.lifecycle_rng);
    daily_pairing(state, &mut rng);
    daily_old_age(state, &mut rng);
    state.lifecycle_rng = rng.0;
}

/// At most one new couple per day: two unpartnered adults, each picked by an
/// independent random index into the eligible list (bumping `b` to the next
/// slot on a collision with `a`) so it isn't always the two lowest ids that
/// find each other.
fn daily_pairing(state: &mut GameState, rng: &mut Rng) {
    if !rng.chance(PARTNER_CHANCE_PER_DAY) {
        return;
    }
    let eligible: Vec<u32> = state
        .survivors
        .iter()
        .filter(|s| s.partner.is_none() && s.stage() == LifeStage::Adult)
        .map(|s| s.id)
        .collect();
    if eligible.len() < 2 {
        return;
    }
    let a_idx = rng.below(eligible.len() as u32) as usize;
    let mut b_idx = rng.below(eligible.len() as u32) as usize;
    if b_idx == a_idx {
        b_idx = (b_idx + 1) % eligible.len();
    }
    let (a, b) = (eligible[a_idx], eligible[b_idx]);
    let mut names = (String::new(), String::new());
    for s in state.survivors.iter_mut() {
        if s.id == a {
            s.partner = Some(b);
            names.0 = s.name.clone();
        } else if s.id == b {
            s.partner = Some(a);
            names.1 = s.name.clone();
        }
    }
    state.morale = (state.morale + MORALE_PARTNER_BONUS).clamp(0.0, 100.0);
    push_event(state, format!("{} and {} have paired up.", names.0, names.1));
}

/// Old age: past `ELDER_AGE_DAYS` the daily death chance grows with how far
/// past it a survivor is. Applied as HP damage rather than a direct removal
/// so the existing death path (corpse, mourning, morale, partner cleanup)
/// runs exactly once, in one place, for every cause of death.
fn daily_old_age(state: &mut GameState, rng: &mut Rng) {
    for s in state.survivors.iter_mut() {
        if s.stage() != LifeStage::Elder {
            continue;
        }
        let past = (s.age_days - ELDER_AGE_DAYS).max(0.0);
        // Doubles roughly every `ELDER_AGE_DAYS / 2` days past the threshold.
        let chance = (OLD_AGE_DEATH_PER_DAY * (1.0 + past / (ELDER_AGE_DAYS * 0.5))).min(0.5);
        if rng.chance(chance) {
            // A flat 0.0 here would be quietly healed back above zero before
            // the death check ever reads it: `tick_lifecycle` runs BEFORE the
            // survivor loop (see `sim::tick`'s call site), and that same
            // loop's warmth/hospital-care regen adds a small positive amount
            // to `hp` every tick for anyone warm and fed — exactly the
            // colony condition where old age should still be able to claim
            // someone. All such same-tick top-ups are well under 1.0; a
            // comfortably negative value survives any of them and still
            // reads as dead once the shared check runs.
            s.hp = -100.0;
        }
    }
}
