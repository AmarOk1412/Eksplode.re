#![allow(clippy::unwrap_used, clippy::expect_used)]

//! The real binary: a failed join ends with a one-line explanation and exit status 1.

use std::net::TcpListener;
use std::process::Command;

#[test]
fn us3_as3_joining_nobody_exits_with_status_1_and_a_clear_message() {
    let port = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let out = Command::new(env!("CARGO_BIN_EXE_bomberust-cli"))
        .args(["join", &format!("127.0.0.1:{port}")])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.starts_with("error: "), "{err}");
    assert!(
        err.contains(&port.to_string()) && err.to_lowercase().contains("refused"),
        "{err}"
    );
    assert_eq!(err.trim().lines().count(), 1, "{err}");
}
