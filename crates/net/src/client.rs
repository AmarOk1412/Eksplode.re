//! Client side: a connection to a host (TCP or in-process) seen as a message channel.

use std::io::{BufReader, Write};
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::codec::{decode, encode, LineReader, NetError};
use crate::protocol::{ClientMsg, ServerMsg, PROTOCOL_VERSION};

type SendFn = Box<dyn Fn(ClientMsg) -> Result<(), NetError> + Send>;

/// A joined (or joining) connection. Dropping it disconnects.
pub struct Client {
    send: SendFn,
    rx: Receiver<ServerMsg>,
    on_drop: Option<Box<dyn FnOnce() + Send>>,
}

impl Client {
    /// Connects over TCP and sends `Join`. Watch for `Welcome` or `Rejected` on the channel.
    pub fn connect(addr: &str, name: &str) -> Result<Client, NetError> {
        let stream = TcpStream::connect(addr)?;
        stream.set_nodelay(true)?;
        let (tx, rx) = mpsc::channel();

        let reader = stream.try_clone()?;
        thread::spawn(move || {
            let mut lines = LineReader::new(BufReader::new(reader));
            while let Ok(Some(line)) = lines.read_line() {
                if let Ok(msg) = decode::<ServerMsg>(&line) {
                    if tx.send(msg).is_err() {
                        break;
                    }
                }
            }
        });

        let closer = stream.try_clone()?;
        let writer = Arc::new(Mutex::new(stream));
        let send: SendFn = Box::new(move |msg| {
            let bytes = encode(&msg)?;
            let mut s = writer
                .lock()
                .map_err(|_| NetError::Protocol("writer poisoned".into()))?;
            s.write_all(&bytes)?;
            Ok(())
        });
        let client = Client {
            send,
            rx,
            on_drop: Some(Box::new(move || {
                let _ = closer.shutdown(std::net::Shutdown::Both);
            })),
        };
        client.send(ClientMsg::Join {
            version: PROTOCOL_VERSION,
            name: name.to_string(),
            look: None,
            room: None,
        })?;
        Ok(client)
    }

    pub(crate) fn from_parts(
        send: SendFn,
        rx: Receiver<ServerMsg>,
        on_drop: Box<dyn FnOnce() + Send>,
    ) -> Client {
        Client {
            send,
            rx,
            on_drop: Some(on_drop),
        }
    }

    /// Sends a message to the host.
    pub fn send(&self, msg: ClientMsg) -> Result<(), NetError> {
        (self.send)(msg)
    }

    /// Next message if one is waiting; `Disconnected` once the host is gone.
    pub fn try_recv(&self) -> Result<ServerMsg, TryRecvError> {
        self.rx.try_recv()
    }

    /// Waits up to `timeout` for the next message.
    pub fn recv_timeout(&self, timeout: Duration) -> Result<ServerMsg, RecvTimeoutError> {
        self.rx.recv_timeout(timeout)
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Some(f) = self.on_drop.take() {
            f();
        }
    }
}
