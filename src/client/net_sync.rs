//! Receives server snapshots and mirrors them into [`GameView`].

use bevy::prelude::*;

use frozen_city::net::protocol::ServerMsg;

use super::{GameView, NetConn, Screen};

pub fn pump_net(net: Res<NetConn>, mut view: ResMut<GameView>) {
    let Some(conn) = &net.0 else { return };
    let msgs = {
        let Ok(conn) = conn.lock() else { return };
        match conn.poll() {
            Ok(msgs) => msgs,
            Err(()) => {
                view.disconnected = true;
                return;
            }
        }
    };
    for msg in msgs {
        match msg {
            ServerMsg::Welcome { player_id, state } => {
                view.player_id = Some(player_id);
                view.tiles = state.tiles.clone();
                view.tiles_version += 1;
                view.state = Some(state);
                view.version += 1;
            }
            ServerMsg::State {
                mut state,
                tiles_included,
            } => {
                if tiles_included {
                    view.tiles = state.tiles.clone();
                    view.tiles_version += 1;
                } else {
                    state.tiles = view.tiles.clone();
                }
                view.state = Some(state);
                view.version += 1;
            }
        }
    }
}

pub fn watch_disconnect(mut view: ResMut<GameView>, mut next: ResMut<NextState<Screen>>) {
    if view.disconnected {
        view.error = Some("Connection to the server was lost.".to_string());
        next.set(Screen::Menu);
    }
}
