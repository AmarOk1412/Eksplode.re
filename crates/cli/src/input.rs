//! Key mapping for up to four players sharing one keyboard.

use eksplode_re_core::{Dir, PlayerInput};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// What a player key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerAction {
    Move(Dir),
    Bomb,
    /// Punch the bomb in front of the player (needs the punch power).
    Punch,
}

/// The meaning of a key press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    Player { slot: usize, action: PlayerAction },
    Quit,
    Rematch,
    Pause,
}

/// The six keyboard actions assigned to one terminal player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyBindings {
    pub up: KeyCode,
    pub down: KeyCode,
    pub left: KeyCode,
    pub right: KeyCode,
    pub bomb: KeyCode,
    pub punch: KeyCode,
}

impl KeyBindings {
    /// Existing default keys for a terminal player slot (0-3).
    pub fn default_for_slot(slot: usize) -> Self {
        match slot {
            1 => Self::letters(['w', 's', 'a', 'd', 'e', 'c']),
            2 => Self::letters(['i', 'k', 'j', 'l', 'o', 'u']),
            3 => Self::letters(['t', 'g', 'f', 'h', 'y', 'n']),
            _ => Self {
                up: KeyCode::Up,
                down: KeyCode::Down,
                left: KeyCode::Left,
                right: KeyCode::Right,
                bomb: KeyCode::Char(' '),
                punch: KeyCode::Char('z'),
            },
        }
    }

    fn letters(keys: [char; 6]) -> Self {
        Self {
            up: KeyCode::Char(keys[0]),
            down: KeyCode::Char(keys[1]),
            left: KeyCode::Char(keys[2]),
            right: KeyCode::Char(keys[3]),
            bomb: KeyCode::Char(keys[4]),
            punch: KeyCode::Char(keys[5]),
        }
    }

    pub(crate) fn keys(self) -> [KeyCode; 6] {
        [
            self.up, self.down, self.left, self.right, self.bomb, self.punch,
        ]
    }
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self::default_for_slot(0)
    }
}

/// The existing per-slot mappings, preserved when a slot is not configured.
pub fn default_key_bindings() -> [KeyBindings; 4] {
    std::array::from_fn(KeyBindings::default_for_slot)
}

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
    map_key_with_bindings(code, &default_key_bindings())
}

/// Maps one key using four independently configured local-player bindings.
pub fn map_key_with_bindings(code: KeyCode, bindings: &[KeyBindings; 4]) -> Option<KeyAction> {
    if let Some(command) = command_key(code) {
        return Some(command);
    }
    for (slot, binding) in bindings.iter().enumerate() {
        if let Some(action) = player_action(code, *binding) {
            return Some(KeyAction::Player { slot, action });
        }
    }
    None
}

/// Maps a key only against one player's controls, preserving terminal command keys.
pub fn map_key_for_player(code: KeyCode, bindings: KeyBindings, slot: usize) -> Option<KeyAction> {
    command_key(code)
        .or_else(|| player_action(code, bindings).map(|action| KeyAction::Player { slot, action }))
}

fn command_key(code: KeyCode) -> Option<KeyAction> {
    match code {
        KeyCode::Char(c) => match c.to_ascii_lowercase() {
            'q' => Some(KeyAction::Quit),
            'r' => Some(KeyAction::Rematch),
            'p' => Some(KeyAction::Pause),
            _ => None,
        },
        _ => None,
    }
}

fn matches_key(input: KeyCode, binding: KeyCode) -> bool {
    match (input, binding) {
        (KeyCode::Char(input), KeyCode::Char(binding)) => input.eq_ignore_ascii_case(&binding),
        _ => input == binding,
    }
}

fn player_action(code: KeyCode, binding: KeyBindings) -> Option<PlayerAction> {
    if matches_key(code, binding.up) {
        Some(PlayerAction::Move(Dir::Up))
    } else if matches_key(code, binding.down) {
        Some(PlayerAction::Move(Dir::Down))
    } else if matches_key(code, binding.left) {
        Some(PlayerAction::Move(Dir::Left))
    } else if matches_key(code, binding.right) {
        Some(PlayerAction::Move(Dir::Right))
    } else if matches_key(code, binding.bomb) {
        Some(PlayerAction::Bomb)
    } else if matches_key(code, binding.punch) {
        Some(PlayerAction::Punch)
    } else {
        None
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
    punch: [bool; 4],
}

impl InputState {
    pub fn new(hold_ticks: u32) -> Self {
        Self {
            hold_ticks,
            held: [None; 4],
            bomb: [false; 4],
            punch: [false; 4],
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
            PlayerAction::Punch => self.punch[slot] = true,
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
            input.punch = std::mem::take(&mut self.punch[slot]);
        }
        out
    }

    /// Forgets everything, e.g. when pausing or restarting.
    pub fn clear(&mut self) {
        self.held = [None; 4];
        self.bomb = [false; 4];
        self.punch = [false; 4];
    }
}
