//! End-to-end networking test: a real server on a real TCP socket, two real
//! clients, shared authoritative state.

use std::time::{Duration, Instant};

use frozen_city::game::types::{BuildingKind, GameState, PlayerCommand};
use frozen_city::net::client::{self, ClientConn};
use frozen_city::net::protocol::{ClientMsg, ServerMsg};
use frozen_city::net::server::{self, ServerConfig};

fn recv_welcome(conn: &ClientConn) -> (u64, GameState) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match conn.rx.recv_timeout(Duration::from_millis(500)) {
            Ok(ServerMsg::Welcome { player_id, state }) => return (player_id, state),
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
        match conn.rx.recv_timeout(Duration::from_millis(500)) {
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

#[test]
fn two_clients_share_one_city() {
    let handle = server::start(ServerConfig {
        port: Some(0), // ephemeral port
        seed: 4242,
        win_days: 12,
        persistent: false,
        verbose: false,
    })
    .expect("server starts");
    let port = handle.addr.expect("bound addr").port();
    let addr = format!("127.0.0.1:{port}");

    let alice = client::connect_tcp(&addr, "Alice").expect("alice connects");
    let (alice_id, state) = recv_welcome(&alice);
    assert!(state.players.iter().any(|p| p.name == "Alice"));

    let bob = client::connect_tcp(&addr, "Bob").expect("bob connects");
    let (bob_id, _) = recv_welcome(&bob);
    assert_ne!(alice_id, bob_id);

    // Bob sees both players.
    wait_state(&bob, |s| s.players.len() == 2);

    // Alice builds a tent; Bob must see it appear.
    let spot = (0..64u8)
        .flat_map(|y| (0..64u8).map(move |x| (x, y)))
        .find(|&(x, y)| state.can_place(BuildingKind::Tent, x, y).is_ok())
        .expect("some valid tent spot");
    alice.send(ClientMsg::Cmd(PlayerCommand::Place {
        kind: BuildingKind::Tent,
        x: spot.0,
        y: spot.1,
    }));
    let seen = wait_state(&bob, |s| {
        s.buildings.iter().any(|b| b.kind == BuildingKind::Tent)
    });
    assert_eq!(seen.buildings.len(), 2, "furnace + tent");

    // Cursor presence reaches the other player.
    alice.send(ClientMsg::Cursor { x: 10.5, y: 20.5 });
    wait_state(&bob, |s| {
        s.players
            .iter()
            .any(|p| p.id == alice_id && p.cursor.is_some())
    });

    // Alice leaves; Bob sees the roster shrink.
    drop(alice);
    wait_state(&bob, |s| s.players.len() == 1);

    handle.stop();
}

#[test]
fn tiles_are_omitted_but_periodically_included() {
    let handle = server::start(ServerConfig {
        port: Some(0),
        seed: 7,
        win_days: 12,
        persistent: false,
        verbose: false,
    })
    .expect("server starts");
    let port = handle.addr.expect("addr").port();
    let conn = client::connect_tcp(&format!("127.0.0.1:{port}"), "Solo").expect("connects");
    let (_, welcome_state) = recv_welcome(&conn);
    assert!(
        !welcome_state.tiles.is_empty(),
        "welcome always carries tiles"
    );

    let mut with_tiles = 0;
    let mut without_tiles = 0;
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline && (with_tiles == 0 || without_tiles == 0) {
        if let Ok(ServerMsg::State { state, tiles_included }) =
            conn.rx.recv_timeout(Duration::from_millis(500))
        {
            if tiles_included {
                assert!(!state.tiles.is_empty());
                with_tiles += 1;
            } else {
                assert!(state.tiles.is_empty());
                without_tiles += 1;
            }
        }
    }
    assert!(with_tiles > 0, "periodic full-tile snapshots expected");
    assert!(without_tiles > 0, "lightweight snapshots expected");

    handle.stop();
}
