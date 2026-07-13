use std::sync::Mutex;
use std::time::{Duration, Instant};

/// True on processes (extra region servers) where account login and the
/// central world are switched off — see `ACCOUNTS_DISABLED_REASON`.
pub(crate) fn accounts_disabled() -> bool {
    std::env::var("FC_DISABLE_ACCOUNTS").is_ok_and(|v| !v.is_empty() && v != "0")
}

/// In-process registration throttle: at most this many new accounts per
/// sliding minute, across all connections. bcrypt hashing is also CPU-heavy,
/// so this doubles as a hash-flood guard.
const MAX_REGISTRATIONS_PER_MINUTE: u32 = 5;

pub(crate) fn register_throttled() -> bool {
    static WINDOW: std::sync::OnceLock<Mutex<(Instant, u32)>> = std::sync::OnceLock::new();
    let window = WINDOW.get_or_init(|| Mutex::new((Instant::now(), 0)));
    let mut w = window.lock().unwrap();
    let now = Instant::now();
    if now.duration_since(w.0) >= Duration::from_secs(60) {
        *w = (now, 0);
    }
    w.1 += 1;
    w.1 > MAX_REGISTRATIONS_PER_MINUTE
}

/// Mint a fresh, unguessable 64-bit token from the OS CSPRNG. Used for
/// session/reconnect tokens: unlike a seeded PRNG stream (which can be
/// inverted from a single observed output), each draw here is independent
/// and can't be predicted from a token an attacker sniffs off the wire.
pub(crate) fn fresh_token() -> u64 {
    let mut b = [0u8; 8];
    getrandom::getrandom(&mut b).expect("OS CSPRNG unavailable");
    u64::from_le_bytes(b)
}
