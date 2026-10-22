use bomberust_cli::input::normalize;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, mods)
}

#[test]
fn every_way_of_sending_enter_means_enter() {
    let none = KeyModifiers::NONE;
    let ctrl = KeyModifiers::CONTROL;
    for (name, ev) in [
        ("Enter", key(KeyCode::Enter, none)),
        ("CR char", key(KeyCode::Char('\r'), none)),
        ("LF char", key(KeyCode::Char('\n'), none)),
        ("Ctrl+J", key(KeyCode::Char('j'), ctrl)),
        ("Ctrl+M", key(KeyCode::Char('m'), ctrl)),
        ("Ctrl+Shift+J", key(KeyCode::Char('J'), ctrl)),
    ] {
        assert_eq!(normalize(ev), Some(KeyCode::Enter), "{name}");
    }
}

#[test]
fn plain_letters_are_not_enter() {
    let none = KeyModifiers::NONE;
    assert_eq!(
        normalize(key(KeyCode::Char('j'), none)),
        Some(KeyCode::Char('j'))
    );
    assert_eq!(
        normalize(key(KeyCode::Char('m'), none)),
        Some(KeyCode::Char('m'))
    );
    assert_eq!(normalize(key(KeyCode::Up, none)), Some(KeyCode::Up));
    assert_eq!(
        normalize(key(KeyCode::Char('A'), KeyModifiers::SHIFT)),
        Some(KeyCode::Char('A'))
    );
}

#[test]
fn other_ctrl_and_alt_combinations_are_ignored() {
    assert_eq!(
        normalize(key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        None
    );
    assert_eq!(
        normalize(key(KeyCode::Char('q'), KeyModifiers::CONTROL)),
        None
    );
    assert_eq!(normalize(key(KeyCode::Char('b'), KeyModifiers::ALT)), None);
}

#[test]
fn key_releases_are_ignored() {
    let ev = KeyEvent::new_with_kind(KeyCode::Enter, KeyModifiers::NONE, KeyEventKind::Release);
    assert_eq!(normalize(ev), None);
    let ev = KeyEvent::new_with_kind(KeyCode::Enter, KeyModifiers::NONE, KeyEventKind::Repeat);
    assert_eq!(normalize(ev), Some(KeyCode::Enter));
}
