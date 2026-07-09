//! Reads the accounts DB the registration Telegram bot maintains (native
//! server only). Authenticates a login/password against the bcrypt hash the
//! bot stored and, on success, hands back the account's in-game display name
//! — an authenticated player is otherwise treated exactly like a guest whose
//! name came from the DB instead of the wire (see `server.rs`).

use std::path::Path;

use rusqlite::{Connection, OpenFlags};

/// Where the registration bot writes the accounts DB. Group-readable by the
/// game server's service user; the server never writes to it. Overridable
/// via `FC_ACCOUNTS_DB` (same variable name the bot itself honors), mainly so
/// tests can point at a throwaway DB instead of the real one.
pub const DEFAULT_DB_PATH: &str = "/var/lib/frozen-city-accounts/accounts.db";

/// Verifies `login`/`password` and returns the display name to join as.
/// Every failure mode — missing DB, unknown login, wrong password — collapses
/// to `None` alike, so a connecting client can't distinguish "no such
/// account" from "wrong password" by response shape.
pub fn authenticate(login: &str, password: &str) -> Option<String> {
    let db_path = std::env::var("FC_ACCOUNTS_DB").unwrap_or_else(|_| DEFAULT_DB_PATH.to_string());
    authenticate_at(&db_path, login, password)
}

fn authenticate_at(db_path: &str, login: &str, password: &str) -> Option<String> {
    if !Path::new(db_path).exists() {
        return None;
    }
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let (display_username, password_hash): (String, String) = conn
        .query_row(
            "SELECT display_username, password_hash FROM accounts WHERE login = ?1",
            [login],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok()?;
    bcrypt::verify(password, &password_hash)
        .ok()
        .filter(|&ok| ok)
        .map(|_| display_username)
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
                Some("Aziz".to_string())
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
                Some("Aziz".to_string())
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
}
