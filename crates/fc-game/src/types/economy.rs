//! Resources: the stockpile and the central-world contribution ledger.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Stockpile {
    pub wood: f32,
    pub coal: f32,
    pub food: f32,
    /// V0.10: raw hides from hunting (`HunterHut`), consumed by a staffed
    /// `TailorShop` to produce `cloth`.
    pub fur: f32,
    /// V0.10: tailored from `fur` by a staffed `TailorShop`; grants a passive
    /// warmth bonus while worn down (see `sim::tick`'s `tailoring_bonus`).
    pub cloth: f32,
    /// V0.11: drawn by a staffed `Well`, consumed directly by every
    /// survivor's `thirst` — a core survival resource on par with `food`,
    /// not a narrow side-economy.
    pub water: f32,
    /// V0.13: earned by selling goods to the Global World through the
    /// Tunnel (or spent buying them back) — see [`TradeGood`],
    /// [`TradeCaravan`]. Never produced by any building directly, only by
    /// trade.
    pub gold: f32,
}

/// V0.10: abstract, colony-wide deer/rabbit populations the `HunterHut`
/// hunts from — same "global counter, not per-tile/per-entity" abstraction
/// as [`Stockpile`] (the Hunter's Hut has no spatial/tile dependency today,
/// unlike Sawmill's forest tiles or CoalMine's deposit tile). Deer are
/// slower-regenerating but yield more food/fur per hunt; rabbits regenerate
/// faster but yield less — see the `DEER_*`/`RABBIT_*` constants in
/// `types.rs` and the `HunterHut` production arm in `sim::tick`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Wildlife {
    pub deer: f32,
    pub rabbit: f32,
}

/// V0.12: abstract, colony-wide cow/sheep populations the `Farmhouse` raises
/// from — same "global counter, not per-tile/per-entity" abstraction as
/// [`Wildlife`], just domesticated instead of hunted. Cows are
/// slower-regenerating but yield more food per unit; sheep regenerate faster
/// but yield less — see the `COW_*`/`SHEEP_*` constants in `types.rs` and the
/// `Farmhouse` production arm in `sim::tick`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Livestock {
    pub cow: f32,
    pub sheep: f32,
}

/// V0.13: a stockpile good the Tunnel trade caravan can carry — see
/// [`TradeCaravan`]. Buying costs more per unit than selling earns (a market
/// spread), so a round trip is never free arbitrage; the spread also varies
/// by good, wider for the more processed/valuable ones (mirrors real trade
/// margins on manufactured vs. raw goods).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TradeGood {
    Wood,
    Coal,
    Food,
    Fur,
    Cloth,
}

impl TradeGood {
    pub const ALL: [TradeGood; 5] =
        [TradeGood::Wood, TradeGood::Coal, TradeGood::Food, TradeGood::Fur, TradeGood::Cloth];

    pub fn name(self) -> &'static str {
        match self {
            TradeGood::Wood => "Wood",
            TradeGood::Coal => "Coal",
            TradeGood::Food => "Food",
            TradeGood::Fur => "Fur",
            TradeGood::Cloth => "Cloth",
        }
    }

    /// Gold earned per unit sold.
    pub fn sell_price(self) -> f32 {
        match self {
            TradeGood::Wood => 0.8,
            TradeGood::Coal => 1.0,
            TradeGood::Food => 0.9,
            TradeGood::Fur => 1.5,
            TradeGood::Cloth => 3.0,
        }
    }

    /// Gold spent per unit bought — always above `sell_price` (see the type
    /// doc's market-spread note).
    pub fn buy_price(self) -> f32 {
        match self {
            TradeGood::Wood => 1.3,
            TradeGood::Coal => 1.6,
            TradeGood::Food => 1.4,
            TradeGood::Fur => 2.2,
            TradeGood::Cloth => 4.2,
        }
    }

    /// Current banked amount of this good in `stock`.
    pub fn amount_in(self, stock: &Stockpile) -> f32 {
        match self {
            TradeGood::Wood => stock.wood,
            TradeGood::Coal => stock.coal,
            TradeGood::Food => stock.food,
            TradeGood::Fur => stock.fur,
            TradeGood::Cloth => stock.cloth,
        }
    }

    /// Adds (or, for a negative `amount`, removes) this good from `stock`.
    pub fn credit(self, stock: &mut Stockpile, amount: f32) {
        let field = match self {
            TradeGood::Wood => &mut stock.wood,
            TradeGood::Coal => &mut stock.coal,
            TradeGood::Food => &mut stock.food,
            TradeGood::Fur => &mut stock.fur,
            TradeGood::Cloth => &mut stock.cloth,
        };
        *field += amount;
    }
}

/// V0.13: a caravan in transit through the Tunnel to the Global World and
/// back — see `GameState.pending_caravan` (single-slot, same convention as
/// `pending_migrant`). A `selling` caravan has already handed over `amount`
/// of `good` (deducted from the stockpile at dispatch) and comes home with
/// `gold` banked at that tick's price; a buying caravan has already paid
/// `gold` up front and comes home with `amount` of `good` instead.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct TradeCaravan {
    pub selling: bool,
    pub good: TradeGood,
    pub amount: u32,
    /// Selling: gold earned, locked in at dispatch and credited on return.
    /// Buying: unused (0.0) — `good`/`amount` is what's credited instead.
    pub gold: f32,
    pub departed_tick: u64,
    pub return_tick: u64,
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
