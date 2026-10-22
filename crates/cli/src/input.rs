//! Key mapping for up to four players sharing one keyboard.

use bomberust_core::{Dir, PlayerInput};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// What a player key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerAction {
    Move(Dir),
    Bomb,
}

/// The meaning of a key press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    Player { slot: usize, action: PlayerAction },
    Quit,
    Rematch,
    Pause,
}

/// Per slot keys: up, down, left, right, bomb.
const KEYS: [[char; 5]; 3] = [
    ['w', 's', 'a', 'd', 'e'],
    ['i', 'k', 'j', 'l', 'o'],
    ['t', 'g', 'f', 'h', 'y'],
];

/// Turns a raw terminal key event into the key the game should react to.
///
/// Enter arrives in several forms (CR, LF, Ctrl+J, Ctrl+M) depending on the terminal; all of
/// them become [`KeyCode::Enter`]. Other Ctrl/Alt combinations are dropped so they are never
/// mistaken for plain letters, and key releases are ignored.
pub fn normalize(ev: KeyEvent) -> Option<KeyCode> {
    if ev.kind == KeyEventKind::Release {
        return None;
    }
    let modified = ev
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
    match ev.code {
        KeyCode::Enter | KeyCode::Char('\r' | '\n') => Some(KeyCode::Enter),
        KeyCode::Char(c) if modified && matches!(c.to_ascii_lowercase(), 'j' | 'm') => {
            Some(KeyCode::Enter)
        }
        KeyCode::Char(_) if modified => None,
        code => Some(code),
    }
}

/// Translates a key code; unknown keys give `None`.
pub fn map_key(code: KeyCode) -> Option<KeyAction> {
    let player = |slot, action| Some(KeyAction::Player { slot, action });
    match code {
        KeyCode::Up => player(0, PlayerAction::Move(Dir::Up)),
        KeyCode::Down => player(0, PlayerAction::Move(Dir::Down)),
        KeyCode::Left => player(0, PlayerAction::Move(Dir::Left)),
        KeyCode::Right => player(0, PlayerAction::Move(Dir::Right)),
        KeyCode::Char(' ') => player(0, PlayerAction::Bomb),
        KeyCode::Char(c) => {
            let c = c.to_ascii_lowercase();
            match c {
                'q' => return Some(KeyAction::Quit),
                'r' => return Some(KeyAction::Rematch),
                'p' => return Some(KeyAction::Pause),
                _ => {}
            }
            for (i, keys) in KEYS.iter().enumerate() {
                let Some(k) = keys.iter().position(|x| *x == c) else {
                    continue;
                };
                let action = match k {
                    0 => PlayerAction::Move(Dir::Up),
                    1 => PlayerAction::Move(Dir::Down),
                    2 => PlayerAction::Move(Dir::Left),
                    3 => PlayerAction::Move(Dir::Right),
                    _ => PlayerAction::Bomb,
                };
                return player(i + 1, action);
            }
            None
        }
        _ => None,
    }
}

/// Collects key presses into one [`PlayerInput`] per tick.
///
/// Terminals report no key releases, so a pressed direction stays active for `hold_ticks`
/// ticks; key repeat keeps it alive while the key is held down.
#[derive(Clone, Debug)]
pub struct InputState {
    hold_ticks: u32,
    held: [Option<(Dir, u32)>; 4],
    bomb: [bool; 4],
}

impl InputState {
    pub fn new(hold_ticks: u32) -> Self {
        Self {
            hold_ticks,
            held: [None; 4],
            bomb: [false; 4],
        }
    }

    /// Records a key press for `slot`; the latest direction wins.
    pub fn press(&mut self, slot: usize, action: PlayerAction) {
        if slot >= 4 {
            return;
        }
        match action {
            PlayerAction::Move(d) => self.held[slot] = Some((d, self.hold_ticks)),
            PlayerAction::Bomb => self.bomb[slot] = true,
        }
    }

    /// Inputs for the next tick; bomb presses are consumed, directions age by one tick.
    pub fn take(&mut self) -> [PlayerInput; 4] {
        let mut out = [PlayerInput::default(); 4];
        for (slot, input) in out.iter_mut().enumerate() {
            if let Some((dir, left)) = self.held[slot] {
                input.mv = Some(dir);
                self.held[slot] = left.checked_sub(1).filter(|l| *l > 0).map(|l| (dir, l));
            }
            input.bomb = std::mem::take(&mut self.bomb[slot]);
        }
        out
    }

    /// Forgets everything, e.g. when pausing or restarting.
    pub fn clear(&mut self) {
        self.held = [None; 4];
        self.bomb = [false; 4];
    }
}
