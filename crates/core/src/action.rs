//! Basic value types shared by rules, bots and frontends.

use crate::error::CoreError;

/// Number of player slots in a match.
pub const MAX_PLAYERS: usize = 4;

/// A cardinal direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    /// All four directions.
    pub const ALL: [Dir; 4] = [Dir::Up, Dir::Down, Dir::Left, Dir::Right];

    /// Offset `(dx, dy)` of one step; `y` grows downwards.
    pub fn delta(self) -> (i8, i8) {
        match self {
            Dir::Up => (0, -1),
            Dir::Down => (0, 1),
            Dir::Left => (-1, 0),
            Dir::Right => (1, 0),
        }
    }
}

/// A tile coordinate; `y` grows downwards.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Pos {
    pub x: u8,
    pub y: u8,
}

impl Pos {
    pub const fn new(x: u8, y: u8) -> Self {
        Self { x, y }
    }

    /// Neighbour in `dir`, or `None` when it would leave a `width` x `height` grid.
    pub fn step(self, dir: Dir, width: u8, height: u8) -> Option<Pos> {
        let (dx, dy) = dir.delta();
        let x = i16::from(self.x) + i16::from(dx);
        let y = i16::from(self.y) + i16::from(dy);
        if x < 0 || y < 0 || x >= i16::from(width) || y >= i16::from(height) {
            return None;
        }
        Some(Pos::new(x as u8, y as u8))
    }
}

/// A player slot, `0..4`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlayerId(u8);

impl PlayerId {
    /// Validates a slot number.
    pub fn new(index: u8) -> Result<Self, CoreError> {
        if usize::from(index) < MAX_PLAYERS {
            Ok(Self(index))
        } else {
            Err(CoreError::InvalidPlayerId(index))
        }
    }

    /// Slot number as an index.
    pub fn index(self) -> usize {
        usize::from(self.0)
    }

    /// All four slots in order.
    pub fn all() -> [PlayerId; MAX_PLAYERS] {
        [PlayerId(0), PlayerId(1), PlayerId(2), PlayerId(3)]
    }

    /// Colour tied to this slot.
    pub fn color(self) -> Color {
        Color::ALL[self.index()]
    }
}

/// The colour of a player slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Color {
    Red,
    Blue,
    Green,
    Yellow,
}

impl Color {
    pub const ALL: [Color; 4] = [Color::Red, Color::Blue, Color::Green, Color::Yellow];
}

/// What a participant wants to do during one tick. The default does nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlayerInput {
    pub mv: Option<Dir>,
    pub bomb: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_id_is_checked() {
        assert!(PlayerId::new(3).is_ok());
        assert_eq!(PlayerId::new(4), Err(CoreError::InvalidPlayerId(4)));
    }

    #[test]
    fn colors_follow_slots() {
        let all = PlayerId::all();
        assert_eq!(all[0].color(), Color::Red);
        assert_eq!(all[3].color(), Color::Yellow);
    }

    #[test]
    fn pos_step_respects_bounds() {
        let p = Pos::new(0, 0);
        assert_eq!(p.step(Dir::Up, 5, 5), None);
        assert_eq!(p.step(Dir::Left, 5, 5), None);
        assert_eq!(p.step(Dir::Right, 5, 5), Some(Pos::new(1, 0)));
        assert_eq!(Pos::new(4, 4).step(Dir::Down, 5, 5), None);
    }

    #[test]
    fn default_input_does_nothing() {
        let i = PlayerInput::default();
        assert_eq!(i.mv, None);
        assert!(!i.bomb);
    }
}
