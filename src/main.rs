//! Frozen City — a cooperative survival city-builder.
//!
//! Modes:
//!   frozen_city                     -> menu (singleplayer / host / join)
//!   frozen_city --host [port]       -> host a co-op game and play
//!   frozen_city --join <ip:port>    -> join a friend's game
//!   frozen_city --server [port]     -> headless dedicated server
//! Options: --name <name>  --seed <n>  --days <n>  --smoke

mod client;

use std::sync::atomic::Ordering;

use bevy::prelude::*;
use bevy::window::{PresentMode, Window, WindowPlugin};

use client::{AutoAction, AutoStart, ClientPlugin, Settings, DEFAULT_PORT};
use frozen_city::game::types::DEFAULT_WIN_DAYS;
use frozen_city::net::server::{self, ServerConfig};

struct Cli {
    mode: Mode,
    name: String,
    seed: Option<u64>,
    win_days: u32,
    join_addr: String,
    host_port: u16,
    smoke: bool,
}

#[derive(PartialEq)]
enum Mode {
    Menu,
    Host,
    Join,
    Dedicated,
}

fn parse_cli() -> Cli {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cli = Cli {
        mode: Mode::Menu,
        name: whoami_default(),
        seed: None,
        win_days: DEFAULT_WIN_DAYS,
        join_addr: format!("127.0.0.1:{DEFAULT_PORT}"),
        host_port: DEFAULT_PORT,
        smoke: false,
    };
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        let next = args.get(i + 1).cloned();
        match arg {
            "--host" => {
                cli.mode = Mode::Host;
                if let Some(p) = next.as_deref().and_then(|s| s.parse::<u16>().ok()) {
                    cli.host_port = p;
                    i += 1;
                }
            }
            "--server" => {
                cli.mode = Mode::Dedicated;
                if let Some(p) = next.as_deref().and_then(|s| s.parse::<u16>().ok()) {
                    cli.host_port = p;
                    i += 1;
                }
            }
            "--join" => {
                cli.mode = Mode::Join;
                if let Some(a) = next {
                    let addr = if a.contains(':') {
                        a
                    } else {
                        format!("{a}:{DEFAULT_PORT}")
                    };
                    cli.join_addr = addr;
                    i += 1;
                } else {
                    eprintln!("--join needs an address (ip[:port])");
                    std::process::exit(2);
                }
            }
            "--name" => {
                if let Some(n) = next {
                    cli.name = n;
                    i += 1;
                }
            }
            "--seed" => {
                if let Some(s) = next.and_then(|s| s.parse::<u64>().ok()) {
                    cli.seed = Some(s);
                    i += 1;
                }
            }
            "--days" => {
                if let Some(d) = next.and_then(|s| s.parse::<u32>().ok()) {
                    cli.win_days = d.max(1);
                    i += 1;
                }
            }
            "--smoke" => cli.smoke = true,
            "--help" | "-h" => {
                println!(
                    "Frozen City\n\
                     \n\
                     Usage: frozen_city [options]\n\
                     --host [port]      host a co-op game (default port {DEFAULT_PORT})\n\
                     --join <ip[:port]> join a game\n\
                     --server [port]    headless dedicated server\n\
                     --name <name>      player name\n\
                     --seed <n>         map seed\n\
                     --days <n>         days to survive (default {DEFAULT_WIN_DAYS})\n\
                     --smoke            auto-exit after a few seconds (CI smoke test)"
                );
                std::process::exit(0);
            }
            _ => eprintln!("Unknown argument: {arg}"),
        }
        i += 1;
    }
    cli
}

fn whoami_default() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "Mayor".to_string())
}

fn main() {
    let cli = parse_cli();

    if cli.mode == Mode::Dedicated {
        run_dedicated(&cli);
        return;
    }

    let auto = match cli.mode {
        Mode::Host => Some(AutoAction::Host),
        Mode::Join => Some(AutoAction::Join),
        Mode::Menu if cli.smoke => Some(AutoAction::Single),
        _ => None,
    };

    App::new()
        .insert_resource(ClearColor(Color::srgb(0.035, 0.055, 0.095)))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Frozen City".to_string(),
                resolution: (1280, 720).into(),
                present_mode: PresentMode::AutoVsync,
                ..default()
            }),
            ..default()
        }))
        .insert_resource(Settings {
            name: cli.name,
            join_addr: cli.join_addr,
            host_port: cli.host_port,
            seed: cli.seed,
            win_days: cli.win_days,
            smoke: cli.smoke,
        })
        .insert_resource(AutoStart(auto))
        .add_plugins(ClientPlugin)
        .run();
}

fn run_dedicated(cli: &Cli) {
    let seed = cli.seed.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1)
    });
    let handle = match server::start(ServerConfig {
        port: Some(cli.host_port),
        seed,
        win_days: cli.win_days,
        persistent: true,
        verbose: true,
    }) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Failed to start server on port {}: {e}", cli.host_port);
            std::process::exit(1);
        }
    };
    println!(
        "Frozen City dedicated server listening on port {} (Ctrl+C to stop)",
        cli.host_port
    );
    while !handle.shutdown.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}
