//! Fuzz-style tests: random and mutated input must never panic the parsers or hurt the server.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::io::Cursor;

use bomberust_net::protocol::{ClientMsg, ServerMsg};
use bomberust_web::http::{parse_head, read_head};
use common::*;
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

const VALID_HEAD: &[u8] = b"GET /ws HTTP/1.1\r\nHost: 127.0.0.1:8080\r\nUpgrade: websocket\r\n\
Connection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nOrigin: http://127.0.0.1:8080\r\n\r\n";

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
                    ][rng.below(6)],
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
