//! End-to-end networking tests: a real server on a real TCP socket, real
//! clients over both the native protocol and WebSocket, shared state.

use std::time::{Duration, Instant};

use frozen_city::game::types::{BuildingKind, GameState, PlayerCommand};
use frozen_city::net::client::{self, ClientConn};
use frozen_city::net::protocol::{decode, encode, ClientMsg, ServerMsg, MAX_FRAME};
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

#[test]
fn two_clients_share_one_city() {
    let handle = server::start(ServerConfig {
        port: Some(0), // ephemeral port
        seed: 4242,
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
    .expect("server starts");
    let port = handle.addr.expect("bound addr").port();
    let addr = format!("127.0.0.1:{port}");

    let alice = client::connect_tcp(&addr, "Alice", None).expect("alice connects");
    let (alice_id, _alice_token, state) = recv_welcome(&alice);
    assert!(state.players.iter().any(|p| p.name == "Alice"));

    let bob = client::connect_tcp(&addr, "Bob", None).expect("bob connects");
    let (bob_id, _bob_token, _) = recv_welcome(&bob);
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
    assert_eq!(seen.buildings.len(), 3, "furnace + tunnel + tent");

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
        save_path: None,
        idle_shutdown: None,
        central: false,
        owner_account: None,
        invites: None,
        world_manager: None,
    })
    .expect("server starts");
    let port = handle.addr.expect("addr").port();
    let conn = client::connect_tcp(&format!("127.0.0.1:{port}"), "Solo", None).expect("connects");
    let (_, _, welcome_state) = recv_welcome(&conn);
    assert!(
        !welcome_state.tiles.is_empty(),
        "welcome always carries tiles"
    );

    let mut with_tiles = 0;
    let mut without_tiles = 0;
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline && (with_tiles == 0 || without_tiles == 0) {
        if let Ok(ServerMsg::State { state, included }) =
            conn.recv_timeout(Duration::from_millis(500))
        {
            if included.tiles {
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

/// The port also speaks WebSocket (for the browser build) — and both kinds of
/// client share the same authoritative world.
#[test]
fn websocket_and_tcp_clients_share_one_city() {
    use tungstenite::Message;

    let handle = server::start(ServerConfig {
        port: Some(0),
        seed: 99,
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
    .expect("server starts");
    let port = handle.addr.expect("addr").port();

    let (mut ws, _resp) =
        tungstenite::connect(format!("ws://127.0.0.1:{port}/")).expect("ws handshake");
    ws.send(Message::Binary(
        encode(&ClientMsg::Hello {
            name: "WebPlayer".into(),
            token: None,
        })
        .unwrap()
        .into(),
    ))
    .expect("send hello");

    // The first binary frame is the Welcome snapshot.
    let state = loop {
        match ws.read().expect("ws read") {
            Message::Binary(b) => match decode::<ServerMsg>(&b, MAX_FRAME as usize).expect("decode") {
                ServerMsg::Welcome { state, .. } => break state,
                _ => continue,
            },
            _ => continue,
        }
    };
    assert!(state.players.iter().any(|p| p.name == "WebPlayer"));

    // A native TCP client joins the same world and sees the web player.
    let tcp =
        client::connect_tcp(&format!("127.0.0.1:{port}"), "Native", None).expect("tcp connects");
    let _ = recv_welcome(&tcp);
    wait_state(&tcp, |s| s.players.len() == 2);

    // The web player builds a tent; the TCP client must see it appear.
    let spot = (0..64u8)
        .flat_map(|y| (0..64u8).map(move |x| (x, y)))
        .find(|&(x, y)| state.can_place(BuildingKind::Tent, x, y).is_ok())
        .expect("some valid tent spot");
    ws.send(Message::Binary(
        encode(&ClientMsg::Cmd(PlayerCommand::Place {
            kind: BuildingKind::Tent,
            x: spot.0,
            y: spot.1,
        }))
        .unwrap()
        .into(),
    ))
    .expect("send place");
    wait_state(&tcp, |s| {
        s.buildings.iter().any(|b| b.kind == BuildingKind::Tent)
    });

    // And the websocket keeps receiving authoritative snapshots.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut ws_saw_tent = false;
    while Instant::now() < deadline && !ws_saw_tent {
        if let Message::Binary(b) = ws.read().expect("ws read") {
            if let Ok(ServerMsg::State { state, .. }) = decode::<ServerMsg>(&b, MAX_FRAME as usize)
            {
                ws_saw_tent = state.buildings.iter().any(|b| b.kind == BuildingKind::Tent);
            }
        }
    }
    assert!(ws_saw_tent, "tent visible in websocket snapshots");

    handle.stop();
}

/// Plain HTTP GET on the same port serves the web build.
#[test]
fn http_get_serves_the_web_page() {
    use std::io::{Read, Write};

    let handle = server::start(ServerConfig {
        port: Some(0),
        seed: 1,
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
    .expect("server starts");
    let port = handle.addr.expect("addr").port();

    let mut sock = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connect");
    sock.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .expect("send request");
    let mut response = String::new();
    sock.read_to_string(&mut response).expect("read response");
    // Tests run from the crate root, where web/index.html exists.
    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "expected 200, got: {}",
        response.lines().next().unwrap_or("")
    );
    assert!(response.contains("FROZEN CITY"), "index.html served");

    handle.stop();
}
