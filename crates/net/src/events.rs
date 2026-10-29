use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

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

    /// Whether the sink's underlying output is currently healthy. Sinks that cannot fail
    /// (like [`NoopEventSink`]) are always healthy.
    fn healthy(&self) -> bool {
        true
    }

    /// Reads and resets the accumulated count of records that did not reach the output
    /// (queue overflow, failed writes, or discards while unhealthy). Sinks that never drop
    /// records always report zero.
    fn take_dropped(&self) -> u64 {
        0
    }
}

#[derive(Debug, Default)]
pub struct NoopEventSink;

impl EventSink for NoopEventSink {
    fn try_emit(&self, _: EventRecord) -> bool {
        true
    }
}

const EVENT_QUEUE_CAPACITY: usize = 4096;
const SHUTDOWN_DRAIN_BUDGET: Duration = Duration::from_secs(2);

enum WriterMsg {
    Record(Box<EventRecord>),
    Stop,
}

/// Bounded, nonblocking schema-version-1 JSONL writer. `try_emit` never blocks the
/// coordinator: once the 4,096-record queue is full, or sending fails for any other reason,
/// the record is conservatively counted as dropped rather than awaited or silently discarded.
/// A single background thread owns the destination and serializes one JSON object per line.
pub struct JsonlEventWriter {
    sender: SyncSender<WriterMsg>,
    dropped: Arc<AtomicU64>,
    healthy: Arc<AtomicBool>,
    completion: Mutex<Option<Receiver<()>>>,
}

impl JsonlEventWriter {
    pub fn start<W: Write + Send + 'static>(mut writer: W) -> Arc<Self> {
        let (sender, receiver) = sync_channel::<WriterMsg>(EVENT_QUEUE_CAPACITY);
        let (completion_tx, completion_rx) = channel::<()>();
        let dropped = Arc::new(AtomicU64::new(0));
        let healthy = Arc::new(AtomicBool::new(true));
        let warned = AtomicBool::new(false);
        let (worker_dropped, worker_healthy) = (dropped.clone(), healthy.clone());
        thread::spawn(move || {
            for msg in receiver {
                let record = match msg {
                    WriterMsg::Record(record) => *record,
                    WriterMsg::Stop => break,
                };
                let wrote = serde_json::to_vec(&record).ok().and_then(|mut line| {
                    line.push(b'\n');
                    writer.write_all(&line).ok()
                });
                if wrote.is_some() {
                    worker_healthy.store(true, Ordering::SeqCst);
                } else {
                    worker_dropped.fetch_add(1, Ordering::SeqCst);
                    worker_healthy.store(false, Ordering::SeqCst);
                    if !warned.swap(true, Ordering::SeqCst) {
                        eprintln!("warning: event log write failed; records are being dropped");
                    }
                }
            }
            let _ = completion_tx.send(());
        });
        Arc::new(Self {
            sender,
            dropped,
            healthy,
            completion: Mutex::new(Some(completion_rx)),
        })
    }

    /// Signals the writer thread to stop after any already-queued records, and waits at most
    /// [`SHUTDOWN_DRAIN_BUDGET`] for it to do so. Never waits indefinitely on a stuck writer:
    /// a `write_all` that never returns leaves the background thread running, but this method
    /// still returns once the budget elapses. Returns the final accumulated dropped count.
    pub fn shutdown(&self) -> u64 {
        let _ = self.sender.try_send(WriterMsg::Stop);
        if let Some(completion) = self
            .completion
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            let _ = completion.recv_timeout(SHUTDOWN_DRAIN_BUDGET);
        }
        self.dropped.load(Ordering::SeqCst)
    }
}

impl EventSink for JsonlEventWriter {
    fn try_emit(&self, event: EventRecord) -> bool {
        match self.sender.try_send(WriterMsg::Record(Box::new(event))) {
            Ok(()) => true,
            Err(_) => {
                self.dropped.fetch_add(1, Ordering::SeqCst);
                false
            }
        }
    }

    fn healthy(&self) -> bool {
        self.healthy.load(Ordering::SeqCst)
    }

