//! Pure Bomberman rules. No I/O, no wall-clock time, no global state.
//!
//! The game advances by calling [`step`] once per tick with one [`PlayerInput`] per slot.
//! Given the same [`Config`], seed and input log the result is always identical.

pub mod action;
pub mod arena;
pub mod bomb;
pub mod closing;
pub mod config;
pub mod drops;
pub mod error;
pub mod kick;
pub mod lobby;
pub mod observation;
pub mod punch;
pub mod rng;
pub mod room;
pub mod state;
pub mod step;
pub mod team;

pub use action::{Color, Dir, PlayerId, PlayerInput, Pos, MAX_PLAYERS};
pub use arena::{Arena, Tile};
pub use bomb::{blast, Blast};
pub use config::Config;
pub use drops::DropTable;
pub use error::CoreError;
pub use lobby::{Controller, Lobby, LobbyError, Slot};
pub use observation::Observation;
pub use rng::Rng;
pub use room::{BoardSize, Mode, RoomSettings, MINUTES, PLAYABLE};
pub use state::{
    ActiveMalus, Bomb, Closing, Event, Falling, Flame, Flight, GameState, Outcome, Participant,
    Phase, PowerUp, PowerUpKind, Role, Slide,
};
pub use step::step;
