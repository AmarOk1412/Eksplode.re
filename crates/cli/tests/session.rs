#![allow(clippy::unwrap_used)]

use bomberust_cli::app::Session;
use bomberust_cli::args::LocalOptions;
use bomberust_core::{Event, Outcome, Phase, Pos};
use ratatui::crossterm::event::KeyCode;

fn session(players: u8, bots: u8) -> Session {
    Session::new(
        &LocalOptions {
            players,
            bots,
            seed: None,
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
fn context_tags_humans_and_unused_slots() {
    let s = session(2, 0);
    let ctx = s.context();
    assert_eq!(ctx.tags[0].as_deref(), Some("human"));
    assert_eq!(ctx.tags[1].as_deref(), Some("human"));
    assert_eq!(ctx.tags[2], None);
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
