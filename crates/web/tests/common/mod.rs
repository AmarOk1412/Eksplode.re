#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use bomberust_core::Config;
use bomberust_net::protocol::{ClientMsg, ServerMsg, PROTOCOL_VERSION};
use bomberust_web::{Server, ServerOptions};
use tungstenite::client::IntoClientRequest;
use tungstenite::{Message, WebSocket};

pub const WAIT: Duration = Duration::from_secs(5);

pub fn options() -> ServerOptions {
    ServerOptions {
        http_port: 0,
        game_port: 0,
        tick: Duration::from_millis(4),
        seed: Some(5),
        request_timeout: Duration::from_secs(1),
        game: Config {
            block_density: 0.0,
            round_limit_ticks: 100_000,
            ..Config::default()
        },
        ..ServerOptions::default()
    }
}

pub fn server(f: impl FnOnce(&mut ServerOptions)) -> Server {
    let mut o = options();
    f(&mut o);
    Server::start(o).expect("server starts")
}

pub struct Http {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Http {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Sends a raw request and reads the whole response (the server closes the connection).
pub fn http(addr: SocketAddr, raw: &str) -> Http {
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(WAIT)).unwrap();
    s.write_all(raw.as_bytes()).unwrap();
    parse_response(&mut s)
}

pub fn parse_response(s: &mut TcpStream) -> Http {
    let mut data = Vec::new();
    let _ = s.read_to_end(&mut data);
    let split = data
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("complete response head");
    let head = String::from_utf8_lossy(&data[..split]).to_string();
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse().ok())
        .expect("status line");
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    Http {
        status,
        headers,
        body: data[split + 4..].to_vec(),
    }
}

pub fn get(addr: SocketAddr, path: &str) -> Http {
    http(
        addr,
        &format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"),
    )
}

/// A WebSocket client speaking the game protocol in text frames.
pub struct Ws {
    pub socket: WebSocket<TcpStream>,
}

impl Ws {
    /// Connects to `/ws`; `Err(status)` when the server answers with an HTTP error.
    pub fn connect(addr: SocketAddr, origin: Option<&str>) -> Result<Ws, u16> {
        let stream = TcpStream::connect(addr).unwrap();
        stream.set_read_timeout(Some(WAIT)).unwrap();
        let mut request = format!("ws://{addr}/ws").into_client_request().unwrap();
        if let Some(o) = origin {
            request.headers_mut().insert("Origin", o.parse().unwrap());
        }
        match tungstenite::client(request, stream) {
            Ok((socket, _)) => Ok(Ws { socket }),
            Err(tungstenite::HandshakeError::Failure(tungstenite::Error::Http(resp))) => {
                Err(resp.status().as_u16())
            }
            Err(e) => panic!("handshake failed: {e}"),
        }
    }

    pub fn open(addr: SocketAddr) -> Ws {
        Ws::connect(addr, None).expect("handshake accepted")
    }

    pub fn send(&mut self, msg: &ClientMsg) {
        self.send_text(&serde_json::to_string(msg).unwrap());
    }

    pub fn send_text(&mut self, text: &str) {
        self.socket.send(Message::text(text)).unwrap();
    }

    pub fn join(&mut self, name: &str) {
        self.send(&ClientMsg::Join {
            version: PROTOCOL_VERSION,
            name: name.into(),
            look: None,
        });
    }

    /// Joins with a look given as loose JSON.
    pub fn join_with_look(&mut self, name: &str, look: serde_json::Value) {
        self.send(&ClientMsg::Join {
            version: PROTOCOL_VERSION,
            name: name.into(),
            look: Some(look),
        });
    }

    /// Next server message; `None` when the socket closed or timed out.
    pub fn recv(&mut self) -> Option<ServerMsg> {
        loop {
            match self.socket.read() {
                Ok(Message::Text(t)) => return serde_json::from_str(t.as_str()).ok(),
                Ok(Message::Close(_)) | Err(_) => return None,
                Ok(_) => {}
            }
        }
    }

    pub fn recv_until(&mut self, mut pred: impl FnMut(&ServerMsg) -> bool) -> Option<ServerMsg> {
        while let Some(m) = self.recv() {
            if pred(&m) {
                return Some(m);
            }
        }
        None
    }

    pub fn welcome_slot(&mut self) -> u8 {
        match self.recv_until(|m| matches!(m, ServerMsg::Welcome { .. })) {
            Some(ServerMsg::Welcome { slot, .. }) => slot,
            other => panic!("expected Welcome, got {other:?}"),
        }
    }

    /// The close code of the next close frame, if the server closes the socket.
    pub fn close_code(&mut self) -> Option<u16> {
        loop {
            match self.socket.read() {
                Ok(Message::Close(Some(frame))) => return Some(u16::from(frame.code)),
                Ok(Message::Close(None)) => return Some(1005),
                Err(_) => return None,
                Ok(_) => {}
            }
        }
    }
}

/// Joins `n` clients one after another so slots follow the order.
pub fn join_ws(addr: SocketAddr, n: usize) -> Vec<Ws> {
    (0..n)
        .map(|i| {
            let mut ws = Ws::open(addr);
            ws.join(&format!("w{i}"));
            assert_eq!(usize::from(ws.welcome_slot()), i);
            ws
        })
        .collect()
}