    fn take_dropped(&self) -> u64 {
        self.dropped.swap(0, Ordering::SeqCst)
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

    #[test]
    fn event_json_never_contains_fields_outside_the_explicit_allowlist() {
        let identity = ServerIdentity {
            server_id: "alpha".into(),
            server_epoch: "epoch".into(),
        };
        let mut event = EventRecord::new(
            &identity,
            EventKind::PlayerJoinRejected,
            EventOutcome::Rejected,
            1,
            1,
        );
        event.details.code = Some(ErrorCode::BadPassword);
        event.details.count = Some(6);
        let value = serde_json::to_value(&event).unwrap();
        let top_level: std::collections::BTreeSet<_> =
            value.as_object().unwrap().keys().cloned().collect();
        let allowed: std::collections::BTreeSet<_> = [
            "schema",
            "timestamp_ms",
            "sequence",
            "level",
            "event",
            "server_id",
            "server_epoch",
            "room_id",
            "actor_id",
            "room_revision",
            "tick",
            "outcome",
            "details",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(top_level, allowed, "no raw/message field ever appears");
        let details: std::collections::BTreeSet<_> = value["details"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let allowed_details: std::collections::BTreeSet<_> =
            ["code", "count"].into_iter().map(String::from).collect();
        assert_eq!(
            details, allowed_details,
            "only explicitly set, allow-listed detail fields serialize"
        );
    }

    struct SharedBuf(Arc<Mutex<Vec<u8>>>);
    impl Write for SharedBuf {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn sample(identity: &ServerIdentity, sequence: u64) -> EventRecord {
        EventRecord::new(
            identity,
            EventKind::RoomCreated,
            EventOutcome::Accepted,
            sequence,
            sequence,
        )
    }

    #[test]
    fn jsonl_writer_delivers_every_record_under_normal_load_with_zero_drops() {
        let buf = Arc::new(Mutex::new(Vec::new()));
        let writer = JsonlEventWriter::start(SharedBuf(buf.clone()));
        let identity = ServerIdentity {
            server_id: "alpha".into(),
            server_epoch: "epoch".into(),
        };
        for sequence in 1..=100u64 {
            assert!(writer.try_emit(sample(&identity, sequence)));
        }
        writer.shutdown();
        assert_eq!(writer.take_dropped(), 0);
        assert!(writer.healthy());
        let lines = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        assert_eq!(lines.lines().count(), 100, "zero drops under normal load");
        for line in lines.lines() {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(value["schema"], 1);
        }
    }

    struct BlockOnFirstWrite(Mutex<Option<Receiver<()>>>);
    impl Write for BlockOnFirstWrite {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if let Some(gate) = self.0.lock().unwrap_or_else(|e| e.into_inner()).take() {
                let _ = gate.recv();
            }
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn jsonl_writer_drops_conservatively_once_the_bounded_queue_saturates() {
        let (release_tx, release_rx) = channel::<()>();
        let writer = JsonlEventWriter::start(BlockOnFirstWrite(Mutex::new(Some(release_rx))));
        let identity = ServerIdentity {
            server_id: "alpha".into(),
            server_epoch: "epoch".into(),
        };
        let mut rejected = 0u64;
        for sequence in 1..=(EVENT_QUEUE_CAPACITY as u64 + 200) {
            if !writer.try_emit(sample(&identity, sequence)) {
                rejected += 1;
            }
        }
        assert!(
            rejected > 0,
            "flooding well past the bounded queue capacity must drop some records"
        );
        assert_eq!(writer.take_dropped(), rejected);
        let _ = release_tx.send(());
        writer.shutdown();
    }

    struct NeverReturns;
    impl Write for NeverReturns {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            loop {
                thread::park();
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn jsonl_writer_shutdown_is_bounded_even_if_the_writer_never_returns() {
        let writer = JsonlEventWriter::start(NeverReturns);
        let identity = ServerIdentity {
            server_id: "alpha".into(),
            server_epoch: "epoch".into(),
        };
        assert!(writer.try_emit(sample(&identity, 1)));
        let started = std::time::Instant::now();
        writer.shutdown();
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "shutdown must not wait indefinitely on a stuck writer"
        );
    }

    struct FlakyThenOk {
        fail_remaining: Mutex<u32>,
        sink: Arc<Mutex<Vec<u8>>>,
    }
    impl Write for FlakyThenOk {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let mut remaining = self
                .fail_remaining
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if *remaining > 0 {
                *remaining -= 1;
                return Err(std::io::Error::other("simulated write failure"));
            }
            drop(remaining);
            self.sink
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn jsonl_writer_recovers_health_and_reports_an_exact_droppped_count() {
        let sink = Arc::new(Mutex::new(Vec::new()));
        let writer = JsonlEventWriter::start(FlakyThenOk {
            fail_remaining: Mutex::new(3),
            sink: sink.clone(),
        });
        let identity = ServerIdentity {
            server_id: "alpha".into(),
            server_epoch: "epoch".into(),
        };
        for sequence in 1..=5u64 {
            writer.try_emit(sample(&identity, sequence));
        }
        writer.shutdown();
        assert!(
            writer.healthy(),
            "the sink recovered after the first three failures"
        );
        assert_eq!(
            writer.take_dropped(),
            3,
            "exactly the failed writes count as dropped"
        );
        assert_eq!(
            writer.take_dropped(),
            0,
            "reading the count resets it; a second read without new failures is zero"
        );
        let lines = String::from_utf8(sink.lock().unwrap().clone()).unwrap();
        assert_eq!(
            lines.lines().count(),
            2,
            "the two post-recovery writes landed"
        );
    }
}
