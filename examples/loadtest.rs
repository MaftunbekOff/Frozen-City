//! Ramps up concurrent native-protocol connections against a running server
//! to find this box's realistic capacity (F5). Talks the real
//! wire protocol via `net::client::connect_tcp`, same as any native client —
//! no protocol reimplementation, no risk of testing something the game
//! doesn't actually do.
//!
//! Usage: cargo run --release --example loadtest -- <host:port> [max] [step] [hold_secs]
//! Defaults: max=130 step=20 hold_secs=5 (MAX_CONNECTIONS is 128, so the
//! default range crosses the cap to confirm the server rejects gracefully
//! rather than misbehaving).

use std::time::{Duration, Instant};

use frozen_city::net::client::connect_tcp;
use frozen_city::net::protocol::{ClientMsg, ServerMsg};

fn main() {
    let mut args = std::env::args().skip(1);
    let addr = args.next().unwrap_or_else(|| "127.0.0.1:4595".to_string());
    let max: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(130);
    let step: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(20);
    let hold_secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(5);

    println!("loadtest: addr={addr} max={max} step={step} hold_secs={hold_secs}");

    let mut conns = Vec::new();
    let mut welcomed = 0usize;
    let mut rejected = 0usize;

    let mut next_target = step;
    while next_target <= max + step {
        let target = next_target.min(max);
        let batch_start = Instant::now();
        while conns.len() < target {
            let name = format!("load-{}", conns.len());
            match connect_tcp(&addr, &name, None) {
                Ok(conn) => conns.push(Some(conn)),
                Err(e) => {
                    rejected += 1;
                    conns.push(None);
                    eprintln!("  connect #{} failed: {e}", conns.len() - 1);
                }
            }
        }
        let connect_elapsed = batch_start.elapsed();

        // Give each live connection a moment to receive its Welcome, then
        // drain and count. Also send a Cursor ping so the server does real
        // per-client work (not just idle sockets), matching a light-activity
        // player rather than a truly idle one.
        std::thread::sleep(Duration::from_millis(300));
        welcomed = 0;
        for c in conns.iter().flatten() {
            c.send(ClientMsg::Cursor { x: 0.0, y: 0.0 });
        }
        for c in conns.iter().flatten() {
            if let Ok(msgs) = c.poll() {
                if msgs.iter().any(|m| matches!(m, ServerMsg::Welcome { .. })) {
                    welcomed += 1;
                }
            }
        }

        println!(
            "step: attempted={} connected_ok={} welcomed={} rejected_total={} connect_time={:?}",
            conns.len(),
            conns.iter().filter(|c| c.is_some()).count(),
            welcomed,
            rejected,
            connect_elapsed,
        );

        if target >= max {
            break;
        }

        std::thread::sleep(Duration::from_secs(hold_secs));
        next_target += step;
    }

    println!(
        "holding {} connections for {hold_secs}s to observe steady-state tick broadcast...",
        conns.iter().filter(|c| c.is_some()).count()
    );
    std::thread::sleep(Duration::from_secs(hold_secs));

    let mut states_seen = 0usize;
    for c in conns.iter().flatten() {
        if let Ok(msgs) = c.poll() {
            states_seen += msgs
                .iter()
                .filter(|m| matches!(m, ServerMsg::State { .. }))
                .count();
        }
    }
    println!("final: welcomed={welcomed} rejected_total={rejected} state_msgs_drained_last_poll={states_seen}");
}
