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

impl Observation {
    /// Makes malformed additive participation fields inert without changing legacy state.
    pub fn sanitize_participation(&mut self) {
        let width = self.arena.width();
        let height = self.arena.height();
        let interior = |pos: crate::Pos| {
            pos.x > 0
                && pos.y > 0
                && pos.x < width.saturating_sub(1)
                && pos.y < height.saturating_sub(1)
        };
        let perimeter = (u16::from(width) + u16::from(height))
            .saturating_mul(2)
            .saturating_sub(4);
        for player in &mut self.players {
            if player.kart.is_some_and(|kart| {
                player.alive
                    || !interior(player.pos)
                    || self.phase != Phase::Playing
                    || self.config.mode == crate::Mode::Chaser
                    || width < 3
                    || height < 3
                    || kart.index >= perimeter
                    || kart.entered_tick > self.tick
                    || kart.cooldown_left > self.config.base_move_cooldown
                    || !matches!(
                        kart.travel,
                        None | Some(crate::Dir::Left | crate::Dir::Right)
                    )
                    || kart.pending_return.is_some_and(|pos| !interior(pos))
            }) {
                player.kart = None;
            }
            player.immobilized_left = if player.alive && self.phase == Phase::Playing {
                player.immobilized_left.min(self.config.tick_hz)
            } else {
                0
            };
        }
        for bomb in &mut self.bombs {
            if bomb.revenge_stint.is_some_and(|tick| tick > self.tick) {
                bomb.revenge_stint = None;
            }
        }
        for flame in &mut self.flames {
            for (slot, source) in flame.revenge_stints.iter_mut().enumerate() {
                if flame.owners & (1 << slot) == 0 || source.is_some_and(|tick| tick > self.tick) {
                    *source = None;
                }
            }
        }
    }
}

impl GameState {
    /// The view of the game for slot `me`.
    pub fn observe(&self, me: PlayerId) -> Observation {
        let mut observation = Observation {
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
        };
        observation.sanitize_participation();
        observation
    }
}
