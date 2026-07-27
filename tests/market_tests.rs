//! Tests for the V0.18 global market (`fc_net::market`): a player-to-player
//! order book living in the accounts DB. These drive the module's public API
//! directly — `market::handle(account, &ClientMsg, &mut GameState)` and
//! `market::claim_wallet(account, &mut GameState)` — against locally built
//! `GameState`s (`sim::new_game`), with no real server/`WorldManager`
//! involved: every one of these commands is a pure function of "which
//! account, which message, which world" plus the shared SQLite file, so a
//! real TCP/WS harness would only add noise here (see `social_server_tests.rs`
//! for the pattern used when the wire protocol itself is what's under test).
//!
//! Deliberately a single `#[test]`: it sets `FC_ACCOUNTS_DB`, a process-wide
//! env var `market::db_path` reads, and cargo runs `#[test]` fns in one
//! binary on separate threads by default — a second test setting its own
//! path here would race this one (same reasoning as every other env-var-
//! setting test in this suite, e.g. `account_world_e2e.rs`).
//!
//! Every scenario uses its own dedicated account id (never reused across
//! sections) so none of them can accidentally interact through the shared
//! `MAX_ORDERS_PER_ACCOUNT` quota or a stray wallet credit.

use std::sync::{Arc, Barrier};
use std::thread;

use frozen_city::game::sim;
use frozen_city::game::types::TradeGood;
use frozen_city::net::market;
use frozen_city::net::protocol::{ClientMsg, MarketOrder, ServerMsg, Wallet};

fn seed_accounts(db_path: &std::path::Path, accounts: &[(i64, &str)]) {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE accounts (
            id INTEGER PRIMARY KEY,
            display_username TEXT NOT NULL
        );",
    )
    .unwrap();
    for (id, name) in accounts {
        conn.execute(
            "INSERT INTO accounts (id, display_username) VALUES (?1, ?2)",
            rusqlite::params![id, name],
        )
        .unwrap();
    }
}

/// Unwraps a successful market response, panicking (with the response shown)
/// if the command was unexpectedly refused — a wrong assumption in the test
/// itself, not the thing under test.
fn expect_market(resp: ServerMsg) -> (Vec<MarketOrder>, Wallet) {
    match resp {
        ServerMsg::Market { orders, wallet } => (orders, wallet.unwrap_or_default()),
        other => panic!("expected ServerMsg::Market, got {other:?}"),
    }
}

/// Unwraps a refusal's system-bubble text (see `server::messages::
/// system_bubble`: `player_id: 0` marks system feedback), panicking if the
/// command unexpectedly succeeded.
fn expect_refusal(resp: ServerMsg) -> String {
    match resp {
        ServerMsg::Bubble { player_id: 0, text, .. } => text,
        other => panic!("expected a refusal bubble, got {other:?}"),
    }
}

