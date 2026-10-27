#![allow(clippy::unwrap_used)]

use eksplode_re_cli::lobby::{render, LobbyScreen};
use eksplode_re_core::{Controller, Lobby};
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;
use ratatui::Terminal;

fn screen() -> LobbyScreen {
    LobbyScreen::new(Lobby::new())
}

fn text(s: &LobbyScreen) -> String {
    let mut terminal = Terminal::new(TestBackend::new(60, 14)).unwrap();
    terminal.draw(|f| render(f, s)).unwrap();
    let buf = terminal.backend().buffer().clone();
    (0..14)
        .map(|y| {
            (0..60)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn us2_as1_b_gives_the_selected_slot_to_a_bot() {
    let mut s = screen();
    s.handle_key(KeyCode::Char('b'));
    assert_eq!(s.lobby.slots[0].controller, Controller::Bot);
    s.handle_key(KeyCode::Down);
    s.handle_key(KeyCode::Char('b'));
    assert_eq!(s.lobby.slots[1].controller, Controller::Bot);
}

#[test]
fn pressing_b_again_removes_the_bot_and_frees_the_slot() {
    let mut s = screen();
    s.handle_key(KeyCode::Char('b'));
    s.handle_key(KeyCode::Char('b'));
    assert_eq!(s.lobby.slots[0].controller, Controller::Empty);
    assert_eq!(s.lobby.participants(), 0);
}

#[test]
fn h_toggles_a_local_human() {
    let mut s = screen();
    s.handle_key(KeyCode::Char('h'));
    assert_eq!(s.lobby.slots[0].controller, Controller::Local);
    s.handle_key(KeyCode::Char('h'));
    assert_eq!(s.lobby.slots[0].controller, Controller::Empty);
}

#[test]
fn a_human_slot_is_not_replaced_by_a_bot() {
    let mut s = screen();
    s.handle_key(KeyCode::Char('h'));
    s.handle_key(KeyCode::Char('b'));
    assert_eq!(s.lobby.slots[0].controller, Controller::Local);
}

#[test]
fn the_selection_stays_inside_the_four_slots() {
    let mut s = screen();
    s.handle_key(KeyCode::Up);
    assert_eq!(s.selected, 0);
    for _ in 0..10 {
        s.handle_key(KeyCode::Down);
    }
    assert_eq!(s.selected, 3);
}

#[test]
fn enter_starts_only_with_a_participant() {
    let mut s = screen();
    s.handle_key(KeyCode::Enter);
    assert!(!s.start);
    s.handle_key(KeyCode::Char('b'));
    s.handle_key(KeyCode::Enter);
    assert!(s.start);
}

#[test]
fn q_quits() {
    let mut s = screen();
    s.handle_key(KeyCode::Char('q'));
    assert!(s.quit);
}

#[test]
fn the_view_lists_all_slots_and_keys() {
    let mut s = screen();
    s.handle_key(KeyCode::Char('h'));
    s.handle_key(KeyCode::Down);
    s.handle_key(KeyCode::Char('b'));
    let t = text(&s);
    for needle in ["Slot 1", "Slot 4", "Human", "Bot", "Empty", "Enter"] {
        assert!(t.contains(needle), "missing {needle}:\n{t}");
    }
}
