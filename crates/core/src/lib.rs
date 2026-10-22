//! Pure Bomberman rules. No I/O, no wall-clock time, no global state.
//!
//! The game advances by calling [`step`] once per tick with one [`PlayerInput`] per slot.
//! Given the same [`Config`], seed and input log the result is always identical.

pub mod action;
pub mod config;
pub mod error;
pub mod rng;

pub use action::{Color, Dir, PlayerId, PlayerInput, Pos};
pub use config::Config;
pub use error::CoreError;
pub use rng::Rng;
