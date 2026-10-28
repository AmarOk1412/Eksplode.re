//! WebSocket endpoint: one browser tab is one in-process client of the match host.

use std::io::{self, Cursor, Read, Write};
use std::net::TcpStream;
use std::sync::atomic::Ordering;
use std::sync::mpsc::TryRecvError;
use std::time::Duration;

use eksplode_re_net::codec::decode_client;
use eksplode_re_net::protocol::{ServerMsg, MAX_LINE};
use eksplode_re_net::Client;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tungstenite::{Error, HandshakeError, Message, WebSocket};

use crate::http::Ctx;

/// Closes the connection after this many malformed messages (same rule as the TCP port).
const MAX_ERRORS: u32 = 10;
const POLL: Duration = Duration::from_millis(20);

/// The request head was already read from the socket; replay it for the handshake.
struct Replay {
    prefix: Cursor<Vec<u8>>,
    inner: TcpStream,
}

impl Read for Replay {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.prefix.read(buf)?;
        if n > 0 {
            return Ok(n);
        }
        self.inner.read(buf)
    }
}

impl Write for Replay {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

type Socket = WebSocket<Replay>;

/// Runs the WebSocket session until either side ends it.
pub fn run(stream: TcpStream, head: Vec<u8>, ctx: &Ctx) {
    let peer = stream.peer_addr().ok();
    let _ = stream.set_read_timeout(Some(POLL));
    let _ = stream.set_nodelay(true);
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_LINE))
        .max_frame_size(Some(MAX_LINE));
    let replay = Replay {
        prefix: Cursor::new(head),
        inner: stream,
    };
    let mut result = tungstenite::accept_with_config(replay, Some(config));
    let mut socket = loop {
        match result {
            Ok(ws) => break ws,
            Err(HandshakeError::Interrupted(mid)) => result = mid.handshake(),
            Err(HandshakeError::Failure(_)) => return,
        }
    };
    let connection = match peer {
        Some(peer) => ctx.host.connect_peer(false, peer),
        None => ctx.host.connect(false),
    };
    let Ok(client) = connection else {
        close(&mut socket, CloseCode::Away, "server stopping");
        return;
    };
    bridge(&mut socket, &client, ctx);
}

fn bridge(socket: &mut Socket, client: &Client, ctx: &Ctx) {
    let mut errors = 0;
    loop {
        match socket.read() {
            Ok(Message::Text(text)) => match decode_client(text.as_bytes()) {
                Ok(msg) => {
                    if client.send(msg).is_err() {
                        return close(socket, CloseCode::Away, "server stopping");
                    }
                }
                Err(e) => {
                    if !malformed(socket, &mut errors, &e.to_string()) {
                        return;
                    }
                }
            },
            Ok(Message::Binary(_)) => {
                if !malformed(socket, &mut errors, "send text frames with JSON") {
                    return;
                }
            }
            Ok(Message::Close(_)) => return,
            Ok(_) => {}
            Err(Error::Io(e)) if is_timeout(&e) => {}
            Err(Error::Capacity(_)) => return close(socket, CloseCode::Size, "message too big"),
            Err(_) => return,
        }

        loop {
            match client.try_recv() {
                Ok(msg) => {
                    if send(socket, &msg).is_err() {
                        return;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    return close(socket, CloseCode::Away, "the match ended");
                }
            }
        }
        if ctx.stop.load(Ordering::SeqCst) {
            return close(socket, CloseCode::Away, "server stopping");
        }
    }
}

fn is_timeout(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

/// Answers a bad message; returns false when the connection was closed for too many of them.
fn malformed(socket: &mut Socket, errors: &mut u32, why: &str) -> bool {
    *errors += 1;
    let _ = send(socket, &ServerMsg::error("Malformed", why));
    if *errors >= MAX_ERRORS {
        close(socket, CloseCode::Policy, "too many malformed messages");
        return false;
    }
    true
}

fn send(socket: &mut Socket, msg: &ServerMsg) -> Result<(), Error> {
    let json = serde_json::to_string(msg).map_err(|e| Error::Utf8(e.to_string()))?;
    match socket.send(Message::text(json)) {
        Err(Error::Io(e)) if is_timeout(&e) => Ok(()),
        other => other,
    }
}

fn close(socket: &mut Socket, code: CloseCode, reason: &str) {
    let frame = CloseFrame {
        code,
        reason: reason.to_string().into(),
    };
    let _ = socket.close(Some(frame));
    let _ = socket.flush();
}
