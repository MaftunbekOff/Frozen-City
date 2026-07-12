//! The accounts DB (native server only). Originally written solely by the
//! registration Telegram bot and only read here; the server now also WRITES:
//! in-client registration (`ClientMsg::Register`) inserts accounts with the
//! same schema/bcrypt format the bot uses, and the friends list lives in an
//! extra `friends` table the bot never touches. Authentication hands back
//! the account's stable id and in-game display name; the id routes the
//! connection to that account's own persistent world (see `world_manager.rs`).

use std::path::Path;

use rusqlite::{Connection, OpenFlags};

/// Where the registration bot writes the accounts DB. Group-readable by the
/// game server's service user; the server never writes to it. Overridable
/// via `FC_ACCOUNTS_DB` (same variable name the bot itself honors), mainly so
/// tests can point at a throwaway DB instead of the real one.
pub const DEFAULT_DB_PATH: &str = "/var/lib/frozen-city-accounts/accounts.db";

/// Verifies `login`/`password` and returns the account id to join as plus the
/// display name to join with. Every failure mode — missing DB, unknown
/// login, wrong password — collapses to `None` alike, so a connecting client
/// can't distinguish "no such account" from "wrong password" by response shape.
pub fn authenticate(login: &str, password: &str) -> Option<(i64, String)> {
    let db_path = std::env::var("FC_ACCOUNTS_DB").unwrap_or_else(|_| DEFAULT_DB_PATH.to_string());
    authenticate_at(&db_path, login, password)
}

