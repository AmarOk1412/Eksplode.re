mod common;

use std::net::TcpListener;
use std::process::Command;

use bomberust_net::protocol::ServerMsg;
use bomberust_web::{Server, ServerError};
use common::*;

#[test]
fn a_busy_http_port_is_reported_with_its_number() {
    let busy = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = busy.local_addr().unwrap().port();
    let mut o = options();
    o.http_port = port;
    let err = Server::start(o).err().expect("must fail");
    assert!(matches!(err, ServerError::Bind { .. }), "{err:?}");
    assert!(err.to_string().contains(&port.to_string()), "{err}");
}

#[test]
fn a_busy_game_port_is_reported_too() {
    let busy = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = busy.local_addr().unwrap().port();
    let mut o = options();
    o.game_port = port;
    let err = Server::start(o).err().expect("must fail");
    assert!(err.to_string().contains(&port.to_string()), "{err}");
}

#[test]
fn shutdown_tells_every_client_and_closes_the_sockets() {
    let s = server(|_| {});
    let mut ws = Ws::open(s.http_addr());
    ws.join("Web");
    ws.welcome_slot();
    let tcp = bomberust_net::Client::connect(&s.game_addr().to_string(), "Tcp").unwrap();
    // wait for the terminal client to be in the lobby
    let welcome = tcp.recv_timeout(WAIT).unwrap();
    assert!(matches!(welcome, ServerMsg::Welcome { slot: 1, .. }));

    s.shutdown();

    let got = ws.recv_until(|m| matches!(m, ServerMsg::Error { code, .. } if code == "HostLeft"));
    assert!(got.is_some(), "the browser is told");
    assert_eq!(ws.close_code().unwrap_or(1001), 1001);
    let mut told = false;
    while let Ok(m) = tcp.recv_timeout(WAIT) {
        if matches!(m, ServerMsg::Error { code, .. } if code == "HostLeft") {
            told = true;
        }
    }
    assert!(told, "the terminal client is told");
}

#[test]
fn the_banner_shows_the_addresses_and_how_to_stop() {
    let s = server(|_| {});
    let banner = s.banner();
    assert!(banner.contains("Bomberust server running"), "{banner}");
    assert!(banner.contains(&format!(
        "Web page:        http://127.0.0.1:{}/",
        s.http_addr().port()
    )));
    assert!(banner.contains(&format!(
        "Terminal client: bomberust-cli join 127.0.0.1:{}",
        s.game_addr().port()
    )));
    assert!(banner.contains("Stop:            Ctrl+C"));
}

#[test]
fn bots_are_pre_added_to_the_lobby() {
    let s = server(|o| o.bots = 2);
    let mut ws = Ws::open(s.http_addr());
    ws.join("Ann");
    assert_eq!(ws.welcome_slot(), 2, "two slots are taken by bots");
}

#[test]
fn us3_as2_the_binary_reports_a_busy_port_and_exits_with_status_1() {
    let busy = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = busy.local_addr().unwrap().port();
    let free = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let out = Command::new(env!("CARGO_BIN_EXE_bomberust-server"))
        .args([
            "--http-port",
            &port.to_string(),
            "--game-port",
            &free.to_string(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains(&port.to_string()));
}

#[test]
fn the_binary_rejects_bad_usage_with_status_2() {
    let out = Command::new(env!("CARGO_BIN_EXE_bomberust-server"))
        .arg("--nope")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let out = Command::new(env!("CARGO_BIN_EXE_bomberust-server"))
        .arg("--help")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("--allow-origin"));
}

#[test]
fn us3_as1_attach_shares_a_match_host_that_someone_else_owns() {
    let host = bomberust_net::Host::start(bomberust_net::HostConfig {
        bind: "127.0.0.1:0".into(),
        tick: std::time::Duration::from_millis(4),
        seed: Some(5),
        game: options().game,
        ..bomberust_net::HostConfig::default()
    })
    .unwrap();
    let web = Server::attach(host.handle(), host.addr(), options()).unwrap();
    assert_eq!(web.game_addr(), host.addr());

    let mut ws = Ws::open(web.http_addr());
    ws.join("Web");
    assert_eq!(ws.welcome_slot(), 0);
    let tcp = bomberust_net::Client::connect(&host.addr().to_string(), "Term").unwrap();
    let joined = tcp.recv_timeout(WAIT).unwrap();
    assert!(
        matches!(joined, ServerMsg::Welcome { slot: 1, .. }),
        "{joined:?}"
    );

    // stopping the page leaves the match host running for the terminal player
    drop(web);
    host.start_match();
    let mut ticks = false;
    while let Ok(m) = tcp.recv_timeout(WAIT) {
        if matches!(m, ServerMsg::Snapshot { .. }) {
            ticks = true;
            break;
        }
    }
    assert!(ticks, "the host keeps playing after the web server stops");
}
