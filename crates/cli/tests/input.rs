use bomberust_cli::input::{map_key, InputState, KeyAction, PlayerAction};
use bomberust_core::Dir;
use ratatui::crossterm::event::KeyCode;

fn player(slot: usize, action: PlayerAction) -> Option<KeyAction> {
    Some(KeyAction::Player { slot, action })
}

#[test]
fn each_key_set_maps_to_its_player() {
    let sets: [(usize, [KeyCode; 5]); 4] = [
        (
            0,
            [
                KeyCode::Up,
                KeyCode::Down,
                KeyCode::Left,
                KeyCode::Right,
                KeyCode::Char(' '),
            ],
        ),
        (1, ['w', 's', 'a', 'd', 'e'].map(KeyCode::Char)),
        (2, ['i', 'k', 'j', 'l', 'o'].map(KeyCode::Char)),
        (3, ['t', 'g', 'f', 'h', 'y'].map(KeyCode::Char)),
    ];
    for (slot, keys) in sets {
        let dirs = [Dir::Up, Dir::Down, Dir::Left, Dir::Right];
        for (key, dir) in keys.iter().zip(dirs) {
            assert_eq!(
                map_key(*key),
                player(slot, PlayerAction::Move(dir)),
                "{key:?}"
            );
        }
        assert_eq!(map_key(keys[4]), player(slot, PlayerAction::Bomb));
    }
}

#[test]
fn letters_are_case_insensitive() {
    assert_eq!(
        map_key(KeyCode::Char('W')),
        player(1, PlayerAction::Move(Dir::Up))
    );
}

#[test]
fn command_keys() {
    assert_eq!(map_key(KeyCode::Char('q')), Some(KeyAction::Quit));
    assert_eq!(map_key(KeyCode::Char('r')), Some(KeyAction::Rematch));
    assert_eq!(map_key(KeyCode::Char('p')), Some(KeyAction::Pause));
}

#[test]
fn unknown_keys_are_ignored() {
    assert_eq!(map_key(KeyCode::Char('x')), None);
    assert_eq!(map_key(KeyCode::F(5)), None);
    assert_eq!(map_key(KeyCode::Enter), None);
}

#[test]
fn last_direction_pressed_wins() {
    let mut s = InputState::new(4);
    s.press(0, PlayerAction::Move(Dir::Up));
    s.press(0, PlayerAction::Move(Dir::Left));
    assert_eq!(s.take()[0].mv, Some(Dir::Left));
}

#[test]
fn a_direction_is_held_for_a_few_ticks() {
    let mut s = InputState::new(4);
    s.press(1, PlayerAction::Move(Dir::Down));
    for _ in 0..4 {
        assert_eq!(s.take()[1].mv, Some(Dir::Down));
    }
    assert_eq!(s.take()[1].mv, None);
}

#[test]
fn bomb_presses_fire_once() {
    let mut s = InputState::new(4);
    s.press(2, PlayerAction::Bomb);
    assert!(s.take()[2].bomb);
    assert!(!s.take()[2].bomb);
}

#[test]
fn out_of_range_slots_are_ignored() {
    let mut s = InputState::new(4);
    s.press(7, PlayerAction::Bomb);
    assert!(s.take().iter().all(|i| !i.bomb));
}

#[test]
fn clear_forgets_pending_input() {
    let mut s = InputState::new(4);
    s.press(0, PlayerAction::Move(Dir::Up));
    s.press(0, PlayerAction::Bomb);
    s.clear();
    let i = s.take()[0];
    assert_eq!((i.mv, i.bomb), (None, false));
}

#[test]
fn f3_us2_as9_each_player_has_a_punch_key_that_clashes_with_nothing() {
    for (slot, key) in [(0, 'z'), (1, 'c'), (2, 'u'), (3, 'n')] {
        assert_eq!(
            map_key(KeyCode::Char(key)),
            player(slot, PlayerAction::Punch),
            "{key}"
        );
    }
    // every other key keeps its meaning
    assert_eq!(map_key(KeyCode::Char('q')), Some(KeyAction::Quit));
    assert_eq!(map_key(KeyCode::Char('p')), Some(KeyAction::Pause));
    assert_eq!(map_key(KeyCode::Char('r')), Some(KeyAction::Rematch));
    assert_eq!(map_key(KeyCode::Char('b')), None);
    assert_eq!(map_key(KeyCode::Char('x')), None);
    assert_eq!(map_key(KeyCode::Char('e')), player(1, PlayerAction::Bomb));
}

#[test]
fn punch_presses_fire_once() {
    let mut s = InputState::new(4);
    s.press(1, PlayerAction::Punch);
    assert!(s.take()[1].punch);
    assert!(!s.take()[1].punch);
    s.press(1, PlayerAction::Punch);
    s.clear();
    assert!(!s.take()[1].punch);
}
