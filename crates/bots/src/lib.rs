//! Computer-controlled players. They only see an [`Observation`], like any human.

mod danger;
mod default_bot;
mod pathing;

pub use danger::DangerMap;
pub use default_bot::DefaultBot;
pub use pathing::{find_path, passable, PathRequest};

use eksplode_re_core::{Observation, PlayerInput};

/// Anything that can play a slot: given what is visible, say what to do this tick.
pub trait Bot {
    fn decide(&mut self, obs: &Observation) -> PlayerInput;
}