fn authenticate_at(db_path: &str, login: &str, password: &str) -> Option<(i64, String)> {
    if !Path::new(db_path).exists() {
        return None;
    }
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let (id, display_username, password_hash): (i64, String, String) = conn
        .query_row(
            "SELECT id, display_username, password_hash FROM accounts WHERE login = ?1",
            [login],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .ok()?;
    bcrypt::verify(password, &password_hash)
        .ok()
        .filter(|&ok| ok)
        .map(|_| (id, display_username))
}

/// Why an in-client registration was refused, each mapped to a client-visible
/// `AuthFailed` reason by the caller.
#[derive(Debug, PartialEq, Eq)]
pub enum RegisterError {
    /// Login or display name already exists (deliberately one error for both,
    /// mirroring `authenticate`'s refusal-shape uniformity).
    Taken,
    /// Failed validation; the message says which rule.
    Invalid(&'static str),
    /// DB unavailable/unwritable — an ops problem, not the player's.
    Io,
}

/// Opens the accounts DB read-write, creating the file and any missing
/// tables. The `accounts` schema is byte-for-byte the one the Telegram bot
/// creates, so whichever side runs first, the other keeps working; `friends`
/// is server-only (the bot never reads it).
fn open_rw(db_path: &str) -> Option<Connection> {
    if let Some(dir) = Path::new(db_path).parent() {
        std::fs::create_dir_all(dir).ok()?;
    }
    let conn = Connection::open(db_path).ok()?;
    // Two writers exist (bot + server): give lock contention a beat to clear
    // instead of failing a registration on a momentary SQLITE_BUSY.
    let _ = conn.busy_timeout(std::time::Duration::from_secs(3));
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS accounts (
            id INTEGER PRIMARY KEY,
            telegram_id INTEGER UNIQUE NOT NULL,
            telegram_username TEXT,
            display_username TEXT UNIQUE NOT NULL,
            first_name TEXT NOT NULL,
            last_name TEXT NOT NULL,
            birth_date TEXT NOT NULL,
            login TEXT UNIQUE NOT NULL,
            password_hash TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS friends (
            account_id INTEGER NOT NULL,
            friend_id INTEGER NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (account_id, friend_id)
        );
        CREATE TABLE IF NOT EXISTS visit_policy (
            account_id INTEGER PRIMARY KEY,
            allow_offline INTEGER NOT NULL DEFAULT 0
        );",
    )
    .ok()?;
    Some(conn)
}

/// Whether `account` currently allows `VisitFriend` guests into their
/// personal world while they're not online there themselves (V0.6
/// "owner-offline entry"). Defaults to `false` (no row yet — same
/// "missing collapses to the safe default" shape as the rest of this module).
pub fn visit_policy(account: i64) -> bool {
    let db_path = std::env::var("FC_ACCOUNTS_DB").unwrap_or_else(|_| DEFAULT_DB_PATH.to_string());
    let Some(conn) = open_rw(&db_path) else {
        return false;
    };
    conn.query_row(
        "SELECT allow_offline FROM visit_policy WHERE account_id = ?1",
        [account],
        |row| row.get::<_, i64>(0),
    )
    .map(|v| v != 0)
    .unwrap_or(false)
}

/// Sets `account`'s owner-offline visit policy. Best-effort: a write failure
/// (DB unavailable) is swallowed by the caller the same way every other
/// account write here is — the in-memory `GameState` isn't affected either
/// way, only whether the choice survives a restart.
pub fn set_visit_policy(account: i64, allow_offline: bool) -> Result<(), FriendError> {
    let db_path = std::env::var("FC_ACCOUNTS_DB").unwrap_or_else(|_| DEFAULT_DB_PATH.to_string());
    let conn = open_rw(&db_path).ok_or(FriendError::Io)?;
    conn.execute(
        "INSERT INTO visit_policy (account_id, allow_offline) VALUES (?1, ?2)
         ON CONFLICT(account_id) DO UPDATE SET allow_offline = excluded.allow_offline",
        rusqlite::params![account, allow_offline as i64],
    )
    .map_err(|_| FriendError::Io)?;
    Ok(())
}

/// Creates an account from inside the game client (no Telegram involved) and
/// returns `(account_id, display_name)` ready to sign in with — the same
/// tuple `authenticate` yields. Validation is deliberately strict-and-simple;
/// uniqueness is enforced by the DB's UNIQUE constraints, not a racy
/// check-then-insert.
pub fn register_account(
    login: &str,
    password: &str,
    name: &str,
) -> Result<(i64, String), RegisterError> {
    let db_path = std::env::var("FC_ACCOUNTS_DB").unwrap_or_else(|_| DEFAULT_DB_PATH.to_string());
    register_account_at(&db_path, login, password, name)
}

fn register_account_at(
    db_path: &str,
    login: &str,
    password: &str,
    name: &str,
) -> Result<(i64, String), RegisterError> {
    let login = login.trim();
    let name = name.trim();
    if login.len() < 3 || login.len() > 24 {
        return Err(RegisterError::Invalid("Login 3-24 belgi bo'lishi kerak."));
    }
    if !login.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(RegisterError::Invalid(
            "Login faqat lotin harf, raqam va _ dan iborat bo'lsin.",
        ));
    }
    if password.len() < 6 || password.len() > 64 {
        return Err(RegisterError::Invalid("Parol 6-64 belgi bo'lishi kerak."));
    }
    if name.is_empty() || name.chars().count() > 24 || name.chars().any(|c| c.is_control()) {
        return Err(RegisterError::Invalid("Ism 1-24 ta oddiy belgidan iborat bo'lsin."));
    }

    let hash = bcrypt::hash(password, bcrypt::DEFAULT_COST).map_err(|_| RegisterError::Io)?;
    // The bot's schema demands a UNIQUE NOT NULL telegram_id; client-born
    // accounts have none, so they get a random NEGATIVE one — real Telegram
    // ids are positive, so the two populations can never collide.
    let mut b = [0u8; 8];
    getrandom::getrandom(&mut b).map_err(|_| RegisterError::Io)?;
    let synthetic_tid = -((u64::from_le_bytes(b) >> 1) as i64) - 1;

    let conn = open_rw(db_path).ok_or(RegisterError::Io)?;
    match conn.execute(
        "INSERT INTO accounts
            (telegram_id, telegram_username, display_username, first_name,
             last_name, birth_date, login, password_hash, created_at)
         VALUES (?1, NULL, ?2, ?2, '', '', ?3, ?4, datetime('now'))",
        rusqlite::params![synthetic_tid, name, login, hash],
    ) {
        Ok(_) => {
            let id = conn.last_insert_rowid();
            Ok((id, name.to_string()))
        }
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            Err(RegisterError::Taken)
        }
        Err(_) => Err(RegisterError::Io),
    }
}

/// Why an AddFriend was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum FriendError {
    /// No account with that display name.
    NotFound,
    /// Adding yourself.
    SelfAdd,
    Io,
}

