//! V0.18: the global market — a player-to-player order book living in the
//! accounts DB (native server only).
//!
//! V0.13 already let a colony trade through the Tunnel, but only with an
//! abstract NPC price list: gold in, goods out, nobody on the other end. This
//! is the other half — the other end is another player's colony.
//!
//! **Where trading happens.** Orders are posted and taken from a player's own
//! PERSONAL world, because that is where their stockpile is. The central
//! world's stock is communal (everyone's contributions in one pile, see
//! `central_ledger`), so selling out of it would be selling other people's
//! wood; market commands are refused there and only the book itself is
//! readable.
//!
//! **Escrow and wallets.** A posted order's goods (selling) or gold (buying)
//! leave the poster's stockpile immediately, so an order is always backed. The
//! counterparty settles instantly into their own stock — but the POSTER may be
//! offline, and reaching into a world that isn't running (or worse, spawning
//! one to deliver two logs of wood) is exactly the kind of cross-world write
//! this codebase avoids everywhere else. So their proceeds land in a
//! [`Wallet`] row instead, and [`claim_wallet`] pays it into whatever world
//! they next join. Cancelling refunds the unfilled remainder the same way.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension};

use fc_game::types::{GameState, TradeGood};

use crate::protocol::{ClientMsg, MarketOrder, ServerMsg, Wallet};

/// Largest single order, mirroring `CARAVAN_MAX_AMOUNT`'s role for the NPC
/// caravan: a bound on how much one click can move.
pub const MAX_ORDER_AMOUNT: u32 = 500;
/// Most open orders one account may hold, so one player can't paper the book.
pub const MAX_ORDERS_PER_ACCOUNT: usize = 8;
/// Rows returned by a `RefreshMarket` — newest first, bounded so the book
/// stays one frame.
pub const MAX_BOOK_ROWS: usize = 60;
/// Price band. Zero/negative or absurd prices are refused outright rather than
/// clamped, so what a player typed is what they get or a clear refusal.
pub const MIN_UNIT_PRICE: f32 = 0.01;
pub const MAX_UNIT_PRICE: f32 = 999.0;

fn db_path() -> String {
    std::env::var("FC_ACCOUNTS_DB")
        .unwrap_or_else(|_| crate::accounts::DEFAULT_DB_PATH.to_string())
}

/// Opens the accounts DB read-write and ensures the two market tables exist.
/// Same shape as `accounts::open_rw` (which is private to that module): both
/// tables are server-only, the registration bot never touches them.
fn open() -> Option<Connection> {
    let path = db_path();
    if let Some(dir) = Path::new(&path).parent() {
        std::fs::create_dir_all(dir).ok()?;
    }
    let conn = Connection::open(&path).ok()?;
    let _ = conn.busy_timeout(std::time::Duration::from_secs(3));
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS market_orders (
            id INTEGER PRIMARY KEY,
            account_id INTEGER NOT NULL,
            good INTEGER NOT NULL,
            amount INTEGER NOT NULL,
            unit_price REAL NOT NULL,
            selling INTEGER NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS market_wallet (
            account_id INTEGER PRIMARY KEY,
            gold REAL NOT NULL DEFAULT 0,
            wood REAL NOT NULL DEFAULT 0,
            coal REAL NOT NULL DEFAULT 0,
            food REAL NOT NULL DEFAULT 0,
            fur REAL NOT NULL DEFAULT 0,
            cloth REAL NOT NULL DEFAULT 0
        );",
    )
    .ok()?;
    Some(conn)
}

/// `TradeGood` <-> stable integer, so the DB never depends on the enum's
/// declaration order the way bincode does.
fn good_code(g: TradeGood) -> i64 {
    match g {
        TradeGood::Wood => 0,
        TradeGood::Coal => 1,
        TradeGood::Food => 2,
        TradeGood::Fur => 3,
        TradeGood::Cloth => 4,
    }
}

fn good_from_code(code: i64) -> Option<TradeGood> {
    Some(match code {
        0 => TradeGood::Wood,
        1 => TradeGood::Coal,
        2 => TradeGood::Food,
        3 => TradeGood::Fur,
        4 => TradeGood::Cloth,
        _ => return None,
    })
}

