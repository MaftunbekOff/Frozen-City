use std::time::{Duration, Instant};

/// Per-connection message-rate limiter: a 1 s sliding window with generous
/// caps so normal play (and the e2e tests) are unaffected, while a
/// misbehaving or malicious client can't flood the sim thread.
pub(crate) struct RateLimiter {
    window_start: Instant,
    cmd: u32,
    chat: u32,
    ping: u32,
    cursor: u32,
    /// Last time `RefreshShowcase` was allowed — a separate, longer-period
    /// throttle (not the 1 s sliding window above): showcase reads a
    /// friend's save file from disk per entry, so it's capped independently
    /// at roughly human "opened the panel" cadence, not command-flood scale.
    last_showcase: Option<Instant>,
}

impl RateLimiter {
    pub(crate) fn new(now: Instant) -> Self {
        RateLimiter {
            window_start: now,
            cmd: 0,
            chat: 0,
            ping: 0,
            cursor: 0,
            last_showcase: None,
        }
    }

    fn reset_if_elapsed(&mut self, now: Instant) {
        if now.duration_since(self.window_start) >= Duration::from_secs(1) {
            self.window_start = now;
            self.cmd = 0;
            self.chat = 0;
            self.ping = 0;
            self.cursor = 0;
        }
    }

    pub(crate) fn allow_cmd(&mut self, now: Instant) -> bool {
        self.reset_if_elapsed(now);
        self.cmd += 1;
        self.cmd <= 30
    }

    pub(crate) fn allow_chat(&mut self, now: Instant) -> bool {
        self.reset_if_elapsed(now);
        self.chat += 1;
        self.chat <= 4
    }

    pub(crate) fn allow_ping(&mut self, now: Instant) -> bool {
        self.reset_if_elapsed(now);
        self.ping += 1;
        self.ping <= 6
    }

    /// Cursor presence is high-frequency but still bounded well above the
    /// client's own ~8/s cadence, so a flood can't monopolise the sim thread.
    pub(crate) fn allow_cursor(&mut self, now: Instant) -> bool {
        self.reset_if_elapsed(now);
        self.cursor += 1;
        self.cursor <= 60
    }

    /// At most one `RefreshShowcase` per [`SHOWCASE_COOLDOWN`], independent
    /// of the 1 s command window (see `last_showcase`'s doc comment).
    pub(crate) fn allow_showcase(&mut self, now: Instant) -> bool {
        if self.last_showcase.is_some_and(|last| now.duration_since(last) < SHOWCASE_COOLDOWN) {
            return false;
        }
        self.last_showcase = Some(now);
        true
    }
}

/// Minimum spacing between `RefreshShowcase` requests from one connection —
/// each entry is a friend's save file read from disk, so this is a
/// human-interaction-scale cap, not a command-flood one.
const SHOWCASE_COOLDOWN: Duration = Duration::from_secs(5);
