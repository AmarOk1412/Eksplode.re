#![allow(clippy::unwrap_used)]

use eksplode_re_cli::setup::{render, SetupScreen};
use eksplode_re_core::{BoardSize, Mode, RoomSettings};
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyCode;
use ratatui::Terminal;

fn screen() -> SetupScreen {
    SetupScreen::new(RoomSettings::default())
}

fn text(s: &SetupScreen) -> String {
    let mut terminal = Terminal::new(TestBackend::new(70, 14)).unwrap();
    terminal.draw(|f| render(f, s)).unwrap();
    let buf = terminal.backend().buffer().clone();
    (0..14)
        .map(|y| {
            (0..70)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn keys(s: &mut SetupScreen, list: &[KeyCode]) {
    for k in list {
        s.handle_key(*k);
    }
}

fn type_text(s: &mut SetupScreen, t: &str) {
    for c in t.chars() {
        s.handle_key(KeyCode::Char(c));
    }
}

#[test]
fn it_shows_the_defaults_and_the_key_help() {
    let t = text(&screen());
    assert!(t.contains("ROOM SETUP"), "{t}");
    assert!(t.contains("Mode") && t.contains("Normal"), "{t}");
    assert!(t.contains("Board size") && t.contains("Small") && t.contains("15 x 13"));
    assert!(t.contains("Round time") && t.contains("3 min"));
    assert!(t.contains("Create room"));
    assert!(t.contains("Up/Down select") && t.contains("Left/Right change"));
    assert!(!t.contains("Friendly fire"), "only Team shows it");
}

#[test]
fn up_and_down_select_and_left_and_right_change() {
    let mut s = screen();
    assert_eq!(s.selected, 0);
    keys(&mut s, &[KeyCode::Down, KeyCode::Down]);
    assert_eq!(s.selected, 2);
    keys(&mut s, &[KeyCode::Right]);
    assert_eq!(s.settings.size, BoardSize::Medium);
    keys(&mut s, &[KeyCode::Right, KeyCode::Right]);
    assert_eq!(s.settings.size, BoardSize::Small, "wraps around");
    keys(&mut s, &[KeyCode::Left]);
    assert_eq!(s.settings.size, BoardSize::Large);

    keys(&mut s, &[KeyCode::Down, KeyCode::Right]);
    assert_eq!(s.settings.minutes, 5);
    keys(&mut s, &[KeyCode::Left, KeyCode::Left]);
    assert_eq!(s.settings.minutes, 2);
    keys(
        &mut s,
        &[KeyCode::Up, KeyCode::Up, KeyCode::Up, KeyCode::Up],
    );
    assert_eq!(s.selected, 0, "Up stops at the first row");
}

#[test]
fn the_mode_cycles_through_the_playable_modes_only() {
    let mut s = screen();
    keys(&mut s, &[KeyCode::Down, KeyCode::Right]);
    assert!(eksplode_re_core::PLAYABLE.contains(&s.settings.mode));
    for _ in 0..5 {
        keys(&mut s, &[KeyCode::Right]);
        assert!(eksplode_re_core::PLAYABLE.contains(&s.settings.mode));
    }
}

#[test]
fn friendly_fire_is_a_row_only_in_team_mode() {
    let mut s = SetupScreen::new(RoomSettings {
        mode: Mode::Team,
        ..RoomSettings::default()
    });
    let t = text(&s);
    if eksplode_re_core::PLAYABLE.contains(&Mode::Team) {
        assert!(t.contains("Friendly fire") && t.contains("on"), "{t}");
        keys(
            &mut s,
            &[KeyCode::Down, KeyCode::Down, KeyCode::Down, KeyCode::Down],
        );
        keys(&mut s, &[KeyCode::Right]);
        assert!(!s.settings.friendly_fire);
    } else {
        // the mode is not playable yet: the screen never offers it
        assert_eq!(s.settings.mode, Mode::Normal);
    }
}

#[test]
fn typing_edits_the_name_up_to_24_characters() {
    let mut s = screen();
    type_text(&mut s, "Friday room");
    assert_eq!(s.settings.name, "Friday room");
    keys(&mut s, &[KeyCode::Backspace, KeyCode::Backspace]);
    assert_eq!(s.settings.name, "Friday ro");
    type_text(&mut s, &"x".repeat(40));
    assert_eq!(s.settings.name.chars().count(), 24);
    assert!(text(&s).contains("Friday ro"));
    // letters are text on the name row, not commands
    assert!(!s.done && !s.quit);
}

#[test]
fn other_rows_ignore_typed_letters() {
    let mut s = screen();
    keys(&mut s, &[KeyCode::Down]);
    type_text(&mut s, "qz");
    assert!(!s.quit);
    assert_eq!(s.settings.name, "");
}

#[test]
fn enter_on_create_room_confirms_and_esc_goes_back() {
    let mut s = screen();
    keys(&mut s, &[KeyCode::Enter]);
    assert!(!s.done, "Enter on a setting does not create the room");
    let last = s.rows() - 1;
    while s.selected < last {
        s.handle_key(KeyCode::Down);
    }
    s.handle_key(KeyCode::Enter);
    assert!(s.done);
    assert!(!s.quit);

    let mut s = screen();
    s.handle_key(KeyCode::Esc);
    assert!(s.quit && !s.done);
}

#[test]
fn control_characters_are_not_added_to_the_name() {
    let mut s = screen();
    s.handle_key(KeyCode::Char('\u{7}'));
    s.handle_key(KeyCode::Char('\t'));
    assert_eq!(s.settings.name, "");
}