/// Adds `amount` of `good` to a wallet row, creating it on first use.
fn credit_wallet_good(conn: &Connection, account: i64, good: TradeGood, amount: f32) {
    let col = match good {
        TradeGood::Wood => "wood",
        TradeGood::Coal => "coal",
        TradeGood::Food => "food",
        TradeGood::Fur => "fur",
        TradeGood::Cloth => "cloth",
    };
    let sql = format!(
        "INSERT INTO market_wallet (account_id, {col}) VALUES (?1, ?2)
         ON CONFLICT(account_id) DO UPDATE SET {col} = {col} + ?2"
    );
    let _ = conn.execute(&sql, rusqlite::params![account, amount as f64]);
}

fn credit_wallet_gold(conn: &Connection, account: i64, gold: f32) {
    let _ = conn.execute(
        "INSERT INTO market_wallet (account_id, gold) VALUES (?1, ?2)
         ON CONFLICT(account_id) DO UPDATE SET gold = gold + ?2",
        rusqlite::params![account, gold as f64],
    );
}

/// Maps one `market_wallet` row into a `Wallet` — shared by `read_wallet`
/// (a peek, used by `book_msg`) and `claim_wallet` (an atomic take-and-clear
/// via `DELETE ... RETURNING`, same column order).
fn wallet_from_row(r: &rusqlite::Row) -> rusqlite::Result<Wallet> {
    Ok(Wallet {
        gold: r.get::<_, f64>(0)? as f32,
        wood: r.get::<_, f64>(1)? as f32,
        coal: r.get::<_, f64>(2)? as f32,
        food: r.get::<_, f64>(3)? as f32,
        fur: r.get::<_, f64>(4)? as f32,
        cloth: r.get::<_, f64>(5)? as f32,
    })
}

fn read_wallet(conn: &Connection, account: i64) -> Wallet {
    conn.query_row(
        "SELECT gold, wood, coal, food, fur, cloth FROM market_wallet WHERE account_id = ?1",
        [account],
        wallet_from_row,
    )
    .optional()
    .ok()
    .flatten()
    .unwrap_or_default()
}

fn read_book(conn: &Connection) -> Vec<MarketOrder> {
    let mut stmt = match conn.prepare(
        "SELECT o.id, o.account_id, COALESCE(a.display_username, 'Unknown'),
                o.good, o.amount, o.unit_price, o.selling
           FROM market_orders o
           LEFT JOIN accounts a ON a.id = o.account_id
          WHERE o.amount > 0
          ORDER BY o.id DESC
          LIMIT ?1",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt.query_map([MAX_BOOK_ROWS as i64], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, f64>(5)?,
            r.get::<_, i64>(6)?,
        ))
    });
    let Ok(rows) = rows else {
        return Vec::new();
    };
    rows.filter_map(|row| {
        let (id, account, name, good, amount, unit_price, selling) = row.ok()?;
        Some(MarketOrder {
            id,
            account,
            name,
            good: good_from_code(good)?,
            amount: amount.max(0) as u32,
            unit_price: unit_price as f32,
            selling: selling != 0,
        })
    })
    .collect()
}

/// The answer every market command gets: the fresh book plus the caller's own
/// wallet. One message shape means the client never has to reconcile a partial
/// update against what it already had.
fn book_msg(conn: &Connection, account: i64) -> ServerMsg {
    ServerMsg::Market {
        orders: read_book(conn),
        wallet: Some(read_wallet(conn, account)),
    }
}

/// Pay out everything the market owes `account` into `state`'s stockpile, and
/// clear the wallet row. Called once per account join (see `simloop`), which is
/// the only moment a world is guaranteed to be running and to be the right one.
pub fn claim_wallet(account: i64, state: &mut GameState) {
    // The central world's stock is communal — paying an individual's market
    // proceeds into it would hand them to everyone. The wallet simply waits
    // for the account to be in its own colony.
    if state.central {
        return;
    }
    let Some(conn) = open() else {
        return;
    };
    // A single atomic DELETE...RETURNING, not a separate SELECT then DELETE:
    // this account's OWN world only ever claims from its own sim_loop thread,
    // but a DIFFERENT account's thread can credit this wallet at any moment
    // via `take()` settling one of this account's orders. A two-step
    // read-then-unconditional-delete would let such a credit land in the gap
    // between them and then be wiped out by the DELETE without ever reaching
    // this account's stock — gold/goods destroyed, not paid. Reading and
    // deleting in one statement means whatever we see is exactly what we claim.
    let claimed: Option<Wallet> = conn
        .query_row(
            "DELETE FROM market_wallet WHERE account_id = ?1
             RETURNING gold, wood, coal, food, fur, cloth",
            [account],
            wallet_from_row,
        )
        .optional()
        .ok()
        .flatten();
    let Some(w) = claimed else {
        return;
    };
    if w.is_empty() {
        return;
    }
    state.stock.gold += w.gold;
    state.stock.wood += w.wood;
    state.stock.coal += w.coal;
    state.stock.food += w.food;
    state.stock.fur += w.fur;
    state.stock.cloth += w.cloth;
    fc_game::sim::push_event(
        state,
        format!(
            "The market settled up: {} gold and goods arrived.",
            w.gold as i64
        ),
    );
}

