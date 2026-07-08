//! Client-side connection: a pair of mpsc channels, backed either by a TCP
//! socket (remote server) or wired directly into the in-process server.

use std::io;
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::Duration;

use crate::net::protocol::{read_frame, write_frame, ClientMsg, ServerMsg};

pub struct ClientConn {
    pub tx: Sender<ClientMsg>,
    pub rx: Receiver<ServerMsg>,
}

impl ClientConn {
    pub fn send(&self, msg: ClientMsg) {
        let _ = self.tx.send(msg);
    }

    /// Drain everything received since the last call. `Ok(msgs)` while the
    /// connection lives; `Err(())` once the server is gone.
    pub fn poll(&self) -> Result<Vec<ServerMsg>, ()> {
        let mut out = Vec::new();
        loop {
            match self.rx.try_recv() {
                Ok(m) => out.push(m),
                Err(TryRecvError::Empty) => return Ok(out),
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
}

/// Connect to a remote server, send Hello, and spawn reader/writer threads.
pub fn connect_tcp(addr: &str, name: &str) -> io::Result<ClientConn> {
    let sock_addr = addr
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "cannot resolve address"))?;
    let mut stream = TcpStream::connect_timeout(&sock_addr, Duration::from_secs(5))?;
    stream.set_nodelay(true).ok();

    write_frame(
        &mut stream,
        &ClientMsg::Hello {
            name: name.to_string(),
        },
    )?;

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

    Ok(ClientConn {
        tx: in_tx,
        rx: out_rx,
    })
}
