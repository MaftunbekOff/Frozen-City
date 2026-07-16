use super::*;

use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use fc_game::sim;
use fc_game::types::{
    GamePhase, GameState, Mission, PlayerCommand, PlayerInfo, Ping, Role, Tech, TICK_MS,
};

use crate::accounts;
use crate::persist;
use crate::protocol::{ClientMsg, Included, ServerMsg, TILES_EVERY_N_TICKS};
use crate::telemetry::{self, Event, WORLD_CENTRAL, WORLD_PERSONAL, WORLD_SHARED};

/// On a persistent server, a finished world (won or lost) restarts with a
/// fresh map after this long, keeping the connected players.
const WORLD_RESET_AFTER: Duration = Duration::from_secs(45);

/// How often a persistent server writes its world to disk. Bounds how much
/// progress a hard kill (crash, OOM, `RuntimeMaxSec`) can lose; a graceful
/// stop (SIGTERM) saves immediately instead of waiting for this.
const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(20);

/// Disconnected-player sessions (kept for reconnect) are capped at this many
/// entries, oldest evicted first, to bound memory on a long-running public
/// server.
const MAX_SESSIONS: usize = 128;

/// Upper bound on client/control messages processed per sim-loop pass before
/// yielding to the tick check, so no single flooding connection can starve
/// simulation for the whole shared world by keeping the drain loop busy.
const MAX_DRAIN_PER_PASS: u32 = 4096;

/// How far (Chebyshev, in tiles) a nearby-chat bubble carries. Roughly the
/// on-screen neighborhood at the default zoom — close enough to feel local,
/// wide enough that two players talking don't have to stand on one tile.
const LOCAL_CHAT_RADIUS: f32 = 12.0;

/// Common client-departure bookkeeping, shared by an explicit `Leave` and the
/// broadcast dead-client cleanup. Drops the connection, stashes the
/// departing player's stats (name/color/built/demolished) under their
/// session token so a later `Hello` carrying that token can reconnect as the
/// same player, then removes them from the live roster. No-op if the
/// connection id is unknown (already cleaned up).
#[allow(clippy::too_many_arguments)]
fn disconnect(
    client_id: u64,
    clients: &mut HashMap<u64, Sender<ServerMsg>>,
    player_of: &mut HashMap<u64, u64>,
    token_of: &mut HashMap<u64, u64>,
    sessions: &mut HashMap<u64, PlayerInfo>,
    session_order: &mut VecDeque<u64>,
    limiters: &mut HashMap<u64, RateLimiter>,
    state: &mut GameState,
    // Telemetry context for the session_end record (world kind/key), plus the
    // join-time map so the sitting's duration can be computed.
    world: &'static str,
    world_key: i64,
    joined_at: &mut HashMap<u64, Instant>,
) {
    clients.remove(&client_id);
    limiters.remove(&client_id);
    let joined = joined_at.remove(&client_id);
    let Some(player_id) = player_of.remove(&client_id) else {
        return;
    };
    let token = token_of.remove(&client_id);
    let saved_info = state.player(player_id).cloned();
    // Snapshot the sitting's length and how far this world got, BEFORE the
    // roster entry is torn down below. No-op unless `FC_TELEMETRY_PATH` is set.
    if let Some(info) = &saved_info {
        let duration_s = joined.map(|t| t.elapsed().as_secs()).unwrap_or(0);
        telemetry::record(Event::session_end(
            world,
            world_key,
            player_id,
            info.account,
            info.name.clone(),
            duration_s,
            state,
        ));
    }
    if let (Some(token), Some(info)) = (token, saved_info) {
        if !sessions.contains_key(&token) {
            session_order.push_back(token);
        }
        sessions.insert(token, info);
        while sessions.len() > MAX_SESSIONS {
            let Some(oldest) = session_order.pop_front() else {
                break;
            };
            sessions.remove(&oldest);
        }
    }
    sim::player_left(state, player_id);
}

