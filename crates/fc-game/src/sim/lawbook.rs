//! V0.18: enacting and repealing laws.
//!
//! Deliberately thin: a law has no per-tick logic of its own — every effect
//! is read through the composed multiplier queries on `GameState`
//! (`law_production_multiplier` and friends), so this module only owns the
//! transition (validate, write, cool down, announce). Adding a law is a data
//! change in `types::law`, never a new branch in `sim::tick`.

use crate::types::*;

use super::push_event;

/// Apply `PlayerCommand::EnactLaw`.
pub fn enact(state: &mut GameState, law: Law) {
    if state.can_enact_law(law).is_err() {
        return;
    }
    state.laws.push(law);
    state.law_cooldown_until = state.tick + LAW_COOLDOWN_TICKS;
    push_event(state, format!("A new law stands: {}.", law.name()));
}

/// Apply `PlayerCommand::RepealLaw`.
pub fn repeal(state: &mut GameState, law: Law) {
    if state.can_repeal_law(law).is_err() {
        return;
    }
    state.laws.retain(|l| *l != law);
    state.law_cooldown_until = state.tick + LAW_COOLDOWN_TICKS;
    push_event(state, format!("{} was repealed.", law.name()));
}
