#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eksplode_re_core::Config;
use eksplode_re_net::protocol::{ClientMsg, ServerMsg};
use eksplode_re_net::{Client, Host, HostConfig};

pub const WAIT: Duration = Duration::from_secs(5);

/// A fast host on an ephemeral loopback port.
pub fn host() -> Host {
    host_with(|_| {})
}

pub fn host_with(f: impl FnOnce(&mut HostConfig)) -> Host {
    let mut cfg = HostConfig {
        bind: "0.0.0.0:0".into(),
        tick: Duration::from_millis(4),
        seed: Some(11),
        ..HostConfig::default()
    };
    f(&mut cfg);
    Host::start(cfg).expect("host starts")
}

pub fn quick_game(limit: u32) -> Config {
    Config {
        round_limit_ticks: limit,
        ..Config::default()
    }
}

/// Reads messages until `pred` accepts one, or the timeout passes.
pub fn wait_for(c: &Client, mut pred: impl FnMut(&ServerMsg) -> bool) -> Option<ServerMsg> {
    let end = Instant::now() + WAIT;
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        match c.recv_timeout(left) {
            Ok(message) if pred(&message) => return Some(message),
            Ok(_) => {}
            Err(_) => return None,
        }
    }
    None
}

pub fn wait_for_game(c: &Client, mut pred: impl FnMut(&ServerMsg) -> bool) -> Option<ServerMsg> {
    wait_for(c, |message| pred(&message.clone().into_game_message()))
        .map(ServerMsg::into_game_message)
}

pub fn welcome_slot(c: &Client) -> u8 {
    match wait_for_game(c, |m| matches!(m, ServerMsg::Welcome { .. })) {
        Some(ServerMsg::Welcome { slot, .. }) => slot,
        other => panic!("expected Welcome, got {other:?}"),
    }
}

/// A bare socket for sending things a real client never would.
pub struct Raw {
    pub stream: TcpStream,
    reader: BufReader<TcpStream>,
}

impl Raw {
    pub fn connect(addr: SocketAddr) -> Raw {
        let stream = TcpStream::connect(addr).unwrap();
        stream.set_read_timeout(Some(WAIT)).unwrap();
        let reader = BufReader::new(stream.try_clone().unwrap());
        Raw { stream, reader }
    }

    pub fn send_line(&mut self, line: &str) {
        self.stream.write_all(line.as_bytes()).unwrap();
        self.stream.write_all(b"\n").unwrap();
    }

    pub fn send(&mut self, msg: &ClientMsg) {
        self.send_line(&serde_json::to_string(msg).unwrap());
    }

    pub fn send_value(&mut self, value: &serde_json::Value) {
        self.send_line(&serde_json::to_string(value).unwrap());
    }

    pub fn recv_value(&mut self) -> Option<serde_json::Value> {
        let line = eksplode_re_net::codec::LineReader::new(&mut self.reader)
            .read_line()
            .ok()??;
        serde_json::from_slice(&line).ok()
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

    /// Next server message, `None` when the connection was closed.
    pub fn recv(&mut self) -> Option<ServerMsg> {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(serde_json::from_str(line.trim_end()).expect("valid server message")),
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
}

pub fn join_room_all(
    host: &Host,
    participants: usize,
) -> (Vec<Client>, eksplode_re_net::protocol::v2::RoomRef) {
    assert!((1..=4).contains(&participants));
    let owner = Client::connect_unjoined(&host.addr().to_string()).unwrap();
    let room = owner
        .create_room("p0", &eksplode_re_core::RoomSettings::default(), 4)
        .unwrap();
    assert_eq!(welcome_slot(&owner), 0);
    let mut clients = vec![owner];
    for index in 1..participants {
        let client = Client::connect_unjoined(&host.addr().to_string()).unwrap();
        client.join_room(&room, &format!("p{index}")).unwrap();
        assert_eq!(usize::from(welcome_slot(&client)), index);
        clients.push(client);
    }
    (clients, room)
}

#[derive(Clone, Default)]
pub struct FakeClock(Arc<AtomicU64>);

impl FakeClock {
    pub fn now(&self) -> Duration {
        Duration::from_millis(self.0.load(Ordering::SeqCst))
    }

    pub fn advance(&self, millis: u64) {
        self.0.fetch_add(millis, Ordering::SeqCst);
    }
}

#[derive(Clone, Default)]
pub struct FakeCredentials {
    counter: Arc<AtomicU64>,
    failed: Arc<AtomicBool>,
}

impl FakeCredentials {
    pub fn fail(&self) {
        self.failed.store(true, Ordering::SeqCst);
    }

    pub fn fill(&self, output: &mut [u8]) -> Result<(), &'static str> {
        if self.failed.load(Ordering::SeqCst) {
            return Err("injected randomness failure");
        }
        let counter = self.counter.fetch_add(1, Ordering::SeqCst).to_le_bytes();
        for (index, byte) in output.iter_mut().enumerate() {
            *byte = counter[index % counter.len()];
        }
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct CapturedEvents(Arc<Mutex<Vec<serde_json::Value>>>);

impl CapturedEvents {
    pub fn push(&self, event: serde_json::Value) {
        self.0.lock().unwrap().push(event);
    }

    pub fn snapshot(&self) -> Vec<serde_json::Value> {
        self.0.lock().unwrap().clone()
    }
}
