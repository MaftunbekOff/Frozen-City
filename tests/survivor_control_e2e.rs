//! End-to-end networking tests for the V0.7 "survivor management" wire
//! commands (`MoveSurvivor`, `SetLeader`): a real server on a real TCP
//! socket, real clients over `client::connect_tcp`, shared authoritative
//! state. Mirrors the style of `net_e2e.rs` / `coop_e2e.rs`.
//!
//! Three things are exercised, all against ONE running server (kept to a
//! single `#[test]` — no env vars are touched here, unlike
//! `account_world_e2e.rs`/`social_server_tests.rs`, but starting one server
//! and walking it through several scenarios in turn is cheaper than booting
//! three and keeps the causal order — move, then lead, then a guest's
//! attempt — obvious top to bottom):
//!
//!  1. `MoveSurvivor` actually drives the survivor's server-authoritative
//!     (x, y) toward the target tick over tick, and clears `move_target` (and
//!     `assigned_building`) once arrived.
//!  2. `SetLeader` is owner-only: the owner's command sets `state.leader`; the
//!     same command from a guest is silently ignored (leader unchanged).
//!  3. `GuestPermission` actually gates `MoveSurvivor` on the wire: under
//!     `ViewOnly` a guest's move command is dropped (no target recorded, no
//!     position change); flipping to `Build` lets the very same guest move a
//!     survivor.

use std::time::{Duration, Instant};

use frozen_city::game::types::{GameState, GuestPermission, PlayerCommand};
use frozen_city::net::client::{self, ClientConn};
use frozen_city::net::protocol::{ClientMsg, ServerMsg};
use frozen_city::net::server::{self, ServerConfig};

fn recv_welcome(conn: &ClientConn) -> (u64, u64, GameState) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match conn.recv_timeout(Duration::from_millis(500)) {
            Ok(ServerMsg::Welcome { player_id, token, state }) => return (player_id, token, state),
            Ok(_) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(e) => panic!("connection died before Welcome: {e:?}"),
        }
    }
    panic!("no Welcome within 10s");
}

fn wait_state(conn: &ClientConn, mut pred: impl FnMut(&GameState) -> bool) -> GameState {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match conn.recv_timeout(Duration::from_millis(500)) {
            Ok(ServerMsg::State { state, .. }) => {
                if pred(&state) {
                    return state;
                }
            }
            Ok(_) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(e) => panic!("connection died: {e:?}"),
        }
    }
    panic!("condition not met within 10s");
}

/// Drains states for `dur`, returning the LAST one seen (or `None` if none
/// arrived) — used where we need to assert something did NOT happen (e.g.
/// leadership did not change) rather than waiting for a specific predicate
/// that might never come true.
fn drain_states_for(conn: &ClientConn, dur: Duration) -> Option<GameState> {
    let deadline = Instant::now() + dur;
    let mut last = None;
    while Instant::now() < deadline {
        if let Ok(ServerMsg::State { state, .. }) = conn.recv_timeout(Duration::from_millis(200)) {
            last = Some(state);
        }
    }
    last
}

fn start_server(seed: u64) -> server::ServerHandle {
    server::start(ServerConfig {
        port: Some(0), // ephemeral port
        seed,
        win_days: 12,
        persistent: false,
        verbose: false,
        save_path: None,
        idle_shutdown: None,
        central: false,
        owner_account: None,
        invites: None,
        world_manager: None,
    })
    .expect("server starts")
}

fn addr_of(handle: &server::ServerHandle) -> String {
    format!("127.0.0.1:{}", handle.addr.expect("bound addr").port())
}

