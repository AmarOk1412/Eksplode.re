use eksplode_re_core::Mode;
use serde::Serialize;

use crate::protocol::v2::ErrorCode;
use crate::room::ServerIdentity;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum EventKind {
    #[serde(rename = "server.started")]
    ServerStarted,
    #[serde(rename = "server.stopping")]
    ServerStopping,
    #[serde(rename = "server.stopped")]
    ServerStopped,
    #[serde(rename = "room.created")]
    RoomCreated,
    #[serde(rename = "room.settings_changed")]
    RoomSettingsChanged,
    #[serde(rename = "room.owner_changed")]
    RoomOwnerChanged,
    #[serde(rename = "room.closed")]
    RoomClosed,
    #[serde(rename = "player.joined")]
    PlayerJoined,
    #[serde(rename = "player.join_rejected")]
    PlayerJoinRejected,
    #[serde(rename = "player.left")]
    PlayerLeft,
    #[serde(rename = "player.disconnected")]
    PlayerDisconnected,
    #[serde(rename = "player.resumed")]
    PlayerResumed,
    #[serde(rename = "player.resume_rejected")]
    PlayerResumeRejected,
    #[serde(rename = "player.reservation_expired")]
    PlayerReservationExpired,
    #[serde(rename = "player.removed")]
    PlayerRemoved,
    #[serde(rename = "permission.denied")]
    PermissionDenied,
    #[serde(rename = "request.rate_limited")]
    RequestRateLimited,
    #[serde(rename = "protocol.rejected")]
    ProtocolRejected,
    #[serde(rename = "match.started")]
    MatchStarted,
    #[serde(rename = "match.ended")]
    MatchEnded,
    #[serde(rename = "discovery.source_available")]
    DiscoveryAvailable,
    #[serde(rename = "discovery.source_unavailable")]
    DiscoveryUnavailable,
    #[serde(rename = "discovery.source_incompatible")]
    DiscoveryIncompatible,
    #[serde(rename = "logging.records_dropped")]
    RecordsDropped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventOutcome {
    Accepted,
    Rejected,
    Expired,
    Closed,
    Unavailable,
    Completed,
    Dropped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingField {
    Name,
    Mode,
    Size,
    Minutes,
    FriendlyFire,
    Capacity,
    Password,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct EventDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<Mode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<ErrorCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub changed_fields: Vec<SettingField>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EventRecord {
    pub schema: u8,
    pub timestamp_ms: u64,
    pub sequence: u64,
    pub level: &'static str,
    pub event: EventKind,
    pub server_id: String,
    pub server_epoch: String,
    pub room_id: Option<String>,
    pub actor_id: Option<String>,
    pub room_revision: Option<u64>,
    pub tick: Option<u32>,
    pub outcome: EventOutcome,
    pub details: EventDetails,
}

impl EventRecord {
    pub fn new(
        identity: &ServerIdentity,
        event: EventKind,
        outcome: EventOutcome,
        timestamp_ms: u64,
        sequence: u64,
    ) -> Self {
        Self {
            schema: 1,
            timestamp_ms,
            sequence,
            level: "info",
            event,
            server_id: identity.server_id.clone(),
            server_epoch: identity.server_epoch.clone(),
            room_id: None,
            actor_id: None,
            room_revision: None,
            tick: None,
            outcome,
            details: EventDetails::default(),
        }
    }
}

pub trait EventSink: Send + Sync {
    fn try_emit(&self, event: EventRecord) -> bool;
}

#[derive(Debug, Default)]
pub struct NoopEventSink;

impl EventSink for NoopEventSink {
    fn try_emit(&self, _: EventRecord) -> bool {
        true
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::protocol::v2::ErrorCode;
    use crate::room::ServerIdentity;
    use std::sync::Mutex;

    #[test]
    fn typed_events_have_correlated_safe_fields_and_an_injectable_sink() {
        struct Capture(Mutex<Vec<EventRecord>>);
        impl EventSink for Capture {
            fn try_emit(&self, event: EventRecord) -> bool {
                self.0.lock().unwrap().push(event);
                true
            }
        }
        let identity = ServerIdentity {
            server_id: "alpha".into(),
            server_epoch: "epoch".into(),
        };
        let mut event = EventRecord::new(
            &identity,
            EventKind::RoomCreated,
            EventOutcome::Accepted,
            123,
            7,
        );
        event.room_id = Some("room".into());
        event.actor_id = Some("member".into());
        event.room_revision = Some(1);
        event.details.capacity = Some(4);
        event.details.password_required = Some(true);
        event.details.code = Some(ErrorCode::BadRoom);
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["schema"], 1);
        assert_eq!(value["event"], "room.created");
        assert_eq!(value["timestamp_ms"], 123);
        assert_eq!(value["sequence"], 7);
        assert_eq!(value["details"]["capacity"], 4);
        assert!(value["details"].get("password").is_none());
        assert!(value["details"].get("proof").is_none());
        let sink = Capture(Mutex::new(Vec::new()));
        assert!(sink.try_emit(event.clone()));
        assert_eq!(sink.0.lock().unwrap().len(), 1);
        assert!(NoopEventSink.try_emit(event));
    }
}
