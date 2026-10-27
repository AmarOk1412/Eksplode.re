#![allow(clippy::unwrap_used)]

use eksplode_re_cli::app::Session;
use eksplode_re_cli::args::LocalOptions;
use eksplode_re_core::{Controller, Event, Lobby, Outcome, Phase, Pos};
use ratatui::crossterm::event::KeyCode;

fn session(players: u8, bots: u8) -> Session {
    Session::new(
        &LocalOptions {
            players,
            bots,
            seed: None,
            lobby: false,
            room: Default::default(),
        },
        7,
    )
    .unwrap()
}

#[test]
fn p1_keys_move_player_one() {
    let mut s = session(1, 0);
    let start = s.state.players[0].pos;
    // Down is open at the start corner.
    s.handle_key(KeyCode::Down);
    let events = s.tick();
    assert!(events.iter().any(|e| matches!(e, Event::Moved { .. })));
    assert_eq!(s.state.players[0].pos, Pos::new(start.x, start.y + 1));
}

#[test]
fn keys_of_missing_humans_are_ignored() {
    let mut s = session(1, 0);
    s.handle_key(KeyCode::Char('w'));
    let before = s.state.players[1].pos;
    s.tick();
    assert_eq!(s.state.players[1].pos, before);
}

#[test]
fn space_places_a_bomb() {
    let mut s = session(1, 0);
    s.handle_key(KeyCode::Char(' '));
    s.tick();
    assert_eq!(s.state.bombs.len(), 1);
}

#[test]
fn pause_stops_time() {
    let mut s = session(1, 0);
    s.handle_key(KeyCode::Char('p'));
    assert!(s.paused);
    assert!(s.tick().is_empty());
    assert_eq!(s.state.tick, 0);
    s.handle_key(KeyCode::Char('p'));
    s.tick();
    assert_eq!(s.state.tick, 1);
}

#[test]
fn quit_sets_the_flag() {
    let mut s = session(1, 0);
    s.handle_key(KeyCode::Char('q'));
    assert!(s.quit);
}

#[test]
fn rematch_only_after_the_round_ended() {
    let mut s = session(2, 0);
    s.handle_key(KeyCode::Char(' '));
    s.tick();
    s.handle_key(KeyCode::Char('r'));
    assert_eq!(s.state.bombs.len(), 1, "round still running, r ignored");

    s.state.phase = Phase::Ended(Outcome::Draw);
    s.handle_key(KeyCode::Char('r'));
    assert_eq!(s.state.phase, Phase::Playing);
    assert!(s.state.bombs.is_empty());
    assert_eq!(s.state.tick, 0);
}

#[test]
fn context_tags_humans_bots_and_unused_slots() {
    let s = session(2, 1);
    let ctx = s.context();
    assert_eq!(ctx.tags[0].as_deref(), Some("human"));
    assert_eq!(ctx.tags[1].as_deref(), Some("human"));
    assert_eq!(ctx.tags[2].as_deref(), Some("bot"));
    assert_eq!(ctx.tags[3], None);
}

#[test]
fn a_solo_round_can_be_played_to_a_draw_by_self_destruction() {
    let mut s = session(1, 0);
    s.handle_key(KeyCode::Char(' '));
    for _ in 0..200 {
        s.tick();
    }
    assert_eq!(s.state.phase, Phase::Ended(Outcome::Draw));
}

#[test]
fn bots_act_on_their_own() {
    let mut s = session(1, 3);
    let start: Vec<Pos> = s.state.players.iter().map(|p| p.pos).collect();
    let mut placed = 0;
    for _ in 0..200 {
        placed += s
            .tick()
            .iter()
            .filter(|e| matches!(e, Event::BombPlaced { player, .. } if player.index() > 0))
            .count();
    }
    let moved = (1..4).any(|i| s.state.players[i].pos != start[i]);
    assert!(moved || placed > 0, "bots stayed idle");
}

#[test]
fn bot_slots_do_not_react_to_keys() {
    let mut s = session(1, 0);
    // slot 1 is empty here, so a key press for it must not create a bomb
    s.handle_key(KeyCode::Char('e'));
    assert!(s
        .tick()
        .iter()
        .all(|e| !matches!(e, Event::BombPlaced { .. })));

    let mut s = session(1, 3);
    s.handle_key(KeyCode::Char('e'));
    let events = s.tick();
    assert!(!events
        .iter()
        .any(|e| matches!(e, Event::BombPlaced { player, .. } if player.index() == 1)));
}

#[test]
fn a_bots_only_match_ends_by_itself() {
    let mut s = session(0, 4);
    for _ in 0..4000 {
        s.tick();
        if s.state.phase != Phase::Playing {
            break;
        }
    }
    assert_ne!(s.state.phase, Phase::Playing);
}

#[test]
fn a_session_can_start_from_a_lobby() {
    let mut lobby = Lobby::new();
    lobby.assign(2, Controller::Local, "Ann").unwrap();
    lobby.assign(3, Controller::Bot, "Bot 4").unwrap();
    let mut s = Session::from_lobby(&lobby, 5).unwrap();
    assert!(!s.state.players[0].alive && !s.state.players[1].alive);
    assert!(s.state.players[2].alive && s.state.players[3].alive);
    // slot 2 uses the I J K L O keys
    s.handle_key(KeyCode::Char('o'));
    s.tick();
    assert!(s.state.bombs.iter().any(|b| b.owner.index() == 2));
}

#[test]
fn a_lobby_without_participants_is_an_error() {
    assert!(Session::from_lobby(&Lobby::new(), 1).is_err());
}