/// `pub(crate)` so `world_manager` can spawn one `sim_loop` per account world,
/// reusing the exact same tick/persistence/session logic as the single
/// shared world instead of duplicating it.
pub(crate) fn sim_loop(config: ServerConfig, rx: Receiver<ToServer>, shutdown: Arc<AtomicBool>) {
    // A persistent (dedicated) server resumes its last save instead of
    // starting fresh, so the periodic systemd restart and a deploy's
    // stop/start don't wipe every player's city. Anything else (singleplayer,
    // host/join) is a throwaway in-memory world, same as before.
    let fresh = || {
        if config.central {
            sim::new_game_central(config.seed)
        } else {
            sim::new_game(config.seed, config.win_days)
        }
    };
    let mut state = if config.persistent {
        match &config.save_path {
            Some(path) => persist::load_at(path),
            None => persist::load(),
        }
        .unwrap_or_else(fresh)
    } else {
        fresh()
    };
    // Re-assert on every boot: a central world stays central even if its save
    // predates the flag or was ever written without it.
    if config.central {
        state.central = true;
    }
    // `state.players` is the CONNECTED roster, not persistent identity — but
    // it's still part of the saved `GameState`, so a restart while someone
    // was connected (a deploy, a systemd restart, this exact process ever
    // stopping) used to leave their entry behind forever: `clients`/
    // `player_of`/`sessions` all start fresh and empty below, so that old
    // entry could never be claimed by a real reconnect (which always
    // allocates a NEW roster entry, via `player_joined_as` or a
    // token-matched `player_rejoined`) or cleaned up by a real disconnect
    // (nothing is "connected" to it anymore to ever disconnect). Left
    // alone, this accumulates a ghost "<name> (owner)" duplicate in the
    // roster on every restart a player happened to be online for — a real
    // production bug found 2026-07-15. `owner_id` is untouched: it's a
    // separate field and ownership must survive with nobody connected.
    state.players.clear();
    // Telemetry dimension for this world, fixed for its lifetime: the central
    // hub, an account's personal world (keyed by that account), or the shared
    // guest world. `-1` matches `world_manager::CENTRAL_KEY`.
    let (tel_world, tel_key): (&'static str, i64) = if config.central {
        (WORLD_CENTRAL, -1)
    } else if let Some(acc) = config.owner_account {
        (WORLD_PERSONAL, acc)
    } else {
        (WORLD_SHARED, 0)
    };
    let mut clients: HashMap<u64, Sender<ServerMsg>> = HashMap::new();
    // Connection id (key of `clients`, the `client` field on `ToServer`) is
    // decoupled from player id: a connection is a socket, a player is a
    // persistent identity that can outlive one and survive a reconnect.
    let mut next_client_id: u64 = 1;
    let mut next_player_id: u64 = 1;
    // Session tokens must be unguessable: a sequential counter would let anyone
    // hijack a disconnected player by trying Hello{token: 1, 2, 3, ...}, and a
    // seeded PRNG stream (even one seeded from wall-clock entropy) is invertible
    // from a single observed output, letting an attacker who sniffs one token
    // predict every subsequent one. Each token is instead drawn independently
    // from the OS CSPRNG via `fresh_token()`.
    let mut player_of: HashMap<u64, u64> = HashMap::new(); // connection id -> player id
    let mut token_of: HashMap<u64, u64> = HashMap::new(); // connection id -> session token
    let mut sessions: HashMap<u64, PlayerInfo> = HashMap::new(); // session token -> saved info of a disconnected player
    let mut session_order: VecDeque<u64> = VecDeque::new(); // insertion order of `sessions`, for eviction
    let mut limiters: HashMap<u64, RateLimiter> = HashMap::new(); // connection id -> rate limiter
    let mut joined_at: HashMap<u64, Instant> = HashMap::new(); // connection id -> join time, for session length telemetry
    let mut pending: Vec<(u64, PlayerCommand)> = Vec::new();
    let mut ever_joined = false;
    // When `config.idle_shutdown` is set, tracks how long `clients` has been
    // continuously empty — reset to `None` the moment anyone joins.
    let mut idle_since: Option<Instant> = None;
    let mut printed_events: u64 = 0;
    let mut printed_chat: u64 = 0;
    let mut game_over_since: Option<Instant> = None;
    // The `ResetCountdown { seconds_left }` value last broadcast (None while
    // the world runs), so the countdown goes out once per second-value
    // change rather than every tick.
    let mut last_countdown_sent: Option<u32> = None;
    // What the last broadcast tick's `State` message carried, so the next
    // one can skip these quiet-most-ticks collections when nothing changed
    // (see `protocol::Included`). `events`/`chat` reuse the monotonic
    // counters already kept for the verbose console log below instead of
    // cloning the (capped but text-heavy) logs just to compare them.
    let mut last_sent_events: u64 = 0;
    let mut last_sent_chat: u64 = 0;
    let mut last_sent_pings: Vec<Ping> = Vec::new();
    let mut last_sent_missions: Vec<Mission> = Vec::new();
    let mut last_sent_techs: Vec<Tech> = Vec::new();

    let tick_dur = Duration::from_millis(TICK_MS);
    let mut next_tick = Instant::now() + tick_dur;
    let mut last_save = Instant::now();

    if config.verbose && config.port.is_some() {
        println!(
            "[server] Frozen City dedicated server up (seed {}, survive {} days)",
            config.seed, config.win_days
        );
    }

    'outer: loop {
        // Drain control/client messages, but never more than a bounded batch
        // per pass so a flood can't postpone the tick indefinitely.
        let mut drained = 0u32;
        loop {
            if drained >= MAX_DRAIN_PER_PASS {
                break;
            }
            let recv = rx.try_recv();
            if recv.is_ok() {
                drained += 1;
            }
            match recv {
                Ok(ToServer::CountOwned { account, reply }) => {
                    let _ = reply.send(state.owned_settlers(account));
                }
                Ok(ToServer::ExtractMigrants { max, reply }) => {
                    let migrants = if state.graduated {
                        Some(sim::extract_migrants(&mut state, max))
                    } else {
                        None
                    };
                    let _ = reply.send(migrants);
                }
                Ok(ToServer::InjectMigrants { account, name, survivors }) => {
                    sim::inject_migrants(&mut state, account, &name, survivors);
                }
                Ok(ToServer::OwnerOnline { owner, reply }) => {
                    let online = state.players.iter().any(|p| p.account == Some(owner));
                    let _ = reply.send(online);
                }
                Ok(ToServer::DeliverServerMsg { account, msg }) => {
                    let targets: Vec<u64> = state
                        .players
                        .iter()
                        .filter(|p| p.account == Some(account))
                        .map(|p| p.id)
                        .collect();
                    if !targets.is_empty() {
                        for (cid, out) in &clients {
                            if player_of.get(cid).is_some_and(|p| targets.contains(p)) {
                                let _ = out.send((*msg).clone());
                            }
                        }
                    }
                }
                Ok(ToServer::Join { name, token, account, out, id_back }) => {
                    let client_id = next_client_id;
                    next_client_id += 1;

                    // A token that still points at a stashed, disconnected
                    // player's info is a reconnect: resume that identity
                    // (and its built/demolished stats) instead of joining
                    // fresh.
                    let reconnect = token.and_then(|t| sessions.remove(&t).map(|info| (t, info)));
                    let (player_id, session_token, reconnected) =
                        if let Some((t, mut saved)) = reconnect {
                            let player_id = saved.id;
                            session_order.retain(|&tok| tok != t);
                            // A stashed session may predate the account field
                            // (or the player first joined as a guest); the
                            // authenticated connection is the fresher truth.
                            if account.is_some() {
                                saved.account = account;
                            }
                            sim::player_rejoined(&mut state, saved);
                            // Rotate the token on every reconnect: the old one
                            // was sent once in plaintext, so a sniffed token is
                            // dead the moment the real client reconnects.
                            (player_id, fresh_token(), true)
                        } else {
                            let player_id = next_player_id;
                            next_player_id += 1;
                            let session_token = fresh_token();
                            sim::player_joined_as(&mut state, player_id, &name, account);
                            (player_id, session_token, false)
                        };

                    // In an account-owned personal world, authority follows
                    // the OWNING ACCOUNT, not join order: an invited visitor
                    // who happens to connect first must never seize the
                    // Owner role (or keep a stale owner_id claim).
                    if let Some(owner_acc) = config.owner_account {
                        let is_owner = account == Some(owner_acc);
                        if let Some(p) = state.players.iter_mut().find(|p| p.id == player_id) {
                            p.role = if is_owner { Role::Owner } else { Role::Guest };
                        }
                        if is_owner {
                            state.owner_id = Some(player_id);
                        } else if state.owner_id == Some(player_id) {
                            state.owner_id = None;
                        }
                    }

                    player_of.insert(client_id, player_id);
                    token_of.insert(client_id, session_token);
                    limiters.insert(client_id, RateLimiter::new(Instant::now()));
                    joined_at.insert(client_id, Instant::now());
                    // Record the sitting's start (no-op unless FC_TELEMETRY_PATH
                    // is set). A reconnect is flagged so it isn't double-counted
                    // as a new sitting; the matching session_end fired when the
                    // previous connection dropped.
                    telemetry::record(Event::session_start(
                        tel_world,
                        tel_key,
                        player_id,
                        account,
                        name.clone(),
                        reconnected,
                    ));

                    let _ = out.send(ServerMsg::Welcome {
                        player_id,
                        token: session_token,
                        state: state.clone(),
                    });
                    // Account sessions get their friends list right away, so
                    // the social panel is populated without an extra request.
                    // Same for their current visit policy (V0.6
                    // "owner-offline entry"), so the settings toggle can show
                    // the right state without a round-trip.
                    if let Some(acc) = account {
                        let _ = out.send(social_for(&state, acc));
                        let _ = out.send(ServerMsg::VisitPolicy {
                            allow_offline: accounts::visit_policy(acc),
                        });
                    }
                    clients.insert(client_id, out);
                    let _ = id_back.send(client_id);
                    ever_joined = true;
                    idle_since = None;
                    if config.verbose {
                        if reconnected {
                            println!(
                                "[server] {} reconnected (#{} as player {})",
                                name, client_id, player_id
                            );
                        } else {
                            println!(
                                "[server] {} connected (#{} as player {})",
                                name, client_id, player_id
                            );
                        }
                    }
                }
                Ok(ToServer::Msg { client, msg }) => {
                    // A message for a connection we've already torn down (an
                    // in-flight frame that raced the disconnect) must be dropped,
                    // never attributed to a fabricated player id.
                    let Some(&pid) = player_of.get(&client) else {
                        continue;
                    };
                    let now = Instant::now();
                    // A missing limiter denies (fail closed); every live
                    // connection has one, inserted on Join.
                    let allowed = match &msg {
                        ClientMsg::Cmd(_) => limiters
                            .get_mut(&client)
                            .is_some_and(|l| l.allow_cmd(now)),
                        ClientMsg::Chat { .. } => limiters
                            .get_mut(&client)
                            .is_some_and(|l| l.allow_chat(now)),
                        ClientMsg::Ping { .. } => limiters
                            .get_mut(&client)
                            .is_some_and(|l| l.allow_ping(now)),
                        ClientMsg::Cursor { .. } => limiters
                            .get_mut(&client)
                            .is_some_and(|l| l.allow_cursor(now)),
                        // First-frame-only messages (consumed before `join()`,
                        // never reach here); a later one is a protocol
                        // violation, not a command.
                        ClientMsg::Hello { .. }
                        | ClientMsg::Login { .. }
                        | ClientMsg::EnterCentral { .. }
                        | ClientMsg::Register { .. }
                        | ClientMsg::VisitFriend { .. } => false,
                        // Social traffic shares the chat budget: all of it is
                        // human-scale (a click or a said line), and Add/Remove
                        // hit the accounts DB, which a flood must not.
                        ClientMsg::ChatLocal { .. }
                        | ClientMsg::AddFriend { .. }
                        | ClientMsg::RemoveFriend { .. }
                        | ClientMsg::RefreshSocial
                        | ClientMsg::Invite { .. } => limiters
                            .get_mut(&client)
                            .is_some_and(|l| l.allow_chat(now)),
                        // Low-frequency owner-only admin action: never rate
                        // limited (structurally rare — one click — and gating
                        // it behind the cmd/chat/ping caps would let a flood
                        // on those channels starve a kick).
                        ClientMsg::Kick { .. } => true,
                        // Showcase reads a save file per friend from disk —
                        // its own dedicated cooldown (see `allow_showcase`),
                        // not the 1 s chat/cmd budget.
                        ClientMsg::RefreshShowcase => limiters
                            .get_mut(&client)
                            .is_some_and(|l| l.allow_showcase(now)),
                        // Rare owner-only setting change; same reasoning as
                        // Kick above.
                        ClientMsg::SetVisitPolicy { .. } => true,
                    };
                    if allowed {
                        match msg {
                            ClientMsg::Cmd(cmd) => pending.push((pid, cmd)),
                            ClientMsg::Cursor { x, y } => sim::set_cursor(&mut state, pid, x, y),
                            ClientMsg::Chat { text } => sim::push_chat(&mut state, pid, &text),
                            ClientMsg::Ping { x, y } => sim::add_ping(&mut state, pid, x, y),
                            ClientMsg::Hello { .. }
                            | ClientMsg::Login { .. }
                            | ClientMsg::EnterCentral { .. }
                            | ClientMsg::Register { .. }
                            | ClientMsg::VisitFriend { .. } => {}
                            ClientMsg::ChatLocal { text } => {
                                let text = sim::sanitize_public_text(&text);
                                if !text.trim().is_empty() {
                                    if let Some(speaker) = state.player(pid).cloned() {
                                        let bubble = ServerMsg::Bubble {
                                            player_id: pid,
                                            name: speaker.name.clone(),
                                            color: speaker.color,
                                            text,
                                        };
                                        for (cid, out) in &clients {
                                            let Some(&other) = player_of.get(cid) else {
                                                continue;
                                            };
                                            // Deliver within earshot; when a
                                            // position is unknown (a player
                                            // who hasn't moved yet) err on
                                            // the side of delivering.
                                            let near = other == pid
                                                || match (
                                                    speaker.cursor,
                                                    state.player(other).and_then(|p| p.cursor),
                                                ) {
                                                    (Some((sx, sy)), Some((ox, oy))) => {
                                                        (sx - ox).abs().max((sy - oy).abs())
                                                            <= LOCAL_CHAT_RADIUS
                                                    }
                                                    _ => true,
                                                };
                                            if near {
                                                let _ = out.send(bubble.clone());
                                            }
                                        }
                                    }
                                }
                            }
                            ClientMsg::AddFriend { name } => {
                                if let Some(acc) = state.player_account(pid) {
                                    let feedback = match accounts::friends_add(acc, &name) {
                                        Ok((_, fname)) => {
                                            format!("{fname} do'stlar ro'yxatiga qo'shildi.")
                                        }
                                        Err(accounts::FriendError::NotFound) => {
                                            "Bunday ismli o'yinchi topilmadi.".to_string()
                                        }
                                        Err(accounts::FriendError::SelfAdd) => {
                                            "O'zingizni qo'sha olmaysiz.".to_string()
                                        }
                                        Err(accounts::FriendError::Io) => {
                                            "Server do'stlar ro'yxatini yangilay olmadi.".to_string()
                                        }
                                    };
                                    if let Some(out) = clients.get(&client) {
                                        let _ = out.send(system_bubble(&feedback));
                                        let _ = out.send(social_for(&state, acc));
                                    }
                                }
                            }
                            ClientMsg::RemoveFriend { account: friend } => {
                                if let Some(acc) = state.player_account(pid) {
                                    let _ = accounts::friends_remove(acc, friend);
                                    if let Some(out) = clients.get(&client) {
                                        let _ = out.send(social_for(&state, acc));
                                    }
                                }
                            }
                            ClientMsg::RefreshSocial => {
                                if let Some(acc) = state.player_account(pid) {
                                    if let Some(out) = clients.get(&client) {
                                        let _ = out.send(social_for(&state, acc));
                                    }
                                }
                            }
                            ClientMsg::Invite { account: target } => {
                                // Only in the central world (that's where
                                // people meet), only by account sessions,
                                // never to yourself.
                                let host_acc = state.player_account(pid);
                                if let (true, Some(host_acc), Some(book)) =
                                    (state.central, host_acc, config.invites.as_ref())
                                {
                                    if host_acc != target {
                                        let targets: Vec<u64> = state
                                            .players
                                            .iter()
                                            .filter(|p| p.account == Some(target))
                                            .map(|p| p.id)
                                            .collect();
                                        let host_name = state
                                            .player(pid)
                                            .map(|p| p.name.clone())
                                            .unwrap_or_default();
                                        let invited_msg = ServerMsg::Invited {
                                            host: host_acc,
                                            host_name: host_name.clone(),
                                        };
                                        // Deliver to the target if they're
                                        // right here in the central world...
                                        let mut delivered_centrally = false;
                                        for (cid, out) in &clients {
                                            if player_of.get(cid).is_some_and(|p| targets.contains(p)) {
                                                let _ = out.send(invited_msg.clone());
                                                delivered_centrally = true;
                                            }
                                        }
                                        // ...or, FAILING that, to their own
                                        // PERSONAL world (V0.6 "guest
                                        // without onboarding" — a friend
                                        // doesn't have to be standing in the
                                        // hub at this exact moment to be
                                        // invited into it). Never both: an
                                        // account connected here AND at home
                                        // (two devices) would get the same
                                        // invite popup twice.
                                        if !delivered_centrally {
                                            if let Some(wm) = config.world_manager.as_ref() {
                                                wm.deliver_to_account(target, invited_msg);
                                            }
                                        }
                                        book.invite(host_acc, target);
                                        if let Some(out) = clients.get(&client) {
                                            let feedback = if targets.is_empty() {
                                                "Taklif yuborildi — do'stingiz hozir Global Olamda emas, lekin o'z olamiga kirganda ko'radi."
                                            } else {
                                                "Taklif yuborildi — o'z olamingizga qaytsangiz, do'stingiz kira oladi."
                                            };
                                            let _ = out.send(system_bubble(feedback));
                                        }
                                    }
                                }
                            }
                            ClientMsg::Kick { target } => {
                                // Owner-only; never self, never another owner.
                                if state.is_owner(pid)
                                    && target != pid
                                    && !state.is_owner(target)
                                {
                                    // Reverse-lookup the connection serving
                                    // this player, if it's still live.
                                    let kc = player_of
                                        .iter()
                                        .find(|(_, &p)| p == target)
                                        .map(|(&c, _)| c);
                                    if let Some(kc) = kc {
                                        // A kick still ends a sitting: record it
                                        // for telemetry (and clear join-time)
                                        // before tearing the connection down.
                                        if let Some(info) = state.player(target).cloned() {
                                            let duration_s = joined_at
                                                .remove(&kc)
                                                .map(|t| t.elapsed().as_secs())
                                                .unwrap_or(0);
                                            telemetry::record(Event::session_end(
                                                tel_world,
                                                tel_key,
                                                target,
                                                info.account,
                                                info.name,
                                                duration_s,
                                                &state,
                                            ));
                                        } else {
                                            joined_at.remove(&kc);
                                        }
                                        // Drop the connection directly rather
                                        // than via `disconnect()`: a kicked
                                        // player must not get a saved
                                        // reconnect session (which would let
                                        // them silently rejoin) or a "left"
                                        // event (kick_player below pushes its
                                        // own "removed by the owner" event).
                                        // Dropping the Sender here ends the
                                        // writer thread, which shuts the
                                        // socket down and unblocks the
                                        // reader thread to clean up its own
                                        // `Leave` (a no-op by then since the
                                        // connection id is already gone).
                                        clients.remove(&kc);
                                        player_of.remove(&kc);
                                        token_of.remove(&kc);
                                        limiters.remove(&kc);
                                    }
                                    sim::kick_player(&mut state, target);
                                }
                            }
                            ClientMsg::RefreshShowcase => {
                                if let Some(acc) = state.player_account(pid) {
                                    if let Some(out) = clients.get(&client) {
                                        // One save-file read per friend —
                                        // disk I/O that must not run on this
                                        // thread (a long friends list would
                                        // stall every tick of this world).
                                        // The per-connection SHOWCASE_COOLDOWN
                                        // bounds how often these spawn; a
                                        // failed spawn just skips the reply
                                        // (better than a stalled world).
                                        let out = out.clone();
                                        let central = state.central;
                                        let ledger = state.central_ledger.clone();
                                        let _ = thread::Builder::new()
                                            .name("fc-showcase".into())
                                            .spawn(move || {
                                                let _ = out.send(showcase_for(central, &ledger, acc));
                                            });
                                    }
                                }
                            }
                            ClientMsg::SetVisitPolicy { allow_offline } => {
                                if let Some(acc) = state.player_account(pid) {
                                    if accounts::set_visit_policy(acc, allow_offline).is_ok() {
                                        if let Some(out) = clients.get(&client) {
                                            let _ = out.send(ServerMsg::VisitPolicy { allow_offline });
                                        }
                                    } else if let Some(out) = clients.get(&client) {
                                        let _ = out.send(system_bubble(
                                            "Sozlamani saqlab bo'lmadi — keyinroq qayta urining.",
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(ToServer::Leave { client }) => {
                    if clients.contains_key(&client) {
                        disconnect(
                            client,
                            &mut clients,
                            &mut player_of,
                            &mut token_of,
                            &mut sessions,
                            &mut session_order,
                            &mut limiters,
                            &mut state,
                            tel_world,
                            tel_key,
                            &mut joined_at,
                        );
                        if config.verbose {
                            println!("[server] client #{} disconnected", client);
                        }
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break 'outer,
            }
        }

        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        if ever_joined && clients.is_empty() && !config.persistent {
            break;
        }
        // Per-account worlds (`idle_shutdown: Some(_)`) save and exit after
        // sitting with nobody connected for that long, instead of running
        // forever — `WorldManager::get_or_spawn` respawns them from disk the
        // next time that account logs in. The single shared world always has
        // `idle_shutdown: None` and is unaffected.
        if let Some(idle_dur) = config.idle_shutdown {
            if clients.is_empty() {
                let since = *idle_since.get_or_insert_with(Instant::now);
                if Instant::now().duration_since(since) >= idle_dur {
                    if config.persistent {
                        if let Err(e) = save_world(&config, &state) {
                            if config.verbose {
                                eprintln!("[server] idle-eviction save failed: {e}");
                            }
                        }
                    }
                    break;
                }
            } else {
                idle_since = None;
            }
        }

        let now = Instant::now();

        if config.persistent && now.duration_since(last_save) >= AUTOSAVE_INTERVAL {
            last_save = now;
            if let Err(e) = save_world(&config, &state) {
                if config.verbose {
                    eprintln!("[server] world autosave failed: {e}");
                }
            }
        }

        if now >= next_tick {
            let day_before = state.day();
            // Drop commands queued by a player who has since left or been
            // kicked: `can_issue` trusts pids absent from the roster, so an
            // unapplied command from a departed guest must not slip through
            // with unrestricted permission.
            pending.retain(|(pid, _)| state.player(*pid).is_some());
            for (pid, cmd) in pending.drain(..) {
                sim::apply_command(&mut state, pid, &cmd);
            }
            sim::tick(&mut state);

            // A dead (or victorious) persistent world starts over, so the
            // public server never sits in a game-over screen for hours.
            // Personal (owner-scoped) worlds are the exception: they now
            // keep ticking in the background for a long time after their
            // owner disconnects (see `world_manager::IDLE_SHUTDOWN`), so a
            // fragile colony dying while nobody's watching must NOT
            // silently reset itself before the owner ever sees what
            // happened — the countdown only starts once someone is actually
            // connected to read it, same as any single-player "you died"
            // screen waits for you to look at it. The shared/central worlds
            // (`owner_account: None`) are unaffected: nobody in particular
            // owns them, so the original behavior (auto-reset regardless of
            // who's connected) still applies.
            if config.persistent {
                if state.phase == GamePhase::Running {
                    game_over_since = None;
                } else if config.owner_account.is_none() || !clients.is_empty() {
                    let since = *game_over_since.get_or_insert(now);
                    if now.duration_since(since) >= WORLD_RESET_AFTER {
                        game_over_since = None;
                        let seed = state.rng ^ 0xA5A5_5A5A_D00D_FEED ^ state.tick;
                        let players = state.players.clone();
                        // Carry ownership across the reset.
                        let owner_id = state.owner_id;
                        // Graduation is a permanent achievement, not a
                        // property of one map: it's what admits this world's
                        // account to the central world, and must not expire
                        // just because the next survival run started before
                        // (or without) the player stepping through.
                        let graduated = state.graduated;
                        state = sim::new_game(seed, config.win_days);
                        state.players = players;
                        state.owner_id = owner_id;
                        state.graduated = graduated;
                        printed_events = 0;
                        printed_chat = 0;
                        // `total_events`/`total_chat` also restart at 0 on a fresh
                        // `new_game`, so the last-sent counters must follow —
                        // otherwise a coincidental match after the reset would
                        // wrongly tell a connected client "no new events/chat"
                        // while it's still holding the previous world's log.
                        last_sent_events = 0;
                        last_sent_chat = 0;
                        sim::push_event(
                            &mut state,
                            "A new expedition arrives - the city rises again.",
                        );
                        if config.verbose {
                            println!("[server] world reset (seed {seed})");
                        }
                    }
                }
            }

            if config.verbose {
                if state.day() != day_before {
                    println!(
                        "[server] day {} — pop {}, wood {:.0}, coal {:.0}, food {:.0}, {:.1} C",
                        state.day(),
                        state.survivors.len(),
                        state.stock.wood,
                        state.stock.coal,
                        state.stock.food,
                        state.temperature()
                    );
                }
                while printed_events < state.total_events {
                    let missed = (state.total_events - printed_events) as usize;
                    let start = state.events.len().saturating_sub(missed);
                    for ev in &state.events[start..] {
                        println!("[server] day {}: {}", ev.day, ev.text);
                    }
                    printed_events = state.total_events;
                }
                while printed_chat < state.total_chat {
                    let missed = (state.total_chat - printed_chat) as usize;
                    let start = state.chat.len().saturating_sub(missed);
                    for line in &state.chat[start..] {
                        println!("[server] chat {}: {}", line.name, line.text);
                    }
                    printed_chat = state.total_chat;
                }
            }

            // Broadcast the snapshot; tiles ride along only every Nth tick, and
            // the other quiet-most-ticks collections (events/chat/pings/
            // missions/techs) only when they actually changed — see
            // `protocol::Included`. With nobody connected (a persistent,
            // currently-empty region) there's nothing to send it to, so skip
            // the clone/serialize/send work entirely — the sim keeps ticking
            // (and autosaving) above regardless. `state.tick`'s tile cadence
            // still advances on its normal schedule either way, so the moment
            // a client joins mid-cycle it sees exactly the tile/no-tile
            // pattern it would have if the broadcast had never stopped.
            if !clients.is_empty() {
                // While a persistent world sits in game-over, tell everyone
                // how long until the automatic reset (`WORLD_RESET_AFTER`):
                // the overlay turns the silent command freeze into a visible
                // countdown. `game_over_since` is only ever Some on the
                // persistent game-over path, so this sends nothing elsewhere.
                let countdown = game_over_since.map(|since| {
                    WORLD_RESET_AFTER
                        .saturating_sub(now.duration_since(since))
                        .as_secs_f32()
                        .ceil() as u32
                });
                if countdown != last_countdown_sent {
                    last_countdown_sent = countdown;
                    if let Some(seconds_left) = countdown {
                        // Send failures mean a dead client; the `State` loop
                        // right below detects and disconnects those.
                        for out in clients.values() {
                            let _ = out.send(ServerMsg::ResetCountdown { seconds_left });
                        }
                    }
                }
                let included = Included {
                    tiles: state.tick % TILES_EVERY_N_TICKS == 0,
                    events: state.total_events != last_sent_events,
                    chat: state.total_chat != last_sent_chat,
                    pings: state.pings != last_sent_pings,
                    missions: state.missions != last_sent_missions,
                    techs: state.techs != last_sent_techs,
                };
                last_sent_events = state.total_events;
                last_sent_chat = state.total_chat;
                if included.pings {
                    last_sent_pings = state.pings.clone();
                }
                if included.missions {
                    last_sent_missions = state.missions.clone();
                }
                if included.techs {
                    last_sent_techs = state.techs.clone();
                }
                let mut wire = state.clone();
                if !included.tiles {
                    wire.tiles = Vec::new();
                }
                if !included.events {
                    wire.events = Vec::new();
                }
                if !included.chat {
                    wire.chat = Vec::new();
                }
                if !included.pings {
                    wire.pings = Vec::new();
                }
                if !included.missions {
                    wire.missions = Vec::new();
                }
                if !included.techs {
                    wire.techs = Vec::new();
                }
                let mut dead: Vec<u64> = Vec::new();
                for (id, out) in &clients {
                    let msg = ServerMsg::State {
                        state: wire.clone(),
                        included,
                    };
                    if out.send(msg).is_err() {
                        dead.push(*id);
                    }
                }
                for id in dead {
                    disconnect(
                        id,
                        &mut clients,
                        &mut player_of,
                        &mut token_of,
                        &mut sessions,
                        &mut session_order,
                        &mut limiters,
                        &mut state,
                        tel_world,
                        tel_key,
                        &mut joined_at,
                    );
                }
            }

            next_tick += tick_dur;
            // If we fell far behind (debugger, laptop sleep), resync.
            if now > next_tick + Duration::from_secs(1) {
                next_tick = now + tick_dur;
            }
        }

        let wait = next_tick.saturating_duration_since(Instant::now());
        if !wait.is_zero() {
            thread::sleep(wait.min(Duration::from_millis(50)));
        }
    }
    if config.persistent {
        if let Err(e) = save_world(&config, &state) {
            if config.verbose {
                eprintln!("[server] final world save on shutdown failed: {e}");
            }
        }
    }
    shutdown.store(true, Ordering::SeqCst);
}

/// Saves to this config's world: the per-account path when set, otherwise
/// the single shared-world default (`persist::save`, i.e. `FC_WORLD_SAVE`).
fn save_world(config: &ServerConfig, state: &GameState) -> io::Result<()> {
    match &config.save_path {
        Some(path) => persist::save_at(state, path),
        None => persist::save(state),
    }
}
