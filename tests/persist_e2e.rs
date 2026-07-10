//! End-to-end test for world persistence: a real persistent-mode server,
//! a real client placing a real building, a real shutdown (`stop()` +
//! `join()`, the same path `run_dedicated`'s SIGTERM handler takes), and a
//! second server instance picking the save back up — same harness style as
//! `coop_e2e.rs` but exercising the disk round trip instead of the wire.
//!
//! Deliberately a single `#[test]`: it sets `FC_WORLD_SAVE`, a process-wide
//! env var `net::persist` reads, and cargo runs `#[test]` fns in one binary
//! on separate threads by default — a second test setting its own path here
//! would race this one (same reasoning as `account_login_e2e.rs`).

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

fn some_tent_spot(state: &GameState) -> (u8, u8) {
    (0..64u8)
        .flat_map(|y| (0..64u8).map(move |x| (x, y)))
        .find(|&(x, y)| state.can_place(BuildingKind::Tent, x, y).is_ok())
        .expect("some valid tent spot")
}

fn start_persistent(seed: u64) -> server::ServerHandle {
    server::start(ServerConfig {
        port: Some(0), // ephemeral port
        seed,
        win_days: 12,
        persistent: true,
        verbose: false,
        save_path: None,
        idle_shutdown: None,
    })
    .expect("server starts")
}

fn addr_of(handle: &server::ServerHandle) -> String {
    let port = handle.addr.expect("bound addr").port();
    format!("127.0.0.1:{port}")
}

#[test]
fn a_built_tent_survives_a_server_restart() {
    let save_path = std::env::temp_dir().join("fc-persist-e2e-world.bin");
    std::fs::remove_file(&save_path).ok();
    std::env::set_var("FC_WORLD_SAVE", &save_path);

    let handle = start_persistent(9001);
    let addr = addr_of(&handle);

    let alice = client::connect_tcp(&addr, "Alice", None).expect("alice connects");
    let (alice_id, _token, alice_state) = recv_welcome(&alice);

    let spot = some_tent_spot(&alice_state);
    alice.send(ClientMsg::Cmd(PlayerCommand::Place {
        kind: BuildingKind::Tent,
        x: spot.0,
        y: spot.1,
    }));
    wait_state(&alice, |s| {
        s.buildings
            .iter()
            .any(|b| b.kind == BuildingKind::Tent && b.owner == Some(alice_id))
    });

    // A real shutdown, same path the dedicated server's SIGTERM handler
    // takes: flip the flag, then block until the sim thread has actually
    // exited (and, since `persistent: true`, written its final save).
    handle.stop();
    handle.join();

    assert!(save_path.exists(), "shutdown should have written a save file");

    // A fresh process would call `run_dedicated` -> `server::start`, which
    // now loads this same file instead of `sim::new_game`.
    let handle2 = start_persistent(4242); // different seed: proves it's not reseeding
    let addr2 = addr_of(&handle2);
    let bob = client::connect_tcp(&addr2, "Bob", None).expect("bob connects");
    let (_bob_id, _token2, bob_state) = recv_welcome(&bob);

    assert!(
        bob_state
            .buildings
            .iter()
            .any(|b| b.kind == BuildingKind::Tent && b.owner == Some(alice_id)),
        "the tent built before shutdown should still be there after restart: {:?}",
        bob_state.buildings
    );

    handle2.stop();
    handle2.join();
    std::fs::remove_file(&save_path).ok();
    std::env::remove_var("FC_WORLD_SAVE");
}
