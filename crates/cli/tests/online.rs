#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use eksplode_re_cli::input::KeyBindings;
use eksplode_re_cli::online::{render, Online, Stage};
use eksplode_re_core::{Config, Controller};
use eksplode_re_net::{Client, Host, HostConfig};
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;

fn host_with(game: Config) -> Host {
    Host::start(HostConfig {
        bind: "0.0.0.0:0".into(),
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
    let probe = host.connect(false).unwrap();
    probe
        .send_v2(eksplode_re_net::protocol::v2::Request::Hello { version: 2 })
        .unwrap();
    let defaults = match probe.recv_timeout(Duration::from_secs(2)).unwrap() {
        eksplode_re_net::protocol::ServerMsg::RoomV2(reply) => match *reply {
            eksplode_re_net::protocol::v2::Reply::Hello { defaults, .. } => defaults,
            _ => panic!("expected Hello"),
        },
        _ => panic!("expected room protocol"),
    };
    drop(probe);
    let client = host.connect(true).unwrap();
    Online::hosting_room(host, client, "Hosty".into(), defaults)
}

fn joined(host: &Host, name: &str) -> Online {
    let client = Client::connect_unjoined(&host.addr().to_string()).unwrap();
    assert!(
        matches!(client.recv_timeout(Duration::from_secs(2)).unwrap(),
        eksplode_re_net::protocol::ServerMsg::RoomV2(reply)
        if matches!(*reply, eksplode_re_net::protocol::v2::Reply::Hello { .. }))
    );
    client
        .send_v2(eksplode_re_net::protocol::v2::Request::create_room(
            eksplode_re_net::protocol::v2::RequestId::new("fixture-create").unwrap(),
            name.into(),
            &eksplode_re_core::RoomSettings::default(),
            4,
        ))
        .unwrap();
    Online::directory(client, name.into(), KeyBindings::default_for_slot(0))
}

fn joined_at(addr: std::net::SocketAddr, name: &str) -> Online {
    selected_at(addr, name, KeyBindings::default_for_slot(0))
}

fn selected_at(addr: std::net::SocketAddr, name: &str, bindings: KeyBindings) -> Online {
    let client = Client::connect_unjoined(&addr.to_string()).unwrap();
    let mut online = Online::directory(client, name.into(), bindings);
    assert!(pump_until(&mut online, |online| online.stage
        == Stage::Directory
        && !online.rooms.is_empty()));
    assert_eq!(
        online.rooms.len(),
        1,
        "this fixture must explicitly select one known test room"
    );
    online.handle_key(KeyCode::Enter);
    online
}

fn filled_room(host: &Host) -> Vec<Online> {
    let mut owner = joined(host, "p0");
    assert!(pump_until(&mut owner, |online| online.slot == Some(0)));
    let mut clients = vec![owner];
    for index in 1..4 {
        let mut client = joined_at(host.addr(), &format!("p{index}"));
        assert!(pump_until(&mut client, |online| online.slot == Some(index)));
        clients.push(client);
    }
    clients
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
fn v2_terminal_directory_creates_joins_and_leaves_explicit_rooms() {
    let host = host_with(open_game(1000));
    let client = Client::connect_unjoined(&host.addr().to_string()).unwrap();
    let mut owner = Online::directory(client, "Owner".into(), KeyBindings::default_for_slot(0));
    assert!(pump_until(&mut owner, |online| online.stage == Stage::Directory));
    assert!(screen(&owner).contains("Rooms"));
    assert_eq!(owner.slot, None);
    owner.handle_key(KeyCode::Char('c'));
    assert_eq!(owner.stage, Stage::Creating);
    for _ in 0..5 {
        owner.handle_key(KeyCode::Enter);
    }
    assert!(pump_until(&mut owner, |online| online.stage == Stage::Lobby));
    assert_eq!(owner.slot, Some(0));
    let client = Client::connect_unjoined(&host.addr().to_string()).unwrap();
    let mut member = Online::directory(client, "Member".into(), KeyBindings::default_for_slot(0));
    assert!(pump_until(&mut member, |online| online.stage
        == Stage::Directory
        && online.rooms.len() == 1));
    assert!(screen(&member).contains("Owner"));
    member.handle_key(KeyCode::Enter);
    assert!(pump_until(&mut member, |online| online.stage == Stage::Lobby));
    assert_eq!(member.slot, Some(1));
    member.handle_key(KeyCode::Esc);
    assert_eq!(member.stage, Stage::ConfirmLeave);
    member.handle_key(KeyCode::Esc);
    assert_eq!(member.stage, Stage::Lobby);
    member.handle_key(KeyCode::Esc);
    member.handle_key(KeyCode::Enter);
    assert!(pump_until(&mut member, |online| online.stage == Stage::Directory));
    assert_eq!(member.slot, None);
    // 011: a human leaving before the match gets their seat refilled with a bot
    assert!(pump_until(&mut owner, |online| online.lobby.slots[1]
        .controller
        == Controller::Bot));
}

#[test]
fn protected_room_and_authoritative_owner_controls_are_visible_to_terminal_players() {
    let host = host_with(open_game(1000));
    let client = Client::connect_unjoined(&host.addr().to_string()).unwrap();
    assert!(
        matches!(client.recv_timeout(Duration::from_secs(2)).unwrap(),
        eksplode_re_net::protocol::ServerMsg::RoomV2(reply)
        if matches!(*reply, eksplode_re_net::protocol::v2::Reply::Hello { .. }))
    );
    let mut request = eksplode_re_net::protocol::v2::Request::create_room(
        eksplode_re_net::protocol::v2::RequestId::new("protected-terminal-room").unwrap(),
        "Hosty".into(),
        &eksplode_re_core::RoomSettings::default(),
        4,
    );
    if let eksplode_re_net::protocol::v2::Request::CreateRoom { password, .. } = &mut request {
        *password = Some(eksplode_re_net::protocol::v2::Secret::new(
            "terminal-password-canary",
        ));
    }
    client.send_v2(request).unwrap();
    let mut owner = Online::directory(client, "Hosty".into(), KeyBindings::default_for_slot(0));
    assert!(pump_until(&mut owner, |online| online.stage
        == Stage::Lobby
        && online.slot == Some(0)));
    let owner_screen = screen(&owner);
    assert!(owner_screen.contains("Owner controls"), "{owner_screen}");
    assert!(!owner_screen.contains("terminal-password-canary"));

    let room_ref = owner.room_ref.clone().unwrap();
    let member_client = Client::connect_unjoined(&host.addr().to_string()).unwrap();
    let _hello = member_client.recv_timeout(Duration::from_secs(2)).unwrap();
    member_client
        .send_v2(eksplode_re_net::protocol::v2::Request::JoinRoom {
            request_id: eksplode_re_net::protocol::v2::RequestId::new("protected-member").unwrap(),
            server_epoch: room_ref.server_epoch.clone(),
            room_id: room_ref.room_id.clone(),
            name: "Member".into(),
            password: Some(eksplode_re_net::protocol::v2::Secret::new(
                "terminal-password-canary",
            )),
            look: None,
        })
        .unwrap();
    let mut member = Online::directory(
        member_client,
        "Member".into(),
        KeyBindings::default_for_slot(0),
    );
    assert!(pump_until(&mut member, |online| online.stage
        == Stage::Lobby
        && online.slot == Some(1)));
    let member_screen = screen(&member);
    assert!(
        member_screen.contains("Password: required"),
        "{member_screen}"
    );
    assert!(
        member_screen.contains("Only Hosty can manage room settings"),
        "{member_screen}"
    );
    assert!(!member_screen.contains("terminal-password-canary"));

    owner.handle_key(KeyCode::Esc);
    owner.handle_key(KeyCode::Enter);
    assert!(pump_until(&mut member, |online| screen(online)
        .contains("Owner controls")));
    assert!(!screen(&member).contains("Only Hosty can manage room settings"));
}

#[test]
fn protected_terminal_join_uses_a_masked_cancellable_password_prompt() {
    let host = host_with(open_game(1000));
    let owner = Client::connect_unjoined(&host.addr().to_string()).unwrap();
    assert!(
        matches!(owner.recv_timeout(Duration::from_secs(2)).unwrap(),
        eksplode_re_net::protocol::ServerMsg::RoomV2(reply)
        if matches!(*reply, eksplode_re_net::protocol::v2::Reply::Hello { .. }))
    );
    let mut create = eksplode_re_net::protocol::v2::Request::create_room(
        eksplode_re_net::protocol::v2::RequestId::new("protected-create").unwrap(),
        "Owner".into(),
        &eksplode_re_core::RoomSettings::default(),
        4,
    );
    if let eksplode_re_net::protocol::v2::Request::CreateRoom { password, .. } = &mut create {
        *password = Some(eksplode_re_net::protocol::v2::Secret::new(
            "join-password-canary",
        ));
    }
    owner.send_v2(create).unwrap();
    let welcome = owner.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(
        matches!(welcome, eksplode_re_net::protocol::ServerMsg::RoomV2(reply)
        if matches!(*reply, eksplode_re_net::protocol::v2::Reply::Welcome { .. }))
    );

    let member = Client::connect_unjoined(&host.addr().to_string()).unwrap();
    let mut member = Online::directory(member, "Member".into(), KeyBindings::default_for_slot(0));
    assert!(pump_until(&mut member, |online| online.stage
        == Stage::Directory
        && online.rooms.len() == 1));
    assert!(screen(&member).contains("password required"));
    member.handle_key(KeyCode::Enter);
    assert_eq!(member.stage, Stage::Password);
    assert!(screen(&member).contains("Password"));
    for character in "join-password-canary".chars() {
        member.handle_key(KeyCode::Char(character));
    }
    let masked = screen(&member);
    assert!(masked.contains("********************"), "{masked}");
    assert!(!masked.contains("join-password-canary"));
    member.handle_key(KeyCode::Esc);
    assert_eq!(member.stage, Stage::Directory);
    assert!(!screen(&member).contains("join-password-canary"));

    member.handle_key(KeyCode::Enter);
    assert_eq!(member.stage, Stage::Password);
    for character in "wrong-password".chars() {
        member.handle_key(KeyCode::Char(character));
    }
    member.handle_key(KeyCode::Enter);
    assert!(pump_until(&mut member, |online| online.stage
        == Stage::Password
        && online.notice.is_some()));
    let retry = screen(&member);
    assert!(retry.contains("BadPassword"), "{retry}");
    assert!(
        retry.contains("Password: "),
        "the input buffer was not cleared: {retry}"
    );
    assert!(!retry.contains("wrong-password"));
    for character in "join-password-canary".chars() {
        member.handle_key(KeyCode::Char(character));
    }
    member.handle_key(KeyCode::Enter);
    assert!(pump_until(&mut member, |online| online.stage
        == Stage::Lobby
        && online.slot == Some(1)));
    assert!(screen(&member).contains("Password: required"));
    assert!(!screen(&member).contains("join-password-canary"));
}

#[test]
fn terminal_owner_can_change_capacity_and_set_or_remove_a_room_password() {
    let host = host_with(open_game(1000));
    let mut owner = hosting(host);
    assert!(pump_until(&mut owner, |online| online.stage
        == Stage::Lobby
        && online.slot == Some(0)));
    let mut browser = Online::directory(
        Client::connect_unjoined(&owner.host_addr().unwrap().to_string()).unwrap(),
        "Observer".into(),
        KeyBindings::default_for_slot(0),
    );
    assert!(pump_until(&mut browser, |online| online.stage == Stage::Directory));
    owner.handle_key(KeyCode::Char('c'));
    browser.refresh_rooms();
    assert!(pump_until(&mut browser, |online| online
        .rooms
        .first()
        .is_some_and(|room| room.capacity == 3)));
    // 011: the seats are bot-filled, so shrinking to 3 keeps the owner and two bots
    assert!(pump_until(&mut owner, |online| screen(online)
        .contains("Capacity: 3/3")));

    owner.handle_key(KeyCode::Char('p'));
    assert_eq!(owner.stage, Stage::Password);
    for character in "owner-room-password".chars() {
        owner.handle_key(KeyCode::Char(character));
    }
    let masked = screen(&owner);
    assert!(masked.contains("*******************"), "{masked}");
    assert!(!masked.contains("owner-room-password"));
    owner.handle_key(KeyCode::Enter);
    assert!(pump_until(&mut owner, |online| online.stage
        == Stage::Lobby
        && screen(online).contains("Password: required")));
    browser.refresh_rooms();
    assert!(pump_until(&mut browser, |online| online
        .rooms
        .first()
        .is_some_and(|room| room.password_required)));

    owner.handle_key(KeyCode::Char('p'));
    assert_eq!(owner.stage, Stage::Password);
    owner.handle_key(KeyCode::Enter);
    assert!(pump_until(&mut owner, |online| online.stage
        == Stage::Lobby
        && screen(online).contains("Password: open")));
    browser.refresh_rooms();
    assert!(pump_until(&mut browser, |online| online
        .rooms
        .first()
        .is_some_and(|room| !room.password_required)));
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
    // 011: the room starts bot-filled, so remove bots (highest first) before adding one back
    assert!(pump_until(&mut h, |o| o.slot == Some(0)
        && o.lobby.slots[3].controller == Controller::Bot));
    h.handle_key(KeyCode::Char('x'));
    assert!(pump_until(&mut h, |o| o.lobby.slots[3].controller == Controller::Empty));
    h.handle_key(KeyCode::Char('x'));
    assert!(pump_until(&mut h, |o| o.lobby.slots[2].controller == Controller::Empty));
    h.handle_key(KeyCode::Char('b'));
    assert!(pump_until(&mut h, |o| o.lobby.slots[2].controller == Controller::Bot));
    assert_eq!(h.lobby.slots[3].controller, Controller::Empty);
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
    // 011: removing bots stays owner-only (adding is open to members but needs an empty seat)
    a.handle_key(KeyCode::Char('x'));
    std::thread::sleep(Duration::from_millis(100));
    a.pump();
    assert_eq!(a.stage, Stage::Lobby);
    assert_eq!(a.lobby.slots[3].controller, Controller::Bot);
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
fn a_joining_clients_custom_keys_drive_only_its_player() {
    let host = host_with(open_game(10_000));
    let mut h = hosting(host);
    assert!(pump_until(&mut h, |o| o.slot == Some(0)));
    let bindings = KeyBindings {
        up: KeyCode::Char('1'),
        down: KeyCode::Char('2'),
        left: KeyCode::Char('3'),
        right: KeyCode::Char('4'),
        bomb: KeyCode::Char('5'),
        punch: KeyCode::Char('6'),
    };
    let mut a = selected_at(h.host_addr().unwrap(), "Ann", bindings);
    assert!(pump_until(&mut a, |o| o.slot == Some(1)));
    h.handle_key(KeyCode::Enter);
    assert!(pump_until(&mut h, |o| o.stage == Stage::Playing));
    assert!(pump_until(&mut a, |o| o.stage == Stage::Playing));

    let start = a.observation.as_ref().unwrap().players[1].pos;
    a.handle_key(KeyCode::Down);
    a.send_input();
    std::thread::sleep(Duration::from_millis(40));
    a.pump();
    assert_eq!(a.observation.as_ref().unwrap().players[1].pos, start);

    let moved = pump_until(&mut a, |o| {
        o.handle_key(KeyCode::Char('2'));
        o.send_input();
        o.observation
            .as_ref()
            .is_some_and(|ob| ob.players[1].pos != start)
    });
    assert!(
        moved,
        "the joining client's configured key never moved its player"
    );
    assert_eq!(
        a.observation.as_ref().unwrap().players[0].pos,
        h.observation.as_ref().unwrap().players[0].pos
    );
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
fn a_full_room_returns_to_the_directory_with_a_message() {
    let host = host_with(open_game(1000));
    let mut firsts = filled_room(&host);
    for f in &mut firsts {
        assert!(pump_until(f, |o| o.slot.is_some()));
    }
    let mut late = joined_at(host.addr(), "late");
    assert!(pump_until(&mut late, |o| o.stage == Stage::Directory
        && o.notice.is_some()));
    assert!(late.notice.unwrap().to_lowercase().contains("full"));
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
    // 011: RemoveBot ('x') is still owner-only; AddBot ('b') is open to members but the
    // bot-filled room has no empty seat, so it is refused as Full
    a.handle_key(KeyCode::Char('x'));
    assert!(pump_until(&mut a, |o| o.notice.is_some()));
    assert!(screen(&a).contains("Only Hosty"), "{}", screen(&a));
    a.handle_key(KeyCode::Char('b'));
    assert!(pump_until(&mut a, |o| o.notice.is_some()));
    assert!(screen(&a).contains("Full"), "{}", screen(&a));
    assert!(pump_until(&mut h, |o| o.lobby.slots[1].name == "Ann"));
    assert_eq!(h.lobby.slots[3].controller, Controller::Bot);
}

#[test]
fn b_and_x_work_for_a_joined_owner() {
    let host = headless();
    let mut a = joined(&host, "Ann");
    // 011: the room starts bot-filled; 'x' frees the highest bot seat and 'b' refills it
    assert!(pump_until(&mut a, |o| o.slot == Some(0)
        && o.lobby.slots[3].controller == Controller::Bot));
    a.handle_key(KeyCode::Char('x'));
    assert!(pump_until(&mut a, |o| o.lobby.slots[3].controller == Controller::Empty));
    a.handle_key(KeyCode::Char('b'));
    assert!(pump_until(&mut a, |o| o.lobby.slots[3].controller == Controller::Bot));
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

use eksplode_re_cli::app::{self, AppError};
use eksplode_re_cli::args::{HostOptions, JoinOptions};
use eksplode_re_net::protocol::RejectReason;

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
        server_id: None,
        limits: Default::default(),
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
        s.contains(&format!("eksplode-re-cli join <your-ip>:{port}")),
        "{s}"
    );
    assert!(s.contains("q quit"), "{s}");

    h.web_url = Some("http://0.0.0.0:8080/".into());
    let s = screen(&h);
    assert!(s.contains("Web page: http://0.0.0.0:8080/"), "{s}");
}

#[test]
fn us3_as1_the_web_page_address_uses_loopback_for_wildcards() {
    assert_eq!(
        app::web_url("0.0.0.0:8080".parse().unwrap()),
        "http://127.0.0.1:8080/"
    );
    assert_eq!(
        app::web_url("0.0.0.0:9000".parse().unwrap()),
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
    assert!(w[0].contains("--bind 127.0.0.1"), "{w:?}");
    // the default game port listens on every interface
    let w = app::host_warnings(&host_opts(None, None));
    assert!(w.len() == 1 && w[0].contains("0.0.0.0"), "{w:?}");
    // loopback is quiet
    assert!(app::host_warnings(&host_opts(Some("127.0.0.1"), Some(8080))).is_empty());
    assert!(!app::host_warnings(&host_opts(Some("0.0.0.0"), Some(8080))).is_empty());
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
        let l = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let err = app::run_join(&JoinOptions {
        addr: format!("0.0.0.0:{port}"),
        name: "Ann".into(),
        key_bindings: eksplode_re_cli::input::KeyBindings::default_for_slot(0),
        room_id: None,
    })
    .expect_err("nobody listens there");
    let text = one_line(&err);
    assert!(text.contains(&format!("0.0.0.0:{port}")), "{text}");
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
    let mut firsts = filled_room(&host);
    for f in &mut firsts {
        assert!(pump_until(f, |o| o.slot.is_some()));
    }
    let mut late = joined_at(host.addr(), "late");
    assert!(late.rejection_shown(Duration::ZERO).is_none());
    assert!(pump_until(&mut late, |o| o.stage == Stage::Directory
        && o.notice.is_some()));
    assert_eq!(
        late.rejection_shown(Duration::ZERO),
        Some(RejectReason::Full)
    );
    assert_eq!(late.rejection_shown(Duration::from_secs(60)), None);
    assert!(firsts[0].rejection_shown(Duration::ZERO).is_none());
    firsts[3].handle_key(KeyCode::Esc);
    firsts[3].handle_key(KeyCode::Enter);
    assert!(pump_until(&mut firsts[3], |online| online.stage == Stage::Directory));
    late.refresh_rooms();
    // 011: the leaver's seat is refilled with a bot, which a joining human then replaces
    assert!(pump_until(&mut late, |online| online.rooms.len() == 1
        && online.rooms[0].occupied == 4
        && online.rooms[0].connected_humans == 3));
    late.handle_key(KeyCode::Enter);
    assert!(pump_until(&mut late, |online| online.slot == Some(3)));
    assert!(
        late.rejection_shown(Duration::ZERO).is_none(),
        "successful retry clears the previous refusal"
    );
}

#[test]
fn us3_as3_names_and_running_matches_are_refused_with_a_reason() {
    let host = host_with(open_game(10_000));
    let mut bad = joined(&host, "");
    assert!(pump_until(&mut bad, |o| o.stage == Stage::Directory
        && o.notice.is_some()));
    assert_eq!(
        bad.rejection_shown(Duration::ZERO),
        Some(RejectReason::BadName)
    );

    let mut a = joined(&host, "Ann");
    assert!(pump_until(&mut a, |o| o.slot == Some(0)));
    a.handle_key(KeyCode::Enter);
    assert!(pump_until(&mut a, |o| o.stage == Stage::Playing));
    let mut late = joined_at(host.addr(), "Late");
    assert!(pump_until(&mut late, |o| o.stage == Stage::Directory
        && o.notice.is_some()));
    assert_eq!(
        late.rejection_shown(Duration::ZERO),
        Some(RejectReason::InMatch)
    );
}

#[test]
fn f3_us1_as7_terminal_players_get_a_default_look_and_keep_working() {
    let host = host_with(open_game(1000));
    // a browser-like participant joins first and chooses a look
    let browser = Client::connect_unjoined(&host.addr().to_string()).unwrap();
    let mut request = eksplode_re_net::protocol::v2::Request::create_room(
        eksplode_re_net::protocol::v2::RequestId::new("browser-create").unwrap(),
        "Web".into(),
        &eksplode_re_core::RoomSettings::default(),
        4,
    );
    if let eksplode_re_net::protocol::v2::Request::CreateRoom { look, .. } = &mut request {
        *look = Some(r#"{"colour":"Green","head":"Crown"}"#.parse().unwrap());
    }
    browser.send_v2(request).unwrap();
    let welcome = std::iter::from_fn(|| browser.recv_timeout(Duration::from_secs(2)).ok())
        .map(eksplode_re_net::protocol::ServerMsg::into_game_message)
        .find(|message| {
            matches!(
                message,
                eksplode_re_net::protocol::ServerMsg::Welcome { .. }
            )
        });
    assert!(welcome.is_some());
    let mut a = joined_at(host.addr(), "Term");
    assert!(pump_until(&mut a, |o| o.slot == Some(1) && o.stage == Stage::Lobby));
    let s = screen(&a);
    assert!(s.contains("Web") && s.contains("Term"), "{s}");
    // later lobby messages carry looks too and are applied without trouble
    a.handle_key(KeyCode::Char('r'));
    assert!(pump_until(&mut a, |o| o.lobby.slots[1].ready));
    assert_eq!(a.stage, Stage::Lobby);
    drop(browser);
}

fn lobby_text_has(o: &Online, needle: &str) -> bool {
    screen(o).contains(needle)
}

fn room_host(defaults: eksplode_re_core::RoomSettings) -> Host {
    Host::start(HostConfig {
        bind: "0.0.0.0:0".into(),
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
    use eksplode_re_core::{BoardSize, RoomSettings};
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
    assert!(pump_until(&mut h, |o| o.lobby.settings.mode
        == eksplode_re_core::Mode::Team));
    assert!(eksplode_re_core::PLAYABLE.contains(&h.lobby.settings.mode));

    // a terminal player sees the same line, with the friendly-fire choice of Team mode
    let mut a = joined_at(h.host_addr().unwrap(), "Ann");
    assert!(pump_until(&mut a, |o| o.slot == Some(1)));
    assert!(
        lobby_text_has(&a, "Lab - Team - 19 x 15 - 5 min - friendly fire off"),
        "{}",
        screen(&a)
    );
}

#[test]
fn only_the_owner_changes_the_settings_in_the_terminal_lobby() {
    use eksplode_re_core::BoardSize;
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

#[test]
fn the_lobby_shows_the_settings_on_the_smallest_terminal() {
    let host = room_host(Default::default());
    let mut h = hosting(host);
    assert!(pump_until(&mut h, |o| o.slot == Some(0)));
    h.handle_key(KeyCode::Char('s'));
    assert!(pump_until(&mut h, |o| o.lobby.settings.size
        == eksplode_re_core::BoardSize::Medium));
    h.handle_key(KeyCode::Char('s'));
    assert!(pump_until(&mut h, |o| o.lobby.settings.size
        == eksplode_re_core::BoardSize::Large));
    let mut t = Terminal::new(TestBackend::new(40, 20)).unwrap();
    t.draw(|f| render(f, &h)).unwrap();
    let buf = t.backend().buffer().clone();
    let text: String = (0..20)
        .map(|y| {
            (0..40)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("Hosty") && text.contains("Large") || text.contains("25 x"),
        "{text}"
    );
}

#[test]
fn f4_us3_as5_the_terminal_lobby_shows_the_teams_and_the_result_names_the_winner() {
    use eksplode_re_core::{Mode, RoomSettings};
    let host = room_host(RoomSettings {
        mode: Mode::Team,
        ..Default::default()
    });
    let mut h = hosting(host);
    assert!(pump_until(&mut h, |o| o.slot == Some(0)));
    for _ in 0..3 {
        h.handle_key(KeyCode::Char('b'));
    }
    assert!(pump_until(&mut h, |o| o.lobby.participants() == 4));
    let s = screen(&h);
    assert!(s.contains("Team"), "{s}");
    let line = |slot: &str| {
        s.lines()
            .find(|l| l.contains(slot))
            .unwrap_or_default()
            .to_string()
    };
    assert!(
        line("Slot 1").contains("team A") && line("Slot 2").contains("team A"),
        "{s}"
    );
    assert!(
        line("Slot 3").contains("team B") && line("Slot 4").contains("team B"),
        "{s}"
    );
}

#[test]
fn f4_us4_as9_the_chaser_lobby_says_the_roles_are_drawn_at_the_start() {
    use eksplode_re_core::{Mode, RoomSettings};
    let host = room_host(RoomSettings {
        mode: Mode::Chaser,
        ..Default::default()
    });
    let mut h = hosting(host);
    assert!(pump_until(&mut h, |o| o.slot == Some(0)));
    let s = screen(&h);
    assert!(s.contains("Chaser"), "{s}");
    assert!(s.contains("roles are drawn at the start"), "{s}");
    assert!(!s.contains("[team"), "{s}");
}

#[test]
fn f11_the_room_list_counts_humans_and_shows_the_bots_apart() {
    // 011: a new room is bot-filled, so occupied seats alone would read as a full room
    let host = host_with(open_game(1000));
    let mut owner = hosting(host);
    assert!(pump_until(&mut owner, |online| online.stage
        == Stage::Lobby
        && online.slot == Some(0)));
    let mut browser = Online::directory(
        Client::connect_unjoined(&owner.host_addr().unwrap().to_string()).unwrap(),
        "Observer".into(),
        KeyBindings::default_for_slot(0),
    );
    assert!(pump_until(&mut browser, |online| online.stage == Stage::Directory));
    browser.refresh_rooms();
    assert!(pump_until(&mut browser, |online| screen(online)
        .contains("1/4 +3 bots")));
}
