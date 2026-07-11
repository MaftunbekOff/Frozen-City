//! End-to-end networking tests for the V0.2 co-op features: chat, pings,
//! building attribution and reconnect-by-token. Mirrors the style of
//! `net_e2e.rs` — a real server on a real TCP socket, real clients over
//! `client::connect_tcp`, shared authoritative state.

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

#[test]
fn chat_is_broadcast() {
    let handle = start_server(1001);
    let addr = addr_of(&handle);

    let alice = client::connect_tcp(&addr, "Alice", None).expect("alice connects");
    let (_alice_id, _alice_token, _) = recv_welcome(&alice);

    let bob = client::connect_tcp(&addr, "Bob", None).expect("bob connects");
    let (_bob_id, _bob_token, _) = recv_welcome(&bob);

    // Make sure both are visible to each other before sending chat.
    wait_state(&bob, |s| s.players.len() == 2);

    alice.send(ClientMsg::Chat { text: "hello team".into() });

    let seen = wait_state(&bob, |s| s.chat.iter().any(|c| c.text == "hello team"));
    let line = seen
        .chat
        .iter()
        .find(|c| c.text == "hello team")
        .expect("chat line present");
    assert_eq!(line.name, "Alice");

    handle.stop();
}

#[test]
fn building_is_attributed() {
    let handle = start_server(1002);
    let addr = addr_of(&handle);

    let alice = client::connect_tcp(&addr, "Alice", None).expect("alice connects");
    let (alice_id, _alice_token, alice_state) = recv_welcome(&alice);

    let bob = client::connect_tcp(&addr, "Bob", None).expect("bob connects");
    let (_bob_id, _bob_token, _) = recv_welcome(&bob);

    wait_state(&bob, |s| s.players.len() == 2);

    let spot = some_tent_spot(&alice_state);
    alice.send(ClientMsg::Cmd(PlayerCommand::Place {
        kind: BuildingKind::Tent,
        x: spot.0,
        y: spot.1,
    }));

    let seen = wait_state(&bob, |s| {
        s.buildings
            .iter()
            .any(|b| b.kind != BuildingKind::Furnace && b.owner == Some(alice_id))
    });
    assert!(
        seen.events.iter().any(|e| e.text.contains("Alice")),
        "expected an event attributing the build to Alice: {:?}",
        seen.events
    );

    handle.stop();
}

#[test]
fn reconnect_resumes_identity() {
    let handle = start_server(1003);
    let addr = addr_of(&handle);

    // Bob connects first and stays connected the whole test, keeping the
    // non-persistent server alive across Alice's disconnect/reconnect.
    let bob = client::connect_tcp(&addr, "Bob", None).expect("bob connects");
    let (_bob_id, _bob_token, _) = recv_welcome(&bob);

    let alice = client::connect_tcp(&addr, "Alice", None).expect("alice connects");
    let (alice_id, alice_token, alice_state) = recv_welcome(&alice);

    wait_state(&bob, |s| s.players.len() == 2);

    let spot = some_tent_spot(&alice_state);
    alice.send(ClientMsg::Cmd(PlayerCommand::Place {
        kind: BuildingKind::Tent,
        x: spot.0,
        y: spot.1,
    }));
    // Confirm the server actually applied (and attributed) the build before
    // we tear the connection down.
    wait_state(&bob, |s| s.buildings.iter().any(|b| b.owner == Some(alice_id)));

    drop(alice);
    wait_state(&bob, |s| s.players.len() == 1);

    let alice2 =
        client::connect_tcp(&addr, "Alice", Some(alice_token)).expect("alice reconnects");
    let (rejoin_id, _rejoin_token, rejoin_state) = recv_welcome(&alice2);
    assert_eq!(rejoin_id, alice_id, "reconnect must resume the same identity");

    let p = rejoin_state
        .players
        .iter()
        .find(|p| p.id == alice_id)
        .expect("alice present in the roster after reconnect");
    assert!(p.built >= 1, "built count preserved across reconnect");

    handle.stop();
}

#[test]
fn ping_is_shared() {
    let handle = start_server(1004);
    let addr = addr_of(&handle);

    let alice = client::connect_tcp(&addr, "Alice", None).expect("alice connects");
    let (alice_id, _alice_token, _) = recv_welcome(&alice);

    let bob = client::connect_tcp(&addr, "Bob", None).expect("bob connects");
    let (_bob_id, _bob_token, _) = recv_welcome(&bob);

    wait_state(&bob, |s| s.players.len() == 2);

    alice.send(ClientMsg::Ping { x: 12.0, y: 34.0 });

    wait_state(&bob, |s| s.pings.iter().any(|p| p.player_id == alice_id));

    handle.stop();
}
