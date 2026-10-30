//! Pre-round lobby: who controls which slot.

use std::fmt;

use crate::action::MAX_PLAYERS;
use crate::room::RoomSettings;

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
    /// The slot holds a human or nobody, so there is no bot to remove from it.
    NotABot(usize),
}

impl fmt::Display for LobbyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LobbyError::SlotOutOfRange(i) => write!(f, "slot {i} does not exist"),
            LobbyError::SlotTaken(i) => write!(f, "slot {i} is already taken"),
            LobbyError::Full => write!(f, "all {MAX_PLAYERS} slots are taken"),
            LobbyError::NotABot(i) => write!(f, "slot {i} is not a bot"),
        }
    }
}

impl std::error::Error for LobbyError {}

/// The four slots of a match before it starts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Lobby {
    pub slots: [Slot; MAX_PLAYERS],
    /// The room settings; the host keeps them here so every client sees them with the slots.
    #[cfg_attr(feature = "serde", serde(default))]
    pub settings: RoomSettings,
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

    /// Puts a bot into exactly `slot`, which must be empty (the lobby's "+" on that seat).
    pub fn add_bot_at(&mut self, slot: usize) -> Result<(), LobbyError> {
        self.assign(slot, Controller::Bot, &format!("Bot {}", slot + 1))
    }

    /// Removes the bot in exactly `slot` (the owner's cross on that seat); the seat stays empty.
    pub fn remove_bot_at(&mut self, slot: usize) -> Result<(), LobbyError> {
        let s = self
            .slots
            .get(slot)
            .ok_or(LobbyError::SlotOutOfRange(slot))?;
        if s.controller != Controller::Bot {
            return Err(LobbyError::NotABot(slot));
        }
        self.clear(slot)
    }

    /// Puts a bot into every empty slot and returns the slots filled, in order; does nothing
    /// when no slot is empty.
    pub fn fill_vacant_with_bots(&mut self) -> Vec<usize> {
        self.fill_vacant_with_bots_up_to(MAX_PLAYERS)
    }

    /// Like [`Lobby::fill_vacant_with_bots`], but only among the first `seats` slots (a room
    /// capacity); slots past it stay empty.
    pub fn fill_vacant_with_bots_up_to(&mut self, seats: usize) -> Vec<usize> {
        let mut filled = Vec::new();
        for slot in 0..seats.min(MAX_PLAYERS) {
            if self.slots[slot].controller == Controller::Empty && self.add_bot_at(slot).is_ok() {
                filled.push(slot);
            }
        }
        filled
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

    /// Whether the number of participants fits the mode of the room.
    pub fn can_start(&self) -> bool {
        self.start_problem().is_none()
    }

    /// Why the match cannot start yet, in words for the owner; `None` when it can.
    pub fn start_problem(&self) -> Option<String> {
        let (min, max) = self.settings.participants_needed();
        let have = self.participants();
        if have >= usize::from(min) && have <= usize::from(max) {
            return None;
        }
        let mode = self.settings.mode.label();
        Some(if min == max {
            let missing = usize::from(min).saturating_sub(have);
            format!("{mode} needs {min} players: {missing} missing, add bots to fill the slots")
        } else {
            format!("{mode} needs at least {min} player")
        })
    }

    /// The lobby owner: the lowest slot held by a human. Only the owner may start the match.
    pub fn owner(&self) -> Option<usize> {
        self.slots
            .iter()
            .position(|s| matches!(s.controller, Controller::Local | Controller::Remote))
    }

    /// The highest slot held by a bot.
    pub fn last_bot(&self) -> Option<usize> {
        self.slots
            .iter()
            .rposition(|s| s.controller == Controller::Bot)
    }
}
