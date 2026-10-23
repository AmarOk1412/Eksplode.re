//! Room settings: the one value that describes a room (name, mode, board size, round time and
//! friendly fire) and the match configuration derived from it.

use crate::config::Config;

/// The longest room name, in characters.
pub const MAX_NAME_CHARS: usize = 24;

/// The round times the owner can choose, in minutes.
pub const MINUTES: [u8; 4] = [2, 3, 5, 10];

/// The modes a room can start today; a mode is added here when its rules exist.
pub const PLAYABLE: &[Mode] = &[Mode::Normal, Mode::Team, Mode::Chaser];

/// How a round is played.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "lowercase")
)]
pub enum Mode {
    /// Everybody against everybody; the board closes in during the last minute.
    #[default]
    Normal,
    /// Two teams of two.
    Team,
    /// One chaser with bombs and hearts against runners without bombs.
    Chaser,
}

impl Mode {
    /// Every mode, in menu order.
    pub const ALL: [Mode; 3] = [Mode::Normal, Mode::Team, Mode::Chaser];

    /// The name used on the wire and in options.
    pub fn name(self) -> &'static str {
        match self {
            Mode::Normal => "normal",
            Mode::Team => "team",
            Mode::Chaser => "chaser",
        }
    }

    /// The mode with this exact name.
    pub fn from_name(name: &str) -> Option<Mode> {
        Self::ALL.into_iter().find(|m| m.name() == name)
    }

    /// The name shown to people.
    pub fn label(self) -> &'static str {
        match self {
            Mode::Normal => "Normal",
            Mode::Team => "Team",
            Mode::Chaser => "Chaser",
        }
    }
}

/// The size of the board.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "lowercase")
)]
pub enum BoardSize {
    /// 15 x 13, the smallest board and the default.
    #[default]
    Small,
    /// 19 x 15.
    Medium,
    /// 25 x 19.
    Large,
}

impl BoardSize {
    /// Every size, smallest first.
    pub const ALL: [BoardSize; 3] = [BoardSize::Small, BoardSize::Medium, BoardSize::Large];

    /// Width and height in tiles; both are odd.
    pub fn dimensions(self) -> (u8, u8) {
        match self {
            BoardSize::Small => (15, 13),
            BoardSize::Medium => (19, 15),
            BoardSize::Large => (25, 19),
        }
    }

    /// The name used on the wire and in options.
    pub fn name(self) -> &'static str {
        match self {
            BoardSize::Small => "small",
            BoardSize::Medium => "medium",
            BoardSize::Large => "large",
        }
    }

    /// The size with this exact name.
    pub fn from_name(name: &str) -> Option<BoardSize> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }

    /// The name shown to people.
    pub fn label(self) -> &'static str {
        match self {
            BoardSize::Small => "Small",
            BoardSize::Medium => "Medium",
            BoardSize::Large => "Large",
        }
    }
}

/// Everything the owner chooses for a room.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RoomSettings {
    /// At most [`MAX_NAME_CHARS`] characters; empty means "use the default name".
    #[cfg_attr(feature = "serde", serde(default))]
    pub name: String,
    #[cfg_attr(feature = "serde", serde(default))]
    pub mode: Mode,
    #[cfg_attr(feature = "serde", serde(default))]
    pub size: BoardSize,
    /// One of [`MINUTES`].
    #[cfg_attr(feature = "serde", serde(default = "default_minutes"))]
    pub minutes: u8,
    /// Whether explosions hurt teammates (Team mode only).
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub friendly_fire: bool,
}

#[cfg(feature = "serde")]
fn default_minutes() -> u8 {
    3
}

#[cfg(feature = "serde")]
fn default_true() -> bool {
    true
}

impl Default for RoomSettings {
    fn default() -> Self {
        Self {
            name: String::new(),
            mode: Mode::Normal,
            size: BoardSize::Small,
            minutes: 3,
            friendly_fire: true,
        }
    }
}

impl RoomSettings {
    /// Applies the shape limits: a clean, short name and a known time.
    pub fn clean(mut self) -> RoomSettings {
        let clean: String = self.name.chars().filter(|c| !c.is_control()).collect();
        self.name = clean.trim().chars().take(MAX_NAME_CHARS).collect();
        self.name = self.name.trim().to_string();
        if !MINUTES.contains(&self.minutes) {
            self.minutes = 3;
        }
        self
    }

    /// [`clean`](Self::clean) plus a playable mode: a mode whose rules do not exist yet becomes
    /// Normal.
    pub fn sanitize(self) -> RoomSettings {
        let mut s = self.clean();
        if !PLAYABLE.contains(&s.mode) {
            s.mode = Mode::Normal;
        }
        s
    }

    /// The match configuration: the small size and 3 minutes keep what `base` says, so a custom
    /// base (short rounds, tiny arenas) still works.
    pub fn to_config(&self, base: &Config) -> Config {
        let mut config = base.clone();
        if self.size != BoardSize::Small {
            (config.width, config.height) = self.size.dimensions();
        }
        if self.minutes != 3 {
            config.round_limit_ticks = u32::from(self.minutes) * 60 * config.tick_hz;
        }
        config.mode = self.mode;
        config.friendly_fire = self.friendly_fire;
        config
    }

    /// The fewest and the most participants (humans and bots) a match of this mode needs.
    pub fn participants_needed(&self) -> (u8, u8) {
        match self.mode {
            Mode::Normal => (1, 4),
            Mode::Team | Mode::Chaser => (4, 4),
        }
    }

    /// One line for the lobby: `name - Mode - 19 x 15 - 5 min`; Team mode adds the friendly-fire
    /// choice.
    pub fn label(&self) -> String {
        let (w, h) = self.size.dimensions();
        let mut line = format!(
            "{} - {} - {} x {} - {} min",
            self.name,
            self.mode.label(),
            w,
            h,
            self.minutes
        );
        if self.mode == Mode::Team {
            line.push_str(if self.friendly_fire {
                " - friendly fire on"
            } else {
                " - friendly fire off"
            });
        }
        line
    }

    /// Moves to the next (`step > 0`) or previous playable mode, wrapping around.
    pub fn step_mode(&mut self, step: i8) {
        let at = PLAYABLE.iter().position(|m| *m == self.mode).unwrap_or(0);
        self.mode = PLAYABLE[wrap(at, step, PLAYABLE.len())];
    }

    /// Moves to the next or previous board size, wrapping around.
    pub fn step_size(&mut self, step: i8) {
        let at = BoardSize::ALL
            .iter()
            .position(|s| *s == self.size)
            .unwrap_or(0);
        self.size = BoardSize::ALL[wrap(at, step, BoardSize::ALL.len())];
    }

    /// Moves to the next or previous round time, wrapping around.
    pub fn step_minutes(&mut self, step: i8) {
        let at = MINUTES.iter().position(|m| *m == self.minutes).unwrap_or(1);
        self.minutes = MINUTES[wrap(at, step, MINUTES.len())];
    }
}

fn wrap(at: usize, step: i8, len: usize) -> usize {
    (at as i64 + i64::from(step)).rem_euclid(len as i64) as usize
}