/// Adds the account whose display name is `friend_name` to `account`'s
/// friends list (one-directional, idempotent). Returns the friend's id+name.
pub fn friends_add(account: i64, friend_name: &str) -> Result<(i64, String), FriendError> {
    let db_path = std::env::var("FC_ACCOUNTS_DB").unwrap_or_else(|_| DEFAULT_DB_PATH.to_string());
    let conn = open_rw(&db_path).ok_or(FriendError::Io)?;
    let (fid, fname): (i64, String) = conn
        .query_row(
            "SELECT id, display_username FROM accounts WHERE display_username = ?1",
            [friend_name.trim()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| FriendError::NotFound)?;
    if fid == account {
        return Err(FriendError::SelfAdd);
    }
    conn.execute(
        "INSERT OR IGNORE INTO friends (account_id, friend_id) VALUES (?1, ?2)",
        rusqlite::params![account, fid],
    )
    .map_err(|_| FriendError::Io)?;
    Ok((fid, fname))
}

/// Removes `friend` from `account`'s list. Idempotent.
pub fn friends_remove(account: i64, friend: i64) -> Result<(), FriendError> {
    let db_path = std::env::var("FC_ACCOUNTS_DB").unwrap_or_else(|_| DEFAULT_DB_PATH.to_string());
    let conn = open_rw(&db_path).ok_or(FriendError::Io)?;
    conn.execute(
        "DELETE FROM friends WHERE account_id = ?1 AND friend_id = ?2",
        rusqlite::params![account, friend],
    )
    .map_err(|_| FriendError::Io)?;
    Ok(())
}

/// `account`'s friends as `(account_id, display_name)`, name-ordered.
/// Missing DB or table collapses to an empty list (a fresh server).
pub fn friends_list(account: i64) -> Vec<(i64, String)> {
    let db_path = std::env::var("FC_ACCOUNTS_DB").unwrap_or_else(|_| DEFAULT_DB_PATH.to_string());
    let Some(conn) = open_rw(&db_path) else {
        return Vec::new();
    };
    let Ok(mut stmt) = conn.prepare(
        "SELECT f.friend_id, a.display_username
         FROM friends f JOIN accounts a ON a.id = f.friend_id
         WHERE f.account_id = ?1 ORDER BY a.display_username",
    ) else {
        return Vec::new();
    };
    stmt.query_map([account], |row| Ok((row.get(0)?, row.get(1)?)))
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a throwaway DB with the same schema the bot creates, seeded
    /// with one account (login `fc000001`, password `correct-horse`).
    fn seed_db(path: &Path) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE accounts (
                id INTEGER PRIMARY KEY,
                telegram_id INTEGER UNIQUE NOT NULL,
                telegram_username TEXT,
                display_username TEXT UNIQUE NOT NULL,
                first_name TEXT NOT NULL,
                last_name TEXT NOT NULL,
                birth_date TEXT NOT NULL,
                login TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL,
                created_at TEXT NOT NULL
            );",
        )
        .unwrap();
        let hash = bcrypt::hash("correct-horse", bcrypt::DEFAULT_COST).unwrap();
        conn.execute(
            "INSERT INTO accounts
                (telegram_id, telegram_username, display_username, first_name,
                 last_name, birth_date, login, password_hash, created_at)
             VALUES (1, 'tguser', 'Aziz', 'Aziz', 'Karimov', '2000-01-01',
                     'fc000001', ?1, '2026-01-01T00:00:00')",
            [&hash],
        )
        .unwrap();
    }

    /// Each test gets its own throwaway DB path (name embeds the test so
    /// parallel `cargo test` runs never collide) and cleans up after itself.
    fn with_seeded_db<T>(name: &str, f: impl FnOnce(&str) -> T) -> T {
        let dir = std::env::temp_dir().join(format!("fc-accounts-test-{name}"));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("accounts.db");
        seed_db(&db);
        let result = f(db.to_str().unwrap());
        std::fs::remove_dir_all(&dir).ok();
        result
    }

    #[test]
    fn correct_login_succeeds() {
        with_seeded_db("ok", |db| {
            assert_eq!(
                authenticate_at(db, "fc000001", "correct-horse"),
                Some((1, "Aziz".to_string()))
            );
        });
    }

    #[test]
    fn wrong_password_is_rejected() {
        with_seeded_db("wrong-pw", |db| {
            assert_eq!(authenticate_at(db, "fc000001", "nope"), None);
        });
    }

    #[test]
    fn unknown_login_is_rejected() {
        with_seeded_db("unknown", |db| {
            assert_eq!(authenticate_at(db, "nope", "whatever"), None);
        });
    }

    /// The registration bot (Python, `bot/register_bot.py`) writes hashes
    /// with the `bcrypt` PyPI package, not this crate — pins that the two
    /// implementations actually agree on the same hash format. This literal
    /// hash was produced by that Python package for the password below.
    #[test]
    fn python_bots_bcrypt_hash_verifies() {
        with_seeded_db("python-hash", |db| {
            let conn = Connection::open(db).unwrap();
            conn.execute(
                "UPDATE accounts SET login = 'fc700928', password_hash = ?1 WHERE login = 'fc000001'",
                [
                    "$2b$12$9s51Kb2k8c84o9l4r./o..LI1M1.trQUv5Sr/xB1./WayLZixEJC2",
                ],
            )
            .unwrap();
            assert_eq!(
                authenticate_at(db, "fc700928", "Jgorguis9a"),
                Some((1, "Aziz".to_string()))
            );
        });
    }

    #[test]
    fn missing_db_is_rejected() {
        assert_eq!(
            authenticate_at("/nonexistent/path/accounts.db", "x", "y"),
            None
        );
    }

    /// In-client registration creates an account `authenticate` accepts, on a
    /// DB that didn't exist beforehand (fresh server, bot never ran).
    #[test]
    fn register_then_authenticate_roundtrip() {
        let dir = std::env::temp_dir().join("fc-accounts-test-register");
        std::fs::remove_dir_all(&dir).ok();
        let db = dir.join("accounts.db");
        let db_str = db.to_str().unwrap();

        let (id, name) =
            register_account_at(db_str, "yangi_01", "parol123", "Yangi").expect("registers");
        assert_eq!(name, "Yangi");
        assert_eq!(
            authenticate_at(db_str, "yangi_01", "parol123"),
            Some((id, "Yangi".to_string()))
        );
        // Same login again: refused as Taken, not a second row.
        assert_eq!(
            register_account_at(db_str, "yangi_01", "boshqa123", "Boshqa"),
            Err(RegisterError::Taken)
        );
        // Same display name under a new login: also Taken (bot rule kept).
        assert_eq!(
            register_account_at(db_str, "yangi_02", "parol123", "Yangi"),
            Err(RegisterError::Taken)
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn register_validation_rejects_bad_input() {
        let dir = std::env::temp_dir().join("fc-accounts-test-register-bad");
        std::fs::remove_dir_all(&dir).ok();
        let db = dir.join("accounts.db");
        let db_str = db.to_str().unwrap();

        assert!(matches!(
            register_account_at(db_str, "ab", "parol123", "X"),
            Err(RegisterError::Invalid(_))
        ));
        assert!(matches!(
            register_account_at(db_str, "bo'sh joy", "parol123", "X"),
            Err(RegisterError::Invalid(_))
        ));
        assert!(matches!(
            register_account_at(db_str, "yaxshi_login", "123", "X"),
            Err(RegisterError::Invalid(_))
        ));
        assert!(matches!(
            register_account_at(db_str, "yaxshi_login", "parol123", "   "),
            Err(RegisterError::Invalid(_))
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The friends table: add by display name, list, remove — plus the
    /// not-found and self-add refusals. Uses env override since the friends
    /// API reads `FC_ACCOUNTS_DB` (no `_at` variants needed elsewhere).
    #[test]
    fn friends_add_list_remove() {
        let dir = std::env::temp_dir().join("fc-accounts-test-friends");
        std::fs::remove_dir_all(&dir).ok();
        let db = dir.join("accounts.db");
        let db_str = db.to_str().unwrap().to_string();

        let (aziz, _) = register_account_at(&db_str, "fr_aziz", "parol123", "Aziz").unwrap();
        let (vali, _) = register_account_at(&db_str, "fr_vali", "parol123", "Vali").unwrap();

        // SAFETY: the sole test in this binary touching FC_ACCOUNTS_DB; the
        // other tests here use explicit `_at` paths.
        unsafe {
            std::env::set_var("FC_ACCOUNTS_DB", &db_str);
        }
        assert_eq!(friends_add(aziz, "Vali"), Ok((vali, "Vali".to_string())));
        // Idempotent re-add.
        assert_eq!(friends_add(aziz, "Vali"), Ok((vali, "Vali".to_string())));
        assert_eq!(friends_add(aziz, "Nomalum"), Err(FriendError::NotFound));
        assert_eq!(friends_add(aziz, "Aziz"), Err(FriendError::SelfAdd));

        assert_eq!(friends_list(aziz), vec![(vali, "Vali".to_string())]);
        assert!(friends_list(vali).is_empty(), "one-directional");

        friends_remove(aziz, vali).unwrap();
        assert!(friends_list(aziz).is_empty());

        // Visit policy (V0.6 "owner-offline entry"): default-deny for an
        // account with no row yet, flips on SetVisitPolicy, and is per
        // account (setting Aziz's never touches Vali's). Piggybacks on this
        // test rather than its own (see the doc comment above: the sole test
        // in this binary touching the env-var-selected `FC_ACCOUNTS_DB`).
        assert!(!visit_policy(aziz), "default is deny (no row yet)");
        assert!(!visit_policy(vali), "default is deny (no row yet)");
        set_visit_policy(aziz, true).unwrap();
        assert!(visit_policy(aziz), "flips on after SetVisitPolicy");
        assert!(!visit_policy(vali), "unaffected by another account's setting");
        // Idempotent update (ON CONFLICT path), and flips back off too.
        set_visit_policy(aziz, true).unwrap();
        assert!(visit_policy(aziz));
        set_visit_policy(aziz, false).unwrap();
        assert!(!visit_policy(aziz));

        std::env::remove_var("FC_ACCOUNTS_DB");
        std::fs::remove_dir_all(&dir).ok();
    }
}
