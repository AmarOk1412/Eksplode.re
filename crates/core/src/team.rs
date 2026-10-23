//! Team mode: two teams of two (slots 1 and 2 against slots 3 and 4), friendly fire and the team
//! result.

use crate::action::PlayerId;
use crate::config::Config;
use crate::room::Mode;
use crate::state::{Flame, GameState, Outcome, Participant};

/// The team of a slot: slots 0 and 1 are team 0, slots 2 and 3 are team 1.
pub fn team_of(slot: usize) -> u8 {
    (slot / 2) as u8
}

/// Whether `flame` eliminates `player`. Every flame does, except in Team mode with friendly fire
/// off, where a flame lit only by teammates of the player (and not by the player) does not.
pub fn hurts(
    config: &Config,
    players: &[Participant],
    flame: &Flame,
    player: &Participant,
) -> bool {
    if config.mode != Mode::Team || config.friendly_fire || flame.owners == 0 {
        return true;
    }
    (0..players.len()).any(|i| {
        let owner = &players[i];
        flame.owned_by(owner.id) && (owner.id == player.id || owner.team != player.team)
    })
}

/// The team that wins now: every living participant belongs to it and two teams took part.
pub fn winner(state: &GameState) -> Option<u8> {
    if state.config.mode != Mode::Team || state.started_teams.count_ones() < 2 {
        return None;
    }
    let mut living = state.players.iter().filter(|p| p.alive).map(|p| p.team);
    let first = living.next()??;
    living.all(|t| t == Some(first)).then_some(first)
}

/// The members of `team` that took part, for messages.
pub fn members(state: &GameState, team: u8) -> Vec<PlayerId> {
    state
        .players
        .iter()
        .filter(|p| p.team == Some(team))
        .map(|p| p.id)
        .collect()
}

/// The outcome for the end of a Team round, if a team has won.
pub fn outcome(state: &GameState) -> Option<Outcome> {
    winner(state).map(Outcome::TeamWin)
}