#[test]
fn survivor_control_over_the_wire() {
    let handle = start_server(5150);
    let addr = addr_of(&handle);

    // Alice connects first: she becomes Owner (first joiner rule — see
    // `sim::player_joined_as`), exactly like `net_e2e.rs`/`coop_e2e.rs`.
    let alice = client::connect_tcp(&addr, "Alice", None).expect("alice connects");
    let (alice_id, _alice_token, welcome_state) = recv_welcome(&alice);
    assert!(welcome_state.players.iter().any(|p| p.id == alice_id));

    // ================= (a) MoveSurvivor drives position =================
    let survivor_id = welcome_state.survivors.first().expect("starting survivors present").id;
    let start = welcome_state
        .survivors
        .iter()
        .find(|s| s.id == survivor_id)
        .map(|s| (s.x, s.y))
        .unwrap();

    // Pick a target a few tiles away from the current spot but still safely
    // in-bounds, so the walk is quick (2.5 tiles/sec) yet the movement delta
    // is unambiguous.
    let target_x = if start.0 < 32.0 { (start.0 as u8).saturating_add(6).min(63) } else { (start.0 as u8).saturating_sub(6) };
    let target_y = if start.1 < 32.0 { (start.1 as u8).saturating_add(6).min(63) } else { (start.1 as u8).saturating_sub(6) };

    alice.send(ClientMsg::Cmd(PlayerCommand::MoveSurvivor {
        survivor: survivor_id,
        x: target_x,
        y: target_y,
    }));

    // First: the target is recorded and the survivor is freed from any work
    // assignment (assigned_building None) — matches the `move_target` doc on
    // `Survivor` and the sim-level test in `movement_tests.rs`, now proven
    // over the real wire.
    let moving = wait_state(&alice, |s| {
        s.survivors
            .iter()
            .find(|sv| sv.id == survivor_id)
            .map(|sv| sv.move_target == Some((target_x, target_y)))
            .unwrap_or(false)
    });
    let moving_survivor = moving.survivors.iter().find(|s| s.id == survivor_id).unwrap();
    assert_eq!(moving_survivor.assigned_building, None, "a manually walked survivor is unassigned from work");

    // Then: position actually changes tick over tick toward the target — not
    // just the target field. Compare against the pre-move (x, y).
    let progressing = wait_state(&alice, |s| {
        s.survivors
            .iter()
            .find(|sv| sv.id == survivor_id)
            .map(|sv| (sv.x, sv.y) != start)
            .unwrap_or(false)
    });
    let mid = progressing.survivors.iter().find(|s| s.id == survivor_id).unwrap();
    assert_ne!((mid.x, mid.y), start, "position must have moved from the spawn point");

    // Finally: once arrived, move_target clears back to None and the
    // survivor snaps exactly onto the target tile's center.
    let arrived = wait_state(&alice, |s| {
        s.survivors
            .iter()
            .find(|sv| sv.id == survivor_id)
            .map(|sv| sv.move_target.is_none())
            .unwrap_or(false)
    });
    let final_survivor = arrived.survivors.iter().find(|s| s.id == survivor_id).unwrap();
    assert_eq!(final_survivor.assigned_building, None, "still idle after arriving (no auto-reassignment)");
    let (fx, fy) = (target_x as f32 + 0.5, target_y as f32 + 0.5);
    let dist = ((final_survivor.x - fx).powi(2) + (final_survivor.y - fy).powi(2)).sqrt();
    assert!(
        dist < 0.1,
        "survivor should have snapped to the target tile center ({fx}, {fy}), got ({}, {})",
        final_survivor.x,
        final_survivor.y
    );

    // ================= (b) SetLeader is owner-only =================
    let leader_candidate = arrived.survivors.first().expect("some survivor to appoint").id;
    alice.send(ClientMsg::Cmd(PlayerCommand::SetLeader { survivor: leader_candidate }));
    let led = wait_state(&alice, |s| s.leader == Some(leader_candidate));
    assert_eq!(led.leader, Some(leader_candidate));

    // Bob joins second: guest (owner already claimed by Alice).
    let bob = client::connect_tcp(&addr, "Bob", None).expect("bob connects");
    let (_bob_id, _bob_token, _) = recv_welcome(&bob);
    wait_state(&alice, |s| s.players.len() == 2);

    // A different survivor for Bob to (attempt to) appoint, so a silent
    // no-op is unambiguous versus "nothing changed because it's the same id".
    let other_candidate = led
        .survivors
        .iter()
        .map(|s| s.id)
        .find(|&id| id != leader_candidate)
        .expect("at least two survivors exist");
    bob.send(ClientMsg::Cmd(PlayerCommand::SetLeader { survivor: other_candidate }));

    // Give the server several ticks to (not) apply it, then confirm the
    // leader is still Alice's earlier pick — a guest's SetLeader must be a
    // total no-op, not just "doesn't crash".
    let after_guest_attempt = drain_states_for(&bob, Duration::from_secs(2));
    if let Some(s) = after_guest_attempt {
        assert_eq!(s.leader, Some(leader_candidate), "guest SetLeader must not change the leader");
    }
    // Belt-and-suspenders: ask Alice's connection too, since Bob might not
    // have received a State in the drain window.
    let confirmed = wait_state(&alice, |s| s.leader == Some(leader_candidate));
    assert_eq!(confirmed.leader, Some(leader_candidate), "leader must remain the owner's original pick");

    // ================= (c) GuestPermission gates MoveSurvivor =================
    // Owner locks the world to ViewOnly: guests may look, not touch.
    alice.send(ClientMsg::SetGuestPermission { perm: GuestPermission::ViewOnly });
    wait_state(&bob, |s| s.guest_perm == GuestPermission::ViewOnly);

    // Pick a survivor Bob will try (and fail) to move, and remember where
    // they are right now.
    let guest_target_survivor = confirmed
        .survivors
        .iter()
        .find(|s| s.id != leader_candidate)
        .map(|s| s.id)
        .unwrap_or(leader_candidate);
    let pre_attempt_pos = confirmed
        .survivors
        .iter()
        .find(|s| s.id == guest_target_survivor)
        .map(|s| (s.x, s.y))
        .unwrap();

    bob.send(ClientMsg::Cmd(PlayerCommand::MoveSurvivor {
        survivor: guest_target_survivor,
        x: 2,
        y: 2,
    }));
    // Under ViewOnly, `can_issue` refuses the command outright — confirm no
    // move_target is ever recorded and the position never leaves its spot.
    let after_denied = drain_states_for(&bob, Duration::from_secs(2));
    if let Some(s) = after_denied {
        let sv = s.survivors.iter().find(|s| s.id == guest_target_survivor).unwrap();
        assert_eq!(sv.move_target, None, "ViewOnly guest's MoveSurvivor must be refused");
        assert_eq!((sv.x, sv.y), pre_attempt_pos, "position must not change under a refused command");
    }

    // Owner relaxes the policy to Build: the SAME guest, same command kind,
    // now succeeds — proving the earlier refusal was the permission gate and
    // not some unrelated bug.
    alice.send(ClientMsg::SetGuestPermission { perm: GuestPermission::Build });
    wait_state(&bob, |s| s.guest_perm == GuestPermission::Build);

    bob.send(ClientMsg::Cmd(PlayerCommand::MoveSurvivor {
        survivor: guest_target_survivor,
        x: 2,
        y: 2,
    }));
    let allowed = wait_state(&bob, |s| {
        s.survivors
            .iter()
            .find(|sv| sv.id == guest_target_survivor)
            .map(|sv| sv.move_target == Some((2, 2)))
            .unwrap_or(false)
    });
    let moved_sv = allowed.survivors.iter().find(|s| s.id == guest_target_survivor).unwrap();
    assert_eq!(moved_sv.move_target, Some((2, 2)), "guest's MoveSurvivor must succeed under Build");

    handle.stop();
}
