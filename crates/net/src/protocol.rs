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
    #[serde(untagged)]
    RoomV2(Box<v2::Request>),
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
    #[serde(untagged)]
    RoomV2(Box<v2::Reply>),
}

impl ServerMsg {
    pub fn error(code: &str, message: &str) -> Self {
        ServerMsg::Error {
            code: code.to_string(),
            message: message.to_string(),
        }
    }

    pub fn into_game_message(self) -> Self {
        use v2::Reply;
        let Self::RoomV2(reply) = self else {
            return self;
        };
        match *reply {
            Reply::Welcome { admission, .. } => Self::Welcome {
                slot: admission.slot,
                config: admission.config,
                lobby: admission.lobby,
                looks: admission.looks,
                dev: admission.dev,
            },
            Reply::RoomUpdated { lobby, looks, .. } => Self::Lobby { lobby, looks },
            Reply::Start { seed, config, .. } => Self::Start { seed, config },
            Reply::Snapshot {
                tick,
                state,
                events,
                ..
            } => Self::Snapshot {
                tick,
                state,
                events,
            },
            Reply::PlayerLeft {
                slot,
                replaced_by_bot,
                ..
            } => Self::PlayerLeft {
                slot,
                replaced_by_bot,
            },
            Reply::Ended { outcome, .. } => Self::Ended { outcome },
            Reply::Pong { nonce } => Self::Pong { nonce },
            Reply::DevState { paused, drops, .. } => Self::DevState { paused, drops },
            Reply::Error { code, message, .. } => Self::Error {
                code: format!("{code:?}"),
                message,
            },
            Reply::RoomClosed { reason, .. } if reason == "shutdown" => {
                Self::error("HostLeft", "the server stopped")
            }
            reply => Self::RoomV2(Box::new(reply)),
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

pub mod v2 {
    use std::fmt;

    use eksplode_re_core::{Config, Dir, Event, Lobby, Observation, Outcome, RoomSettings};
    use serde::{Deserialize, Deserializer, Serialize};
    use serde_json::Value;

    use crate::look::Look;

    pub const VERSION: u32 = 2;

    #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
    #[serde(transparent)]
    pub struct RequestId(String);

    impl RequestId {
        pub fn new(value: impl Into<String>) -> Result<Self, &'static str> {
            let value = value.into();
            if value.is_empty()
                || value.len() > 64
                || !value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
            {
                return Err("request ID must contain 1 to 64 printable ASCII characters");
            }
            Ok(Self(value))
        }

        pub fn as_str(&self) -> &str {
            &self.0
        }
    }

    impl<'de> Deserialize<'de> for RequestId {
        fn deserialize<Reader: Deserializer<'de>>(reader: Reader) -> Result<Self, Reader::Error> {
            Self::new(String::deserialize(reader)?).map_err(serde::de::Error::custom)
        }
    }

    #[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct Secret(String);

    impl Secret {
        pub fn new(value: impl Into<String>) -> Self {
            Self(value.into())
        }

        pub fn expose(&self) -> &str {
            &self.0
        }
    }

    impl fmt::Debug for Secret {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("[REDACTED]")
        }
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Scope {
        #[default]
        Local,
        Known,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(tag = "type")]
    pub enum Request {
        Hello {
            version: u32,
        },
        ListRooms {
            request_id: RequestId,
            #[serde(default)]
            scope: Scope,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            cursor: Option<String>,
        },
        CreateRoom {
            request_id: RequestId,
            name: String,
            settings: Value,
            capacity: u8,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            password: Option<Secret>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            look: Option<Value>,
        },
        JoinRoom {
            request_id: RequestId,
            server_epoch: String,
            room_id: String,
            name: String,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            password: Option<Secret>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            look: Option<Value>,
        },
        ResumeRoom {
            request_id: RequestId,
            server_epoch: String,
            room_id: String,
            membership_id: String,
            proof: Secret,
        },
        AckResume {
            request_id: RequestId,
            room_id: String,
            resume_request_id: RequestId,
            proof: Secret,
        },
        RequestStatus {
            request_id: RequestId,
            target_request_id: RequestId,
        },
        CancelRequest {
            request_id: RequestId,
            target_request_id: RequestId,
        },
        SetRoom {
            request_id: RequestId,
            room_id: String,
            expected_revision: u64,
            settings: Value,
            capacity: u8,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            password: Option<Secret>,
        },
        RemovePlayer {
            request_id: RequestId,
            room_id: String,
            membership_id: String,
        },
        LeaveRoom {
            request_id: RequestId,
            room_id: String,
        },
        Start {
            request_id: RequestId,
            room_id: String,
        },
        /// Any member: put a bot into the empty seat `slot` (the lobby's "+"). Without `slot`
        /// (older clients, the terminal client) the first free seat is used.
        AddBot {
            request_id: RequestId,
            room_id: String,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            slot: Option<u8>,
        },
        /// Owner only: remove the bot in seat `slot` (the lobby's cross); the seat stays empty.
        /// Without `slot` (older clients, the terminal client) the highest bot seat is freed.
        RemoveBot {
            request_id: RequestId,
            room_id: String,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            slot: Option<u8>,
        },
        SetReady {
            request_id: RequestId,
            room_id: String,
            ready: bool,
        },
        SetLook {
            request_id: RequestId,
            room_id: String,
            look: Value,
        },
        Input {
            room_id: String,
            tick: u32,
            mv: Option<Dir>,
            bomb: bool,
            #[serde(default)]
            punch: bool,
        },
        DevPauseBots {
            request_id: RequestId,
            room_id: String,
            paused: bool,
        },
        DevSetDrops {
            request_id: RequestId,
            room_id: String,
            chances: Vec<f32>,
        },
        DevResetDrops {
            request_id: RequestId,
            room_id: String,
        },
        Ping {
            nonce: u64,
        },
    }

    impl Request {
        pub fn create_room(
            request_id: RequestId,
            name: String,
            settings: &RoomSettings,
            capacity: u8,
        ) -> Self {
            Self::CreateRoom {
                request_id,
                name,
                settings: serde_json::to_value(settings).unwrap_or(Value::Null),
                capacity,
                password: None,
                look: None,
            }
        }

        pub fn request_id(&self) -> Option<&RequestId> {
            match self {
                Self::Hello { .. } | Self::Input { .. } | Self::Ping { .. } => None,
                Self::ListRooms { request_id, .. }
                | Self::CreateRoom { request_id, .. }
                | Self::JoinRoom { request_id, .. }
                | Self::ResumeRoom { request_id, .. }
                | Self::AckResume { request_id, .. }
                | Self::RequestStatus { request_id, .. }
                | Self::CancelRequest { request_id, .. }
                | Self::SetRoom { request_id, .. }
                | Self::RemovePlayer { request_id, .. }
                | Self::LeaveRoom { request_id, .. }
                | Self::Start { request_id, .. }
                | Self::AddBot { request_id, .. }
                | Self::RemoveBot { request_id, .. }
                | Self::SetReady { request_id, .. }
                | Self::SetLook { request_id, .. }
                | Self::DevPauseBots { request_id, .. }
                | Self::DevSetDrops { request_id, .. }
                | Self::DevResetDrops { request_id, .. } => Some(request_id),
            }
        }

        pub fn room_id(&self) -> Option<&str> {
            match self {
                Self::JoinRoom { room_id, .. }
                | Self::ResumeRoom { room_id, .. }
                | Self::AckResume { room_id, .. }
                | Self::SetRoom { room_id, .. }
                | Self::RemovePlayer { room_id, .. }
                | Self::LeaveRoom { room_id, .. }
                | Self::Start { room_id, .. }
                | Self::AddBot { room_id, .. }
                | Self::RemoveBot { room_id, .. }
                | Self::SetReady { room_id, .. }
                | Self::SetLook { room_id, .. }
                | Self::Input { room_id, .. }
                | Self::DevPauseBots { room_id, .. }
                | Self::DevSetDrops { room_id, .. }
                | Self::DevResetDrops { room_id, .. } => Some(room_id),
                _ => None,
            }
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub struct RoomRef {
        pub server_id: String,
        pub server_epoch: String,
        pub room_id: String,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum RoomPhase {
        Lobby,
        Running,
        Closing,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub struct RoomSummary {
        #[serde(flatten)]
        pub reference: RoomRef,
        #[serde(flatten)]
        pub settings: RoomSettings,
        pub capacity: u8,
        pub occupied: u8,
        pub connected_humans: u8,
        pub reserved_humans: u8,
        pub bots: u8,
        pub phase: RoomPhase,
        pub password_required: bool,
        pub revision: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub game_addr: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub web_origin: Option<String>,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Limits {
        pub max_rooms: u16,
        pub max_connections: u16,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    pub struct Admission {
        #[serde(flatten)]
        pub reference: RoomRef,
        pub membership_id: String,
        pub slot: u8,
        pub resume_proof: Secret,
        pub reconnect_window_seconds: u64,
        pub owner_membership_id: Option<String>,
        pub owner_slot: Option<u8>,
        pub revision: u64,
        pub capacity: u8,
        pub password_required: bool,
        pub config: Config,
        pub lobby: Lobby,
        pub looks: Vec<Look>,
        pub dev: bool,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    pub enum ErrorCode {
        BadVersion,
        BadName,
        BadRoom,
        BadPassword,
        Full,
        InMatch,
        RoomNotFound,
        RoomClosed,
        AlreadyInRoom,
        NotInRoom,
        NotOwner,
        LookAdjusted,
        StaleRevision,
        CapacityConflict,
        ModeNeedsPlayers,
        ResumeExpired,
        ResumeDenied,
        RequestExpired,
        Cancelled,
        Busy,
        RateLimited,
        ServerFull,
        Unsupported,
        Unavailable,
        /// A seat index outside the four seats of a room.
        BadSlot,
        /// `RemoveBot` named a seat that holds a human or nobody.
        NotABot,
        /// `AddBot` named a seat that is already taken.
        SlotTaken,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum RequestState {
        Pending,
        Committed,
        Rejected,
        Cancelled,
        Expired,
        Unknown,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(tag = "type")]
    pub enum Reply {
        Hello {
            version: u32,
            server_id: String,
            server_epoch: String,
            capabilities: Vec<String>,
            limits: Limits,
            #[serde(default)]
            defaults: RoomSettings,
        },
        Rooms {
            request_id: RequestId,
            sources: Vec<Value>,
            rooms: Vec<RoomSummary>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            next_cursor: Option<String>,
        },
        Welcome {
            request_id: RequestId,
            #[serde(flatten)]
            admission: Admission,
        },
        Resumed {
            request_id: RequestId,
            #[serde(flatten)]
            admission: Admission,
            phase: RoomPhase,
            tick: Option<u32>,
            state: Option<Box<Observation>>,
            result: Option<Outcome>,
        },
        ResumeAcknowledged {
            request_id: RequestId,
            #[serde(flatten)]
            reference: RoomRef,
            resume_request_id: RequestId,
        },
        RequestStatus {
            request_id: RequestId,
            target_request_id: RequestId,
            status: RequestState,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            result: Option<Box<Reply>>,
        },
        RoomUpdated {
            #[serde(flatten)]
            reference: RoomRef,
            revision: u64,
            owner_membership_id: Option<String>,
            owner_slot: Option<u8>,
            capacity: u8,
            password_required: bool,
            memberships: Vec<Value>,
            lobby: Lobby,
            looks: Vec<Look>,
        },
        LeftRoom {
            request_id: RequestId,
            #[serde(flatten)]
            reference: RoomRef,
        },
        Removed {
            #[serde(flatten)]
            reference: RoomRef,
            reason: String,
        },
        RoomClosed {
            #[serde(flatten)]
            reference: RoomRef,
            reason: String,
        },
        Error {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            request_id: Option<RequestId>,
            code: ErrorCode,
            message: String,
        },
        Pong {
            nonce: u64,
        },
        Start {
            #[serde(flatten)]
            reference: RoomRef,
            seed: u64,
            config: Config,
        },
        Snapshot {
            #[serde(flatten)]
            reference: RoomRef,
            tick: u32,
            state: Box<Observation>,
            events: Vec<Event>,
        },
        PlayerLeft {
            #[serde(flatten)]
            reference: RoomRef,
            slot: u8,
            replaced_by_bot: bool,
        },
        Ended {
            #[serde(flatten)]
            reference: RoomRef,
            outcome: Outcome,
        },
        DevState {
            #[serde(flatten)]
            reference: RoomRef,
            paused: bool,
            drops: [f32; 8],
        },
    }
}