#[test]
fn market_order_book() {
    let db_dir = std::env::temp_dir().join(format!("fc-market-tests-db-{}", std::process::id()));
    std::fs::create_dir_all(&db_dir).unwrap();
    let db_path = db_dir.join("accounts.db");
    // SAFETY: this test binary's only test function, so nothing else in the
    // process reads/writes this environment variable concurrently.
    unsafe {
        std::env::set_var("FC_ACCOUNTS_DB", &db_path);
    }
    seed_accounts(
        &db_path,
        &[
            (1, "Poster"),
            (2, "Taker"),
            (3, "Buyer"),
            (4, "Seller"),
            (5, "CancelPoster"),
            (6, "PartialTaker"),
            (7, "RefusalAcct"),
            (8, "QuotaAcct"),
            (9, "PoorGoldTaker"),
            (10, "CentralAcct"),
            (11, "BuyRefusalPoster"),
            (12, "PoorGoodsTaker"),
            (13, "CentralPayoutTaker"),
            (30, "RacePoster"),
            (31, "RaceTakerA"),
            (32, "RaceTakerB"),
        ],
    );

    // === Sell order lifecycle: post -> partial fill -> conservation ->
    // full fill -> claim. === .
    let mut poster = sim::new_game(101, 12);
    poster.stock.wood = 100.0;
    poster.stock.gold = 50.0; // unrelated starting gold, to prove claim ADDS rather than overwrites.
    let mut taker = sim::new_game(102, 12);
    taker.stock.gold = 1000.0;
    taker.stock.wood = 0.0; // `new_game` seeds a starting 60 wood by default; zero it for exact arithmetic below.

    let resp = market::handle(
        1,
        &ClientMsg::PostOrder { good: TradeGood::Wood, amount: 40, unit_price: 2.0, selling: true },
        &mut poster,
    );
    let (orders, wallet) = expect_market(resp);
    assert_eq!(poster.stock.wood, 60.0, "escrow must leave the poster's stockpile immediately");
    assert_eq!(wallet.gold, 0.0, "nothing owed yet — no trade has happened");
    assert_eq!(orders.len(), 1);
    let order_id = orders[0].id;
    assert_eq!(orders[0].account, 1);
    assert_eq!(orders[0].good, TradeGood::Wood);
    assert_eq!(orders[0].amount, 40);
    assert!((orders[0].unit_price - 2.0).abs() < 1e-6);
    assert!(orders[0].selling);

    // Partial fill: take 15 of the 40.
    let resp = market::handle(2, &ClientMsg::TakeOrder { order: order_id, amount: 15 }, &mut taker);
    let (orders, taker_wallet) = expect_market(resp);
    assert_eq!(taker.stock.wood, 15.0);
    assert_eq!(taker.stock.gold, 1000.0 - 30.0);
    assert!(taker_wallet.is_empty(), "a taker's proceeds settle instantly, never through a wallet");
    let remaining = orders.iter().find(|o| o.id == order_id).expect("order still open after a partial fill");
    assert_eq!(remaining.amount, 25);

    // The poster's own gold is untouched — the 30 gold owed sits in their
    // wallet until they claim it, even though they're the one who asks.
    let (_, poster_wallet) = expect_market(market::handle(1, &ClientMsg::RefreshMarket, &mut poster));
    assert_eq!(poster_wallet.gold, 30.0);
    assert_eq!(poster.stock.gold, 50.0, "unclaimed proceeds must not silently appear in stock");

    // Conservation: nothing was created or destroyed, only moved.
    assert_eq!(
        poster.stock.wood + taker.stock.wood + 25.0,
        100.0,
        "original 100 wood == poster's remainder + taker's haul + what's still escrowed in the order"
    );
    assert_eq!(
        taker.stock.gold + poster_wallet.gold,
        1000.0,
        "taker's original 1000 gold == taker's remainder + poster's pending wallet"
    );

    // Second take asks for far more than the 25 left — clamps rather than
    // erroring, and retires the now fully-filled order.
    let resp = market::handle(2, &ClientMsg::TakeOrder { order: order_id, amount: 999 }, &mut taker);
    let (orders, _) = expect_market(resp);
    assert!(orders.iter().all(|o| o.id != order_id), "a fully filled order must be retired from the book");
    assert_eq!(taker.stock.wood, 40.0);
    assert_eq!(taker.stock.gold, 1000.0 - 80.0);

    // claim_wallet pays out exactly once and clears.
    market::claim_wallet(1, &mut poster);
    assert_eq!(poster.stock.gold, 50.0 + 80.0);
    market::claim_wallet(1, &mut poster);
    assert_eq!(poster.stock.gold, 130.0, "a second claim must be a no-op — the wallet was already cleared");
    let (_, poster_wallet) = expect_market(market::handle(1, &ClientMsg::RefreshMarket, &mut poster));
    assert!(poster_wallet.is_empty());

    // === Buy-side, symmetric: gold escrowed at post time, goods delivered
    // through the wallet at claim time. === .
    let mut buyer = sim::new_game(103, 12);
    buyer.stock.gold = 200.0;
    buyer.stock.coal = 0.0; // `new_game` seeds a starting 40 coal by default; zero it for exact arithmetic below.
    let resp = market::handle(
        3,
        &ClientMsg::PostOrder { good: TradeGood::Coal, amount: 20, unit_price: 3.0, selling: false },
        &mut buyer,
    );
    let (orders, _) = expect_market(resp);
    assert_eq!(buyer.stock.gold, 200.0 - 60.0);
    let buy_order_id = orders[0].id;
    assert!(!orders[0].selling);

    let mut seller = sim::new_game(104, 12);
    seller.stock.coal = 50.0;
    let resp = market::handle(4, &ClientMsg::TakeOrder { order: buy_order_id, amount: 20 }, &mut seller);
    let (orders, _) = expect_market(resp);
    assert_eq!(seller.stock.coal, 30.0);
    assert_eq!(seller.stock.gold, 60.0);
    assert!(orders.iter().all(|o| o.id != buy_order_id));

    market::claim_wallet(3, &mut buyer);
    assert_eq!(buyer.stock.coal, 20.0, "goods bought are delivered through the wallet, symmetric to gold selling");
    assert_eq!(buyer.stock.gold, 140.0, "gold was already spent at post time — claiming must not spend it again");

    // === Cancel refunds exactly the unfilled remainder. === .
    let mut cposter = sim::new_game(105, 12);
    cposter.stock.food = 100.0;
    let resp = market::handle(
        5,
        &ClientMsg::PostOrder { good: TradeGood::Food, amount: 30, unit_price: 1.5, selling: true },
        &mut cposter,
    );
    let (orders, _) = expect_market(resp);
    assert_eq!(cposter.stock.food, 70.0);
    let cancel_order_id = orders[0].id;

    let mut ctaker = sim::new_game(106, 12);
    ctaker.stock.gold = 100.0;
    ctaker.stock.food = 0.0; // `new_game` seeds a starting 25 food by default; zero it for exact arithmetic below.
    let resp = market::handle(6, &ClientMsg::TakeOrder { order: cancel_order_id, amount: 10 }, &mut ctaker);
    let (orders, _) = expect_market(resp);
    assert_eq!(ctaker.stock.food, 10.0);
    assert_eq!(ctaker.stock.gold, 85.0);
    let remaining = orders.iter().find(|o| o.id == cancel_order_id).unwrap();
    assert_eq!(remaining.amount, 20);

    let resp = market::handle(5, &ClientMsg::CancelOrder { order: cancel_order_id }, &mut cposter);
    let (orders, _) = expect_market(resp);
    assert!(orders.iter().all(|o| o.id != cancel_order_id), "cancelling must remove the order from the book");

    market::claim_wallet(5, &mut cposter);
    assert_eq!(
        cposter.stock.food, 70.0 + 20.0,
        "cancel must refund exactly the unfilled remainder (20), not the original 30"
    );
    assert_eq!(cposter.stock.gold, 15.0, "the 10 units already sold before cancelling still pay out (10 * 1.5)");

    // === Refusals. === .
    let mut r = sim::new_game(107, 12);
    r.stock.wood = 5.0;
    r.stock.gold = 1.0;
    let before = r.stock;

    assert_eq!(
        expect_refusal(market::handle(
            7,
            &ClientMsg::PostOrder { good: TradeGood::Wood, amount: 0, unit_price: 1.0, selling: true },
            &mut r
        )),
        "That is not a tradeable amount."
    );
    assert_eq!(
        expect_refusal(market::handle(
            7,
            &ClientMsg::PostOrder {
                good: TradeGood::Wood,
                amount: market::MAX_ORDER_AMOUNT + 1,
                unit_price: 1.0,
                selling: true
            },
            &mut r
        )),
        "That is not a tradeable amount."
    );
    assert_eq!(
        expect_refusal(market::handle(
            7,
            &ClientMsg::PostOrder { good: TradeGood::Wood, amount: 1, unit_price: 0.0, selling: true },
            &mut r
        )),
        "That price is out of range."
    );
    assert_eq!(
        expect_refusal(market::handle(
            7,
            &ClientMsg::PostOrder {
                good: TradeGood::Wood,
                amount: 1,
                unit_price: market::MAX_UNIT_PRICE + 1.0,
                selling: true
            },
            &mut r
        )),
        "That price is out of range."
    );
    assert_eq!(expect_refusal(market::handle(
        7,
        &ClientMsg::PostOrder { good: TradeGood::Wood, amount: 6, unit_price: 1.0, selling: true },
        &mut r
    )), "You do not have that much to sell.");
    assert_eq!(expect_refusal(market::handle(
        7,
        &ClientMsg::PostOrder { good: TradeGood::Wood, amount: 5, unit_price: 1.0, selling: false },
        &mut r
    )), "Not enough gold.");
    assert_eq!(r.stock, before, "every refused post above must leave stock completely untouched");

    // A legitimate order to test ownership/existence refusals against.
    let (orders, _) = expect_market(market::handle(
        7,
        &ClientMsg::PostOrder { good: TradeGood::Wood, amount: 5, unit_price: 1.0, selling: true },
        &mut r,
    ));
    let own_order = orders[0].id;
    assert_eq!(
        expect_refusal(market::handle(7, &ClientMsg::TakeOrder { order: own_order, amount: 1 }, &mut r)),
        "That is your own order."
    );
    assert_eq!(
        expect_refusal(market::handle(7, &ClientMsg::TakeOrder { order: 987_654_321, amount: 1 }, &mut r)),
        "That order is gone."
    );

    // Take-side affordability: a taker without enough gold for a sell order.
    let mut poor_gold = sim::new_game(108, 12);
    poor_gold.stock.wood = 0.0; // `new_game` seeds a starting 60 wood by default; zero it so the assertion below is meaningful.
    assert_eq!(
        expect_refusal(market::handle(9, &ClientMsg::TakeOrder { order: own_order, amount: 5 }, &mut poor_gold)),
        "Not enough gold."
    );
    assert_eq!(poor_gold.stock.wood, 0.0, "a refused take must not deliver any goods");

    // Take-side affordability: a taker without enough goods for a buy order.
    let mut buy_refusal_poster = sim::new_game(109, 12);
    buy_refusal_poster.stock.gold = 50.0;
    let (orders, _) = expect_market(market::handle(
        11,
        &ClientMsg::PostOrder { good: TradeGood::Coal, amount: 10, unit_price: 2.0, selling: false },
        &mut buy_refusal_poster,
    ));
    let buy_target = orders[0].id;
    let mut poor_goods = sim::new_game(110, 12);
    poor_goods.stock.coal = 0.0; // `new_game` seeds a starting 40 coal by default, which would silently satisfy this take.
    assert_eq!(
        expect_refusal(market::handle(12, &ClientMsg::TakeOrder { order: buy_target, amount: 10 }, &mut poor_goods)),
        "You do not have that much."
    );
    assert_eq!(poor_goods.stock.gold, 0.0, "a refused take must not pay out any gold");

    // Too many open orders.
    let mut quota = sim::new_game(111, 12);
    quota.stock.wood = 100.0;
    for _ in 0..market::MAX_ORDERS_PER_ACCOUNT {
        expect_market(market::handle(
            8,
            &ClientMsg::PostOrder { good: TradeGood::Wood, amount: 1, unit_price: 1.0, selling: true },
            &mut quota,
        ));
    }
    let quota_wood = quota.stock.wood;
    assert_eq!(quota_wood, 100.0 - market::MAX_ORDERS_PER_ACCOUNT as f32);
    assert_eq!(
        expect_refusal(market::handle(
            8,
            &ClientMsg::PostOrder { good: TradeGood::Wood, amount: 1, unit_price: 1.0, selling: true },
            &mut quota
        )),
        "You already have too many orders standing."
    );
    assert_eq!(quota.stock.wood, quota_wood, "a refused post (quota) must not touch stock either");

    // === Every market command is refused from the central world; only the
    // book itself stays readable there. === .
    let mut central = sim::new_game_central(500);
    assert!(central.central);
    assert_eq!(
        expect_refusal(market::handle(
            10,
            &ClientMsg::PostOrder { good: TradeGood::Wood, amount: 1, unit_price: 1.0, selling: true },
            &mut central
        )),
        "Trade from your own city, not the Global World."
    );
    assert_eq!(
        expect_refusal(market::handle(10, &ClientMsg::TakeOrder { order: 1, amount: 1 }, &mut central)),
        "Trade from your own city, not the Global World."
    );
    assert_eq!(
        expect_refusal(market::handle(10, &ClientMsg::CancelOrder { order: 1 }, &mut central)),
        "Trade from your own city, not the Global World."
    );
    // Reading the book must still work centrally — panics via `expect_market`
    // if it comes back as a refusal instead.
    expect_market(market::handle(10, &ClientMsg::RefreshMarket, &mut central));

    // `claim_wallet` must be a no-op centrally, and — crucially — must not
    // destroy a pending payout in the process: give account 10 something to
    // claim (from its own personal world), then try claiming it while
    // standing in the central world, then prove the payout is still there
    // for the real personal world afterward.
    let mut personal10 = sim::new_game(510, 12);
    personal10.stock.wood = 10.0;
    let (orders, _) = expect_market(market::handle(
        10,
        &ClientMsg::PostOrder { good: TradeGood::Wood, amount: 10, unit_price: 4.0, selling: true },
        &mut personal10,
    ));
    let central_payout_order = orders[0].id;
    let mut central_payout_taker = sim::new_game(511, 12);
    central_payout_taker.stock.gold = 1000.0;
    expect_market(market::handle(
        13,
        &ClientMsg::TakeOrder { order: central_payout_order, amount: 10 },
        &mut central_payout_taker,
    ));

    let central_before = central.stock;
    market::claim_wallet(10, &mut central);
    assert_eq!(central.stock, central_before, "a personal payout must never land in the communal pile");
    market::claim_wallet(10, &mut personal10);
    assert_eq!(personal10.stock.gold, 40.0, "the payout must survive an intervening no-op central claim");

    // === Regression test: two takers racing the full amount of the same
    // order must never both settle. Before the fix, a lost race still
    // credited the poster's wallet (that credit happened BEFORE the atomic
    // amount-decrement was even attempted), even though the order's `amount`
    // was left untouched — so the same goods could be sold again to the next
    // taker, duplicating the payout. === .
    let mut race_poster = sim::new_game(600, 12);
    race_poster.stock.wood = 10.0;
    let (orders, _) = expect_market(market::handle(
        30,
        &ClientMsg::PostOrder { good: TradeGood::Wood, amount: 10, unit_price: 2.0, selling: true },
        &mut race_poster,
    ));
    let race_order = orders[0].id;

    let barrier = Arc::new(Barrier::new(2));
    let (ba, bb) = (barrier.clone(), barrier.clone());
    let ta = thread::spawn(move || {
        let mut taker = sim::new_game(601, 12);
        taker.stock.gold = 1000.0;
        taker.stock.wood = 0.0; // `new_game` seeds a starting 60 wood by default; zero it for exact arithmetic below.
        ba.wait();
        let resp = market::handle(31, &ClientMsg::TakeOrder { order: race_order, amount: 10 }, &mut taker);
        (resp, taker)
    });
    let tb = thread::spawn(move || {
        let mut taker = sim::new_game(602, 12);
        taker.stock.gold = 1000.0;
        taker.stock.wood = 0.0; // `new_game` seeds a starting 60 wood by default; zero it for exact arithmetic below.
        bb.wait();
        let resp = market::handle(32, &ClientMsg::TakeOrder { order: race_order, amount: 10 }, &mut taker);
        (resp, taker)
    });
    let (resp_a, taker_a) = ta.join().unwrap();
    let (resp_b, taker_b) = tb.join().unwrap();

    let a_won = matches!(resp_a, ServerMsg::Market { .. });
    let b_won = matches!(resp_b, ServerMsg::Market { .. });
    assert_ne!(a_won, b_won, "exactly one of two takers racing the full amount of one order may win");
    let (winner, loser) = if a_won { (&taker_a, &taker_b) } else { (&taker_b, &taker_a) };
    assert_eq!(winner.stock.wood, 10.0);
    assert_eq!(winner.stock.gold, 1000.0 - 20.0);
    assert_eq!(loser.stock.wood, 0.0, "the losing side of the race must be left completely untouched");
    assert_eq!(loser.stock.gold, 1000.0);

    let (orders, race_wallet) = expect_market(market::handle(30, &ClientMsg::RefreshMarket, &mut race_poster));
    assert!(orders.iter().all(|o| o.id != race_order), "the fully-raced order must be retired");
    assert_eq!(race_wallet.gold, 20.0, "the poster's wallet must reflect the ONE trade that happened, never both");

    std::fs::remove_dir_all(&db_dir).ok();
    std::env::remove_var("FC_ACCOUNTS_DB");
}
