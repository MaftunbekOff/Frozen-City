use crate::types::*;

/// Shared push path for both event kinds: appends the event, bumps the
/// monotonic counter, then evicts if the capped log overflowed.
fn push_event_inner(state: &mut GameState, text: String, system: bool) {
    let day = state.day();
    state.events.push(GameEvent { day, text, system });
    state.total_events += 1;
    if state.events.len() > 12 {
        // Prefer evicting the oldest cosmetic (non-system) event first, so a
        // burst of "built a Tent" spam can't push a death or victory line out
        // of the log; only fall back to the oldest overall once every event
        // in the log is a system event.
        let evict = state
            .events
            .iter()
            .position(|e| !e.system)
            .unwrap_or(0);
        state.events.remove(evict);
    }
}

/// Push a world/system event (deaths, weather, arrivals, joins, victory,
/// defeat) — protected from eviction by cosmetic player-action spam.
pub fn push_event(state: &mut GameState, text: impl Into<String>) {
    push_event_inner(state, text.into(), true);
}

/// Push a cosmetic player-action event (build/demolish attribution) — the
/// first to be evicted once the capped log overflows.
pub fn push_action_event(state: &mut GameState, text: impl Into<String>) {
    push_event_inner(state, text.into(), false);
}

/// Characters that must never survive into text stored and broadcast to every
/// client (chat messages, display names): a bare `is_control()` misses
/// U+202E (RIGHT-TO-LEFT OVERRIDE), zero-width joiners and friends, which
/// would let a line reorder or hide text for every viewer on the shared
/// server.
fn is_unsafe_char(c: char) -> bool {
    c.is_control()
        || matches!(c,
            '\u{200B}'..='\u{200F}'   // zero-width space/joiners, LRM/RLM
            | '\u{202A}'..='\u{202E}' // bidi embeddings & overrides
            | '\u{2060}'..='\u{2069}' // word-joiner, bidi isolates
            | '\u{061C}'              // arabic letter mark
            | '\u{FEFF}'              // zero-width no-break space (BOM)
        )
}

/// Combining marks to cap when stacked ("zalgo" text): a handful of accents
/// on one base character is normal typing/IME behavior, but dozens turn a
/// single line into multi-row visual noise for every viewer on the shared
/// server.
fn is_combining_mark(c: char) -> bool {
    matches!(c,
        '\u{0300}'..='\u{036F}'   // combining diacritical marks
        | '\u{0483}'..='\u{0489}' // Cyrillic combining
        | '\u{0591}'..='\u{05C7}' // Hebrew points
        | '\u{0610}'..='\u{061A}' // Arabic
        | '\u{064B}'..='\u{065F}' // Arabic tashkil
        | '\u{06D6}'..='\u{06ED}' // Arabic
        | '\u{0900}'..='\u{0903}' // Devanagari
        | '\u{093A}'..='\u{094F}' // Devanagari matras
        | '\u{0951}'..='\u{0957}' // Devanagari
        | '\u{0E31}'..='\u{0E3A}' // Thai vowels/tones
        | '\u{0E47}'..='\u{0E4E}' // Thai tones
        | '\u{1AB0}'..='\u{1AFF}' // combining diacritical marks extended
        | '\u{1DC0}'..='\u{1DFF}' // combining diacritical marks supplement
        | '\u{20D0}'..='\u{20FF}' // combining diacritical marks for symbols
        | '\u{FE20}'..='\u{FE2F}' // combining half marks
    )
}

/// Shared sanitizer for any untrusted, free-form text a player supplies that
/// ends up stored and broadcast verbatim to every other client (chat lines,
/// display names). Strips control/invisible-format/bidi characters, THEN
/// caps stacked combining marks, and only THEN applies `max_len` — in that
/// order, so a zalgo-heavy prefix can't eat the whole length budget and
/// silently discard legitimate trailing text.
fn sanitize_text(text: &str, max_len: usize) -> String {
    text.trim()
        .chars()
        .filter(|c| !is_unsafe_char(*c))
        .scan(0u32, |run, c| {
            *run = if is_combining_mark(c) { *run + 1 } else { 0 };
            Some((c, *run))
        })
        .filter(|(_, run)| *run <= 2)
        .map(|(c, _)| c)
        .take(max_len)
        .collect()
}

/// Sanitize an incoming player display name with the same rules as chat text
/// (see [`sanitize_text`]), capped to [`MAX_NAME_LEN`]. Falls back to a safe
/// placeholder if nothing legible survives (e.g. a name made entirely of
/// bidi overrides or zero-width characters).
pub(crate) fn sanitize_name(name: &str) -> String {
    let cleaned = sanitize_text(name, MAX_NAME_LEN);
    if cleaned.trim().is_empty() {
        "Player".to_string()
    } else {
        cleaned
    }
}

/// The chat sanitizer, exposed for text that is broadcast but never stored in
/// the world snapshot (nearby-chat bubbles, `ServerMsg::Bubble`): identical
/// rules to persistent chat, so a bubble can't smuggle in what a chat line
/// can't.
pub fn sanitize_public_text(text: &str) -> String {
    sanitize_text(text, MAX_CHAT_LEN)
}

/// Append a chat line from `player_id`, sanitizing and length-capping the text.
/// Silently dropped if the player isn't connected or the text is empty after sanitizing.
pub fn push_chat(state: &mut GameState, player_id: u64, text: &str) {
    let Some(p) = state.player(player_id) else {
        return;
    };
    let name = p.name.clone();
    let color = p.color;

    let sanitized = sanitize_text(text, MAX_CHAT_LEN);
    if sanitized.trim().is_empty() {
        return;
    }

    state.chat.push(ChatLine {
        player_id,
        name,
        color,
        text: sanitized,
    });
    while state.chat.len() > MAX_CHAT {
        state.chat.remove(0);
    }
    state.total_chat += 1;
}

/// Drop a transient map ping from `player_id` at world tile coordinates `(x, y)`.
/// Silently ignored if the player isn't connected, the game is over (a frozen
/// world never advances its tick, so a post-game ping could never expire), or
/// the coordinates aren't finite (a crafted client sending NaN/inf, which
/// would otherwise be stored and broadcast to every viewer's renderer).
/// Finite coordinates are clamped into map bounds.
pub fn add_ping(state: &mut GameState, player_id: u64, x: f32, y: f32) {
    if state.phase != GamePhase::Running {
        return;
    }
    if !x.is_finite() || !y.is_finite() {
        return;
    }
    let Some(p) = state.player(player_id) else {
        return;
    };
    let color = p.color;
    let x = x.clamp(0.0, MAP_W as f32);
    let y = y.clamp(0.0, MAP_H as f32);

    state.pings.push(Ping {
        player_id,
        color,
        x,
        y,
        tick: state.tick,
    });
    // Per-player cap first (evict this player's own oldest), then the global cap.
    while state.pings.iter().filter(|q| q.player_id == player_id).count() > MAX_PINGS_PER_PLAYER {
        if let Some(pos) = state.pings.iter().position(|q| q.player_id == player_id) {
            state.pings.remove(pos);
        } else {
            break;
        }
    }
    while state.pings.len() > MAX_PINGS {
        state.pings.remove(0);
    }
}
