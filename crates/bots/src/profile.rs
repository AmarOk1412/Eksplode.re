//! What kind of player a bot is: its behaviour (how it plays) and its difficulty (how well).

use eksplode_re_core::{BotDifficulty, Rng};

/// How a bot plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Behaviour {
    /// Hunts opponents first and from farther away.
    Aggressive,
    /// Collects power-ups and digs first, hunts only close opponents and keeps its distance.
    Protective,
    /// Power-ups, then opponents, then blocks: the bot from before behaviours existed.
    Balanced,
}

impl Behaviour {
    /// Every behaviour.
    pub const ALL: [Behaviour; 3] = [
        Behaviour::Aggressive,
        Behaviour::Protective,
        Behaviour::Balanced,
    ];

    /// One behaviour, each equally likely; the same seed always gives the same behaviour.
    pub fn pick(seed: u64) -> Behaviour {
        Self::ALL[Rng::new(seed ^ BEHAVIOUR_SALT).next_below(3) as usize]
    }

    /// The name used on the wire.
    pub fn name(self) -> &'static str {
        match self {
            Behaviour::Aggressive => "aggressive",
            Behaviour::Protective => "protective",
            Behaviour::Balanced => "balanced",
        }
    }

    /// The behaviour with this exact name.
    pub fn from_name(name: &str) -> Option<Behaviour> {
        Self::ALL.into_iter().find(|b| b.name() == name)
    }

    /// The name shown to people.
    pub fn label(self) -> &'static str {
        match self {
            Behaviour::Aggressive => "Aggressive",
            Behaviour::Protective => "Protective",
            Behaviour::Balanced => "Balanced",
        }
    }
}

// Keeps behaviour picks off the stream the bot's own Rng draws from the same seed.
const BEHAVIOUR_SALT: u64 = 0x5bd1_e995_9e37_79b9;

/// The timing and accuracy of a difficulty.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skill {
    /// Ticks between two fresh decisions; at least 1.
    pub think_every: u32,
    /// An opponent's bomb younger than this many ticks is not noticed yet.
    pub react_ticks: u32,
    /// Chance of a deliberate mistake per fresh decision.
    pub mistake_chance: f32,
}

impl Skill {
    /// How a bot of this difficulty plays.
    pub fn of(difficulty: BotDifficulty) -> Skill {
        let (think_every, react_ticks, mistake_chance) = match difficulty {
            BotDifficulty::Easy => (6, 10, 0.20),
            BotDifficulty::Medium => (3, 5, 0.06),
            BotDifficulty::Hard => (1, 0, 0.0),
        };
        Skill {
            think_every,
            react_ticks,
            mistake_chance,
        }
    }
}

/// The behaviour and difficulty of one bot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Profile {
    pub behaviour: Behaviour,
    pub difficulty: BotDifficulty,
}

impl Profile {
    /// A random behaviour (see [`Behaviour::pick`]) at `difficulty`.
    pub fn pick(seed: u64, difficulty: BotDifficulty) -> Profile {
        Profile {
            behaviour: Behaviour::pick(seed),
            difficulty,
        }
    }
}

impl Default for Profile {
    /// Balanced at Hard: the bot from before behaviours existed.
    fn default() -> Self {
        Self {
            behaviour: Behaviour::Balanced,
            difficulty: BotDifficulty::Hard,
        }
    }
}
