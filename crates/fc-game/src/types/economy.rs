//! Resources: the stockpile and the central-world contribution ledger.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Stockpile {
    pub wood: f32,
    pub coal: f32,
    pub food: f32,
}

/// One account's running contribution totals in the central world (V0.5
/// "economy v1"): credited for what their owned settlers' staffed buildings
/// produce there, and for what they spend placing buildings there. Purely a
/// ledger — it does not gate anything (yet); it's the data source for
/// `ServerMsg::Showcase`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct ContributionTotals {
    pub wood: f32,
    pub coal: f32,
    pub food: f32,
    /// Wood spent placing buildings (a cost, tracked separately from the
    /// production credits above so a showcase can show "built" vs "produced").
    pub wood_spent: f32,
}

/// One row of the central-world contribution ledger, keyed by account.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct LedgerEntry {
    pub account: i64,
    pub totals: ContributionTotals,
}
