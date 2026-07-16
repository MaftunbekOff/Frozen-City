//! End-to-end networking tests for the V0.7 "survivor management" wire
//! commands (`MoveSurvivor`, `SetLeader`): a real server on a real TCP
//! socket, real clients over `client::connect_tcp`, shared authoritative
//! state. Mirrors the style of `net_e2e.rs` / `coop_e2e.rs`.
//!
//! Two things are exercised, both against ONE running server (kept to a
//! single `#[test]` — no env vars are touched here, unlike
//! `account_world_e2e.rs`/`social_server_tests.rs`, but starting one server
//! and walking it through several scenarios in turn is cheaper than booting
//! two and keeps the causal order — move, then lead — obvious top to
//! bottom):
//!
//!  1. `MoveSurvivor` actually drives the survivor's server-authoritative
//!     (x, y) toward the target tick over tick, and clears `move_target` (and
//!     `assigned_building`) once arrived.
//!  2. `SetLeader` works over the wire for a guest, not just the owner: guests
//!     have full command authority alongside the owner (see
//!     `GameState::can_issue`), so the same command from a guest also sets
//!     `state.leader`.

use std::time::{Duration, Instant};

use frozen_city::game::sim;
use frozen_city::game::types::{GameState, PlayerCommand};
use frozen_city::net::client::{self, ClientConn};
use frozen_city::net::persist;
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

/// This suite is about the survivor-control wire commands themselves (move,
/// lead) — not about the "one leader lights the furnace" opening
/// (`SetLeader`'s guest check in particular wants a second survivor to
/// appoint, so the assertion is unambiguous — see the `other_candidate` pick
/// below). Pre-seed a save file with `sim::new_game_bootstrapped`'s starting
/// state (furnace already lit, full population) and point a `persistent`
/// server at it, exactly like `central_world_e2e.rs` does for the same
/// reason, rather than making the wire test itself drive the furnace build
/// and wait on the real-time (150s/day) natural-arrival roll.
fn start_server(seed: u64) -> server::ServerHandle {
    let win_days = 12;
    let save_path = std::env::temp_dir()
        .join(format!("fc-survivor-control-e2e-{}-{}.bin", std::process::id(), seed))
        .to_str()
        .unwrap()
        .to_string();
    persist::save_at(&sim::new_game_bootstrapped(seed, win_days), &save_path).expect("seed save");

    server::start(ServerConfig {
        port: Some(0), // ephemeral port
        seed,
        win_days,
        persistent: true,
        verbose: false,
        save_path: Some(save_path),
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

    // ================= (b) SetLeader works for a guest too =================
    let leader_candidate = arrived.survivors.first().expect("some survivor to appoint").id;
    alice.send(ClientMsg::Cmd(PlayerCommand::SetLeader { survivor: leader_candidate }));
    let led = wait_state(&alice, |s| s.leader == Some(leader_candidate));
    assert_eq!(led.leader, Some(leader_candidate));

    // Bob joins second: guest (owner already claimed by Alice).
    let bob = client::connect_tcp(&addr, "Bob", None).expect("bob connects");
    let (_bob_id, _bob_token, _) = recv_welcome(&bob);
    wait_state(&alice, |s| s.players.len() == 2);

    // A different survivor for Bob to appoint, so the change is unambiguous
    // versus "nothing changed because it's the same id".
    let other_candidate = led
        .survivors
        .iter()
        .map(|s| s.id)
        .find(|&id| id != leader_candidate)
        .expect("at least two survivors exist");
    bob.send(ClientMsg::Cmd(PlayerCommand::SetLeader { survivor: other_candidate }));

    // Guests have full command authority alongside the owner (see
    // `GameState::can_issue`), so this must actually take effect.
    let guest_led = wait_state(&alice, |s| s.leader == Some(other_candidate));
    assert_eq!(
        guest_led.leader,
        Some(other_candidate),
        "a guest's SetLeader must succeed — guests have full command authority"
    );

    handle.stop();
}
