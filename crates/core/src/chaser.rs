//! Chaser mode: one participant with bombs and hearts against runners without bombs, random
//! bonuses and the results of that mode.

use crate::action::MAX_PLAYERS;
use crate::rng::Rng;
use crate::room::Mode;
use crate::state::{Event, GameState, Outcome, PowerUp, PowerUpKind, Role};

/// What appears on the board and how often, out of a total of 10: runners can fight back with
/// the kick and the punch but never get a bomb or a longer flame.
pub const BONUS_WEIGHTS: [(PowerUpKind, u32); 6] = [
    (PowerUpKind::Speed, 3),
    (PowerUpKind::Kick, 2),
    (PowerUpKind::Punch, 2),
    (PowerUpKind::Slow, 1),
    (PowerUpKind::HyperSpeed, 1),
    (PowerUpKind::SmallBomb, 1),
];

/// Draws the chaser among the participating slots; `None` when fewer than two take part.
pub fn choose(rng: &mut Rng, active: &[bool; MAX_PLAYERS]) -> Option<usize> {
    let slots: Vec<usize> = (0..MAX_PLAYERS).filter(|i| active[*i]).collect();
    if slots.len() < 2 {
        return None;
    }
    Some(slots[rng.next_below(slots.len() as u32) as usize])
}

/// Whether this round has a chaser (a Chaser room with at least two participants).
pub fn applies(state: &GameState) -> bool {
    state.config.mode == Mode::Chaser && state.players.iter().any(|p| p.role == Role::Chaser)
}

/// Counts down to the next bonus and puts one on a random free square while fewer than
/// `bonus_cap` lie on the board. Draws from the state's random generator, so a seed gives the
/// same bonuses.
pub fn spawn_bonus(state: &mut GameState, events: &mut Vec<Event>) {
    if !applies(state) {
        return;
    }
    state.bonus_in = state.bonus_in.saturating_sub(1);
    if state.bonus_in > 0 {
        return;
    }
    state.bonus_in = state.config.bonus_every_ticks;
    if state.powerups.len() >= usize::from(state.config.bonus_cap) {
        return;
    }
    let free: Vec<_> = state
        .arena
        .positions_of(crate::arena::Tile::Floor)
        .into_iter()
        .filter(|p| {
            !state.bombs.iter().any(|b| b.pos == *p)
                && !state.powerups.iter().any(|u| u.pos == *p)
                && !state.players.iter().any(|pl| pl.alive && pl.pos == *p)
                && !state.flames.iter().any(|f| f.pos == *p)
        })
        .collect();
    if free.is_empty() {
        return;
    }
    let pos = free[state.rng.next_below(free.len() as u32) as usize];
    let total: u32 = BONUS_WEIGHTS.iter().map(|(_, w)| w).sum();
    let mut roll = state.rng.next_below(total);
    let kind = BONUS_WEIGHTS
        .iter()
        .find(|(_, w)| {
            if roll < *w {
                true
            } else {
                roll -= w;
                false
            }
        })
        .map_or(PowerUpKind::Speed, |(k, _)| *k);
    state.powerups.push(PowerUp { pos, kind });
    events.push(Event::PowerUpSpawned { pos, kind });
}

/// The result of a Chaser round: the chaser wins when no runner is alive, the runners win when
/// the chaser is out or the time ran out with the chaser and a runner alive; everybody out is a
/// draw. `None` while it goes on.
pub fn outcome(state: &GameState) -> Option<Outcome> {
    let chaser = state
        .players
        .iter()
        .any(|p| p.role == Role::Chaser && p.alive);
    let runners = state
        .players
        .iter()
        .any(|p| p.role == Role::Runner && p.alive);
    match (chaser, runners) {
        (false, false) => Some(Outcome::Draw),
        (true, false) => Some(Outcome::ChaserWins),
        (false, true) => Some(Outcome::RunnersWin),
        (true, true) if state.tick >= state.config.round_limit_ticks => Some(Outcome::RunnersWin),
        (true, true) => None,
    }
}
