mod common;

use std::net::{IpAddr, Ipv4Addr};

use common::*;
use eksplode_re_web::args::{parse, Command};
use eksplode_re_web::origin::{cors_header, origin_allowed, warnings, AllowedOrigins};
use eksplode_re_web::ServerOptions;

const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const LAN: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 7));

fn list(items: &[&str]) -> AllowedOrigins {
    AllowedOrigins::List(items.iter().map(|s| s.to_string()).collect())
}

#[test]
fn same_origin_is_always_allowed() {
    for policy in [
        AllowedOrigins::SameOrigin,
        list(&["http://other.example"]),
        AllowedOrigins::Any,
    ] {
        assert!(origin_allowed(
            &policy,
            Some("http://127.0.0.1:8080"),
            Some("127.0.0.1:8080"),
            LAN
        ));
        assert!(origin_allowed(
            &policy,
            Some("https://game.example"),
            Some("game.example"),
            LAN
        ));
    }
}

#[test]
fn another_origin_is_refused_by_default() {
    let p = AllowedOrigins::SameOrigin;
    assert!(!origin_allowed(
        &p,
        Some("http://evil.example"),
        Some("127.0.0.1:8080"),
        LAN
    ));
    assert!(!origin_allowed(
        &p,
        Some("http://127.0.0.1:9999"),
        Some("127.0.0.1:8080"),
        LOOPBACK
    ));
    assert!(!origin_allowed(
        &p,
        Some("null"),
        Some("127.0.0.1:8080"),
        LOOPBACK
    ));
    assert!(!origin_allowed(
        &p,
        Some("http://127.0.0.1:8080.evil.example"),
        Some("127.0.0.1:8080"),
        LAN
    ));
}

#[test]
fn a_missing_origin_is_only_accepted_from_loopback() {
    let p = AllowedOrigins::SameOrigin;
    assert!(origin_allowed(&p, None, Some("127.0.0.1:8080"), LOOPBACK));
    assert!(!origin_allowed(&p, None, Some("192.168.1.7:8080"), LAN));
}

#[test]
fn listed_origins_match_exactly() {
    let p = list(&["http://localhost:3000"]);
    assert!(origin_allowed(
        &p,
        Some("http://localhost:3000"),
        Some("127.0.0.1:8080"),
        LAN
    ));
    assert!(origin_allowed(
        &p,
        Some("HTTP://LOCALHOST:3000"),
        Some("127.0.0.1:8080"),
        LAN
    ));
    assert!(!origin_allowed(
        &p,
        Some("http://localhost:3001"),
        Some("127.0.0.1:8080"),
        LAN
    ));
    assert!(!origin_allowed(
        &p,
        Some("http://localhost:3000.evil.example"),
        Some("127.0.0.1:8080"),
        LAN
    ));
}

#[test]
fn any_origin_accepts_everything() {
    let p = AllowedOrigins::Any;
    assert!(origin_allowed(
        &p,
        Some("http://evil.example"),
        Some("127.0.0.1:8080"),
        LAN
    ));
    assert!(origin_allowed(&p, None, Some("x"), LAN));
}

#[test]
fn cors_headers_only_for_relaxed_policies() {
    assert_eq!(
        cors_header(&AllowedOrigins::SameOrigin, Some("http://a.example")),
        None
    );
    assert_eq!(
        cors_header(&AllowedOrigins::Any, Some("http://a.example")).as_deref(),
        Some("*")
    );
    let p = list(&["http://a.example"]);
    assert_eq!(
        cors_header(&p, Some("http://a.example")).as_deref(),
        Some("http://a.example")
    );
    assert_eq!(cors_header(&p, Some("http://b.example")), None);
    assert_eq!(cors_header(&p, None), None);
}

#[test]
fn the_defaults_print_no_warnings() {
    assert!(warnings(&ServerOptions::default()).is_empty());
}

