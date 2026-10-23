//! Game state types.

use crate::action::{Dir, PlayerId, Pos, MAX_PLAYERS};
use crate::arena::{Arena, Tile};
use crate::config::Config;
use crate::drops::DropTable;
use crate::error::CoreError;
use crate::rng::Rng;

/// A collectible that improves a player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PowerUpKind {
    /// One more bomb at a time.
    ExtraBomb,
    /// Longer explosions (the flame).
    Range,
    /// Faster movement (the roller).
    Speed,
    /// Lets the player punch the bomb they face.
    Punch,
    /// Malus: slower movement for a while.
    Slow,
    /// Malus: much faster movement for a while.
    HyperSpeed,
    /// Malus: bombs reach one square for a while.
    SmallBomb,
    /// Walking into a landed bomb makes it slide until the next square is blocked.
    Kick,
}

impl PowerUpKind {
    /// Every kind, in the order the drop table uses.
    pub const ALL: [PowerUpKind; 8] = [
        PowerUpKind::ExtraBomb,
        PowerUpKind::Range,
        PowerUpKind::Speed,
        PowerUpKind::Punch,
        PowerUpKind::Slow,
        PowerUpKind::HyperSpeed,
        PowerUpKind::SmallBomb,
        PowerUpKind::Kick,
    ];

    /// Maluses are timed and hurt the player who takes them; bonuses last for the round.
    pub fn is_malus(self) -> bool {
        matches!(
            self,
            PowerUpKind::Slow | PowerUpKind::HyperSpeed | PowerUpKind::SmallBomb
        )
    }
}

/// A malus that currently applies to a player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ActiveMalus {
    /// One of the malus kinds.
    pub kind: PowerUpKind,
    pub ticks_left: u32,
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
    /// Whether the player may punch the bomb they face.
    pub has_punch: bool,
    /// Whether walking into a landed bomb pushes it.
    #[cfg_attr(feature = "serde", serde(default))]
    pub has_kick: bool,
    /// The malus that applies now, if any.
    pub malus: Option<ActiveMalus>,
}

impl Participant {
    /// Ticks between two moves with the malus applied: slow `ceil(base * 4 / 3)`, hyper speed
    /// `max(1, base / 2)`.
    pub fn effective_cooldown(&self) -> u32 {
        let base = self.move_cooldown;
        match self.malus.map(|m| m.kind) {
            Some(PowerUpKind::Slow) => (base * 4).div_ceil(3),
            Some(PowerUpKind::HyperSpeed) => (base / 2).max(1),
            _ => base,
        }
    }

    /// The reach of a bomb placed now: one square with the small-bomb malus.
    pub fn placed_range(&self) -> u8 {
        match self.malus {
            Some(m) if m.kind == PowerUpKind::SmallBomb => 1,
            _ => self.range,
        }
    }
}

/// A punched bomb on its way; it hops one square at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Flight {
    pub dir: Dir,
    /// Ticks until the next hop.
    pub hop_left: u32,
    /// The square the bomb was punched from.
    pub origin: Pos,
}

/// A kicked bomb on its way: it moves one square every `slide_ticks` until the next square is
/// blocked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Slide {
    pub dir: Dir,
    /// Ticks until the next step.
    pub step_left: u32,
}

/// A placed bomb. It blocks movement into its tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Bomb {
    pub owner: PlayerId,
    pub pos: Pos,
    pub ticks_left: u32,
    pub range: u8,
    /// Set while the bomb is flying after a punch; it then blocks nothing and flames ignore it.
    pub flight: Option<Flight>,
    /// Set while the bomb slides after a kick; it still blocks and flames still reach it.
    #[cfg_attr(feature = "serde", serde(default))]
    pub slide: Option<Slide>,
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
    BombPunched {
        player: PlayerId,
        from: Pos,
        dir: Dir,
    },
    BombHopped {
        pos: Pos,
    },
    BombLanded {
        pos: Pos,
    },
    BombKicked {
        player: PlayerId,
        from: Pos,
        dir: Dir,
    },
    BombSlid {
        pos: Pos,
    },
    BombStopped {
        pos: Pos,
    },
    MalusStarted {
        player: PlayerId,
        kind: PowerUpKind,
    },
    MalusEnded {
        player: PlayerId,
        kind: PowerUpKind,
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
    /// What destroyed blocks drop; decided when a block breaks, so it can change during a round.
    pub drops: DropTable,
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
        Self::build(config, arena, starts, rng)
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
            has_punch: false,
            has_kick: false,
            malus: None,
        });
        Ok(GameState {
            drops: DropTable::defaults(config.powerup_chance),
            config,
            tick: 0,
            arena,
            players,
            bombs: Vec::new(),
            flames: Vec::new(),
            powerups: Vec::new(),
            rng,
            phase: Phase::Playing,
            started: started as u8,
        })
    }

    /// Replaces the drop table; an invalid table is refused and nothing changes.
    pub fn set_drops(&mut self, drops: DropTable) -> Result<(), CoreError> {
        drops.validate()?;
        self.drops = drops;
        Ok(())
    }

    /// Number of living participants.
    pub fn alive_count(&self) -> usize {
        self.players.iter().filter(|p| p.alive).count()
    }
}
