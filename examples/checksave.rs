//! Ops sanity tool: verify world save files load under the current format,
//! including the V1 → V2 migration path (`net::legacy`). Run against copies
//! of the production saves BEFORE a deploy that changes the save format —
//! `persist::load_at` collapses any decode failure to "start a fresh world",
//! which in production means a silently wiped city.
//!
//!   cargo run --release --example checksave -- /path/to/world.bin ...

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: checksave <world.bin> [more.bin ...]");
        std::process::exit(2);
    }
    let mut failed = false;
    for path in paths {
        match frozen_city::net::persist::load_at(&path) {
            Some(s) => println!(
                "OK   {path}: day {}, pop {}, buildings {}, players-known {}, graduated {}, central {}",
                s.day(),
                s.survivors.len(),
                s.buildings.len(),
                s.players.len(),
                s.graduated,
                s.central
            ),
            None => {
                println!("FAIL {path}: does not decode under the current format");
                failed = true;
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
}
