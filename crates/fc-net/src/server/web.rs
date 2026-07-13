use super::*;

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{Sender, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

use tungstenite::Message;

use crate::protocol::{decode, encode, ClientMsg, ServerMsg, MAX_FRAME};

/// Directory the built-in HTTP server serves the web build from.
const WEB_ROOT: &str = "web";

/// A browser said "GET ": read the request head, then either upgrade to a
/// WebSocket or serve the static web build.
pub(crate) fn handle_http(
    mut stream: TcpStream,
    to_server: Sender<ToServer>,
    world_manager: Option<Arc<crate::world_manager::WorldManager>>,
) {
    // Read byte-by-byte so nothing past the head is consumed (the bytes that
    // follow the upgrade response are WebSocket frames).
    let mut head = Vec::with_capacity(512);
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") && head.len() < 8192 {
        match stream.read(&mut byte) {
            Ok(1) => head.push(byte[0]),
            _ => return,
        }
    }
    let text = String::from_utf8_lossy(&head).to_string();
    let path = text
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_string();
    let lower = text.to_ascii_lowercase();
    let is_upgrade = lower.contains("upgrade: websocket");
    if is_upgrade {
        serve_websocket(stream, head, to_server, world_manager);
    } else {
        // Serve the precompressed .gz sibling when the client accepts gzip —
        // the assetless wasm is ~66 MB raw vs ~15 MB gzipped, which matters a
        // lot on mobile web.
        let accepts_gzip = lower
            .lines()
            .any(|l| l.starts_with("accept-encoding:") && l.contains("gzip"));
        serve_static(stream, &path, accepts_gzip);
    }
}

/// WebSocket clients get a single service thread (this one): reads use a
/// short timeout so queued snapshots can be written between frames.
fn serve_websocket(
    stream: TcpStream,
    head: Vec<u8>,
    to_server: Sender<ToServer>,
    world_manager: Option<Arc<crate::world_manager::WorldManager>>,
) {
    let prefixed = PrefixedStream {
        prefix: head,
        pos: 0,
        inner: stream,
    };
    // Cap WebSocket message/frame sizes at the same MAX_FRAME the native
    // length-prefixed protocol enforces (protocol.rs) — the default 64 MiB
    // tungstenite limit would otherwise let a browser client push far more
    // than a native client ever could.
    // WebSocketConfig is #[non_exhaustive], so it can't be built with a struct
    // literal from outside tungstenite — start from default and set the caps.
    let mut ws_config = tungstenite::protocol::WebSocketConfig::default();
    ws_config.max_message_size = Some(MAX_FRAME as usize);
    ws_config.max_frame_size = Some(MAX_FRAME as usize);
    let Ok(mut ws) = tungstenite::accept_with_config(prefixed, Some(ws_config)) else {
        return;
    };

    // First frame must be Hello, Login or EnterCentral (still under the 10 s
    // read timeout). `target` is whichever world this connection ends up in —
    // see `route_first_msg` — and is reused for every message this connection
    // sends afterward, not just the initial join.
    let (target, joined) = loop {
        match ws.read() {
            Ok(Message::Binary(b)) => match decode::<ClientMsg>(&b, MAX_FRAME as usize) {
                Ok(msg) => match route_first_msg(msg, &to_server, &world_manager) {
                    FirstMsgOutcome::Joined(target, joined) => break (target, joined),
                    FirstMsgOutcome::Refused(reason) => {
                        if let Ok(bytes) = encode(&ServerMsg::AuthFailed {
                            reason: reason.to_string(),
                        }) {
                            let _ = ws.send(Message::Binary(bytes.into()));
                        }
                        return;
                    }
                    FirstMsgOutcome::Drop => return,
                },
                Err(_) => return,
            },
            Ok(Message::Ping(_) | Message::Pong(_)) => continue,
            _ => return,
        }
    };

    let Some((id, out_rx)) = joined else {
        return;
    };
    let _ = ws
        .get_mut()
        .inner
        .set_read_timeout(Some(Duration::from_millis(20)));
    // Same write-timeout robustness fix as the native path: a stalled
    // WebSocket write should error out instead of letting the outbound queue
    // grow unbounded.
    let _ = ws
        .get_mut()
        .inner
        .set_write_timeout(Some(Duration::from_secs(30)));

    'session: loop {
        match ws.read() {
            Ok(Message::Binary(b)) => {
                if let Ok(msg) = decode::<ClientMsg>(&b, MAX_FRAME as usize) {
                    if target.send(ToServer::Msg { client: id, msg }).is_err() {
                        break 'session;
                    }
                }
            }
            Ok(Message::Close(_)) => break 'session,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if e.kind() == io::ErrorKind::WouldBlock
                    || e.kind() == io::ErrorKind::TimedOut => {}
            Err(_) => break 'session,
        }
        loop {
            match out_rx.try_recv() {
                Ok(msg) => {
                    let Ok(bytes) = encode(&msg) else {
                        break 'session;
                    };
                    if ws.send(Message::Binary(bytes.into())).is_err() {
                        break 'session;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break 'session,
            }
        }
    }
    let _ = ws.close(None);
    let _ = target.send(ToServer::Leave { client: id });
}

/// Minimal static file server for the web build (`web/` next to the binary).
fn serve_static(mut stream: TcpStream, path: &str, accepts_gzip: bool) {
    let rel = path.split('?').next().unwrap_or("/");
    let rel = if rel == "/" { "/index.html" } else { rel };
    let candidate = PathBuf::from(WEB_ROOT).join(rel.trim_start_matches('/'));
    // No path traversal: every component must be a plain name (no "..", no
    // roots), which keeps the resolved path inside WEB_ROOT.
    let safe = candidate
        .components()
        .all(|c| matches!(c, Component::Normal(_)));
    // Prefer a precompressed `.gz` sibling (produced by build-web.sh) when the
    // client accepts gzip — turns a 66 MB wasm transfer into ~15 MB.
    let gz = PathBuf::from(format!("{}.gz", candidate.to_string_lossy()));
    let (body, gzipped) = if safe && accepts_gzip {
        match std::fs::read(&gz) {
            Ok(b) => (Some(b), true),
            Err(_) => (std::fs::read(&candidate).ok(), false),
        }
    } else if safe {
        (std::fs::read(&candidate).ok(), false)
    } else {
        (None, false)
    };
    let response = match body {
        Some(body) => {
            let mime = content_type(&candidate);
            let enc = if gzipped { "Content-Encoding: gzip\r\n" } else { "" };
            // The wasm/js bundles are not content-hashed, so cache briefly
            // with revalidation rather than forever (avoids a stale build
            // after a redeploy against the server-authoritative wire
            // format). A plain string check, not `Path::starts_with` (which
            // compares whole components — "pkg-webgpu" would never match a
            // "pkg" prefix that way).
            let cache = if rel.trim_start_matches('/').starts_with("pkg-") {
                "Cache-Control: public, max-age=3600\r\n"
            } else {
                "Cache-Control: no-cache\r\n"
            };
            let mut r = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\n{enc}{cache}Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .into_bytes();
            r.extend_from_slice(&body);
            r
        }
        None => {
            let msg = "Frozen City server. Web build not found — run build-web.sh first.";
            format!(
                "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{msg}",
                msg.len()
            )
            .into_bytes()
        }
    };
    let _ = stream.write_all(&response);
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Both);
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}
