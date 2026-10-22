#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use bomberust_core::Config;
use bomberust_net::protocol::{ClientMsg, ServerMsg};
use bomberust_net::{Client, Host, HostConfig};

pub const WAIT: Duration = Duration::from_secs(5);

/// A fast host on an ephemeral loopback port.
pub fn host() -> Host {
    host_with(|_| {})
}

pub fn host_with(f: impl FnOnce(&mut HostConfig)) -> Host {
    let mut cfg = HostConfig {
        bind: "127.0.0.1:0".into(),
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

pub fn connect(addr: SocketAddr, name: &str) -> Client {
    Client::connect(&addr.to_string(), name).expect("connects")
}

/// Reads messages until `pred` accepts one, or the timeout passes.
pub fn wait_for(c: &Client, mut pred: impl FnMut(&ServerMsg) -> bool) -> Option<ServerMsg> {
    let end = Instant::now() + WAIT;
    while let Some(left) = end.checked_duration_since(Instant::now()) {
        match c.recv_timeout(left) {
            Ok(m) if pred(&m) => return Some(m),
            Ok(_) => {}
            Err(_) => return None,
        }
    }
    None
}

pub fn welcome_slot(c: &Client) -> u8 {
    match wait_for(c, |m| matches!(m, ServerMsg::Welcome { .. })) {
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

/// Connects `n` clients one after another so slots follow the order.
pub fn join_all(host: &Host, n: usize) -> Vec<Client> {
    (0..n)
        .map(|i| {
            let c = connect(host.addr(), &format!("p{i}"));
            assert_eq!(usize::from(welcome_slot(&c)), i);
            c
        })
        .collect()
}
