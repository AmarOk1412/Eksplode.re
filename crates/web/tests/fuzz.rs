//! Fuzz-style tests: random and mutated input must never panic the parsers or hurt the server.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::io::Cursor;

use common::*;
use eksplode_re_net::protocol::{ClientMsg, ServerMsg};
use eksplode_re_web::http::{parse_head, read_head};
use tungstenite::Message;

/// A small deterministic xorshift generator so failures are reproducible.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// Between 0 and `max - 1` random bytes.
    fn bytes(&mut self, max: usize) -> Vec<u8> {
        let len = self.below(max);
        (0..len).map(|_| (self.next() & 0xff) as u8).collect()
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const VALID_HEAD: &[u8] = b"GET /ws HTTP/1.1\r\nHost: 0.0.0.0:8080\r\nUpgrade: websocket\r\n\
Connection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nOrigin: http://0.0.0.0:8080\r\n\r\n";

#[test]
fn the_http_head_parser_survives_random_and_mutated_input() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    for _ in 0..3000 {
        // pure noise, with and without a terminating blank line
        let mut noise = rng.bytes(600);
        let _ = parse_head(&noise);
        noise.extend_from_slice(b"\r\n\r\n");
        let _ = parse_head(&noise);
        let _ = read_head(&mut Cursor::new(noise));

        // a valid head with a few bytes changed, dropped or duplicated
        let mut head = VALID_HEAD.to_vec();
        for _ in 0..=rng.below(6) {
            let i = rng.below(head.len());
            match rng.below(3) {
                0 => head[i] = (rng.next() & 0xff) as u8,
                1 => {
                    head.remove(i);
                }
                _ => head.insert(i, head[i]),
            }
        }
        let _ = parse_head(&head);
        let _ = read_head(&mut Cursor::new(head));
    }
}

#[test]
fn oversized_and_truncated_heads_are_errors_not_panics() {
    assert!(read_head(&mut Cursor::new(vec![b'a'; 100_000])).is_err());
    assert!(read_head(&mut Cursor::new(Vec::<u8>::new())).is_err());
    assert!(read_head(&mut Cursor::new(b"GET / HTTP/1.1\r\n".to_vec())).is_err());
    assert!(parse_head(b"").is_none());
    assert!(parse_head(b"\r\n\r\n").is_none());
}

#[test]
fn the_websocket_bridge_survives_random_frames_and_stays_fair() {
    let s = server(|_| {});
    let mut rng = Rng(0xdead_beef_cafe_f00d);

    let mut good = Ws::open(s.http_addr());
    good.join("Ann");
    good.welcome_slot();

    for round in 0..4 {
        let mut bad = Ws::open(s.http_addr());
        bad.join(&format!("Mal{round}"));
        bad.welcome_slot();
        // the connection closes after ten bad messages; sending more must not hurt anybody
        for _ in 0..40 {
            let frame = match rng.below(4) {
                0 => Message::binary(rng.bytes(200)),
                1 => Message::text(String::from_utf8_lossy(&rng.bytes(200)).to_string()),
                2 => Message::text(format!(
                    "{{\"type\":\"Input\",\"tick\":{},\"mv\":\"{}\",\"bomb\":{}}}",
                    rng.next(),
                    ["Up", "Left", "Sideways", ""][rng.below(4)],
                    rng.below(3)
                )),
                _ => Message::text(
                    [
                        "{",
                        "[]",
                        "null",
                        "{\"type\":null}",
                        "{\"type\":\"Join\"}",
                        "\u{0}",
                        "{\"type\":\"SetLook\",\"look\":{\"colour\":7}}",
                        "{\"type\":\"SetLook\"}",
                        "{\"type\":\"Input\",\"tick\":1,\"mv\":null,\"bomb\":false,\"punch\":\"yes\"}",
                        "{\"type\":\"DevSetDrops\",\"chances\":[1e999,-1,null]}",
                        "{\"type\":\"DevSetDrops\",\"chances\":\"all\"}",
                        "{\"type\":\"DevPauseBots\"}",
                        "{\"type\":\"Join\",\"version\":1,\"name\":\"F\",\"look\":[[]]}",
                        "{\"type\":\"RoomQuery\"}",
                        "{\"type\":\"SetRoom\",\"settings\":{\"mode\":9,\"minutes\":\"x\"}}",
                        "{\"type\":\"SetRoom\",\"settings\":[1]}",
                        "{\"type\":\"SetRoom\"}",
                        "{\"type\":\"Join\",\"version\":1,\"name\":\"G\",\"room\":{\"size\":[]}}",
                    ][rng.below(18)],
                ),
            };
            if bad.socket.send(frame).is_err() {
                break;
            }
        }
        drop(bad);
    }

    // everybody else is unaffected and the server still plays a match
    good.send(&ClientMsg::AddBot);
    good.send(&ClientMsg::Start);
    assert!(good
        .recv_until(|m| matches!(m, ServerMsg::Snapshot { .. }))
        .is_some());
    assert_eq!(get(s.http_addr(), "/").status, 200);
}

#[test]
fn f3_random_developer_frames_are_refused_or_ignored_and_never_hurt_the_server() {
    for dev in [false, true] {
        let s = server(|o| o.dev = dev);
        let mut rng = Rng(0x0dd5_eed5_0000_0001 ^ u64::from(dev));
        let mut owner = Ws::open(s.http_addr());
        owner.join("Ann");
        owner.welcome_slot();
        for _ in 0..200 {
            let chances: Vec<String> = (0..rng.below(10))
                .map(|_| match rng.below(5) {
                    0 => "null".to_string(),
                    1 => format!("{}", (rng.next() % 300) as f64 / 100.0 - 1.0),
                    2 => "\"x\"".to_string(),
                    3 => "1e400".to_string(),
                    _ => format!("{}", rng.below(2)),
                })
                .collect();
            let text = match rng.below(4) {
                0 => format!(
                    "{{\"type\":\"DevSetDrops\",\"chances\":[{}]}}",
                    chances.join(",")
                ),
                1 => format!(
                    "{{\"type\":\"DevPauseBots\",\"paused\":{}}}",
                    ["true", "false", "3", "null"][rng.below(4)]
                ),
                2 => "{\"type\":\"DevResetDrops\"}".to_string(),
                _ => String::from_utf8_lossy(&rng.bytes(60)).to_string(),
            };
            if owner.socket.send(Message::text(text)).is_err() {
                break; // refused too often: the host closed the connection
            }
        }
        // whatever happened, the owner (if still connected) and the server are fine
        let mut fresh = Ws::open(s.http_addr());
        fresh.join("Bob");
        assert!(fresh
            .recv_until(|m| matches!(m, ServerMsg::Welcome { .. } | ServerMsg::Rejected { .. }))
            .is_some());
        assert_eq!(get(s.http_addr(), "/").status, 200);
    }
}
