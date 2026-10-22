//! Game state types.

use std::collections::BTreeMap;

use crate::action::{Dir, PlayerId, Pos, MAX_PLAYERS};
use crate::arena::{Arena, Tile};
use crate::config::Config;
use crate::error::CoreError;
use crate::rng::Rng;

/// A collectible that improves a player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PowerUpKind {
    ExtraBomb,
    Range,
    Speed,
}

impl PowerUpKind {
    pub const ALL: [PowerUpKind; 3] = [
        PowerUpKind::ExtraBomb,
        PowerUpKind::Range,
        PowerUpKind::Speed,
    ];
}

/// A visible power-up lying on a tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PowerUp {
    pub pos: Pos,
    pub kind: PowerUpKind,
}

/// One of the four player slots.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Participant {
    pub id: PlayerId,
    pub pos: Pos,
    pub alive: bool,
    pub facing: Dir,
    pub bombs_max: u8,
    pub bombs_active: u8,
    pub range: u8,
    /// Ticks between two moves.
    pub move_cooldown: u32,
    /// Ticks left until the next move is allowed.
    pub cooldown_left: u32,
}

/// A placed bomb. It blocks movement into its tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Bomb {
    pub owner: PlayerId,
    pub pos: Pos,
    pub ticks_left: u32,
    pub range: u8,
}

/// One burning tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Flame {
    pub pos: Pos,
    pub ticks_left: u32,
}

/// How a round ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Outcome {
    Winner(PlayerId),
    Draw,
}

/// Whether the round is still running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Phase {
    Playing,
    Ended(Outcome),
}

/// Something that happened during a tick; used for effects and test assertions.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Event {
    Moved {
        player: PlayerId,
        from: Pos,
        to: Pos,
    },
    BombPlaced {
        player: PlayerId,
        pos: Pos,
    },
    BombExploded {
        owner: PlayerId,
        pos: Pos,
    },
    BlockDestroyed {
        pos: Pos,
    },
    PowerUpSpawned {
        pos: Pos,
        kind: PowerUpKind,
    },
    PowerUpCollected {
        player: PlayerId,
        pos: Pos,
        kind: PowerUpKind,
    },
    PowerUpDestroyed {
        pos: Pos,
    },
    PlayerEliminated {
        player: PlayerId,
    },
    RoundEnded(Outcome),
}

/// The complete simulation state of one round.
#[derive(Clone, Debug, PartialEq)]
pub struct GameState {
    pub config: Config,
    pub tick: u32,
    pub arena: Arena,
    pub players: [Participant; MAX_PLAYERS],
    pub bombs: Vec<Bomb>,
    pub flames: Vec<Flame>,
    pub powerups: Vec<PowerUp>,
    /// Power-ups still hidden under blocks; never exposed to players.
    pub hidden_powerups: BTreeMap<Pos, PowerUpKind>,
    pub rng: Rng,
    pub phase: Phase,
    /// Number of participants that started the round.
    pub started: u8,
}

impl GameState {
    /// Creates a round on a generated arena. `active[i]` says whether slot `i` plays.
    pub fn new(
        config: Config,
        seed: u64,
        active: [bool; MAX_PLAYERS],
    ) -> Result<GameState, CoreError> {
        config.validate()?;
        let mut rng = Rng::new(seed);
        let arena = Arena::generate(&config, &mut rng);
        let corners = arena.start_positions();
        let starts = [0, 1, 2, 3].map(|i| active[i].then_some(corners[i]));
        let mut state = Self::build(config, arena, starts, rng)?;
        for p in state.arena.positions_of(Tile::Block) {
            if state.rng.chance(state.config.powerup_chance) {
                let kind = PowerUpKind::ALL[state.rng.next_below(3) as usize];
                state.hidden_powerups.insert(p, kind);
            }
        }
        Ok(state)
    }

    /// Creates a round on a given arena with explicit start positions (tests, scenarios).
    pub fn custom(
        mut config: Config,
        arena: Arena,
        starts: [Option<Pos>; MAX_PLAYERS],
        seed: u64,
    ) -> Result<GameState, CoreError> {
        config.width = arena.width();
        config.height = arena.height();
        config.validate()?;
        Self::build(config, arena, starts, Rng::new(seed))
    }

    fn build(
        config: Config,
        arena: Arena,
        starts: [Option<Pos>; MAX_PLAYERS],
        rng: Rng,
    ) -> Result<GameState, CoreError> {
        let started = starts.iter().flatten().count();
        if started == 0 {
            return Err(CoreError::InvalidSlots(
                "at least one participant is needed".into(),
            ));
        }
        for p in starts.iter().flatten() {
            if arena.tile(*p) != Tile::Floor {
                return Err(CoreError::InvalidSlots(format!(
                    "start {p:?} is not a floor tile"
                )));
            }
        }
        let ids = PlayerId::all();
        let players = [0, 1, 2, 3].map(|i| Participant {
            id: ids[i],
            pos: starts[i].unwrap_or(Pos::new(0, 0)),
            alive: starts[i].is_some(),
            facing: Dir::Down,
            bombs_max: config.start_bombs,
            bombs_active: 0,
            range: config.start_range,
            move_cooldown: config.base_move_cooldown,
            cooldown_left: 0,
        });
        Ok(GameState {
            config,
            tick: 0,
            arena,
            players,
            bombs: Vec::new(),
            flames: Vec::new(),
            powerups: Vec::new(),
            hidden_powerups: BTreeMap::new(),
            rng,
            phase: Phase::Playing,
            started: started as u8,
        })
    }

    /// Number of living participants.
    pub fn alive_count(&self) -> usize {
        self.players.iter().filter(|p| p.alive).count()
    }
}
