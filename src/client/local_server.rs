//! Singleplayer on wasm: the browser has no threads, so the authoritative sim
//! runs as a Bevy system at the same fixed 5 Hz cadence and feeds the same
//! channel-shaped `ClientConn` the real server would. The rest of the client
//! cannot tell the difference.

use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Mutex;

use bevy::prelude::*;

use frozen_city::game::sim;
use frozen_city::game::types::{GameState, TICK_MS};
use frozen_city::net::client::ClientConn;
use frozen_city::net::protocol::{ClientMsg, Included, ServerMsg, TILES_EVERY_N_TICKS};

use super::{ServerRes, Screen};

const LOCAL_PLAYER_ID: u64 = 1;

pub struct LocalServer {
    state: GameState,
    from_client: Mutex<Receiver<ClientMsg>>,
    to_client: Sender<ServerMsg>,
    /// Real-time seconds not yet consumed by fixed 200 ms ticks.
    acc: f32,
}

/// Start a fresh single-player world and hand back the client's connection.
pub fn start(seed: u64, win_days: u32, name: &str) -> (LocalServer, ClientConn) {
    let (out_tx, out_rx) = channel::<ServerMsg>(); // sim -> client
    let (in_tx, in_rx) = channel::<ClientMsg>(); // client -> sim

    let mut state = sim::new_game(seed, win_days);
    sim::player_joined(&mut state, LOCAL_PLAYER_ID, name);
    // Singleplayer never reconnects, so the token can just mirror the id.
    let _ = out_tx.send(ServerMsg::Welcome {
        player_id: LOCAL_PLAYER_ID,
        token: LOCAL_PLAYER_ID,
        state: state.clone(),
    });

    (
        LocalServer {
            state,
            from_client: Mutex::new(in_rx),
            to_client: out_tx,
            acc: 0.0,
        },
        ClientConn::Channels {
            tx: in_tx,
            rx: out_rx,
        },
    )
}

/// Fixed-step driver, mirroring the native server's sim thread.
pub fn tick(mut server: ResMut<ServerRes>, time: Res<Time>) {
    let Some(srv) = server.0.as_mut() else { return };
    let step = TICK_MS as f32 / 1000.0;
    // After a suspended tab, catch up a little but never spiral.
    srv.acc = (srv.acc + time.delta_secs()).min(2.0);
    while srv.acc >= step {
        srv.acc -= step;

        if let Ok(rx) = srv.from_client.lock() {
            loop {
                match rx.try_recv() {
                    Ok(ClientMsg::Cmd(cmd)) => {
                        sim::apply_command(&mut srv.state, LOCAL_PLAYER_ID, &cmd)
                    }
                    Ok(ClientMsg::Cursor { x, y }) => {
                        sim::set_cursor(&mut srv.state, LOCAL_PLAYER_ID, x, y)
                    }
                    Ok(ClientMsg::Chat { text }) => {
                        sim::push_chat(&mut srv.state, LOCAL_PLAYER_ID, &text)
                    }
                    Ok(ClientMsg::Ping { x, y }) => {
                        sim::add_ping(&mut srv.state, LOCAL_PLAYER_ID, x, y)
                    }
                    // Singleplayer has no accounts (and no central world, no
                    // friends, no invites), so every account-flavored message
                    // (like `Hello`) is just ignored — the connection is
                    // already set up in `start()`.
                    Ok(
                        ClientMsg::Hello { .. }
                        | ClientMsg::Login { .. }
                        | ClientMsg::EnterCentral { .. }
                        | ClientMsg::Register { .. }
                        | ClientMsg::VisitFriend { .. }
                        | ClientMsg::ChatLocal { .. }
                        | ClientMsg::AddFriend { .. }
                        | ClientMsg::RemoveFriend { .. }
                        | ClientMsg::RefreshSocial
                        | ClientMsg::Invite { .. }
                        | ClientMsg::RefreshShowcase
                        | ClientMsg::SetVisitPolicy { .. },
                    ) => {}
                    // Singleplayer is a solo owner: nobody to kick.
                    Ok(ClientMsg::Kick { .. }) => {}
                    Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
                }
            }
        }

        sim::tick(&mut srv.state);

        // In-memory and free of network cost, so unlike the real server there's
        // no point tracking what changed — just send everything every tick
        // (tiles keep the same throttle cadence purely so both server
        // implementations agree on the tile/no-tile pattern, per the module doc).
        let include_tiles = srv.state.tick % TILES_EVERY_N_TICKS == 0;
        let mut wire = srv.state.clone();
        if !include_tiles {
            wire.tiles = Vec::new();
        }
        let _ = srv.to_client.send(ServerMsg::State {
            state: wire,
            included: Included {
                tiles: include_tiles,
                events: true,
                chat: true,
                pings: true,
                missions: true,
                techs: true,
            },
        });
    }
}

pub fn plugin(app: &mut App) {
    // The world itself is dropped by `teardown_game` (ServerRes -> None).
    app.add_systems(
        Update,
        tick.before(super::net_sync::pump_net)
            .run_if(in_state(Screen::Game)),
    );
}
