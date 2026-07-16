//! Pure-simulation tests for V0.13: the Tunnel trade caravan (sell goods for
//! gold, or buy goods with gold, via a fixed-duration round trip). Style
//! mirrors `mission_tests.rs`'s Tunnel-adjacent tests and `event_tests.rs`'s
//! single-slot pending-event pattern (`pending_migrant`/`pending_event`).

use frozen_city::game::sim;
use frozen_city::game::types::*;

const SEED: u64 = 12345;

#[test]
fn dispatching_a_caravan_requires_an_unlocked_tunnel() {
    let mut state = sim::new_game(SEED, 12);
    state.stock.wood = 500.0;
    assert!(!state.tunnel.unlocked, "sanity: Tunnel starts locked");

    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::DispatchTradeCaravan { good: TradeGood::Wood, amount: 20, selling: true },
    );

    assert!(state.pending_caravan.is_none(), "a locked Tunnel must refuse a caravan dispatch");
    assert_eq!(state.stock.wood, 500.0, "no wood should be spent on a refused dispatch");
}

#[test]
fn selling_caravan_deducts_goods_immediately_and_credits_gold_on_return() {
    let mut state = sim::new_game(SEED, 12);
    state.tunnel.unlocked = true;
    state.stock.wood = 500.0;

    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::DispatchTradeCaravan { good: TradeGood::Wood, amount: 20, selling: true },
    );

    assert_eq!(state.stock.wood, 480.0, "the goods should leave the stockpile the moment the caravan departs");
    let caravan = state.pending_caravan.expect("a caravan should now be en route");
    assert!(caravan.selling);
    assert_eq!(caravan.good, TradeGood::Wood);
    assert_eq!(caravan.amount, 20);
    assert_eq!(caravan.gold, 20.0 * TradeGood::Wood.sell_price());
    assert_eq!(state.stock.gold, 0.0, "gold isn't credited until the caravan actually returns");

    for _ in 0..(CARAVAN_TRIP_TICKS + 1) {
        sim::tick(&mut state);
    }

    assert!(state.pending_caravan.is_none(), "the caravan should have arrived home");
    assert_eq!(state.stock.gold, 20.0 * TradeGood::Wood.sell_price(), "gold should be credited on return");
    assert!(
        state.events.iter().any(|e| e.text.contains("returned with") && e.text.contains("gold")),
        "a return event should be logged: {:?}",
        state.events
    );
}

#[test]
fn buying_caravan_deducts_gold_immediately_and_credits_goods_on_return() {
    let mut state = sim::new_game(SEED, 12);
    state.tunnel.unlocked = true;
    state.stock.gold = 1000.0;
    let wood_before = state.stock.wood;

    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::DispatchTradeCaravan { good: TradeGood::Wood, amount: 20, selling: false },
    );

    let expected_cost = 20.0 * TradeGood::Wood.buy_price();
    assert_eq!(state.stock.gold, 1000.0 - expected_cost, "gold should be spent the moment the caravan departs");
    assert_eq!(state.stock.wood, wood_before, "wood isn't credited until the caravan actually returns");
    let caravan = state.pending_caravan.expect("a caravan should now be en route");
    assert!(!caravan.selling);

    for _ in 0..(CARAVAN_TRIP_TICKS + 1) {
        sim::tick(&mut state);
    }

    assert!(state.pending_caravan.is_none(), "the caravan should have arrived home");
    assert_eq!(state.stock.wood, wood_before + 20.0, "wood should be credited on return");
}

#[test]
fn only_one_caravan_at_a_time() {
    let mut state = sim::new_game(SEED, 12);
    state.tunnel.unlocked = true;
    state.stock.wood = 500.0;
    state.stock.coal = 500.0;

    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::DispatchTradeCaravan { good: TradeGood::Wood, amount: 20, selling: true },
    );
    let first = state.pending_caravan.expect("first dispatch should succeed");
    let coal_before = state.stock.coal;

    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::DispatchTradeCaravan { good: TradeGood::Coal, amount: 20, selling: true },
    );

    assert_eq!(
        state.pending_caravan,
        Some(first),
        "a second dispatch must be refused while one is already en route"
    );
    assert_eq!(state.stock.coal, coal_before, "the refused dispatch must not spend any coal");
}

#[test]
fn selling_more_than_owned_is_a_noop() {
    let mut state = sim::new_game(SEED, 12);
    state.tunnel.unlocked = true;
    state.stock.wood = 10.0;

    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::DispatchTradeCaravan { good: TradeGood::Wood, amount: 20, selling: true },
    );

    assert!(state.pending_caravan.is_none(), "can't sell more than what's banked");
    assert_eq!(state.stock.wood, 10.0);
}

#[test]
fn buying_without_enough_gold_is_a_noop() {
    let mut state = sim::new_game(SEED, 12);
    state.tunnel.unlocked = true;
    state.stock.gold = 1.0;
    let wood_before = state.stock.wood;

    sim::apply_command(
        &mut state,
        1,
        &PlayerCommand::DispatchTradeCaravan { good: TradeGood::Wood, amount: 20, selling: false },
    );

    assert!(state.pending_caravan.is_none(), "can't buy without enough gold");
    assert_eq!(state.stock.gold, 1.0);
    assert_eq!(state.stock.wood, wood_before);
}

#[test]
fn central_world_may_never_dispatch_a_caravan() {
    let state = sim::new_game_central(SEED);
    assert!(!state.can_issue(
        1,
        &PlayerCommand::DispatchTradeCaravan { good: TradeGood::Wood, amount: 20, selling: true }
    ));
}

#[test]
fn buy_then_sell_prices_have_a_market_spread() {
    // A round trip (buy then immediately sell the same amount back) must
    // never be free money — otherwise trading isn't a real economic choice.
    for good in TradeGood::ALL {
        assert!(
            good.buy_price() > good.sell_price(),
            "{good:?}'s buy price ({}) should exceed its sell price ({}) — a market spread, not arbitrage",
            good.buy_price(),
            good.sell_price()
        );
    }
}
