mod common;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use common::*;

const CSP: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; connect-src 'self' ws: wss:; img-src 'self' data:";

#[test]
fn us1_as1_the_page_and_its_files_are_served() {
    let s = server(|_| {});
    for (path, ctype) in [
        ("/", "text/html; charset=utf-8"),
        ("/index.html", "text/html; charset=utf-8"),
        ("/app.js", "text/javascript; charset=utf-8"),
        ("/look.js", "text/javascript; charset=utf-8"),
        ("/room.js", "text/javascript; charset=utf-8"),
        ("/audio.js", "text/javascript; charset=utf-8"),
Vorago did support a RST_STAT register, but it is not in the latest Programmer's Guide because it was not working as expected. For now, we do not know if they plan to fix it, but every reset event is considered as a Power On Reset and any system reset (POR, external reset, watchdog reset, or software reset) will fully reset the DAC’s logic,
It depends the "fatal error":

If it's a watchdog kick, or a EDAC error, it's not kept in the NVM. For EDAC errors, a APP error can be retrieven by PS-COMM. For now, we do not persist the information. FRAM is immune to radiations when it's not used, so we try to minimize the usage of this memory. The only drawback is that a multi bit error will
         ("/style.css", "text/css; charset=utf-8"),
        ("/eksplode_re_render.wasm", "application/wasm"),
    ] {
        let r = get(s.http_addr(), path);
        assert_eq!(r.status, 200, "{path}");
        assert_eq!(r.header("content-type"), Some(ctype), "{path}");
        assert!(!r.body.is_empty(), "{path} has a body");
        assert_eq!(
            r.header("content-length"),
            Some(r.body.len().to_string().as_str())
        );
        assert_eq!(r.header("cache-control"), Some("no-cache"), "{path}");
        assert_eq!(r.header("x-content-type-options"), Some("nosniff"));
        assert_eq!(r.header("referrer-policy"), Some("no-referrer"));
        assert_eq!(r.header("content-security-policy"), Some(CSP));
    }
}

#[test]
fn the_wasm_file_is_a_real_module() {
    let s = server(|_| {});
    let r = get(s.http_addr(), "/eksplode_re_render.wasm");
    assert_eq!(&r.body[..4], b"\0asm");
}

#[test]
fn the_page_mentions_what_it_loads() {
    let s = server(|_| {});
    let html = String::from_utf8(get(s.http_addr(), "/").body).unwrap();
    assert!(html.contains("/app.js") && html.contains("/style.css"));
    assert!(html.contains("controls-button") && html.contains("controls-panel"));
    assert!(html.contains("id=\"view\"") && html.contains("tabindex=\"0\""));
    assert!(
        !html.contains("<script>"),
        "no inline scripts because of the CSP"
    );
}

#[test]
fn mobile_match_controls_are_accessible_without_forcing_orientation() {
    let s = server(|_| {});
    let html = String::from_utf8(get(s.http_addr(), "/").body).unwrap();
    for id in [
        "touch-controls",
        "touch-joystick",
        "touch-bomb",
        "touch-punch",
    ] {
        assert!(html.contains(id), "missing mobile control {id}");
    }
    assert!(html.contains("aria-label=\"Movement joystick\""));
    assert!(html.contains("aria-label=\"Place bomb\""));
    assert!(html.contains("aria-label=\"Punch bomb\""));
    assert!(!html.contains("Rotate your phone to landscape"));
}

#[test]
fn round_results_have_a_scoreboard_and_return_action() {
    let s = server(|_| {});
    let html = String::from_utf8(get(s.http_addr(), "/").body).unwrap();
    for id in [
        "results-panel",
        "results-title",
        "results-rows",
        "return-to-lobby",
    ] {
        assert!(html.contains(id), "missing round result element {id}");
    }
    assert!(html.contains("Final standings"));
    assert!(html.contains("Return to Lobby"));
}

#[test]
fn the_controls_panel_describes_controller_selection() {
    let s = server(|_| {});
    let html = String::from_utf8(get(s.http_addr(), "/").body).unwrap();
    assert!(html.contains("controller-select"));
    assert!(html.contains("controller-status"));
    assert!(html.contains("controller-mapping"));
}

#[test]
fn the_navigation_hub_and_settings_panels_are_labelled_dialogs() {
    let s = server(|_| {});
    let html = String::from_utf8(get(s.http_addr(), "/").body).unwrap();
    for id in [
        "menu-hub",
        "hub-sound",
        "hub-controls",
        "hub-dev",
        "hub-resume",
    ] {
        assert!(html.contains(id), "missing {id}");
    }
    for id in ["sound-panel", "controls-panel"] {
        let start = html.find(&format!("id=\"{id}\"")).unwrap();
        let panel = &html[start..html[start..].find('>').unwrap() + start];
        assert!(
            panel.contains("role=\"dialog\""),
            "{id} needs dialog semantics"
        );
        assert!(
            panel.contains("aria-label="),
            "{id} needs an accessible label"
        );
    }
}

#[test]
fn the_on_screen_keyboard_is_a_focusable_dialog_and_lobby_controls_exist() {
    let s = server(|_| {});
    let html = String::from_utf8(get(s.http_addr(), "/").body).unwrap();
    for id in [
        "osk",
        "osk-keys",
        "room-create",
        "name",
        "join",
        "start",
        "add-bot",
        "change-look",
    ] {
        assert!(html.contains(id), "missing {id}");
    }
    assert!(html.contains("role=\"dialog\""));
    assert!(html.contains("aria-label=\"On-screen keyboard\""));
}

