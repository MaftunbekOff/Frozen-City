//! Ops sanity tool: dial a running server over the native TCP protocol and
//! report how the very first message is answered — Welcome (with which world)
//! or AuthFailed (with which reason). Lets a deploy be verified from the
//! server itself, where the game port is reachable on localhost, without
//! driving browser UI.
//!
//!   cargo run --release --example wireprobe -- <addr> hello <name>
//!   cargo run --release --example wireprobe -- <addr> login <login> <password>
//!   cargo run --release --example wireprobe -- <addr> central <login> <password>

use std::time::Duration;

use frozen_city::net::client::{connect_tcp_with, ClientConn};
use frozen_city::net::protocol::{ClientMsg, ServerMsg};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage: wireprobe <addr> hello <name> | login <login> <pw> | central <login> <pw>";
    let (addr, mode) = match (args.first(), args.get(1)) {
        (Some(a), Some(m)) => (a.clone(), m.clone()),
        _ => {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    };
    let first = match (mode.as_str(), args.get(2), args.get(3)) {
        ("hello", Some(name), _) => ClientMsg::Hello {
            name: name.clone(),
            token: None,
        },
        ("login", Some(login), Some(pw)) => ClientMsg::Login {
            login: login.clone(),
            password: pw.clone(),
            token: None,
        },
        ("central", Some(login), Some(pw)) => ClientMsg::EnterCentral {
            login: login.clone(),
            password: pw.clone(),
            token: None,
        },
        _ => {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    };

    let conn: ClientConn = match connect_tcp_with(&addr, first) {
        Ok(c) => c,
        Err(e) => {
            println!("DIAL-FAIL {addr}: {e}");
            std::process::exit(1);
        }
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        match conn.recv_timeout(Duration::from_millis(500)) {
            Ok(ServerMsg::Welcome { player_id, state, .. }) => {
                println!(
                    "WELCOME player {player_id}: central {}, day {}, pop {}, my-account {:?}, owned-here {}",
                    state.central,
                    state.day(),
                    state.survivors.len(),
                    state.player(player_id).and_then(|p| p.account),
                    state
                        .player(player_id)
                        .and_then(|p| p.account)
                        .map(|a| state.owned_settlers(a))
                        .unwrap_or(0),
                );
                return;
            }
            Ok(ServerMsg::AuthFailed { reason }) => {
                println!("AUTH-FAILED: {reason}");
                return;
            }
            Ok(_) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(e) => {
                println!("CONN-DIED before an answer: {e:?}");
                std::process::exit(1);
            }
        }
    }
    println!("TIMEOUT: no answer within 10s");
    std::process::exit(1);
}
