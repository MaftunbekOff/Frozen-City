//! Frozen City — a cooperative survival city-builder.
//!
//! Modes:
//!   frozen_city                     -> menu (singleplayer / host / join)
//!   frozen_city --host [port]       -> host a co-op game and play
//!   frozen_city --join <ip:port>    -> join a friend's game
//!   frozen_city --server [port]     -> headless dedicated server
//! Options: --name <name>  --seed <n>  --days <n>  --smoke

mod client;

use bevy::prelude::*;
use bevy::window::{PresentMode, Window, WindowPlugin};

use client::{AutoAction, AutoStart, ClientPlugin, Settings, DEFAULT_PORT};
use frozen_city::game::types::DEFAULT_WIN_DAYS;

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
    #[allow(unused_mut)]
    let mut cli = parse_cli();
    #[cfg(target_arch = "wasm32")]
    apply_web_overrides(&mut cli);

    #[cfg(not(target_arch = "wasm32"))]
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
                // On the web, fill the browser window (no-op on native).
                fit_canvas_to_parent: true,
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

#[cfg(not(target_arch = "wasm32"))]
fn run_dedicated(cli: &Cli) {
    use frozen_city::net::server::{self, ServerConfig};

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
    println!("Browsers can play at http://<this-host>:{}/", cli.host_port);
    while !handle.shutdown.load(std::sync::atomic::Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

/// In the browser, read settings from the page URL instead of a CLI:
/// `?name=Aziz&server=ws://host:4595&seed=7&days=12&join` (bare `join`
/// connects immediately). The default server is the page's own host.
#[cfg(target_arch = "wasm32")]
fn apply_web_overrides(cli: &mut Cli) {
    let Some(win) = web_sys::window() else { return };
    let location = win.location();
    if let Ok(host) = location.host() {
        if !host.is_empty() {
            let scheme = match location.protocol().ok().as_deref() {
                Some("https:") => "wss",
                _ => "ws",
            };
            // The /ws path lets a reverse proxy route game traffic to the
            // server while serving the static files itself; the game server
            // accepts an upgrade on any path.
            cli.join_addr = format!("{scheme}://{host}/ws");
        }
    }
    let Ok(search) = location.search() else { return };
    for pair in search.trim_start_matches('?').split('&') {
        let mut it = pair.splitn(2, '=');
        let key = it.next().unwrap_or("");
        let val = it.next().unwrap_or("").replace('+', " ");
        match key {
            "server" if !val.is_empty() => cli.join_addr = val,
            "name" if !val.is_empty() => cli.name = val,
            "seed" => {
                if let Ok(s) = val.parse() {
                    cli.seed = Some(s);
                }
            }
            "days" => {
                if let Ok(d) = val.parse::<u32>() {
                    cli.win_days = d.max(1);
                }
            }
            "join" => cli.mode = Mode::Join,
            _ => {}
        }
    }
}
