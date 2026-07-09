//! Client-side connection. One `ClientConn` API over three transports: a pair
//! of mpsc channels (in-process server or the TCP pump threads on native), or
//! a browser WebSocket on wasm.

use std::sync::mpsc::{Receiver, Sender, TryRecvError};

use crate::net::protocol::{ClientMsg, ServerMsg};

pub enum ClientConn {
    /// Channel pair, pumped by threads (TCP) or wired straight into the
    /// in-process server / local sim.
    Channels {
        tx: Sender<ClientMsg>,
        rx: Receiver<ServerMsg>,
    },
    /// Browser WebSocket. The socket itself lives in a thread-local registry
    /// (JS values are not `Send`; wasm is single-threaded anyway) so this enum
    /// stays `Send` for Bevy's resource requirements.
    #[cfg(target_arch = "wasm32")]
    WebSocket {
        slot: usize,
        rx: Receiver<ServerMsg>,
    },
}

impl ClientConn {
    pub fn send(&self, msg: ClientMsg) {
        match self {
            ClientConn::Channels { tx, .. } => {
                let _ = tx.send(msg);
            }
            #[cfg(target_arch = "wasm32")]
            ClientConn::WebSocket { slot, .. } => crate::net::ws::send(*slot, &msg),
        }
    }

    /// Drain everything received since the last call. `Ok(msgs)` while the
    /// connection lives; `Err(())` once the server is gone.
    pub fn poll(&self) -> Result<Vec<ServerMsg>, ()> {
        let (rx, closed) = match self {
            ClientConn::Channels { rx, .. } => (rx, false),
            #[cfg(target_arch = "wasm32")]
            ClientConn::WebSocket { slot, rx } => (rx, crate::net::ws::is_closed(*slot)),
        };
        let mut out = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(m) => out.push(m),
                Err(TryRecvError::Empty) => {
                    if closed && out.is_empty() {
                        return Err(());
                    }
                    return Ok(out);
                }
                Err(TryRecvError::Disconnected) => {
                    if out.is_empty() {
                        return Err(());
                    } else {
                        return Ok(out);
                    }
                }
            }
        }
    }

    /// Block until the next message or timeout (native tests and tools).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn recv_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Result<ServerMsg, std::sync::mpsc::RecvTimeoutError> {
        match self {
            ClientConn::Channels { rx, .. } => rx.recv_timeout(timeout),
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for ClientConn {
    fn drop(&mut self) {
        if let ClientConn::WebSocket { slot, .. } = self {
            crate::net::ws::close(*slot);
        }
    }
}

/// Connect to a remote server over raw TCP (native only) as a guest, send
/// Hello, and spawn reader/writer threads. Pass `token` from a previous
/// `Welcome` to resume the same player identity (reconnect); `None` for a
/// fresh join.
#[cfg(not(target_arch = "wasm32"))]
pub fn connect_tcp(addr: &str, name: &str, token: Option<u64>) -> std::io::Result<ClientConn> {
    connect_tcp_with(
        addr,
        ClientMsg::Hello {
            name: name.to_string(),
            token,
        },
    )
}

/// Same as `connect_tcp`, but the caller builds the first message itself —
/// used for account sign-in (`ClientMsg::Login`) as well as guest `Hello`.
#[cfg(not(target_arch = "wasm32"))]
pub fn connect_tcp_with(addr: &str, hello: ClientMsg) -> std::io::Result<ClientConn> {
    use std::io;
    use std::net::{Shutdown, TcpStream, ToSocketAddrs};
    use std::sync::mpsc::channel;
    use std::thread;
    use std::time::Duration;

    use crate::net::protocol::{read_frame, write_frame};

    let sock_addr = addr
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "cannot resolve address"))?;
    let mut stream = TcpStream::connect_timeout(&sock_addr, Duration::from_secs(5))?;
    stream.set_nodelay(true).ok();

    write_frame(&mut stream, &hello)?;

    let (in_tx, in_rx) = channel::<ClientMsg>(); // app -> socket
    let (out_tx, out_rx) = channel::<ServerMsg>(); // socket -> app

    let write_stream = stream.try_clone()?;
    thread::Builder::new()
        .name("fc-client-writer".into())
        .spawn(move || {
            let mut w = io::BufWriter::new(write_stream);
            for msg in in_rx {
                if write_frame(&mut w, &msg).is_err() {
                    break;
                }
            }
            if let Ok(s) = w.into_inner() {
                let _ = s.shutdown(Shutdown::Both);
            }
        })
        .expect("spawn client writer");

    thread::Builder::new()
        .name("fc-client-reader".into())
        .spawn(move || {
            loop {
                match read_frame::<_, ServerMsg>(&mut stream) {
                    Ok(msg) => {
                        if out_tx.send(msg).is_err() {
                            break;
                        }
                    }
                    Err(_) => break, // dropping out_tx signals disconnect
                }
            }
        })
        .expect("spawn client reader");

    Ok(ClientConn::Channels {
        tx: in_tx,
        rx: out_rx,
    })
}
