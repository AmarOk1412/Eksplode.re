//! The public view of a game: what any participant, human or bot, is allowed to see.

use crate::action::{PlayerId, MAX_PLAYERS};
use crate::arena::Arena;
use crate::config::Config;
use crate::state::{Bomb, Falling, Flame, GameState, Participant, Phase, PowerUp};

/// A read-only snapshot without the RNG and without power-ups still hidden under blocks.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Observation {
    pub me: PlayerId,
    pub tick: u32,
    pub config: Config,
    pub arena: Arena,
    pub players: [Participant; MAX_PLAYERS],
    pub bombs: Vec<Bomb>,
    pub flames: Vec<Flame>,
    pub powerups: Vec<PowerUp>,
    pub phase: Phase,
    /// Blocks announced by the end pattern.
    #[cfg_attr(feature = "serde", serde(default))]
    pub falling: Vec<Falling>,
}

impl GameState {
    /// The view of the game for slot `me`.
    pub fn observe(&self, me: PlayerId) -> Observation {
        Observation {
            me,
            tick: self.tick,
            config: self.config.clone(),
            arena: self.arena.clone(),
            players: self.players.clone(),
            bombs: self.bombs.clone(),
            flames: self.flames.clone(),
            powerups: self.powerups.clone(),
            phase: self.phase,
            falling: self.closing.falling.clone(),
        }
    }
}
