use super::*;

use std::io;
use std::net::{Shutdown, TcpStream};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::protocol::{read_frame, write_frame, ClientMsg, ServerMsg};

/// The native desktop protocol: length-prefixed bincode frames.
pub(crate) fn handle_native(
    mut stream: TcpStream,
    to_server: Sender<ToServer>,
    world_manager: Option<Arc<crate::world_manager::WorldManager>>,
) {
    // The very first frame must be Hello, Login or EnterCentral (the 10 s
    // timeout is already set). `target` is whichever world this connection
    // ends up in — the shared world for a guest `Hello`, or (when
    // authenticated and a `world_manager` is wired up) that account's own
    // world or the central one — and is reused for every message this
    // connection sends afterward, not just the initial join.
    let (target, joined) = match read_frame::<_, ClientMsg>(&mut stream) {
        Ok(msg) => match route_first_msg(msg, &to_server, &world_manager) {
            FirstMsgOutcome::Joined(target, joined) => (target, joined),
            FirstMsgOutcome::Refused(reason) => {
                let _ = write_frame(
                    &mut stream,
                    &ServerMsg::AuthFailed {
                        reason: reason.to_string(),
                    },
                );
                return;
            }
            FirstMsgOutcome::Drop => return,
        },
        Err(_) => return,
    };
    let _ = stream.set_read_timeout(None);

    let Some((id, out_rx)) = joined else {
        return;
    };

    // Writer thread: serialize server messages onto the socket. When the
    // server drops this client's sender, shut the socket down so the blocking
    // reader below unblocks and cleans up.
    let Ok(write_stream) = stream.try_clone() else {
        let _ = target.send(ToServer::Leave { client: id });
        return;
    };
    // A stalled client (e.g. suspended tab, dead NAT) would otherwise let its
    // outbound channel grow forever since the writer blocks indefinitely.
    // Bound the write so a stuck send errors out, the writer thread exits and
    // drops the receiver, and the sim thread reaps the client.
    let _ = write_stream.set_write_timeout(Some(Duration::from_secs(30)));
    thread::Builder::new()
        .name("fc-conn-writer".into())
        .spawn(move || {
            let mut w = io::BufWriter::new(write_stream);
            for msg in out_rx {
                if write_frame(&mut w, &msg).is_err() {
                    break;
                }
            }
            if let Ok(s) = w.into_inner() {
                let _ = s.shutdown(Shutdown::Both);
            }
        })
        .ok();

    while let Ok(msg) = read_frame::<_, ClientMsg>(&mut stream) {
        if target.send(ToServer::Msg { client: id, msg }).is_err() {
            break;
        }
    }
    let _ = target.send(ToServer::Leave { client: id });
}
