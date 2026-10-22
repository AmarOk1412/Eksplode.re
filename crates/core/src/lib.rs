//! Pure Bomberman rules. No I/O, no wall-clock time, no global state.
//!
//! The game advances by calling [`step`] once per tick with one [`PlayerInput`] per slot.
//! Given the same [`Config`], seed and input log the result is always identical.

pub mod action;
pub mod arena;
pub mod bomb;
pub mod config;
pub mod error;
pub mod observation;
pub mod rng;
pub mod state;
pub mod step;

pub use action::{Color, Dir, PlayerId, PlayerInput, Pos, MAX_PLAYERS};
pub use arena::{Arena, Tile};
pub use config::Config;
pub use error::CoreError;
pub use observation::Observation;
pub use rng::Rng;
pub use state::{Bomb, Event, Flame, GameState, Outcome, Participant, Phase, PowerUp, PowerUpKind};
pub use step::step;
