#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use bomberust_cli::online::{render, Online, Stage};
use bomberust_core::{Config, Controller};
use bomberust_net::{Client, Host, HostConfig};
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;

fn host_with(game: Config) -> Host {
    Host::start(HostConfig {
        bind: "127.0.0.1:0".into(),
        tick: Duration::from_millis(4),
        seed: Some(3),
        game,
        ..HostConfig::default()
    })
    .expect("host")
}

fn open_game(limit: u32) -> Config {
    Config {
        block_density: 0.0,
        round_limit_ticks: limit,
        ..Config::default()
    }
}

fn hosting(host: Host) -> Online {
    let client = host.local_client("Hosty").unwrap();
    Online::hosting(host, client)
}

fn joined(host: &Host, name: &str) -> Online {
    joined_at(host.addr(), name)
}

fn joined_at(addr: std::net::SocketAddr, name: &str) -> Online {
    Online::new(Client::connect(&addr.to_string(), name).unwrap())
}

fn pump_until(o: &mut Online, mut done: impl FnMut(&mut Online) -> bool) -> bool {
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        o.pump();
        if done(o) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

fn screen(o: &Online) -> String {
    let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
    t.draw(|f| render(f, o)).unwrap();
    let buf = t.backend().buffer().clone();
    (0..24)
        .map(|y| {
            (0..80)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_client_reaches_the_lobby_with_its_slot() {
    let host = host_with(open_game(1000));
    let mut a = joined(&host, "Ann");
    assert_eq!(a.stage, Stage::Connecting);
    assert!(pump_until(&mut a, |o| o.stage == Stage::Lobby && o.slot == Some(0)));
    assert!(screen(&a).contains("Ann"));
    assert!(screen(&a).contains("(you)"));
}

#[test]
fn the_host_player_sees_the_address_and_hosting_keys() {
    let host = host_with(open_game(1000));
    let port = host.addr().port();
    let mut h = hosting(host);
    assert!(pump_until(&mut h, |o| o.slot == Some(0)));
    let s = screen(&h);
    assert!(s.contains(&format!("port {port}")), "{s}");
    assert!(s.contains("add bot") && s.contains("Enter start"));
    assert_eq!(h.lobby.slots[0].controller, Controller::Local);
}

#[test]
fn clients_can_toggle_ready_and_the_host_sees_it() {
    let host = host_with(open_game(1000));
    let mut h = hosting(host);
    assert!(pump_until(&mut h, |o| o.slot == Some(0)));
    let mut a = joined_at(h.host_addr().unwrap(), "Ann");
    assert!(pump_until(&mut a, |o| o.slot == Some(1)));
    a.handle_key(KeyCode::Char('r'));
    assert!(pump_until(&mut h, |o| o.lobby.slots[1].ready));
    assert!(screen(&h).contains("[ready]"));
    assert!(pump_until(&mut a, |o| o.lobby.slots[1].ready));
    a.handle_key(KeyCode::Char('r'));
    assert!(pump_until(&mut h, |o| !o.lobby.slots[1].ready));
}

#[test]
fn the_host_adds_and_removes_bots_and_starts() {
    let host = host_with(open_game(1000));
    let mut h = hosting(host);
    assert!(pump_until(&mut h, |o| o.slot == Some(0)));
    h.handle_key(KeyCode::Char('b'));
    assert!(pump_until(&mut h, |o| o.lobby.slots[1].controller == Controller::Bot));
    h.handle_key(KeyCode::Char('b'));
    assert!(pump_until(&mut h, |o| o.lobby.slots[2].controller == Controller::Bot));
    h.handle_key(KeyCode::Char('x'));
    assert!(pump_until(&mut h, |o| o.lobby.slots[2].controller == Controller::Empty));
    h.handle_key(KeyCode::Enter);
    assert!(pump_until(&mut h, |o| o.stage == Stage::Playing
        && o.observation.is_some()));
    assert!(screen(&h).contains("P1"));
}

#[test]
fn only_the_host_can_start() {
    let host = host_with(open_game(1000));
    let mut h = hosting(host);
    assert!(pump_until(&mut h, |o| o.slot == Some(0)));
    let mut a = joined_at(h.host_addr().unwrap(), "Ann");
    assert!(pump_until(&mut a, |o| o.slot == Some(1)));
    a.handle_key(KeyCode::Enter);
    a.handle_key(KeyCode::Char('b'));
    std::thread::sleep(Duration::from_millis(100));
    a.pump();
    assert_eq!(a.stage, Stage::Lobby);
    assert_eq!(a.lobby.slots[2].controller, Controller::Empty);
}

#[test]
fn key_presses_move_the_players_character() {
    let host = host_with(open_game(10_000));
    let mut h = hosting(host);
    assert!(pump_until(&mut h, |o| o.slot == Some(0)));
    h.handle_key(KeyCode::Char('b'));
    assert!(pump_until(&mut h, |o| o.lobby.slots[1].controller == Controller::Bot));
    h.handle_key(KeyCode::Enter);
    assert!(pump_until(&mut h, |o| o.observation.is_some()));
    let start = h.observation.as_ref().unwrap().players[0].pos;
    let moved = pump_until(&mut h, |o| {
        o.handle_key(KeyCode::Down);
        o.send_input();
        o.observation
            .as_ref()
            .is_some_and(|ob| ob.players[0].pos != start)
    });
    assert!(moved, "the character never moved");
}

#[test]
fn the_result_shows_up_and_the_lobby_returns() {
    let host = host_with(open_game(40));
    let mut h = hosting(host);
    assert!(pump_until(&mut h, |o| o.slot == Some(0)));
    h.handle_key(KeyCode::Char('b'));
    assert!(pump_until(&mut h, |o| o.lobby.slots[1].controller == Controller::Bot));
    h.handle_key(KeyCode::Enter);
    assert!(pump_until(&mut h, |o| o.stage == Stage::Playing));
    assert!(pump_until(&mut h, |o| o.stage == Stage::Lobby && o.notice.is_some()));
    let n = h.notice.clone().unwrap();
    assert!(n.contains("won") || n.contains("Draw"), "{n}");
}

#[test]
fn the_host_leaving_closes_the_client() {
    let host = host_with(open_game(1000));
    let mut a = joined(&host, "Ann");
    assert!(pump_until(&mut a, |o| o.slot == Some(0)));
    drop(host);
    assert!(pump_until(&mut a, |o| o.stage == Stage::Closed));
    assert_eq!(a.notice.as_deref(), Some("the host left"));
    assert!(screen(&a).contains("the host left"));
}

#[test]
fn a_full_match_closes_with_a_message() {
    let host = host_with(open_game(1000));
    let mut firsts: Vec<Online> = (0..4).map(|i| joined(&host, &format!("p{i}"))).collect();
    for f in &mut firsts {
        assert!(pump_until(f, |o| o.slot.is_some()));
    }
    let mut late = joined(&host, "late");
    assert!(pump_until(&mut late, |o| o.stage == Stage::Closed));
    assert!(late.notice.unwrap().contains("full"));
}

#[test]
fn q_quits_in_every_stage() {
    let host = host_with(open_game(1000));
    let mut a = joined(&host, "Ann");
    a.handle_key(KeyCode::Char('q'));
    assert!(a.quit);
}

fn headless() -> Host {
    host_with(open_game(10_000))
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

#[test]
fn us3_as4_enter_starts_for_the_owner_client() {
    let host = headless();
    let mut a = joined(&host, "Ann");
    assert!(pump_until(&mut a, |o| o.slot == Some(0) && o.stage == Stage::Lobby));
    a.handle_key(KeyCode::Enter);
    assert!(pump_until(&mut a, |o| o.stage == Stage::Playing
        && o.observation.is_some()));
}

#[test]
fn us3_as4_enter_by_a_non_owner_shows_a_notice() {
    let host = host_with(open_game(10_000));
    let mut h = hosting(host);
    assert!(pump_until(&mut h, |o| o.slot == Some(0)));
    let mut a = joined_at(h.host_addr().unwrap(), "Ann");
    assert!(pump_until(&mut a, |o| o.slot == Some(1)));
    a.handle_key(KeyCode::Enter);
    assert!(pump_until(&mut a, |o| o
        .notice
        .as_deref()
        .is_some_and(|n| n.contains("Only Hosty"))));
    assert_eq!(a.stage, Stage::Lobby);
}

#[test]
fn us3_as4_lf_and_ctrl_j_start_the_match() {
    for ev in [
        KeyEvent::new(KeyCode::Char('\n'), KeyModifiers::NONE),
        ctrl('j'),
        ctrl('m'),
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
    ] {
        let host = headless();
        let mut a = joined(&host, "Ann");
        assert!(pump_until(&mut a, |o| o.slot == Some(0)));
        a.handle_event(ev);
        assert!(pump_until(&mut a, |o| o.stage == Stage::Playing), "{ev:?}");
    }
}

#[test]
fn a_refused_command_is_shown_to_the_user() {
    let host = host_with(open_game(10_000));
    let mut h = hosting(host);
    assert!(pump_until(&mut h, |o| o.slot == Some(0)));
    let mut a = joined_at(h.host_addr().unwrap(), "Ann");
    assert!(pump_until(&mut a, |o| o.slot == Some(1)));
    a.handle_key(KeyCode::Char('b'));
    assert!(pump_until(&mut a, |o| o.notice.is_some()));
    assert!(screen(&a).contains("Only Hosty"), "{}", screen(&a));
    assert_eq!(h.lobby.slots[2].controller, Controller::Empty);
}

#[test]
fn b_and_x_work_for_a_joined_owner() {
    let host = headless();
    let mut a = joined(&host, "Ann");
    assert!(pump_until(&mut a, |o| o.slot == Some(0)));
    a.handle_key(KeyCode::Char('b'));
    assert!(pump_until(&mut a, |o| o.lobby.slots[1].controller == Controller::Bot));
    a.handle_key(KeyCode::Char('x'));
    assert!(pump_until(&mut a, |o| o.lobby.slots[1].controller == Controller::Empty));
}

#[test]
fn control_c_is_not_a_game_key() {
    let host = headless();
    let mut a = joined(&host, "Ann");
    assert!(pump_until(&mut a, |o| o.slot == Some(0)));
    a.handle_event(ctrl('c'));
    assert!(!a.quit);
}

// ---- feature 002, US3: hosting with the web page and clearer join errors ----

use bomberust_cli::app::{self, AppError};
use bomberust_cli::args::{HostOptions, JoinOptions};
use bomberust_net::protocol::RejectReason;

fn host_opts(bind: Option<&str>, web_port: Option<u16>) -> HostOptions {
    HostOptions {
        port: 7878,
        bots: 0,
        name: "Host".into(),
        seed: None,
        web_port,
        bind: bind.map(|b| b.parse().unwrap()),
        dev: false,
        room: Default::default(),
        room_given: false,
        no_setup: true,
    }
}

#[test]
fn f3_us4_as6_host_dev_prints_the_developer_warning_and_the_default_does_not() {
    let mut opts = host_opts(Some("127.0.0.1"), Some(8080));
    assert!(app::host_warnings(&opts).is_empty());
    opts.dev = true;
    let w = app::host_warnings(&opts);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(
        w[0].starts_with("WARNING:") && w[0].contains("developer mode"),
        "{w:?}"
    );
    assert!(w[0].contains("untrusted network"), "{w:?}");
}

#[test]
fn us3_as1_the_lobby_shows_the_web_page_only_when_there_is_one() {
    let host = host_with(open_game(1000));
    let port = host.addr().port();
    let mut h = hosting(host);
    assert!(pump_until(&mut h, |o| o.slot == Some(0)));
    let s = screen(&h);
    assert!(!s.contains("Web page"), "{s}");
    assert!(
        s.contains(&format!("bomberust-cli join <your-ip>:{port}")),
        "{s}"
    );
    assert!(s.contains("q quit"), "{s}");

    h.web_url = Some("http://127.0.0.1:8080/".into());
    let s = screen(&h);
    assert!(s.contains("Web page: http://127.0.0.1:8080/"), "{s}");
}

#[test]
fn us3_as1_the_web_page_address_uses_loopback_for_wildcards() {
    assert_eq!(
        app::web_url("0.0.0.0:8080".parse().unwrap()),
        "http://127.0.0.1:8080/"
    );
    assert_eq!(
        app::web_url("127.0.0.1:9000".parse().unwrap()),
        "http://127.0.0.1:9000/"
    );
    assert_eq!(
        app::web_url("[::1]:80".parse().unwrap()),
        "http://[::1]:80/"
    );
    assert_eq!(
        app::web_url("192.168.1.5:8080".parse().unwrap()),
        "http://192.168.1.5:8080/"
    );
}

#[test]
fn us3_as1_non_loopback_binds_print_the_exposure_warning() {
    let w = app::host_warnings(&host_opts(Some("192.168.1.10"), Some(8080)));
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].starts_with("WARNING:"), "{w:?}");
    assert!(
        w[0].contains("192.168.1.10") && w[0].contains("no encryption"),
        "{w:?}"
    );
    // the default game port listens on every interface
    let w = app::host_warnings(&host_opts(None, None));
    assert!(w.len() == 1 && w[0].contains("0.0.0.0"), "{w:?}");
    // loopback is quiet
    assert!(app::host_warnings(&host_opts(Some("127.0.0.1"), Some(8080))).is_empty());
    assert!(app::host_warnings(&host_opts(Some("::1"), None)).is_empty());
}

fn one_line(e: &AppError) -> String {
    let text = e.to_string();
    assert!(!text.contains('\n'), "{text:?}");
    assert!(!text.is_empty());
    text
}

#[test]
fn us3_as3_a_refused_connection_is_explained_in_one_line() {
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let err = app::run_join(&JoinOptions {
        addr: format!("127.0.0.1:{port}"),
        name: "Ann".into(),
    })
    .expect_err("nobody listens there");
    let text = one_line(&err);
    assert!(text.contains(&format!("127.0.0.1:{port}")), "{text}");
    assert!(text.to_lowercase().contains("refused"), "{text}");
    assert!(text.contains("host"), "{text}");
}

#[test]
fn us3_as3_every_refusal_has_its_own_explanation() {
    let texts: Vec<String> = [
        RejectReason::Full,
        RejectReason::BadVersion,
        RejectReason::BadName,
        RejectReason::InMatch,
    ]
    .into_iter()
    .map(|r| one_line(&app::join_failure(r)))
    .collect();
    for (text, word) in texts.iter().zip(["full", "version", "name", "running"]) {
        assert!(text.to_lowercase().contains(word), "{text}");
    }
    let mut unique = texts.clone();
    unique.dedup();
    assert_eq!(unique.len(), 4);
}

#[test]
fn us3_as3_a_rejection_is_remembered_so_the_notice_can_stay_visible() {
    let host = host_with(open_game(1000));
    let mut firsts: Vec<Online> = (0..4).map(|i| joined(&host, &format!("p{i}"))).collect();
    for f in &mut firsts {
        assert!(pump_until(f, |o| o.slot.is_some()));
    }
    let mut late = joined(&host, "late");
    assert!(late.rejection_shown(Duration::ZERO).is_none());
    assert!(pump_until(&mut late, |o| o.stage == Stage::Closed));
    assert_eq!(
        late.rejection_shown(Duration::ZERO),
        Some(RejectReason::Full)
    );
    assert_eq!(late.rejection_shown(Duration::from_secs(60)), None);
    assert!(firsts[0].rejection_shown(Duration::ZERO).is_none());
}

#[test]
fn us3_as3_names_and_running_matches_are_refused_with_a_reason() {
    let host = host_with(open_game(10_000));
    let mut bad = joined(&host, "");
    assert!(pump_until(&mut bad, |o| o.stage == Stage::Closed));
    assert_eq!(
        bad.rejection_shown(Duration::ZERO),
        Some(RejectReason::BadName)
    );

    let mut a = joined(&host, "Ann");
    assert!(pump_until(&mut a, |o| o.slot == Some(0)));
    a.handle_key(KeyCode::Enter);
    assert!(pump_until(&mut a, |o| o.stage == Stage::Playing));
    let mut late = joined(&host, "Late");
    assert!(pump_until(&mut late, |o| o.stage == Stage::Closed));
    assert_eq!(
        late.rejection_shown(Duration::ZERO),
        Some(RejectReason::InMatch)
    );
}

#[test]
fn f3_us1_as7_terminal_players_get_a_default_look_and_keep_working() {
    use std::io::Write;
    let host = host_with(open_game(1000));
    // a browser-like participant joins first and chooses a look
    let mut raw = std::net::TcpStream::connect(host.addr()).unwrap();
    raw.write_all(
        br#"{"type":"Join","version":1,"name":"Web","look":{"colour":"Green","head":"Crown"}}
"#,
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(150));
    let mut a = joined(&host, "Term");
    assert!(pump_until(&mut a, |o| o.slot == Some(1) && o.stage == Stage::Lobby));
    let s = screen(&a);
    assert!(s.contains("Web") && s.contains("Term"), "{s}");
    // later lobby messages carry looks too and are applied without trouble
    a.handle_key(KeyCode::Char('r'));
    assert!(pump_until(&mut a, |o| o.lobby.slots[1].ready));
    assert_eq!(a.stage, Stage::Lobby);
    drop(raw);
}

fn lobby_text_has(o: &Online, needle: &str) -> bool {
    screen(o).contains(needle)
}

fn room_host(defaults: bomberust_core::RoomSettings) -> Host {
    Host::start(HostConfig {
        bind: "127.0.0.1:0".into(),
        tick: Duration::from_millis(4),
        seed: Some(3),
        game: open_game(1000),
        defaults,
        ..HostConfig::default()
    })
    .expect("host")
}

#[test]
fn f4_us1_as7_the_terminal_host_sets_and_shows_the_same_settings() {
    use bomberust_core::{BoardSize, RoomSettings};
    let host = room_host(RoomSettings {
        name: "Lab".into(),
        ..RoomSettings::default()
    });
    let mut h = hosting(host);
    assert!(pump_until(&mut h, |o| o.slot == Some(0)));
    assert!(
        lobby_text_has(&h, "Lab - Normal - 15 x 13 - 3 min"),
        "{}",
        screen(&h)
    );

    h.handle_key(KeyCode::Char('s'));
    assert!(pump_until(&mut h, |o| o.lobby.settings.size == BoardSize::Medium));
    assert!(
        lobby_text_has(&h, "Lab - Normal - 19 x 15 - 3 min"),
        "{}",
        screen(&h)
    );

    h.handle_key(KeyCode::Char('t'));
    assert!(pump_until(&mut h, |o| o.lobby.settings.minutes == 5));
    assert!(lobby_text_has(&h, "5 min"));

    h.handle_key(KeyCode::Char('f'));
    assert!(pump_until(&mut h, |o| !o.lobby.settings.friendly_fire));

    h.handle_key(KeyCode::Char('m'));
    h.pump();
    assert!(bomberust_core::PLAYABLE.contains(&h.lobby.settings.mode));

    // a terminal player sees the same line
    let mut a = joined_at(h.host_addr().unwrap(), "Ann");
    assert!(pump_until(&mut a, |o| o.slot == Some(1)));
    assert!(
        lobby_text_has(&a, "Lab - Normal - 19 x 15 - 5 min"),
        "{}",
        screen(&a)
    );
}

#[test]
fn only_the_owner_changes_the_settings_in_the_terminal_lobby() {
    use bomberust_core::BoardSize;
    let host = room_host(Default::default());
    let mut h = hosting(host);
    assert!(pump_until(&mut h, |o| o.slot == Some(0)));
    let mut a = joined_at(h.host_addr().unwrap(), "Ann");
    assert!(pump_until(&mut a, |o| o.slot == Some(1)));
    a.handle_key(KeyCode::Char('s'));
    assert!(pump_until(&mut a, |o| o
        .notice
        .as_deref()
        .is_some_and(|n| n.contains("Only"))));
    assert_eq!(a.lobby.settings.size, BoardSize::Small);
    h.pump();
    assert_eq!(h.lobby.settings.size, BoardSize::Small);
    // and the owner's change reaches the other terminal
    h.handle_key(KeyCode::Char('s'));
    assert!(pump_until(&mut a, |o| o.lobby.settings.size == BoardSize::Medium));
}

#[test]
fn a_terminal_player_who_joins_an_empty_room_gets_the_defaults() {
    let host = room_host(Default::default());
    let mut a = joined(&host, "Ann");
    assert!(pump_until(&mut a, |o| o.slot == Some(0)));
    assert!(
        lobby_text_has(&a, "Ann's room - Normal - 15 x 13 - 3 min"),
        "{}",
        screen(&a)
    );
}