#[test]
fn head_returns_headers_without_a_body() {
    let s = server(|_| {});
    let addr = s.http_addr();
    let r = http(addr, &format!("HEAD / HTTP/1.1\r\nHost: {addr}\r\n\r\n"));
    assert_eq!(r.status, 200);
    assert!(r.body.is_empty());
    assert!(
        r.header("content-length")
            .unwrap()
            .parse::<usize>()
            .unwrap()
            > 0
    );
}

#[test]
fn unknown_paths_and_traversal_are_404() {
    let s = server(|_| {});
    for path in [
        "/nope",
        "/../Cargo.toml",
        "/app.js/../secret",
        "/%2e%2e/etc/passwd",
        "/ws2",
    ] {
        assert_eq!(get(s.http_addr(), path).status, 404, "{path}");
    }
}

#[test]
fn queries_are_ignored_when_routing() {
    let s = server(|_| {});
    assert_eq!(get(s.http_addr(), "/app.js?v=3").status, 200);
}

#[test]
fn other_methods_are_405_with_allow() {
    let s = server(|_| {});
    let addr = s.http_addr();
    for method in ["POST", "PUT", "DELETE", "PATCH"] {
        let r = http(
            addr,
            &format!("{method} / HTTP/1.1\r\nHost: {addr}\r\nContent-Length: 0\r\n\r\n"),
        );
        assert_eq!(r.status, 405, "{method}");
        assert_eq!(r.header("allow"), Some("GET, HEAD"));
    }
}

#[test]
fn a_plain_get_of_the_websocket_path_is_not_an_upgrade() {
    let s = server(|_| {});
    let r = get(s.http_addr(), "/ws");
    assert_eq!(r.status, 426, "Upgrade Required");
}

#[test]
fn garbage_requests_get_400() {
    let s = server(|_| {});
    let addr = s.http_addr();
    for raw in [
        "HELLO\r\n\r\n",
        "GET\r\n\r\n",
        "GET / FTP/1.0\r\n\r\n",
        "\r\n\r\n",
    ] {
        let r = http(addr, raw);
        assert_eq!(r.status, 400, "{raw:?}");
    }
}

#[test]
fn a_request_head_over_8_kib_is_431() {
    let s = server(|_| {});
    let addr = s.http_addr();
    let big = format!(
        "GET / HTTP/1.1\r\nHost: {addr}\r\nX-Pad: {}\r\n\r\n",
        "a".repeat(9000)
    );
    assert_eq!(http(addr, &big).status, 431);
    // without a terminator it is cut off as well
    let endless = format!("GET / HTTP/1.1\r\nX-Pad: {}", "a".repeat(9000));
    assert_eq!(http(addr, &endless).status, 431);
}

#[test]
fn a_silent_connection_is_closed() {
    let s = server(|o| o.request_timeout = Duration::from_millis(300));
    let mut c = TcpStream::connect(s.http_addr()).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let started = Instant::now();
    let mut buf = [0u8; 16];
    let n = c.read(&mut buf).unwrap_or(0);
    assert_eq!(n, 0, "closed without a response");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn a_slow_request_that_never_finishes_is_closed() {
    let s = server(|o| o.request_timeout = Duration::from_millis(300));
    let mut c = TcpStream::connect(s.http_addr()).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    c.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n").unwrap();
    let mut buf = [0u8; 16];
    assert_eq!(c.read(&mut buf).unwrap_or(0), 0);
}

#[test]
fn beyond_the_connection_limit_the_answer_is_503() {
    let s = server(|o| o.max_connections = 2);
    let addr = s.http_addr();
    let _a = TcpStream::connect(addr).unwrap();
    let _b = TcpStream::connect(addr).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(get(addr, "/").status, 503);
    drop(_a);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(get(addr, "/").status, 200, "a freed slot is usable again");
}

#[test]
fn random_bytes_do_not_hurt_the_server() {
    let s = server(|_| {});
    let addr = s.http_addr();
    let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
    for len in [1usize, 7, 64, 500, 3000] {
        let bytes: Vec<u8> = (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x & 0xff) as u8
            })
            .collect();
        let mut c = TcpStream::connect(addr).unwrap();
        c.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let _ = c.write_all(&bytes);
        let _ = c.write_all(b"\r\n\r\n");
        let mut sink = Vec::new();
        let _ = c.read_to_end(&mut sink);
    }
    assert_eq!(get(addr, "/").status, 200, "still serving");
}

#[test]
fn sc005_the_whole_page_stays_small() {
    let total: usize = [
        "/",
        "/app.js",
        "/look.js",
        "/room.js",
        "/audio.js",
        "/nav.js",
        "/style.css",
        "/eksplode_re_render.wasm",
    ]
    .iter()
    .map(|p| eksplode_re_web::assets::lookup(p).unwrap().body.len())
    .sum();
    let wasm = eksplode_re_web::assets::lookup("/eksplode_re_render.wasm")
        .unwrap()
        .body
        .len();
    println!("page total {total} bytes, wasm {wasm} bytes");
    assert!(wasm <= 300 * 1024, "wasm is {wasm} bytes");
    assert!(total <= 500 * 1024, "page is {total} bytes");
}
