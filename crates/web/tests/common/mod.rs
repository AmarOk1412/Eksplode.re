#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use eksplode_re_core::Config;
use eksplode_re_net::protocol::{ClientMsg, ServerMsg};
use eksplode_re_web::{Server, ServerOptions};
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

    pub fn send_value(&mut self, value: &serde_json::Value) {
        self.send_text(&serde_json::to_string(value).unwrap());
    }

    pub fn recv_value(&mut self) -> Option<serde_json::Value> {
        let deadline = Instant::now() + WAIT;
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            self.socket
                .get_mut()
                .set_read_timeout(Some(remaining))
                .ok()?;
            match self.socket.read() {
                Ok(Message::Text(text)) if text.len() <= eksplode_re_net::protocol::MAX_LINE => {
                    return serde_json::from_str(text.as_str()).ok();
                }
                Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {}
                _ => return None,
            }
        }
        None
    }

    pub fn hello_v2(&mut self) -> Option<serde_json::Value> {
        self.send_value(&serde_json::json!({"type": "Hello", "version": 2}));
        self.recv_value()
    }

    pub fn create_room_v2(&mut self, request_id: &str, name: &str) {
        self.send_value(&serde_json::json!({
            "type": "CreateRoom", "request_id": request_id, "name": name,
            "settings": {"name": name, "mode": "normal", "size": "small",
                "minutes": 3, "friendly_fire": true}, "capacity": 4
        }));
    }

    pub fn join_room_v2(&mut self, request_id: &str, room: &serde_json::Value, name: &str) {
        self.send_value(&serde_json::json!({
            "type": "JoinRoom", "request_id": request_id, "name": name,
            "room_id": room["room_id"], "server_epoch": room["server_epoch"]
        }));
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

pub struct RoomWs {
    transport: Ws,
    pub reference: eksplode_re_net::protocol::v2::RoomRef,
    initial: Option<ServerMsg>,
    request_sequence: u64,
}

impl RoomWs {
    pub fn create(addr: SocketAddr, name: &str) -> Self {
        Self::create_with_look(addr, name, None)
    }

    pub fn create_with_look(addr: SocketAddr, name: &str, look: Option<serde_json::Value>) -> Self {
        let mut transport = Ws::open(addr);
        let hello = transport.hello_v2().expect("Hello reply");
        assert_eq!(hello["type"], "Hello");
        transport.send_value(&serde_json::json!({
            "type":"CreateRoom", "request_id":"fixture-create", "name":name,
            "settings":hello["defaults"], "capacity":4, "look":look
        }));
        Self::admitted(transport)
    }

    pub fn join(
        addr: SocketAddr,
        reference: &eksplode_re_net::protocol::v2::RoomRef,
        name: &str,
    ) -> Self {
        Self::join_with_look(addr, reference, name, None)
    }

    pub fn join_with_look(
        addr: SocketAddr,
        reference: &eksplode_re_net::protocol::v2::RoomRef,
        name: &str,
        look: Option<serde_json::Value>,
    ) -> Self {
        let mut transport = Ws::open(addr);
        assert_eq!(transport.hello_v2().expect("Hello reply")["type"], "Hello");
        transport.send_value(&serde_json::json!({"type":"JoinRoom", "request_id":"fixture-join",
            "name":name, "room_id":reference.room_id, "server_epoch":reference.server_epoch, "look":look}));
        let joined = Self::admitted(transport);
        assert_eq!(&joined.reference, reference);
        joined
    }

    fn admitted(mut transport: Ws) -> Self {
        use eksplode_re_net::protocol::v2::Reply;
        let reply: Reply =
            serde_json::from_value(transport.recv_value().expect("admission reply")).unwrap();
        let reference = match &reply {
            Reply::Welcome { admission, .. } => admission.reference.clone(),
            _ => panic!("expected Welcome"),
        };
        Self {
            transport,
            reference,
            initial: Some(ServerMsg::RoomV2(Box::new(reply))),
            request_sequence: 0,
        }
    }

    pub fn send(&mut self, message: &ClientMsg) {
        assert!(!matches!(
            message,
            ClientMsg::Join { .. }
                | ClientMsg::RoomQuery
                | ClientMsg::SetRoom { .. }
                | ClientMsg::RoomV2(_)
        ));
        let mut value = serde_json::to_value(message).unwrap();
        value["room_id"] = serde_json::json!(self.reference.room_id);
        if matches!(message, ClientMsg::Leave) {
            value["type"] = serde_json::json!("LeaveRoom");
        }
        if !matches!(message, ClientMsg::Input { .. } | ClientMsg::Ping { .. }) {
            self.request_sequence = self.request_sequence.checked_add(1).unwrap();
            value["request_id"] = serde_json::json!(format!("fixture-{}", self.request_sequence));
        }
        self.transport.send_value(&value);
    }

    pub fn recv(&mut self) -> Option<ServerMsg> {
        self.initial
            .take()
            .or_else(|| self.transport.recv())
            .map(ServerMsg::into_game_message)
    }

    pub fn recv_until(&mut self, mut pred: impl FnMut(&ServerMsg) -> bool) -> Option<ServerMsg> {
        while let Some(message) = self.recv() {
            if pred(&message) {
                return Some(message);
            }
        }
        None
    }

    pub fn welcome_slot(&mut self) -> u8 {
        match self.recv_until(|message| matches!(message, ServerMsg::Welcome { .. })) {
            Some(ServerMsg::Welcome { slot, .. }) => slot,
            _ => panic!("expected Welcome"),
        }
    }
}

impl std::ops::Deref for RoomWs {
    type Target = Ws;

    fn deref(&self) -> &Ws {
        &self.transport
    }
}

impl std::ops::DerefMut for RoomWs {
    fn deref_mut(&mut self) -> &mut Ws {
        &mut self.transport
    }
}

/// Joins `n` clients one after another so slots follow the order.
pub fn join_room_ws(addr: SocketAddr, participants: usize) -> Vec<RoomWs> {
    assert!((1..=4).contains(&participants));
    let mut clients: Vec<RoomWs> = Vec::new();
    for index in 0..participants {
        let name = format!("w{index}");
        let mut client = match clients.first() {
            Some(owner) => RoomWs::join(addr, &owner.reference, &name),
            None => RoomWs::create(addr, &name),
        };
        assert_eq!(usize::from(client.welcome_slot()), index);
        clients.push(client);
    }
    clients
}
