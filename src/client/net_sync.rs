//! Receives server snapshots and mirrors them into [`GameView`]. Also drives
//! transparent reconnection: a dropped `Join` connection is re-dialed with the
//! saved session token so the player resumes the same identity. On native the
//! dial runs on a background thread so the app never freezes while connecting.

use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Mutex;

use bevy::prelude::*;

use frozen_city::net::client::ClientConn;
use frozen_city::net::protocol::{ClientMsg, ServerMsg};

use super::{GameView, NetConn, Screen, Session};

/// Holds an in-flight background reconnect dial (native). `None` when idle.
#[derive(Resource, Default)]
pub struct Reconnecting {
    pending: Option<Mutex<Receiver<Option<ClientConn>>>>,
}

pub fn pump_net(
    net: Res<NetConn>,
    mut view: ResMut<GameView>,
    mut session: ResMut<Session>,
    mut next: ResMut<NextState<Screen>>,
) {
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
            ServerMsg::Welcome {
                player_id,
                token,
                state,
            } => {
                view.player_id = Some(player_id);
                view.token = Some(token);
                session.token = Some(token);
                // A successful (re)connect resets the outage attempt counter.
                session.attempts = 0;
                view.tiles = state.tiles.clone();
                view.tiles_version += 1;
                view.state = Some(state);
                view.version += 1;
            }
            ServerMsg::State { mut state, included } => {
                if included.tiles {
                    view.tiles = state.tiles.clone();
                    view.tiles_version += 1;
                } else {
                    state.tiles = view.tiles.clone();
                }
                // Collections the server left empty because they haven't
                // changed since the last snapshot it sent us — carry over
                // whatever we already have instead of wiping them out.
                if let Some(prev) = &view.state {
                    if !included.events {
                        state.events = prev.events.clone();
                    }
                    if !included.chat {
                        state.chat = prev.chat.clone();
                    }
                    if !included.pings {
                        state.pings = prev.pings.clone();
                    }
                    if !included.missions {
                        state.missions = prev.missions.clone();
                    }
                    if !included.techs {
                        state.techs = prev.techs.clone();
                    }
                }
                view.state = Some(state);
                view.version += 1;
            }
            ServerMsg::AuthFailed { reason } => {
                // Unlike a generic drop, a bad login/password should not
                // auto-retry — go straight back to the menu with the reason
                // shown. `OnExit(Screen::Game) -> teardown_game` (registered
                // in `ClientPlugin`) closes `net` and clears per-run state;
                // it preserves `view.error`, so the message survives.
                view.error = Some(reason);
                next.set(Screen::Menu);
            }
        }
    }
}

/// Only `Join` games are worth re-dialing, and only a bounded number of times
/// so a truly dead server still returns the player to the menu promptly.
const MAX_RECONNECT_ATTEMPTS: u32 = 3;

pub fn watch_disconnect(
    mut view: ResMut<GameView>,
    mut net: ResMut<NetConn>,
    mut session: ResMut<Session>,
    mut reconnecting: ResMut<Reconnecting>,
    mut next: ResMut<NextState<Screen>>,
) {
    // 1) Poll an in-flight (native) reconnect dial without blocking.
    let outcome: Option<Option<ClientConn>> = reconnecting.pending.as_ref().and_then(|m| {
        m.lock().ok().and_then(|rx| match rx.try_recv() {
            Ok(conn) => Some(conn),                  // completed: Some=ok, None=failed
            Err(TryRecvError::Empty) => None,        // still dialing
            Err(TryRecvError::Disconnected) => Some(None), // dial thread gone
        })
    });
    match outcome {
        Some(Some(conn)) => {
            reconnecting.pending = None;
            net.0 = Some(Mutex::new(conn));
            view.disconnected = false;
            return;
        }
        Some(None) => {
            reconnecting.pending = None;
            // Dial failed; fall through and either retry or give up below.
        }
        None => {
            if reconnecting.pending.is_some() {
                return; // still dialing — keep the world on screen
            }
        }
    }

    if !view.disconnected {
        return;
    }
    if session.reconnectable
        && session.token.is_some()
        && session.attempts < MAX_RECONNECT_ATTEMPTS
    {
        session.attempts += 1;
        if start_reconnect(&session, &mut net, &mut view, &mut reconnecting) {
            return;
        }
    }
    view.error = Some("Connection to the server was lost.".to_string());
    next.set(Screen::Menu);
}

/// The first message to redial with: `Login` (with the same credentials the
/// session originally signed in with) if this was an account session,
/// otherwise a guest `Hello`. Either way carries the last known session
/// token so a successful redial resumes the same player identity.
fn reconnect_hello(session: &Session) -> ClientMsg {
    match &session.auth {
        Some(auth) => ClientMsg::Login {
            login: auth.login.clone(),
            password: auth.password.clone(),
            token: session.token,
        },
        None => ClientMsg::Hello {
            name: session.name.clone(),
            token: session.token,
        },
    }
}

/// Kick off a reconnect. Native: spawn a background dial (returns true, result
/// polled next frames). Wasm: `ws::connect` is already non-blocking, so install
/// immediately. Returns true if a reconnect is now in progress or established.
#[cfg(not(target_arch = "wasm32"))]
fn start_reconnect(
    session: &Session,
    _net: &mut NetConn,
    _view: &mut GameView,
    reconnecting: &mut Reconnecting,
) -> bool {
    let (tx, rx) = std::sync::mpsc::channel::<Option<ClientConn>>();
    let addr = session.join_addr.clone();
    let hello = reconnect_hello(session);
    let spawned = std::thread::Builder::new()
        .name("fc-reconnect".into())
        .spawn(move || {
            let conn = frozen_city::net::client::connect_tcp_with(&addr, hello).ok();
            let _ = tx.send(conn);
        })
        .is_ok();
    if spawned {
        reconnecting.pending = Some(Mutex::new(rx));
    }
    spawned
}

#[cfg(target_arch = "wasm32")]
fn start_reconnect(
    session: &Session,
    net: &mut NetConn,
    view: &mut GameView,
    _reconnecting: &mut Reconnecting,
) -> bool {
    let url = if session.join_addr.starts_with("ws://") || session.join_addr.starts_with("wss://") {
        session.join_addr.clone()
    } else {
        format!("ws://{}", session.join_addr)
    };
    match frozen_city::net::ws::connect_with(&url, reconnect_hello(session)) {
        Ok(conn) => {
            net.0 = Some(Mutex::new(conn));
            view.disconnected = false;
            true
        }
        Err(_) => false,
    }
}
