//! Messages exchanged between host and clients; see contracts/network-protocol.md.

use eksplode_re_core::{Config, Dir, Event, Lobby, Observation, Outcome, RoomSettings};
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
        /// Room settings (see [`crate::room::from_wire`]); used only when this join creates
        /// the room, ignored otherwise.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        room: Option<Value>,
    },
    /// Allowed before `Join`: asks whether a room is open; answered with `RoomInfo`.
    RoomQuery,
    /// Lobby owner only: replace the room settings (read leniently; not an object gives
    /// `Error{BadRoom}`).
    SetRoom {
        settings: Value,
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
    /// Developer mode, owner only: stop or resume every bot (placed bombs keep burning).
    DevPauseBots {
        paused: bool,
    },
    /// Developer mode, owner only: the drop chance of each power in the order
    /// `ExtraBomb, Range, Speed, Punch, Slow, HyperSpeed, SmallBomb, Kick`; eight finite values in
    /// `0.0..=1.0`, otherwise `Error{BadValue}`.
    DevSetDrops {
        chances: Vec<f32>,
    },
    /// Developer mode, owner only: back to the default drop table.
    DevResetDrops,
}

impl ClientMsg {
    /// A `SetRoom` carrying `settings`.
    pub fn set_room(settings: &RoomSettings) -> ClientMsg {
        ClientMsg::SetRoom {
            settings: serde_json::to_value(settings).unwrap_or(Value::Null),
        }
    }
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
        /// The host runs in developer mode; absent from old hosts (means `false`).
        #[serde(default)]
        dev: bool,
    },
    Rejected {
        reason: RejectReason,
    },
    /// The answer to `RoomQuery`: whether a person is in the room, its settings (the start-up
    /// defaults while it is closed) and how many humans are in it.
    RoomInfo {
        open: bool,
        settings: RoomSettings,
        humans: u8,
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
    /// Developer mode: the bot pause and the live drop table; sent to a joiner and to everybody
    /// when it changes.
    DevState {
        paused: bool,
        drops: [f32; 8],
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

/// The line printed when a host starts in developer mode.
pub const DEV_WARNING: &str = "WARNING: developer mode is on: the owner can pause bots and change \
drop chances. It is for testing; do not expose it to an untrusted network.";

/// A player name is 1 to 16 printable characters.
pub fn valid_name(name: &str) -> bool {
    let n = name.chars().count();
    (1..=MAX_NAME_CHARS).contains(&n)
        && !name.chars().any(char::is_control)
        && !name.trim().is_empty()
}
