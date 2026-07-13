//! Playtest telemetry, end to end: a real sitting (join → build → leave) must
//! land a `session_start` and a `session_end` (carrying a progress snapshot)
//! in the JSONL file named by `FC_TELEMETRY_PATH`. This is the feature that
//! finally lets us measure real players instead of bots, so — like everything
//! else here — it ships with a test that exercises the whole path: the sim
//! thread's records, the off-thread writer, and the file on disk.

use std::path::Path;
use std::time::{Duration, Instant};

use frozen_city::game::types::{BuildingKind, GameState, PlayerCommand};
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

fn read_lines(path: &Path) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("each telemetry line is valid JSON"))
        .collect()
}

#[test]
fn a_session_is_recorded_from_join_to_leave() {
    // Aim telemetry at a fresh temp file BEFORE anything can lazily init the
    // process-wide sink (the first `record()` reads the env var once).
    let path = std::env::temp_dir().join(format!("fc_telemetry_{}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&path);
    std::env::set_var("FC_TELEMETRY_PATH", &path);

    let handle = server::start(ServerConfig {
        port: Some(0), // ephemeral
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
    let port = handle.addr.expect("bound addr").port();
    let addr = format!("127.0.0.1:{port}");

    let aziz = client::connect_tcp(&addr, "Aziz", None).expect("Aziz connects");
    let (_id, _tok, state) = recv_welcome(&aziz);

    // Build a tent so the leave snapshot shows genuine progress (furnace + tent).
    let spot = (0..64u8)
        .flat_map(|y| (0..64u8).map(move |x| (x, y)))
        .find(|&(x, y)| state.can_place(BuildingKind::Tent, x, y).is_ok())
        .expect("some valid tent spot");
    aziz.send(ClientMsg::Cmd(PlayerCommand::Place {
        kind: BuildingKind::Tent,
        x: spot.0,
        y: spot.1,
    }));
    wait_state(&aziz, |s| s.buildings.iter().any(|b| b.kind == BuildingKind::Tent));

    // Leaving fires the session_end record with its snapshot.
    drop(aziz);

    // The writer is off the tick thread; poll the file until both land.
    let deadline = Instant::now() + Duration::from_secs(10);
    let lines = loop {
        let lines = read_lines(&path);
        let has_start = lines.iter().any(|v| v["event"] == "session_start");
        let has_end = lines.iter().any(|v| v["event"] == "session_end");
        if (has_start && has_end) || Instant::now() >= deadline {
            break lines;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    handle.stop();

    let start = lines
        .iter()
        .find(|v| v["event"] == "session_start")
        .expect("a session_start was written");
    assert_eq!(start["name"], "Aziz");
    assert_eq!(start["world"], "shared_guest");
    assert_eq!(start["reconnect"], false);
    assert_eq!(start["account"], serde_json::Value::Null, "guest has no account");

    let end = lines
        .iter()
        .find(|v| v["event"] == "session_end")
        .expect("a session_end was written");
    assert_eq!(end["name"], "Aziz");
    assert_eq!(end["world"], "shared_guest");
    assert_eq!(end["phase"], "running");
    assert_eq!(end["graduated"], false);
    assert!(
        end["buildings"].as_u64().expect("buildings is a number") >= 2,
        "furnace + tent snapshotted, got {:?}",
        end["buildings"]
    );
    assert!(
        end["duration_s"].as_u64().is_some(),
        "session length recorded"
    );
    assert!(
        end["missions_total"].as_u64().expect("missions_total is a number") >= 1,
        "the personal/guest world seeds missions"
    );

    let _ = std::fs::remove_file(&path);
}
