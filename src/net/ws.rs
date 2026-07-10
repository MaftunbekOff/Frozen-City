//! Browser WebSocket transport (wasm only). Bridges `web_sys::WebSocket` into
//! the same channel-shaped `ClientConn` the rest of the client already speaks.
//!
//! JS values are not `Send`, so live sockets are kept in a thread-local
//! registry and `ClientConn::WebSocket` refers to its slot by index —
//! wasm32-unknown-unknown is single-threaded, so this is always the same
//! thread.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{channel, Sender};

use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{BinaryType, MessageEvent, WebSocket};

use crate::net::client::ClientConn;
use crate::net::protocol::{decode, encode, ClientMsg, ServerMsg, MAX_FRAME};

struct Slot {
    ws: WebSocket,
    shared: Rc<Shared>,
    /// Keeps the JS event callbacks alive for the life of the socket.
    _callbacks: Vec<Closure<dyn FnMut(MessageEvent)>>,
}

#[derive(Default)]
struct Shared {
    open: RefCell<bool>,
    closed: RefCell<bool>,
    /// Frames queued before the socket finished opening (Hello first).
    pending: RefCell<Vec<Vec<u8>>>,
}

thread_local! {
    static SOCKETS: RefCell<Vec<Option<Slot>>> = RefCell::new(Vec::new());
}

/// Open a WebSocket to `url` as a guest, queue the Hello frame, and return a
/// live connection. Snapshots arrive on the connection's receiver. Pass
/// `token` from a previous `Welcome` to reconnect as the same player.
pub fn connect(url: &str, name: &str, token: Option<u64>) -> Result<ClientConn, String> {
    connect_with(
        url,
        ClientMsg::Hello {
            name: name.to_string(),
            token,
        },
    )
}

/// Same as `connect`, but the caller builds the first message itself — used
/// for account sign-in (`ClientMsg::Login`) as well as guest `Hello`.
pub fn connect_with(url: &str, hello: ClientMsg) -> Result<ClientConn, String> {
    let ws = WebSocket::new(url).map_err(|_| format!("Invalid server address: {url}"))?;
    ws.set_binary_type(BinaryType::Arraybuffer);

    let (out_tx, out_rx) = channel::<ServerMsg>();
    let shared = Rc::new(Shared::default());
    let mut callbacks: Vec<Closure<dyn FnMut(MessageEvent)>> = Vec::new();

    // Incoming binary frame -> ServerMsg.
    let on_message = {
        let out_tx: Sender<ServerMsg> = out_tx;
        Closure::<dyn FnMut(MessageEvent)>::new(move |ev: MessageEvent| {
            if let Ok(buf) = ev.data().dyn_into::<js_sys::ArrayBuffer>() {
                let bytes = js_sys::Uint8Array::new(&buf).to_vec();
                if let Ok(msg) = decode::<ServerMsg>(&bytes, MAX_FRAME as usize) {
                    let _ = out_tx.send(msg);
                }
            }
        })
    };
    ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    callbacks.push(on_message);

    // Open: flush everything queued while connecting.
    let on_open = {
        let shared = shared.clone();
        let ws = ws.clone();
        Closure::<dyn FnMut(MessageEvent)>::new(move |_| {
            *shared.open.borrow_mut() = true;
            for frame in shared.pending.borrow_mut().drain(..) {
                let _ = ws.send_with_u8_array(&frame);
            }
        })
    };
    ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
    callbacks.push(on_open);

    // Close / error: mark dead so poll() reports the disconnect.
    for setter in [WebSocket::set_onclose, WebSocket::set_onerror] {
        let shared = shared.clone();
        let cb = Closure::<dyn FnMut(MessageEvent)>::new(move |_| {
            *shared.closed.borrow_mut() = true;
        });
        setter(&ws, Some(cb.as_ref().unchecked_ref()));
        callbacks.push(cb);
    }

    let hello = encode(&hello).map_err(|e| e.to_string())?;
    shared.pending.borrow_mut().push(hello);

    let slot = SOCKETS.with(|s| {
        let mut s = s.borrow_mut();
        let slot = Slot {
            ws,
            shared,
            _callbacks: callbacks,
        };
        if let Some(i) = s.iter().position(Option::is_none) {
            s[i] = Some(slot);
            i
        } else {
            s.push(Some(slot));
            s.len() - 1
        }
    });

    Ok(ClientConn::WebSocket { slot, rx: out_rx })
}

pub(crate) fn send(slot: usize, msg: &ClientMsg) {
    let Ok(bytes) = encode(msg) else {
        return;
    };
    SOCKETS.with(|s| {
        let s = s.borrow();
        let Some(Some(slot)) = s.get(slot) else {
            return;
        };
        if *slot.shared.closed.borrow() {
            return;
        }
        if *slot.shared.open.borrow() {
            let _ = slot.ws.send_with_u8_array(&bytes);
        } else {
            slot.shared.pending.borrow_mut().push(bytes);
        }
    });
}

pub(crate) fn is_closed(slot: usize) -> bool {
    SOCKETS.with(|s| {
        s.borrow()
            .get(slot)
            .and_then(Option::as_ref)
            .is_none_or(|slot| *slot.shared.closed.borrow())
    })
}

pub(crate) fn close(slot: usize) {
    SOCKETS.with(|s| {
        if let Some(entry) = s.borrow_mut().get_mut(slot) {
            if let Some(slot) = entry.take() {
                slot.ws.set_onmessage(None);
                slot.ws.set_onopen(None);
                slot.ws.set_onclose(None);
                slot.ws.set_onerror(None);
                let _ = slot.ws.close();
            }
        }
    });
}