/// Handle one market `ClientMsg` for `account`, against the world they're
/// currently in. Always answers with a `ServerMsg` (the refreshed book, or a
/// system bubble explaining a refusal) so a client never waits on silence.
pub fn handle(account: i64, msg: &ClientMsg, state: &mut GameState) -> ServerMsg {
    let Some(conn) = open() else {
        return crate::server::messages::system_bubble("The market is closed right now.");
    };
    match msg {
        ClientMsg::RefreshMarket => book_msg(&conn, account),
        ClientMsg::PostOrder { good, amount, unit_price, selling } => {
            match post(&conn, account, state, *good, *amount, *unit_price, *selling) {
                Ok(()) => book_msg(&conn, account),
                Err(why) => crate::server::messages::system_bubble(why),
            }
        }
        ClientMsg::TakeOrder { order, amount } => match take(&conn, account, state, *order, *amount)
        {
            Ok(()) => book_msg(&conn, account),
            Err(why) => crate::server::messages::system_bubble(why),
        },
        ClientMsg::CancelOrder { order } => match cancel(&conn, account, state, *order) {
            Ok(()) => book_msg(&conn, account),
            Err(why) => crate::server::messages::system_bubble(why),
        },
        // Not a market message; the caller only routes the four above.
        _ => book_msg(&conn, account),
    }
}

fn trading_world(state: &GameState) -> Result<(), &'static str> {
    if state.central {
        return Err("Trade from your own city, not the Global World.");
    }
    Ok(())
}

fn post(
    conn: &Connection,
    account: i64,
    state: &mut GameState,
    good: TradeGood,
    amount: u32,
    unit_price: f32,
    selling: bool,
) -> Result<(), &'static str> {
    trading_world(state)?;
    if amount == 0 || amount > MAX_ORDER_AMOUNT {
        return Err("That is not a tradeable amount.");
    }
    if !unit_price.is_finite() || !(MIN_UNIT_PRICE..=MAX_UNIT_PRICE).contains(&unit_price) {
        return Err("That price is out of range.");
    }
    let open_orders: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM market_orders WHERE account_id = ?1 AND amount > 0",
            [account],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if open_orders as usize >= MAX_ORDERS_PER_ACCOUNT {
        return Err("You already have too many orders standing.");
    }
    // Check affordability, THEN insert the order, and only THEN escrow the
    // stock — deducting before the row exists would mean a failed INSERT
    // (disk error, etc.) destroys goods/gold that have nowhere to go and no
    // order to eventually refund them; inserting first and deducting only
    // once it succeeded needs no rollback path at all.
    if selling {
        if good.amount_in(&state.stock) < amount as f32 {
            return Err("You do not have that much to sell.");
        }
    } else if state.stock.gold < amount as f32 * unit_price {
        return Err("Not enough gold.");
    }
    conn.execute(
        "INSERT INTO market_orders (account_id, good, amount, unit_price, selling)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            account,
            good_code(good),
            amount as i64,
            unit_price as f64,
            i64::from(selling)
        ],
    )
    .map_err(|_| "The market did not accept that order.")?;
    // Escrow now that the order is really in the book, so every order is
    // backed by goods or gold that have already left the poster's colony.
    if selling {
        good.credit(&mut state.stock, -(amount as f32));
    } else {
        state.stock.gold -= amount as f32 * unit_price;
    }
    fc_game::sim::push_event(
        state,
        format!(
            "{} {} {} on the global market at {:.2} gold.",
            if selling { "Offered" } else { "Bid for" },
            amount,
            good.name(),
            unit_price
        ),
    );
    Ok(())
}

