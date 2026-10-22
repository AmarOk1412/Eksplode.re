#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use bomberust_cli::online::{render, Online, Stage};
use bomberust_core::{Config, Controller};
use bomberust_net::{Client, Host, HostConfig};
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;
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
