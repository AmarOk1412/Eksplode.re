mod common;

use std::net::TcpListener;
use std::process::Command;

use common::*;
use eksplode_re_net::protocol::ServerMsg;
use eksplode_re_web::{Server, ServerError};

#[test]
fn a_busy_http_port_is_reported_with_its_number() {
    let busy = TcpListener::bind("0.0.0.0:0").unwrap();
    let port = busy.local_addr().unwrap().port();
    let mut o = options();
    o.http_port = port;
    let err = Server::start(o).err().expect("must fail");
    assert!(matches!(err, ServerError::Bind { .. }), "{err:?}");
    assert!(err.to_string().contains(&port.to_string()), "{err}");
}

#[test]
fn a_busy_game_port_is_reported_too() {
    let busy = TcpListener::bind("0.0.0.0:0").unwrap();
    let port = busy.local_addr().unwrap().port();
    let mut o = options();
    o.game_port = port;
    let err = Server::start(o).err().expect("must fail");
    assert!(err.to_string().contains(&port.to_string()), "{err}");
}

#[test]
fn shutdown_tells_every_client_and_closes_the_sockets() {
    let s = server(|_| {});
    let mut ws = RoomWs::create(s.http_addr(), "Web");
    ws.welcome_slot();
    let tcp = eksplode_re_net::Client::connect_unjoined(&s.game_addr().to_string()).unwrap();
    tcp.join_room(&ws.reference, "Tcp").unwrap();
    // wait for the terminal client to be in the lobby
    let welcome = std::iter::from_fn(|| tcp.recv_timeout(WAIT).ok())
        .map(ServerMsg::into_game_message)
        .find(|message| matches!(message, ServerMsg::Welcome { .. }))
        .unwrap();
    assert!(matches!(welcome, ServerMsg::Welcome { slot: 1, .. }));

    s.shutdown();

    let got = ws.recv_until(|m| matches!(m, ServerMsg::Error { code, .. } if code == "HostLeft"));
    assert!(got.is_some(), "the browser is told");
    assert_eq!(ws.close_code().unwrap_or(1001), 1001);
    let mut told = false;
    while let Ok(m) = tcp.recv_timeout(WAIT) {
        if matches!(m.into_game_message(), ServerMsg::Error { code, .. } if code == "HostLeft") {
            told = true;
        }
    }
    assert!(told, "the terminal client is told");
}

#[test]
fn detached_web_service_withdraws_its_advertised_origin_without_stopping_the_host() {
    use eksplode_re_net::{Host, HostConfig};
    let host = Host::start(HostConfig {
        bind: "0.0.0.0:0".into(),
        ..HostConfig::default()
    })
    .unwrap();
    let web = Server::attach(host.handle(), host.addr(), options()).unwrap();
    let advertised: serde_json::Value =
        serde_json::to_value(host.handle().directory(None).unwrap()).unwrap();
    assert_eq!(
        advertised["sources"][0]["web_origin"],
        format!("http://{}", web.http_addr())
    );
    assert_eq!(
        advertised["sources"][0]["game_addr"],
        format!("127.0.0.1:{}", host.addr().port())
    );
    drop(web);
    let remaining: serde_json::Value =
        serde_json::to_value(host.handle().directory(None).unwrap()).unwrap();
    assert!(remaining["sources"][0]["web_origin"].is_null());
    assert_eq!(remaining["sources"][0]["status"], "fresh");
}

#[test]
fn the_banner_shows_the_addresses_and_how_to_stop() {
    let s = server(|_| {});
    let banner = s.banner();
    assert!(banner.contains("Eksplode.re server running"), "{banner}");
    assert!(banner.contains(&format!(
        "Web page:        http://127.0.0.1:{}/",
        s.http_addr().port()
    )));
    assert!(banner.contains(&format!(
        "Terminal client: eksplode-re-cli join 127.0.0.1:{}",
        s.game_addr().port()
    )));
    assert!(banner.contains("Stop:            Ctrl+C"));
}

#[test]
fn bots_fill_the_room_only_after_explicit_room_creation() {
    let s = server(|o| o.bots = 2);
    let directory: serde_json::Value =
        serde_json::from_slice(&get(s.http_addr(), "/rooms").body).unwrap();
    assert!(directory["rooms"].as_array().unwrap().is_empty());
    let mut ws = RoomWs::create(s.http_addr(), "Ann");
    let welcome = ws
        .recv_until(|message| matches!(message, ServerMsg::Welcome { .. }))
        .unwrap();
    match welcome {
        ServerMsg::Welcome { slot, lobby, .. } => {
            assert_eq!(slot, 0, "creation admits the owner before the bots");
            // 011: every empty seat up to the capacity (4) gets a bot; `bots = 2` no longer limits it
            assert_eq!(lobby.participants(), 4);
            for seat in 1..4 {
                assert_eq!(
                    lobby.slots[seat].controller,
                    eksplode_re_core::Controller::Bot
                );
            }
        }
        _ => panic!("expected Welcome"),
    }
}

#[test]
fn us3_as2_the_binary_reports_a_busy_port_and_exits_with_status_1() {
    let busy = TcpListener::bind("0.0.0.0:0").unwrap();
    let port = busy.local_addr().unwrap().port();
    let free = {
        let l = TcpListener::bind("0.0.0.0:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let out = Command::new(env!("CARGO_BIN_EXE_eksplode-re-server"))
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
    let out = Command::new(env!("CARGO_BIN_EXE_eksplode-re-server"))
        .arg("--nope")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let out = Command::new(env!("CARGO_BIN_EXE_eksplode-re-server"))
        .arg("--help")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("--allow-origin"));
}

#[test]
fn us3_as1_attach_shares_a_match_host_that_someone_else_owns() {
    let host = eksplode_re_net::Host::start(eksplode_re_net::HostConfig {
        bind: "0.0.0.0:0".into(),
        tick: std::time::Duration::from_millis(4),
        seed: Some(5),
        game: options().game,
        ..eksplode_re_net::HostConfig::default()
    })
    .unwrap();
    let web = Server::attach(host.handle(), host.addr(), options()).unwrap();
    assert_eq!(web.game_addr(), host.addr());

    let mut ws = RoomWs::create(web.http_addr(), "Web");
    assert_eq!(ws.welcome_slot(), 0);
    let tcp = eksplode_re_net::Client::connect_unjoined(&host.addr().to_string()).unwrap();
    tcp.join_room(&ws.reference, "Term").unwrap();
    let joined = std::iter::from_fn(|| tcp.recv_timeout(WAIT).ok())
        .map(ServerMsg::into_game_message)
        .find(|message| matches!(message, ServerMsg::Welcome { .. }))
        .unwrap();
    assert!(
        matches!(joined, ServerMsg::Welcome { slot: 1, .. }),
        "{joined:?}"
    );

    // stopping the page leaves the match host running for the terminal player
    let room = ws.reference.clone();
    drop(web);
    host.handle().start_room(&room).unwrap();
    let mut ticks = false;
    while let Ok(m) = tcp.recv_timeout(WAIT) {
        if matches!(m.into_game_message(), ServerMsg::Snapshot { .. }) {
            ticks = true;
            break;
        }
    }
    assert!(ticks, "the host keeps playing after the web server stops");
}
