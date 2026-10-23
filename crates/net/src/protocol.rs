//! Messages exchanged between host and clients; see contracts/network-protocol.md.

use bomberust_core::{Config, Dir, Event, Lobby, Observation, Outcome};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::look::Look;

pub const PROTOCOL_VERSION: u32 = 1;
/// Longest accepted line, in bytes.
pub const MAX_LINE: usize = 64 * 1024;
pub const MAX_NAME_CHARS: usize = 16;
pub const DEFAULT_PORT: u16 = 7878;

/// Client to host.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMsg {
    /// `look` is read leniently (see [`Look::from_wire`]); old clients send none.
    Join {
        version: u32,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        look: Option<Value>,
    },
    /// Lobby only: change the sender's look.
    SetLook {
        look: Value,
    },
    SetReady {
        ready: bool,
    },
    /// `tick` is the tick of the latest snapshot the client has seen.
    Input {
        tick: u32,
        mv: Option<Dir>,
        bomb: bool,
        /// Punch the bomb on the faced square; absent from old clients (means no punch).
        #[serde(default)]
        punch: bool,
    },
    Leave,
    Ping {
        nonce: u64,
    },
    /// Lobby owner only: start the match.
    Start,
    /// Lobby owner only: put a bot into the first free slot.
    AddBot,
    /// Lobby owner only: free the highest bot slot.
    RemoveBot,
}

/// Why a join was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectReason {
    Full,
    BadVersion,
    BadName,
    InMatch,
}

/// Host to client.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMsg {
    Welcome {
        slot: u8,
        config: Config,
        lobby: Lobby,
        /// The look of each slot (four entries); absent from old hosts.
        #[serde(default)]
        looks: Vec<Look>,
    },
    Rejected {
        reason: RejectReason,
    },
    Lobby {
        lobby: Lobby,
        #[serde(default)]
        looks: Vec<Look>,
    },
    Start {
        seed: u64,
        config: Config,
    },
    Snapshot {
        tick: u32,
        state: Box<Observation>,
        events: Vec<Event>,
    },
    PlayerLeft {
        slot: u8,
        replaced_by_bot: bool,
    },
    Ended {
        outcome: Outcome,
    },
    Error {
        code: String,
        message: String,
    },
    Pong {
        nonce: u64,
    },
}

impl ServerMsg {
    pub fn error(code: &str, message: &str) -> Self {
        ServerMsg::Error {
            code: code.to_string(),
            message: message.to_string(),
        }
    }
}

/// A player name is 1 to 16 printable characters.
pub fn valid_name(name: &str) -> bool {
    let n = name.chars().count();
    (1..=MAX_NAME_CHARS).contains(&n)
        && !name.chars().any(char::is_control)
        && !name.trim().is_empty()
}