#[test]
fn relaxed_settings_print_warnings() {
    let o = ServerOptions {
        origins: list(&["http://localhost:3000", "http://b.example"]),
        ..ServerOptions::default()
    };
    let w = warnings(&o);
    assert_eq!(w.len(), 2);
    assert!(w.iter().all(|l| l.starts_with("WARNING:")));
    assert!(w[0].contains("http://localhost:3000") && w[1].contains("http://b.example"));
    assert!(w[0].contains("CORS"));

    let o = ServerOptions {
        origins: AllowedOrigins::Any,
        ..ServerOptions::default()
    };
    let w = warnings(&o);
    assert_eq!(w.len(), 1);
    assert!(w[0].starts_with("WARNING:") && w[0].contains("--allow-any-origin"));

    let o = ServerOptions {
        bind: LAN,
        ..ServerOptions::default()
    };
    let w = warnings(&o);
    assert_eq!(w.len(), 1);
    assert!(
        w[0].contains("encryption")
            && w[0].contains("authentication")
            && w[0].contains("192.168.1.7")
    );
    let o = ServerOptions {
        bind: "0.0.0.0".parse().unwrap(),
        ..ServerOptions::default()
    };
    assert_eq!(warnings(&o).len(), 1);
}

#[test]
fn parsed_origin_options_reach_the_policy() {
    let args = [
        "--allow-origin",
        "http://a.example",
        "--allow-origin",
        "http://b.example",
    ];
    let Ok(Command::Run(o)) = parse(args.iter().map(|s| s.to_string())) else {
        panic!("should parse");
    };
    assert_eq!(o.origins, list(&["http://a.example", "http://b.example"]));
}

#[test]
fn sc007_cross_origin_upgrades_are_refused_by_default() {
    let s = server(|_| {});
    for i in 0..100 {
        let origin = format!("http://evil{i}.example");
        assert_eq!(
            Ws::connect(s.http_addr(), Some(&origin)).err(),
            Some(403),
            "{origin}"
        );
    }
}

#[test]
fn the_own_origin_and_command_line_tools_get_in() {
    let s = server(|_| {});
    let addr = s.http_addr();
    assert!(Ws::connect(addr, Some(&format!("http://{addr}"))).is_ok());
    assert!(Ws::connect(addr, None).is_ok(), "no Origin from loopback");
}

#[test]
fn listed_and_any_origins_can_be_enabled() {
    let s = server(|o| o.origins = list(&["http://localhost:3000"]));
    assert!(Ws::connect(s.http_addr(), Some("http://localhost:3000")).is_ok());
    assert_eq!(
        Ws::connect(s.http_addr(), Some("http://evil.example")).err(),
        Some(403)
    );

    let s = server(|o| o.origins = AllowedOrigins::Any);
    assert!(Ws::connect(s.http_addr(), Some("http://evil.example")).is_ok());
}

#[test]
fn a_refused_upgrade_never_reaches_the_match() {
    let s = server(|_| {});
    let mut good = Ws::open(s.http_addr());
    good.join("Ann");
    assert_eq!(good.welcome_slot(), 0);
    assert!(Ws::connect(s.http_addr(), Some("http://evil.example")).is_err());
    let mut next = Ws::open(s.http_addr());
    next.join("Bob");
    assert_eq!(next.welcome_slot(), 1, "the refused client took no slot");
}

#[test]
fn relaxed_policies_add_cors_headers_to_static_files() {
    let s = server(|o| o.origins = AllowedOrigins::Any);
    let r = get(s.http_addr(), "/app.js");
    assert_eq!(r.header("access-control-allow-origin"), Some("*"));

    let s = server(|o| o.origins = list(&["http://localhost:3000"]));
    let addr = s.http_addr();
    let r = http(
        addr,
        &format!("GET /app.js HTTP/1.1\r\nHost: {addr}\r\nOrigin: http://localhost:3000\r\n\r\n"),
    );
    assert_eq!(
        r.header("access-control-allow-origin"),
        Some("http://localhost:3000")
    );
    assert_eq!(r.header("vary"), Some("Origin"));
    let r = http(
        addr,
        &format!("GET /app.js HTTP/1.1\r\nHost: {addr}\r\nOrigin: http://evil.example\r\n\r\n"),
    );
    assert_eq!(r.header("access-control-allow-origin"), None);

    let s = server(|_| {});
    assert_eq!(
        get(s.http_addr(), "/app.js").header("access-control-allow-origin"),
        None
    );
}
