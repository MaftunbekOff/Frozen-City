//! End-to-end networking tests for the roles/ownership system: a real server
//! on a real TCP socket, real clients over `client::connect_tcp`, shared
//! authoritative state. Mirrors the style of `net_e2e.rs` / `coop_e2e.rs`.

use std::time::{Duration, Instant};

use frozen_city::game::types::{BuildingKind, GameState, PlayerCommand};
use frozen_city::net::client::{self, ClientConn};
use frozen_city::net::protocol::{ClientMsg, ServerMsg};
use frozen_city::net::server::{self, ServerConfig};

fn recv_welcome(conn: &ClientConn) -> (u64, u64, GameState) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match conn.recv_timeout(Duration::from_millis(500)) {
            Ok(ServerMsg::Welcome { player_id, token, state }) => {
                return (player_id, token, state)
            }
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
    let port = handle.addr.expect("bound addr").port();
    format!("127.0.0.1:{port}")
}

fn some_tent_spot(state: &GameState) -> (u8, u8) {
    (0..64u8)
        .flat_map(|y| (0..64u8).map(move |x| (x, y)))
        .find(|&(x, y)| state.can_place(BuildingKind::Tent, x, y).is_ok())
        .expect("some valid tent spot")
}

/// The owner is whoever connects first; a guest may place buildings (guests
/// have full command authority alongside the owner), and the owner sees them
/// appear.
#[test]
fn guest_can_place_by_default() {
    let handle = start_server(2001);
    let addr = addr_of(&handle);

    // Bob connects first: he owns this world.
    let bob = client::connect_tcp(&addr, "Bob", None).expect("bob connects");
    let (_bob_id, _bob_token, _) = recv_welcome(&bob);

    // Carol connects second: she is a guest, under the default Build policy.
    let carol = client::connect_tcp(&addr, "Carol", None).expect("carol connects");
    let (carol_id, _carol_token, carol_state) = recv_welcome(&carol);

    wait_state(&bob, |s| s.players.len() == 2);

    let spot = some_tent_spot(&carol_state);
    carol.send(ClientMsg::Cmd(PlayerCommand::Place {
        kind: BuildingKind::Tent,
        x: spot.0,
        y: spot.1,
        facing: 0,
    }));

    let seen = wait_state(&bob, |s| {
        s.buildings
            .iter()
            .any(|b| b.kind == BuildingKind::Tent && b.owner == Some(carol_id))
    });
    assert_eq!(seen.buildings.len(), 3, "furnace + tunnel + the guest's tent");

    handle.stop();
}

/// The owner can forcibly remove a guest: the roster shrinks and the guest's
/// connection is severed by the server (not merely ignored).
#[test]
fn owner_can_kick_a_guest() {
    let handle = start_server(2004);
    let addr = addr_of(&handle);

    let bob = client::connect_tcp(&addr, "Bob", None).expect("bob connects");
    let (_bob_id, _bob_token, _) = recv_welcome(&bob);

    let carol = client::connect_tcp(&addr, "Carol", None).expect("carol connects");
    let (carol_id, _carol_token, _) = recv_welcome(&carol);

    wait_state(&bob, |s| s.players.len() == 2);

    bob.send(ClientMsg::Kick { target: carol_id });

    wait_state(&bob, |s| s.players.len() == 1);

    // The kicked connection must actually die (server drops the socket),
    // rather than just silently stop receiving updates.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut died = false;
    while Instant::now() < deadline {
        match carol.recv_timeout(Duration::from_millis(500)) {
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                died = true;
                break;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Ok(_) => continue,
        }
    }
    assert!(died, "kicked guest's connection should be closed by the server");

    handle.stop();
}
