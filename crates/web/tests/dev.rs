mod common;

use bomberust_net::protocol::{ClientMsg, ServerMsg};
use bomberust_web::origin::warnings;
use bomberust_web::ServerOptions;
use common::*;

fn is_error(code: &'static str) -> impl FnMut(&ServerMsg) -> bool {
    move |m| matches!(m, ServerMsg::Error { code: c, .. } if c == code)
}

#[test]
fn f3_us4_as1_the_dev_panel_is_offered_only_in_dev_mode() {
    let s = server(|o| o.dev = true);
    let mut ws = Ws::open(s.http_addr());
    ws.join("Ann");
    match ws.recv_until(|m| matches!(m, ServerMsg::Welcome { .. })) {
        Some(ServerMsg::Welcome { dev, .. }) => assert!(dev),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        ws.recv_until(|m| matches!(m, ServerMsg::DevState { .. })),
        Some(ServerMsg::DevState { paused: false, .. })
    ));
    ws.send(&ClientMsg::DevPauseBots { paused: true });
    assert!(matches!(
        ws.recv_until(|m| matches!(m, ServerMsg::DevState { .. })),
        Some(ServerMsg::DevState { paused: true, .. })
    ));

    let s = server(|_| {});
    let mut ws = Ws::open(s.http_addr());
    ws.join("Ann");
    match ws.recv_until(|m| matches!(m, ServerMsg::Welcome { .. })) {
        Some(ServerMsg::Welcome { dev, .. }) => assert!(!dev),
        other => panic!("{other:?}"),
    }
    ws.send(&ClientMsg::DevResetDrops);
    assert!(ws.recv_until(is_error("DevDisabled")).is_some());
}

#[test]
fn the_start_up_warning_names_the_risk_and_the_banner_shows_the_mode() {
    let quiet = ServerOptions::default();
    assert!(warnings(&quiet).is_empty());
    let dev = ServerOptions {
        dev: true,
        ..ServerOptions::default()
    };
    let w = warnings(&dev);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].starts_with("WARNING:"), "{w:?}");
    assert!(w[0].contains("developer mode"), "{w:?}");
    assert!(
        w[0].contains("testing") && w[0].contains("untrusted network"),
        "{w:?}"
    );

    let s = server(|o| o.dev = true);
    assert!(s.banner().contains("Developer mode: on"), "{}", s.banner());
    let s = server(|_| {});
    assert!(!s.banner().contains("Developer mode"), "{}", s.banner());
}