fn take(
    conn: &Connection,
    account: i64,
    state: &mut GameState,
    order_id: i64,
    want: u32,
) -> Result<(), &'static str> {
    trading_world(state)?;
    let row: Option<(i64, i64, i64, f64, i64)> = conn
        .query_row(
            "SELECT account_id, good, amount, unit_price, selling
               FROM market_orders WHERE id = ?1 AND amount > 0",
            [order_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()
        .map_err(|_| "The market did not answer.")?;
    let Some((poster, good_code_i, left, unit_price, selling)) = row else {
        return Err("That order is gone.");
    };
    if poster == account {
        return Err("That is your own order.");
    }
    let Some(good) = good_from_code(good_code_i) else {
        return Err("That order is gone.");
    };
    let qty = want.min(left.max(0) as u32);
    if qty == 0 {
        return Err("That is not a tradeable amount.");
    }
    let unit_price = unit_price as f32;
    let total = qty as f32 * unit_price;

    // Affordability check BEFORE anything is written, so a refusal here
    // leaves both the taker's stock and the order completely untouched.
    if selling != 0 {
        if state.stock.gold < total {
            return Err("Not enough gold.");
        }
    } else if good.amount_in(&state.stock) < qty as f32 {
        return Err("You do not have that much.");
    }

    // Decrement (and retire) the order FIRST, atomically, BEFORE touching
    // either side's stock. Guarded on `amount >= qty` so two simultaneous
    // takers can never both fill the same last unit — the second one's
    // UPDATE matches no row and is refused. This ordering matters: settling
    // stock first and rolling it back on a lost race (the previous shape)
    // still credited the poster's wallet before the UPDATE ran, so a lost
    // race paid the poster while leaving the order's `amount` untouched —
    // the same goods/gold could then be sold again to the next taker,
    // duplicating them out of thin air. Settling only once the UPDATE
    // confirms this taker actually won means there is nothing to undo.
    let changed = conn
        .execute(
            "UPDATE market_orders SET amount = amount - ?2 WHERE id = ?1 AND amount >= ?2",
            rusqlite::params![order_id, qty as i64],
        )
        .unwrap_or(0);
    if changed == 0 {
        return Err("Someone else took that order first.");
    }

    // The order is ours now — settle both sides.
    if selling != 0 {
        // The poster was selling: the taker pays gold and receives goods,
        // which were already sitting in escrow (deducted at post time).
        state.stock.gold -= total;
        good.credit(&mut state.stock, qty as f32);
        credit_wallet_gold(conn, poster, total);
    } else {
        // The poster was buying: the taker hands over goods and is paid out
        // of the gold escrowed at post time.
        good.credit(&mut state.stock, -(qty as f32));
        state.stock.gold += total;
        credit_wallet_good(conn, poster, good, qty as f32);
    }
    let _ = conn.execute("DELETE FROM market_orders WHERE amount <= 0", []);
    fc_game::sim::push_event(
        state,
        format!("Traded {} {} on the global market.", qty, good.name()),
    );
    Ok(())
}

fn cancel(
    conn: &Connection,
    account: i64,
    state: &GameState,
    order_id: i64,
) -> Result<(), &'static str> {
    trading_world(state)?;
    // A single atomic DELETE...RETURNING, not a separate SELECT then DELETE:
    // the two-step version read `amount` and then deleted unconditionally,
    // so a `take()` partially filling this SAME order (a different account's
    // thread — entirely possible between these two statements) would still
    // refund the STALE, larger `amount` on top of what `take()` already paid
    // out — creating goods/gold out of thin air. Deleting and reading the row
    // in one statement means whatever we see is exactly what we removed.
    let row: Option<(i64, i64, f64, i64)> = conn
        .query_row(
            "DELETE FROM market_orders WHERE id = ?1 AND account_id = ?2
             RETURNING good, amount, unit_price, selling",
            rusqlite::params![order_id, account],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .map_err(|_| "The market did not answer.")?;
    let Some((good_code_i, left, unit_price, selling)) = row else {
        return Err("That is not your order.");
    };
    // Refund whatever never filled — into the wallet like any other market
    // payout, not directly into `state.stock`, so this refund path is exactly
    // the same one `take()`/`claim_wallet` already use (one settlement
    // channel, not two).
    let left = left.max(0) as f32;
    if left > 0.0 {
        if selling != 0 {
            if let Some(good) = good_from_code(good_code_i) {
                credit_wallet_good(conn, account, good, left);
            }
        } else {
            credit_wallet_gold(conn, account, left * unit_price as f32);
        }
    }
    Ok(())
}
