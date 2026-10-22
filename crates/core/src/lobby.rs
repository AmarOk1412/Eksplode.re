//! Pre-round lobby: who controls which slot.

use std::fmt;

use crate::action::MAX_PLAYERS;

/// Who controls a slot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Controller {
    /// A human on this machine.
    Local,
    /// A human connected over the network.
    Remote,
    Bot,
    #[default]
    Empty,
}

/// One lobby slot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Slot {
    pub controller: Controller,
    pub name: String,
    pub ready: bool,
}

/// Lobby errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LobbyError {
    SlotOutOfRange(usize),
    SlotTaken(usize),
    Full,
}

impl fmt::Display for LobbyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LobbyError::SlotOutOfRange(i) => write!(f, "slot {i} does not exist"),
            LobbyError::SlotTaken(i) => write!(f, "slot {i} is already taken"),
            LobbyError::Full => write!(f, "all {MAX_PLAYERS} slots are taken"),
        }
    }
}

impl std::error::Error for LobbyError {}

/// The four slots of a match before it starts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Lobby {
    pub slots: [Slot; MAX_PLAYERS],
}

impl Lobby {
    /// A lobby with four empty slots.
    pub fn new() -> Self {
        Self::default()
    }

    /// Local humans take the first slots, bots the following ones.
    pub fn from_counts(humans: u8, bots: u8) -> Result<Lobby, LobbyError> {
        let mut lobby = Lobby::new();
        for i in 0..humans {
            lobby.assign(
                usize::from(i),
                Controller::Local,
                &format!("Player {}", i + 1),
            )?;
        }
        for _ in 0..bots {
            lobby.add_bot()?;
        }
        Ok(lobby)
    }

    /// Number of occupied slots.
    pub fn participants(&self) -> usize {
        self.slots
            .iter()
            .filter(|s| s.controller != Controller::Empty)
            .count()
    }

    pub fn first_free(&self) -> Option<usize> {
        self.slots
            .iter()
            .position(|s| s.controller == Controller::Empty)
    }

    /// Puts `controller` into a free slot.
    pub fn assign(
        &mut self,
        slot: usize,
        controller: Controller,
        name: &str,
    ) -> Result<(), LobbyError> {
        let s = self
            .slots
            .get_mut(slot)
            .ok_or(LobbyError::SlotOutOfRange(slot))?;
        if s.controller != Controller::Empty {
            return Err(LobbyError::SlotTaken(slot));
        }
        *s = Slot {
            controller,
            name: name.to_string(),
            ready: controller == Controller::Bot,
        };
        Ok(())
    }

    /// Adds a bot to the first free slot and returns that slot.
    pub fn add_bot(&mut self) -> Result<usize, LobbyError> {
        let slot = self.first_free().ok_or(LobbyError::Full)?;
        self.assign(slot, Controller::Bot, &format!("Bot {}", slot + 1))?;
        Ok(slot)
    }

    /// Frees a slot; freeing an empty slot is fine.
    pub fn clear(&mut self, slot: usize) -> Result<(), LobbyError> {
        let s = self
            .slots
            .get_mut(slot)
            .ok_or(LobbyError::SlotOutOfRange(slot))?;
        *s = Slot::default();
        Ok(())
    }

    /// Which slots take part in the round.
    pub fn active(&self) -> [bool; MAX_PLAYERS] {
        [0, 1, 2, 3].map(|i| self.slots[i].controller != Controller::Empty)
    }

    /// At least one participant is needed to start.
    pub fn can_start(&self) -> bool {
        self.participants() > 0
    }
}
