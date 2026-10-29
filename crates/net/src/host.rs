//! The authoritative host: lobby, 20 Hz simulation, validation and disconnect handling.

use std::collections::HashMap;
use std::io::{BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use eksplode_re_bots::{Bot, DefaultBot};
use eksplode_re_core::{
    step, Config, Controller, DropTable, GameState, Lobby, Phase, PlayerId, PlayerInput,
    RoomSettings, MAX_PLAYERS,
};
use serde_json::Value;

use crate::client::Client;
use crate::codec::{decode_client, encode, LineReader, NetError};
use crate::events::{EventKind, EventOutcome, EventRecord, EventSink, NoopEventSink};
use crate::look::{resolve_colour, Colour, Look};
use crate::protocol::{ClientMsg, RejectReason, ServerMsg};
use crate::room::from_wire;
use crate::room::{
    AdmissionLedger, ControlBudget, InputBudget, MonotonicClock, OsRandom, RandomSource,
    RoomLimits, ServerIdentity, ServiceClock,
};

/// Connections that cause this many errors are closed.
const MAX_ERRORS: u32 = 10;
/// Inputs older than this many ticks are dropped.
const MAX_INPUT_AGE: u32 = 20;
const INBOUND_CAPACITY: usize = 256;
const CONNECTION_CAPACITY: usize = 128;

#[derive(Clone)]
pub struct HostServices {
    pub clock: Arc<dyn ServiceClock>,
    pub random: Arc<dyn RandomSource>,
    pub events: Arc<dyn EventSink>,
}

impl HostServices {
    pub fn new(clock: Arc<dyn ServiceClock>, random: Arc<dyn RandomSource>) -> Self {
        Self {
            clock,
            random,
            events: Arc::new(NoopEventSink),
        }
    }

    pub fn with_events(mut self, events: Arc<dyn EventSink>) -> Self {
        self.events = events;
        self
    }
}

impl Default for HostServices {
    fn default() -> Self {
        Self::new(Arc::new(MonotonicClock::default()), Arc::new(OsRandom))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConnectionContext {
    pub local: bool,
    pub peer: Option<SocketAddr>,
}

/// Host settings.
#[derive(Clone, Debug)]
pub struct HostConfig {
    pub bind: String,
    pub tick: Duration,
    /// Fixed seed for reproducible matches; `None` picks one per match.
    pub seed: Option<u64>,
    pub game: Config,
    /// While a match runs, a player silent for this long counts as disconnected.
    pub silence_timeout: Duration,
    /// Developer mode: the owner may pause bots and change the drop table (testing only).
    pub dev: bool,
    /// What a new room starts with (the defaults of the room creation menu).
    pub defaults: RoomSettings,
    pub server_id: Option<String>,
    pub limits: RoomLimits,
    pub default_bots: u8,
}

impl Default for HostConfig {
    fn default() -> Self {
        let game = Config::default();
        Self {
            bind: format!("0.0.0.0:{}", crate::protocol::DEFAULT_PORT),
            tick: Duration::from_secs(1) / game.tick_hz,
            seed: None,
            game,
            silence_timeout: Duration::from_secs(10),
            dev: false,
            defaults: RoomSettings::default(),
            server_id: None,
            limits: RoomLimits::default(),
            default_bots: 0,
        }
    }
}

enum Ev {
    WebAddress {
        address: SocketAddr,
        active: bool,
    },
    RoomControl {
        reference: crate::protocol::v2::RoomRef,
        command: OwnerCommand,
        response: SyncSender<Result<(), crate::protocol::v2::ErrorCode>>,
    },
    Directory {
        cursor: Option<String>,
        response: SyncSender<crate::protocol::v2::Reply>,
    },
    New {
        id: u64,
        tx: SyncSender<ServerMsg>,
        context: ConnectionContext,
    },
    Msg {
        id: u64,
        msg: ClientMsg,
        pending: Arc<AtomicUsize>,
    },
    Bad {
        id: u64,
        err: String,
    },
    Gone {
        id: u64,
    },
    Shutdown,
}

/// Attaches in-process clients to a running host; stays valid while the host lives.
#[derive(Clone)]
pub struct HostHandle {
    tx: SyncSender<Ev>,
    next_id: Arc<AtomicU64>,
    limits: RoomLimits,
}

impl HostHandle {
    pub fn advertise_web(&self, address: SocketAddr, active: bool) -> Result<(), NetError> {
        self.tx
            .try_send(Ev::WebAddress { address, active })
            .map_err(queue_error)
    }
    pub fn add_room_bot(&self, reference: &crate::protocol::v2::RoomRef) -> Result<(), NetError> {
        self.control_room(reference, OwnerCommand::AddBot)
    }

    pub fn remove_room_bot(
        &self,
        reference: &crate::protocol::v2::RoomRef,
    ) -> Result<(), NetError> {
        self.control_room(reference, OwnerCommand::RemoveBot)
    }

    pub fn start_room(&self, reference: &crate::protocol::v2::RoomRef) -> Result<(), NetError> {
        self.control_room(reference, OwnerCommand::Start)
    }

    fn control_room(
        &self,
        reference: &crate::protocol::v2::RoomRef,
        command: OwnerCommand,
    ) -> Result<(), NetError> {
        let (response, receiver) = mpsc::sync_channel(1);
        self.tx
            .try_send(Ev::RoomControl {
                reference: reference.clone(),
                command,
                response,
            })
            .map_err(queue_error)?;
        receiver
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| NetError::Protocol("room control unavailable".into()))?
            .map_err(|code| NetError::Protocol(format!("{code:?}")))
    }
    pub fn directory(
        &self,
        cursor: Option<String>,
    ) -> Result<crate::protocol::v2::Reply, NetError> {
        let (response, receiver) = mpsc::sync_channel(1);
        self.tx
            .try_send(Ev::Directory { cursor, response })
            .map_err(queue_error)?;
        receiver
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| NetError::Protocol("directory unavailable".into()))
    }
    pub fn limits(&self) -> RoomLimits {
        self.limits
    }
    /// See [`Host::connect`]. Fails with `Closed` once the host is gone.
    pub fn connect(&self, local: bool) -> Result<Client, NetError> {
        self.connect_context(ConnectionContext { local, peer: None })
    }

    pub fn connect_peer(&self, local: bool, peer: SocketAddr) -> Result<Client, NetError> {
        self.connect_context(ConnectionContext {
            local,
            peer: Some(peer),
        })
    }

    fn connect_context(&self, context: ConnectionContext) -> Result<Client, NetError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (stx, srx) = mpsc::sync_channel(CONNECTION_CAPACITY);
        self.tx
            .try_send(Ev::New {
                id,
                tx: stx,
                context,
            })
            .map_err(queue_error)?;
        let to_host = self.tx.clone();
        let on_drop = self.tx.clone();
        let pending = Arc::new(AtomicUsize::new(0));
        Ok(Client::from_parts(
            Box::new(move |msg| enqueue_input(&to_host, &pending, id, msg)),
            srx,
            Box::new(move || {
                let _ = on_drop.send(Ev::Gone { id });
            }),
        ))
    }
}

/// A running host. Dropping it ends the match and disconnects everybody.
pub struct Host {
    addr: SocketAddr,
    tx: SyncSender<Ev>,
    next_id: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    server: Option<JoinHandle<()>>,
    limits: RoomLimits,
}

impl Host {
    /// Binds the listener and starts the server threads.
    pub fn start(cfg: HostConfig) -> Result<Host, NetError> {
        Self::start_with_services(cfg, HostServices::default())
    }

    pub fn start_with_services(cfg: HostConfig, services: HostServices) -> Result<Host, NetError> {
        if cfg.tick.is_zero() || cfg.default_bots > 3 {
            return Err(NetError::Protocol(
                "invalid simulation tick or default bot count".into(),
            ));
        }
        cfg.game
            .validate()
            .map_err(|e| NetError::Protocol(e.to_string()))?;
        cfg.limits
            .validate()
            .map_err(|error| NetError::Protocol(error.to_string()))?;
        let identity = ServerIdentity::new(cfg.server_id.as_deref(), services.random.as_ref())
            .map_err(|error| NetError::Protocol(error.to_string()))?;
        let listener = TcpListener::bind(&cfg.bind)?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let (tx, rx) = mpsc::sync_channel(INBOUND_CAPACITY);
        let next_id = Arc::new(AtomicU64::new(1));
        let stop = Arc::new(AtomicBool::new(false));

        {
            let (tx, next_id, stop) = (tx.clone(), next_id.clone(), stop.clone());
            let connection_limit = usize::from(cfg.limits.max_connections);
            thread::spawn(move || accept_loop(listener, tx, next_id, stop, connection_limit));
        }
        let limits = cfg.limits;
        let server = thread::spawn(move || Server::new(cfg, rx, identity, services, addr).run());
        Ok(Host {
            addr,
            tx,
            next_id,
            stop,
            server: Some(server),
            limits,
        })
    }

    /// Address clients can connect to.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// A cloneable connector other threads can use to attach in-process clients.
    pub fn handle(&self) -> HostHandle {
        HostHandle {
            tx: self.tx.clone(),
            next_id: self.next_id.clone(),
            limits: self.limits,
        }
    }

    /// An unjoined in-process connection: send Hello, then explicitly create or select a room.
    ///
    /// `local` marks the connection as the person running the host (slot controller `Local`).
    pub fn connect(&self, local: bool) -> Result<Client, NetError> {
        self.handle().connect(local)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.tx.send(Ev::Shutdown);
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.server.take() {
            let _ = h.join();
        }
    }
}

fn accept_loop(
    listener: TcpListener,
    tx: SyncSender<Ev>,
    next_id: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    connection_limit: usize,
) {
    let active = Arc::new(AtomicUsize::new(0));
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                if active
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                        if count < connection_limit {
                            Some(count + 1)
                        } else {
                            None
                        }
                    })
                    .is_err()
                {
                    let _ = stream.set_write_timeout(Some(Duration::from_millis(100)));
                    if let Ok(bytes) =
                        encode(&ServerMsg::error("ServerFull", "connection limit reached"))
                    {
                        let _ = stream.write_all(&bytes);
                    }
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                    continue;
                }
                let permit = Arc::new(ConnectionPermit(active.clone()));
                let id = next_id.fetch_add(1, Ordering::SeqCst);
                if spawn_connection(stream, id, &tx, permit).is_err() {
                    continue;
                }
            }
            Err(_) => thread::sleep(Duration::from_millis(10)),
        }
    }
}

struct ConnectionPermit(Arc<AtomicUsize>);

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn queue_error(error: TrySendError<Ev>) -> NetError {
    match error {
        TrySendError::Full(_) => NetError::Protocol("host input queue full".into()),
        TrySendError::Disconnected(_) => NetError::Closed,
    }
}

fn enqueue_input(
    events: &SyncSender<Ev>,
    pending: &Arc<AtomicUsize>,
    id: u64,
    msg: ClientMsg,
) -> Result<(), NetError> {
    pending
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
            if count < CONNECTION_CAPACITY {
                Some(count + 1)
            } else {
                None
            }
        })
        .map_err(|_| NetError::Protocol("connection input queue full".into()))?;
    if let Err(error) = events.try_send(Ev::Msg {
        id,
        msg,
        pending: pending.clone(),
    }) {
        pending.fetch_sub(1, Ordering::SeqCst);
        return Err(queue_error(error));
    }
    Ok(())
}

/// One reader thread feeding the server loop and one writer thread draining its replies.
fn spawn_connection(
    stream: TcpStream,
    id: u64,
    events: &SyncSender<Ev>,
    permit: Arc<ConnectionPermit>,
) -> Result<(), NetError> {
    stream.set_nonblocking(false)?;
    stream.set_nodelay(true)?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let (stx, srx) = mpsc::sync_channel::<ServerMsg>(CONNECTION_CAPACITY);
    let mut writer = stream.try_clone()?;
    let writer_permit = permit.clone();
    thread::spawn(move || {
        let _permit = writer_permit;
        for msg in srx {
            let Ok(bytes) = encode(&msg) else {
                continue;
            };
            if writer.write_all(&bytes).is_err() {
                break;
            }
        }
        let _ = writer.shutdown(std::net::Shutdown::Both);
    });
    events
        .send(Ev::New {
            id,
            tx: stx,
            context: ConnectionContext {
                local: false,
                peer: stream.peer_addr().ok(),
            },
        })
        .map_err(|_| NetError::Closed)?;

    let events = events.clone();
    thread::spawn(move || {
        let _permit = permit;
        let pending = Arc::new(AtomicUsize::new(0));
        let mut lines = LineReader::new(BufReader::new(stream));
        loop {
            match lines.read_line() {
                Ok(Some(line)) => {
                    let result = match decode_client(&line) {
                        Ok(msg) => enqueue_input(&events, &pending, id, msg),
                        Err(error) => events
                            .send(Ev::Bad {
                                id,
                                err: error.to_string(),
                            })
                            .map_err(|_| NetError::Closed),
                    };
                    if result.is_err() {
                        break;
                    }
                }
                Err(NetError::LineTooLong) => {
                    let _ = events.send(Ev::Bad {
                        id,
                        err: NetError::LineTooLong.to_string(),
                    });
                    break;
                }
                _ => break,
            }
        }
        let _ = events.send(Ev::Gone { id });
    });
    Ok(())
}

enum DevCommand {
    Pause(bool),
    Drops(Vec<f32>),
    Reset,
}

#[derive(Clone, Copy)]
enum OwnerCommand {
    Start,
    AddBot,
    RemoveBot,
}

struct Conn {
    tx: SyncSender<ServerMsg>,
    context: ConnectionContext,
    slot: Option<usize>,
    errors: u32,
    last_seen: Instant,
    connected_at: Duration,
    negotiated_v2: bool,
    admissions: AdmissionLedger,
    failed: AtomicBool,
    control_budget: ControlBudget,
    input_budget: InputBudget,
    room_id: Option<String>,
    cursor: Option<DirectoryCursor>,
}

#[derive(Clone)]
struct DirectoryCursor {
    token: String,
    generation: Vec<(String, u64)>,
    offset: usize,
    expires: Duration,
}

impl Conn {
    fn deliver(&self, message: ServerMsg) {
        if self.tx.try_send(message).is_err() {
            self.failed.store(true, Ordering::SeqCst);
        }
    }
}

struct Running {
    state: GameState,
    pending: [Option<PlayerInput>; MAX_PLAYERS],
    bots: [Option<DefaultBot>; MAX_PLAYERS],
}

struct Server {
    cfg: HostConfig,
    rx: Receiver<Ev>,
    lobby: Lobby,
    conns: HashMap<u64, Conn>,
    game: Option<Running>,
    next_tick: Instant,
    matches: u64,
    /// The look of each slot; only the entries of seated slots mean something.
    looks: [Look; MAX_PLAYERS],
    /// Developer mode: bots give no input while this is set.
    bots_paused: bool,
    /// What destroyed blocks drop; changed live in developer mode and applied to every match.
    drops: DropTable,
    identity: ServerIdentity,
    services: HostServices,
    event_sequence: u64,
    rooms: HashMap<String, crate::room::RoomRuntime>,
    http_cursors: HashMap<String, DirectoryCursor>,
    game_addr: SocketAddr,
    web_addr: Option<SocketAddr>,
}

impl Server {
    fn new(
        cfg: HostConfig,
        rx: Receiver<Ev>,
        identity: ServerIdentity,
        services: HostServices,
        game_addr: SocketAddr,
    ) -> Self {
        let lobby = Lobby {
            settings: cfg.defaults.clone().sanitize(),
            ..Lobby::new()
        };
        Self {
            drops: DropTable::defaults(cfg.game.powerup_chance),
            cfg,
            rx,
            lobby,
            conns: HashMap::new(),
            game: None,
            next_tick: Instant::now(),
            matches: 0,
            looks: [0, 1, 2, 3].map(Look::default_for),
            bots_paused: false,
            identity,
            services,
            event_sequence: 0,
            rooms: HashMap::new(),
            http_cursors: HashMap::new(),
            game_addr: client_address(game_addr),
            web_addr: None,
        }
    }

    fn run(mut self) {
        self.emit_event(EventKind::ServerStarted, EventOutcome::Accepted);
        loop {
            let mut timeout = if self.game.is_some() {
                self.next_tick.saturating_duration_since(Instant::now())
            } else {
                Duration::from_millis(50)
            };
            let service_now = self.services.clock.now();
            if let Some(next) = self
                .rooms
                .values()
                .filter_map(crate::room::RoomRuntime::next_tick)
                .min()
            {
                timeout = timeout.min(next.saturating_sub(service_now));
            }
            match self.rx.recv_timeout(timeout) {
                Ok(Ev::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                    self.shutdown();
                    return;
                }
                Ok(ev) => self.handle(ev),
                Err(RecvTimeoutError::Timeout) => {}
            }
            if self.game.is_some() {
                let now = Instant::now();
                if now >= self.next_tick {
                    self.tick();
                    self.next_tick += self.cfg.tick;
                    if now > self.next_tick + self.cfg.tick * 10 {
                        self.next_tick = now + self.cfg.tick;
                    }
                }
            }
            self.drop_silent();
            let service_now = self.services.clock.now();
            let replies: Vec<_> = self
                .rooms
                .values_mut()
                .flat_map(|room| room.advance(service_now, self.cfg.tick))
                .collect();
            for (connection, reply) in replies {
                self.send(connection, ServerMsg::RoomV2(Box::new(reply)));
            }
            let service_now = self.services.clock.now();
            let expired: Vec<u64> = self
                .conns
                .iter()
                .filter(|(_, connection)| {
                    connection.failed.load(Ordering::SeqCst)
                        || (connection.slot.is_none()
                            && !connection.negotiated_v2
                            && service_now.saturating_sub(connection.connected_at)
                                >= Duration::from_secs(2))
                })
                .map(|(connection_id, _)| *connection_id)
                .collect();
            for connection_id in expired {
                self.remove(connection_id);
            }
        }
    }

    fn emit_event(&mut self, kind: EventKind, outcome: EventOutcome) {
        let Some(sequence) = self.event_sequence.checked_add(1) else {
            return;
        };
        self.event_sequence = sequence;
        let event = EventRecord::new(
            &self.identity,
            kind,
            outcome,
            self.services.clock.timestamp_ms(),
            sequence,
        );
        self.services.events.try_emit(event);
    }

    fn handle(&mut self, ev: Ev) {
        match ev {
            Ev::WebAddress { address, active } => {
                let address = client_address(address);
                if active {
                    self.web_addr = Some(address);
                } else if self.web_addr == Some(address) {
                    self.web_addr = None;
                }
            }
            Ev::RoomControl {
                reference,
                command,
                response,
            } => {
                use crate::protocol::v2::{ErrorCode, Request, RequestId};
                let result = self
                    .rooms
                    .get_mut(&reference.room_id)
                    .filter(|room| room.reference == reference)
                    .ok_or(ErrorCode::RoomNotFound)
                    .and_then(|room| {
                        let owner = room
                            .members
                            .owner_membership_id
                            .as_deref()
                            .and_then(|owner| room.members.get(owner))
                            .and_then(|owner| owner.connection_id)
                            .ok_or(ErrorCode::NotOwner)?;
                        let request_id =
                            RequestId::new("host-control").map_err(|_| ErrorCode::Unavailable)?;
                        let room_id = reference.room_id;
                        let request = match command {
                            OwnerCommand::AddBot => Request::AddBot {
                                request_id,
                                room_id,
                            },
                            OwnerCommand::RemoveBot => Request::RemoveBot {
                                request_id,
                                room_id,
                            },
                            OwnerCommand::Start => Request::Start {
                                request_id,
                                room_id,
                            },
                        };
                        room.command(
                            owner,
                            request,
                            self.services.clock.now(),
                            self.cfg.tick,
                            self.services.random.as_ref(),
                        )
                    });
                match result {
                    Ok(replies) => {
                        for (connection, reply) in replies {
                            self.send(connection, ServerMsg::RoomV2(Box::new(reply)));
                        }
                        let _ = response.try_send(Ok(()));
                    }
                    Err(code) => {
                        let _ = response.try_send(Err(code));
                    }
                }
            }
            Ev::Directory { cursor, response } => {
                let reply = self.http_directory(cursor);
                let _ = response.try_send(reply);
            }
            Ev::New { id, tx, context } => {
                if self.conns.len() >= usize::from(self.cfg.limits.max_connections) {
                    let _ = tx.try_send(ServerMsg::error("ServerFull", "connection limit reached"));
                    return;
                }
                self.conns.insert(
                    id,
                    Conn {
                        tx,
                        context,
                        slot: None,
                        errors: 0,
                        last_seen: Instant::now(),
                        connected_at: self.services.clock.now(),
                        negotiated_v2: false,
                        admissions: AdmissionLedger::default(),
                        failed: AtomicBool::new(false),
                        control_budget: ControlBudget::default(),
                        input_budget: InputBudget::default(),
                        room_id: None,
                        cursor: None,
                    },
                );
            }
            Ev::Msg { id, msg, pending } => {
                pending.fetch_sub(1, Ordering::SeqCst);
                self.on_message(id, msg);
            }
            Ev::Bad { id, err } => self.error(id, "Malformed", &err),
            Ev::Gone { id } => self.remove(id),
            Ev::Shutdown => {}
        }
    }

    fn on_message(&mut self, id: u64, msg: ClientMsg) {
        let now = self.services.clock.now();
        let Some(conn) = self.conns.get_mut(&id) else {
            return;
        };
        conn.last_seen = Instant::now();
        let joined = conn.slot.is_some();
        if conn.negotiated_v2 && !matches!(&msg, ClientMsg::RoomV2(_) | ClientMsg::Ping { .. }) {
            self.send_room_error(id, None, crate::protocol::v2::ErrorCode::BadVersion);
            self.remove(id);
            return;
        }
        let input = match &msg {
            ClientMsg::Input { .. } => true,
            ClientMsg::RoomV2(request) => {
                matches!(request.as_ref(), crate::protocol::v2::Request::Input { .. })
            }
            _ => false,
        };
        let legacy_input = matches!(&msg, ClientMsg::Input { .. }) && !conn.negotiated_v2;
        let allowed = if input {
            legacy_input || conn.input_budget.allow(now)
        } else {
            conn.control_budget.allow(now)
        };
        if !allowed {
            conn.errors = conn.errors.saturating_add(1);
            let close = conn.errors >= MAX_ERRORS;
            let reply = match &msg {
                ClientMsg::RoomV2(request) => {
                    ServerMsg::RoomV2(Box::new(crate::protocol::v2::Reply::Error {
                        request_id: request.request_id().cloned(),
                        code: crate::protocol::v2::ErrorCode::RateLimited,
                        message: "request rate limit reached".into(),
                    }))
                }
                _ => ServerMsg::error("RateLimited", "request rate limit reached"),
            };
            self.send(id, reply);
            self.emit_event(EventKind::RequestRateLimited, EventOutcome::Rejected);
            if close {
                self.remove(id);
            }
            return;
        }
        match msg {
            ClientMsg::RoomV2(request) => self.room_message(id, *request),
            ClientMsg::Join { .. } => {
                self.send(
                    id,
                    ServerMsg::Rejected {
                        reason: RejectReason::BadVersion,
                    },
                );
                self.remove(id);
            }
            ClientMsg::Ping { nonce } => self.send(id, ServerMsg::Pong { nonce }),
            ClientMsg::RoomQuery => self.send(id, self.room_info()),
            _ if !joined => {
                self.send(id, ServerMsg::error("NotJoined", "send Join first"));
                self.remove(id);
            }
            ClientMsg::Leave => self.remove(id),
            ClientMsg::SetLook { look } => self.set_look(id, &look),
            ClientMsg::SetRoom { settings } => self.set_room(id, &settings),
            ClientMsg::SetReady { ready } => self.set_ready(id, ready),
            ClientMsg::Input {
                tick,
                mv,
                bomb,
                punch,
            } => self.input(id, tick, PlayerInput { mv, bomb, punch }),
            ClientMsg::Start => self.owner_command(id, OwnerCommand::Start),
            ClientMsg::AddBot => self.owner_command(id, OwnerCommand::AddBot),
            ClientMsg::RemoveBot => self.owner_command(id, OwnerCommand::RemoveBot),
            ClientMsg::DevPauseBots { paused } => self.dev_command(id, DevCommand::Pause(paused)),
            ClientMsg::DevSetDrops { chances } => self.dev_command(id, DevCommand::Drops(chances)),
            ClientMsg::DevResetDrops => self.dev_command(id, DevCommand::Reset),
        }
    }

    fn room_message(&mut self, id: u64, request: crate::protocol::v2::Request) {
        use crate::protocol::v2::{ErrorCode, Limits, Reply, Request, RequestState, VERSION};

        let negotiated = self
            .conns
            .get(&id)
            .is_some_and(|connection| connection.negotiated_v2);
        let admission_id = match &request {
            Request::CreateRoom { request_id, .. }
            | Request::JoinRoom { request_id, .. }
            | Request::ResumeRoom { request_id, .. }
                if negotiated =>
            {
                Some(request_id.clone())
            }
            _ => None,
        };
        if let Some(request_id) = admission_id.as_ref() {
            let now = self.services.clock.now();
            let Some(connection) = self.conns.get_mut(&id) else {
                return;
            };
            let (state, result) = connection.admissions.status(request_id, now);
            if state != RequestState::Unknown {
                let reply = result.unwrap_or_else(|| Reply::RequestStatus {
                    request_id: request_id.clone(),
                    target_request_id: request_id.clone(),
                    status: state,
                    result: None,
                });
                self.send(id, ServerMsg::RoomV2(Box::new(reply)));
                return;
            }
            if let Err(code) = connection.admissions.begin(request_id.clone(), now) {
                self.send(
                    id,
                    ServerMsg::RoomV2(Box::new(Reply::Error {
                        request_id: Some(request_id.clone()),
                        code,
                        message: "admission is busy".into(),
                    })),
                );
                return;
            }
        }
        match request {
            Request::Hello { version } if version != VERSION => {
                self.send(
                    id,
                    ServerMsg::Rejected {
                        reason: RejectReason::BadVersion,
                    },
                );
                self.conns.remove(&id);
            }
            Request::Hello { .. }
                if !negotiated
                    && self
                        .conns
                        .get(&id)
                        .is_some_and(|connection| connection.slot.is_none()) =>
            {
                if let Some(connection) = self.conns.get_mut(&id) {
                    connection.negotiated_v2 = true;
                }
                self.send(
                    id,
                    ServerMsg::RoomV2(Box::new(Reply::Hello {
                        version: VERSION,
                        server_id: self.identity.server_id.clone(),
                        server_epoch: self.identity.server_epoch.clone(),
                        capabilities: vec!["rooms".into(), "directory".into()],
                        defaults: self.cfg.defaults.clone(),
                        limits: Limits {
                            max_rooms: self.cfg.limits.max_rooms,
                            max_connections: self.cfg.limits.max_connections,
                        },
                    })),
                );
            }
            Request::Ping { nonce } if negotiated => {
                self.send(id, ServerMsg::RoomV2(Box::new(Reply::Pong { nonce })));
            }
            Request::ListRooms {
                request_id, cursor, ..
            } if negotiated => {
                self.list_rooms(id, request_id, cursor);
            }
            Request::CreateRoom {
                request_id,
                name,
                settings,
                capacity,
                password,
                look,
            } if negotiated => {
                let result = if password.is_some_and(|value| !value.expose().is_empty()) {
                    Err(ErrorCode::Unsupported)
                } else {
                    self.create_room(id, &name, settings, capacity, look.as_ref())
                        .map(|(room, admission)| {
                            (
                                Reply::Welcome {
                                    request_id: request_id.clone(),
                                    admission,
                                },
                                room,
                            )
                        })
                };
                self.finish_room_admission(id, request_id, result, look.as_ref());
            }
            Request::JoinRoom {
                request_id,
                server_epoch,
                room_id,
                name,
                look,
                ..
            } if negotiated => {
                let result = if self.conns.get(&id).is_some_and(|connection| {
                    connection.room_id.is_some() || connection.slot.is_some()
                }) {
                    Err(ErrorCode::AlreadyInRoom)
                } else if server_epoch != self.identity.server_epoch {
                    Err(ErrorCode::RoomNotFound)
                } else {
                    self.rooms
                        .get(&room_id)
                        .ok_or(ErrorCode::RoomNotFound)
                        .and_then(crate::room::RoomRuntime::admission_candidate)
                        .and_then(|mut room| {
                            let admission = room.admit(
                                id,
                                &name,
                                self.conns
                                    .get(&id)
                                    .is_some_and(|connection| connection.context.local),
                                look.as_ref(),
                                self.services.random.as_ref(),
                            )?;
                            Ok((
                                Reply::Welcome {
                                    request_id: request_id.clone(),
                                    admission,
                                },
                                room,
                            ))
                        })
                };
                self.finish_room_admission(id, request_id, result, look.as_ref());
            }
            Request::LeaveRoom {
                request_id,
                room_id,
            } if negotiated => {
                if self
                    .conns
                    .get(&id)
                    .and_then(|connection| connection.room_id.as_deref())
                    != Some(room_id.as_str())
                {
                    self.send_room_error(id, Some(request_id), ErrorCode::NotInRoom);
                    return;
                }
                let Some(reference) = self.rooms.get(&room_id).map(|room| room.reference.clone())
                else {
                    self.send_room_error(id, Some(request_id), ErrorCode::RoomNotFound);
                    return;
                };
                if let Some(connection) = self.conns.get_mut(&id) {
                    connection.room_id = None;
                }
                self.detach_room(id, &room_id);
                self.send(
                    id,
                    ServerMsg::RoomV2(Box::new(Reply::LeftRoom {
                        request_id,
                        reference,
                    })),
                );
            }
            request @ (Request::Start { .. }
            | Request::AddBot { .. }
            | Request::RemoveBot { .. }
            | Request::SetReady { .. }
            | Request::SetLook { .. }
            | Request::Input { .. }
            | Request::DevPauseBots { .. }
            | Request::DevSetDrops { .. }
            | Request::DevResetDrops { .. }
            | Request::SetRoom { .. })
                if negotiated =>
            {
                let request_id = request.request_id().cloned();
                let bound = self
                    .conns
                    .get(&id)
                    .and_then(|connection| connection.room_id.clone());
                if bound.as_deref() != request.room_id() || bound.is_none() {
                    self.send_room_error(id, request_id, ErrorCode::NotInRoom);
                    return;
                }
                let now = self.services.clock.now();
                let result = bound
                    .as_ref()
                    .and_then(|room| self.rooms.get_mut(room))
                    .ok_or(ErrorCode::RoomNotFound)
                    .and_then(|room| {
                        room.command(
                            id,
                            request,
                            now,
                            self.cfg.tick,
                            self.services.random.as_ref(),
                        )
                    });
                match result {
                    Ok(replies) => {
                        for (connection, reply) in replies {
                            self.send(connection, ServerMsg::RoomV2(Box::new(reply)));
                        }
                    }
                    Err(code) => self.send_room_error(id, request_id, code),
                }
            }
            Request::RequestStatus {
                request_id,
                target_request_id,
            } if negotiated => {
                let now = self.services.clock.now();
                if let Some(connection) = self.conns.get_mut(&id) {
                    let (status, result) = connection.admissions.status(&target_request_id, now);
                    self.send(
                        id,
                        ServerMsg::RoomV2(Box::new(Reply::RequestStatus {
                            request_id,
                            target_request_id,
                            status,
                            result: result.map(Box::new),
                        })),
                    );
                }
            }
            Request::CancelRequest {
                request_id,
                target_request_id,
            } if negotiated => {
                let now = self.services.clock.now();
                if let Some(connection) = self.conns.get_mut(&id) {
                    let (status, result) = connection.admissions.cancel(&target_request_id, now);
                    self.send(
                        id,
                        ServerMsg::RoomV2(Box::new(Reply::RequestStatus {
                            request_id,
                            target_request_id,
                            status,
                            result: result.map(Box::new),
                        })),
                    );
                }
            }
            request => {
                let mut reply = Reply::Error {
                    request_id: request.request_id().cloned(),
                    code: if negotiated {
                        if request.room_id().is_some_and(|room_id| {
                            self.conns
                                .get(&id)
                                .and_then(|connection| connection.room_id.as_deref())
                                != Some(room_id)
                        }) {
                            ErrorCode::NotInRoom
                        } else {
                            ErrorCode::Unsupported
                        }
                    } else {
                        ErrorCode::BadVersion
                    },
                    message: if negotiated {
                        "room operation is not available".into()
                    } else {
                        "send protocol 2 Hello first".into()
                    },
                };
                if let Some(request_id) = admission_id {
                    if let Some(connection) = self.conns.get_mut(&id) {
                        if let Err(code) = connection.admissions.finish(
                            &request_id,
                            self.services.clock.now(),
                            reply.clone(),
                            false,
                        ) {
                            reply = Reply::Error {
                                request_id: Some(request_id),
                                code,
                                message: "admission is no longer pending".into(),
                            };
                        }
                    }
                }
                self.send(id, ServerMsg::RoomV2(Box::new(reply)));
                if !negotiated {
                    self.conns.remove(&id);
                }
            }
        }
    }

    fn send_room_error(
        &self,
        id: u64,
        request_id: Option<crate::protocol::v2::RequestId>,
        code: crate::protocol::v2::ErrorCode,
    ) {
        let message = if code == crate::protocol::v2::ErrorCode::NotOwner {
            self.conns
                .get(&id)
                .and_then(|connection| connection.room_id.as_ref())
                .and_then(|room_id| self.rooms.get(room_id))
                .and_then(|room| {
                    room.members
                        .owner_membership_id
                        .as_deref()
                        .and_then(|owner| room.members.get(owner))
                        .and_then(|owner| room.lobby.slots.get(usize::from(owner.slot)))
                })
                .map(|owner| format!("Only {} can manage this room", owner.name))
                .unwrap_or_else(|| "Only the room owner can perform this action".into())
        } else if code == crate::protocol::v2::ErrorCode::ModeNeedsPlayers {
            self.conns
                .get(&id)
                .and_then(|connection| connection.room_id.as_ref())
                .and_then(|room_id| self.rooms.get(room_id))
                .and_then(|room| room.lobby.start_problem())
                .unwrap_or_else(|| "This mode needs more participants".into())
        } else {
            format!("{code:?}")
        };
        self.send(
            id,
            ServerMsg::RoomV2(Box::new(crate::protocol::v2::Reply::Error {
                request_id,
                code,
                message,
            })),
        );
    }

    fn list_rooms(
        &mut self,
        id: u64,
        request_id: crate::protocol::v2::RequestId,
        cursor: Option<String>,
    ) {
        let saved = self
            .conns
            .get(&id)
            .and_then(|connection| connection.cursor.clone());
        match self.directory_page(request_id.clone(), cursor, saved) {
            Ok((reply, next)) => {
                if let Some(connection) = self.conns.get_mut(&id) {
                    connection.cursor = next;
                }
                self.send(id, ServerMsg::RoomV2(Box::new(reply)));
            }
            Err(code) => self.send_room_error(id, Some(request_id), code),
        }
    }

    fn directory_page(
        &self,
        request_id: crate::protocol::v2::RequestId,
        cursor: Option<String>,
        saved: Option<DirectoryCursor>,
    ) -> Result<(crate::protocol::v2::Reply, Option<DirectoryCursor>), crate::protocol::v2::ErrorCode>
    {
        use crate::protocol::v2::{ErrorCode, Reply};
        let mut rooms: Vec<_> = self
            .rooms
            .values()
            .map(|room| {
                let mut summary = room.summary();
                summary.game_addr = Some(self.game_addr.to_string());
                summary.web_origin = self.web_addr.map(|address| format!("http://{address}"));
                summary
            })
            .collect();
        rooms.sort_by(|first, second| first.reference.room_id.cmp(&second.reference.room_id));
        let generation: Vec<_> = rooms
            .iter()
            .map(|room| (room.reference.room_id.clone(), room.revision))
            .collect();
        let now = self.services.clock.now();
        let offset = if let Some(token) = cursor {
            match saved {
                Some(saved)
                    if saved.token == token
                        && saved.generation == generation
                        && now < saved.expires =>
                {
                    saved.offset
                }
                _ => return Err(ErrorCode::StaleRevision),
            }
        } else {
            0
        };
        let end = (offset + 32).min(rooms.len());
        let page = rooms[offset..end].to_vec();
        let next = if end < rooms.len() {
            match crate::room::random_id(self.services.random.as_ref()) {
                Ok(token) => Some(DirectoryCursor {
                    token,
                    generation,
                    offset: end,
                    expires: now
                        .checked_add(Duration::from_secs(5))
                        .unwrap_or(Duration::MAX),
                }),
                Err(_) => return Err(ErrorCode::Unavailable),
            }
        } else {
            None
        };
        let next_cursor = next.as_ref().map(|saved| saved.token.clone());
        Ok((
            Reply::Rooms {
                request_id,
                sources: vec![serde_json::json!({
                    "server_id": self.identity.server_id, "server_epoch": self.identity.server_epoch,
                    "protocol_version": crate::protocol::v2::VERSION,
                    "capabilities": ["rooms", "directory"], "status": "fresh",
                    "last_success_age_ms": 0, "last_checked_age_ms": 0,
                    "game_addr": self.game_addr.to_string(),
                    "web_origin": self.web_addr.map(|address| format!("http://{address}"))
                })],
                rooms: page,
                next_cursor,
            },
            next,
        ))
    }

    fn http_directory(&mut self, cursor: Option<String>) -> crate::protocol::v2::Reply {
        use crate::protocol::v2::{ErrorCode, Reply, RequestId};
        let request_id = match RequestId::new("http-directory") {
            Ok(request_id) => request_id,
            Err(_) => return Reply::Pong { nonce: 0 },
        };
        let now = self.services.clock.now();
        self.http_cursors.retain(|_, saved| now < saved.expires);
        if cursor.is_none() && self.rooms.len() > 32 && self.http_cursors.len() >= 128 {
            return Reply::Error {
                request_id: Some(request_id),
                code: ErrorCode::Busy,
                message: "directory cursor limit reached".into(),
            };
        }
        let saved = cursor
            .as_ref()
            .and_then(|token| self.http_cursors.get(token))
            .cloned();
        match self.directory_page(request_id.clone(), cursor.clone(), saved) {
            Ok((reply, next)) => {
                if let Some(token) = cursor {
                    self.http_cursors.remove(&token);
                }
                if let Some(next) = next {
                    self.http_cursors.insert(next.token.clone(), next);
                }
                reply
            }
            Err(code) => Reply::Error {
                request_id: Some(request_id),
                message: format!("{code:?}"),
                code,
            },
        }
    }

    fn create_room(
        &self,
        id: u64,
        name: &str,
        settings: Value,
        capacity: u8,
        look: Option<&Value>,
    ) -> Result<
        (crate::room::RoomRuntime, crate::protocol::v2::Admission),
        crate::protocol::v2::ErrorCode,
    > {
        use crate::protocol::v2::{ErrorCode, RoomRef};
        if self
            .conns
            .get(&id)
            .is_some_and(|connection| connection.room_id.is_some() || connection.slot.is_some())
        {
            return Err(ErrorCode::AlreadyInRoom);
        }
        if self.rooms.len() >= usize::from(self.cfg.limits.max_rooms) {
            return Err(ErrorCode::ServerFull);
        }
        let mut settings: RoomSettings =
            serde_json::from_value(settings).map_err(|_| ErrorCode::BadRoom)?;
        if settings.name.chars().any(char::is_control) {
            return Err(ErrorCode::BadRoom);
        }
        settings.name = settings.name.trim().to_string();
        if settings.name.chars().count() > 24 {
            return Err(ErrorCode::BadRoom);
        }
        if settings.name.is_empty() {
            settings.name = format!("{name}'s room").chars().take(24).collect();
        }
        let room_id = crate::room::random_id(self.services.random.as_ref())
            .map_err(|_| ErrorCode::Unavailable)?;
        if self.rooms.contains_key(&room_id) {
            return Err(ErrorCode::Unavailable);
        }
        let reference = RoomRef {
            server_id: self.identity.server_id.clone(),
            server_epoch: self.identity.server_epoch.clone(),
            room_id: room_id.clone(),
        };
        let mut room = crate::room::RoomRuntime::new(
            reference,
            settings,
            capacity,
            self.cfg.game.clone(),
            self.cfg.dev,
            self.cfg.seed,
        )?;
        let local = self
            .conns
            .get(&id)
            .is_some_and(|connection| connection.context.local);
        let creator = room.admit(id, name, local, look, self.services.random.as_ref())?;
        for _ in 0..self.cfg.default_bots.min(capacity.saturating_sub(1)) {
            let request_id = crate::protocol::v2::RequestId::new("startup-bot")
                .map_err(|_| ErrorCode::Unavailable)?;
            room.command(
                id,
                crate::protocol::v2::Request::AddBot {
                    request_id,
                    room_id: room_id.clone(),
                },
                self.services.clock.now(),
                self.cfg.tick,
                self.services.random.as_ref(),
            )?;
        }
        let admission = room.admission(&creator.membership_id)?;
        Ok((room, admission))
    }

    fn finish_room_admission(
        &mut self,
        id: u64,
        request_id: crate::protocol::v2::RequestId,
        result: Result<
            (crate::protocol::v2::Reply, crate::room::RoomRuntime),
            crate::protocol::v2::ErrorCode,
        >,
        look: Option<&Value>,
    ) {
        use crate::protocol::v2::{ErrorCode, Reply};
        let (mut reply, candidate) = match result {
            Ok((reply, room)) => (reply, Some(room)),
            Err(code) => (
                Reply::Error {
                    request_id: Some(request_id.clone()),
                    message: format!("{code:?}"),
                    code,
                },
                None,
            ),
        };
        let mut committed = false;
        if let Some(connection) = self.conns.get_mut(&id) {
            if connection
                .admissions
                .finish(
                    &request_id,
                    self.services.clock.now(),
                    reply.clone(),
                    candidate.is_some(),
                )
                .is_err()
            {
                reply = Reply::Error {
                    request_id: Some(request_id),
                    code: ErrorCode::RequestExpired,
                    message: "admission deadline expired".into(),
                };
            } else {
                committed = candidate.is_some();
            }
        }
        if committed {
            if let Some(room) = candidate {
                let room_id = room.reference.room_id.clone();
                self.rooms.insert(room_id.clone(), room);
                if let Some(connection) = self.conns.get_mut(&id) {
                    connection.room_id = Some(room_id);
                }
            }
        }
        self.send(id, ServerMsg::RoomV2(Box::new(reply)));
        if committed {
            if let Some(room_id) = self
                .conns
                .get(&id)
                .and_then(|connection| connection.room_id.clone())
            {
                self.broadcast_room(&room_id);
                if let Some(notice) = self
                    .rooms
                    .get(&room_id)
                    .and_then(|room| room.look_notice(id, look))
                {
                    self.send(id, ServerMsg::RoomV2(Box::new(notice)));
                }
                if self.cfg.dev {
                    if let Some(room) = self.rooms.get(&room_id) {
                        self.send(id, ServerMsg::RoomV2(Box::new(room.dev_state())));
                    }
                }
            }
        }
    }

    fn broadcast_room(&self, room_id: &str) {
        if let Some(room) = self.rooms.get(room_id) {
            let update = room.updated();
            for connection in self
                .conns
                .values()
                .filter(|connection| connection.room_id.as_deref() == Some(room_id))
            {
                connection.deliver(ServerMsg::RoomV2(Box::new(update.clone())));
            }
        }
    }

    fn detach_room(&mut self, id: u64, room_id: &str) {
        use crate::protocol::v2::{Reply, RoomPhase};
        let left = self.rooms.get_mut(room_id).and_then(|room| {
            let slot = room
                .members
                .values()
                .find(|member| member.connection_id == Some(id))
                .map(|member| member.slot);
            room.remove_connection(id);
            slot.map(|slot| (slot, room.phase, room.reference.clone()))
        });
        let empty = self
            .rooms
            .get(room_id)
            .is_some_and(|room| room.members.values().next().is_none());
        if empty {
            self.rooms.remove(room_id);
        } else {
            if let Some((slot, RoomPhase::Running, reference)) = left {
                for connection in self
                    .conns
                    .values()
                    .filter(|connection| connection.room_id.as_deref() == Some(room_id))
                {
                    connection.deliver(ServerMsg::RoomV2(Box::new(Reply::PlayerLeft {
                        reference: reference.clone(),
                        slot,
                        replaced_by_bot: true,
                    })));
                }
            }
            self.broadcast_room(room_id);
        }
    }

    /// Developer commands: refused unless the host runs in developer mode and the sender is
    /// the owner; values are validated before anything changes.
    fn dev_command(&mut self, id: u64, command: DevCommand) {
        if !self.cfg.dev {
            return self.error(
                id,
                "DevDisabled",
                "developer mode is off (start with --dev)",
            );
        }
        let Some(slot) = self.conns.get(&id).and_then(|c| c.slot) else {
            return;
        };
        match self.lobby.owner() {
            Some(owner) if owner == slot => {}
            Some(owner) => {
                let message = format!(
                    "Only {} can use the developer commands",
                    self.lobby.slots[owner].name
                );
                return self.error(id, "NotOwner", &message);
            }
            None => return,
        }
        match command {
            DevCommand::Pause(paused) => self.bots_paused = paused,
            DevCommand::Reset => self.set_drops(DropTable::defaults(self.cfg.game.powerup_chance)),
            DevCommand::Drops(chances) => {
                let table = <[f32; 8]>::try_from(chances.as_slice())
                    .ok()
                    .and_then(|c| DropTable::new(c).ok());
                match table {
                    Some(table) => self.set_drops(table),
                    None => {
                        return self.error(
                            id,
                            "BadValue",
                            "send eight drop chances, each between 0 and 1",
                        )
                    }
                }
            }
        }
        self.broadcast_dev_state();
    }

    /// Stores the table and applies it to the running match for the blocks destroyed from now on.
    fn set_drops(&mut self, table: DropTable) {
        if let Some(game) = self.game.as_mut() {
            let _ = game.state.set_drops(table.clone());
        }
        self.drops = table;
    }

    fn dev_state(&self) -> ServerMsg {
        ServerMsg::DevState {
            paused: self.bots_paused,
            drops: self.drops.chances,
        }
    }

    fn broadcast_dev_state(&self) {
        self.broadcast(self.dev_state());
    }

    /// Lobby commands that only the owner (lowest human slot) may give; every refusal is
    /// answered with an `Error` so the user always sees why nothing happened.
    fn owner_command(&mut self, id: u64, command: OwnerCommand) {
        let Some(slot) = self.conns.get(&id).and_then(|c| c.slot) else {
            return;
        };
        match self.lobby.owner() {
            Some(owner) if owner == slot => {}
            Some(owner) => {
                let what = match command {
                    OwnerCommand::Start => "start the match",
                    OwnerCommand::AddBot | OwnerCommand::RemoveBot => "change the bots",
                };
                let message = format!("Only {} can {what}", self.lobby.slots[owner].name);
                return self.error(id, "NotOwner", &message);
            }
            None => return,
        }
        match command {
            OwnerCommand::Start if self.game.is_some() => {
                self.error(id, "CannotStart", "a match is already running")
            }
            OwnerCommand::Start => match self.lobby.start_problem() {
                Some(problem) => self.error(id, "CannotStart", &problem),
                None => self.start(),
            },
            _ if self.game.is_some() => {
                self.error(id, "WrongPhase", "bots can only be changed in the lobby")
            }
            OwnerCommand::AddBot => match self.lobby.add_bot() {
                Ok(slot) => {
                    self.seat_look(slot, None);
                    self.broadcast_lobby();
                }
                Err(_) => self.error(id, "NoSlot", "all slots are taken"),
            },
            OwnerCommand::RemoveBot => {
                if let Some(bot) = self.lobby.last_bot() {
                    let _ = self.lobby.clear(bot);
                    self.broadcast_lobby();
                }
            }
        }
    }

    /// The room is open while a person is in it.
    fn room_info(&self) -> ServerMsg {
        let humans = self
            .lobby
            .slots
            .iter()
            .filter(|s| matches!(s.controller, Controller::Local | Controller::Remote))
            .count();
        ServerMsg::RoomInfo {
            open: humans > 0,
            settings: self.lobby.settings.clone(),
            humans: humans as u8,
        }
    }

    /// Forgets the settings of a room nobody is in; the next person creates it anew.
    fn close_room_if_empty(&mut self) {
        if self.lobby.owner().is_none() {
            self.lobby.settings = self.cfg.defaults.clone().sanitize();
        }
    }

    fn set_room(&mut self, id: u64, settings: &Value) {
        let Some(slot) = self.conns.get(&id).and_then(|c| c.slot) else {
            return;
        };
        match self.lobby.owner() {
            Some(owner) if owner == slot => {}
            Some(owner) => {
                let message = format!(
                    "Only {} can change the room settings",
                    self.lobby.slots[owner].name
                );
                return self.error(id, "NotOwner", &message);
            }
            None => return,
        }
        if self.game.is_some() {
            return self.error(id, "WrongPhase", "the room can only change in the lobby");
        }
        if !settings.is_object() {
            return self.error(id, "BadRoom", "the room settings must be an object");
        }
        let mut next = from_wire(settings).sanitize();
        if next.name.is_empty() {
            next.name = self.lobby.settings.name.clone();
        }
        self.lobby.settings = next;
        self.broadcast_lobby();
    }

    /// Gives `slot` the requested look (`None` means its default) with a colour no other seated
    /// participant has; returns the colour it had to be moved to, if any.
    fn seat_look(&mut self, slot: usize, requested: Option<&Value>) -> Option<Colour> {
        let wanted = requested.map_or_else(
            || Look::default_for(slot),
            |value| Look::from_wire(value, slot),
        );
        let taken: Vec<Colour> = (0..MAX_PLAYERS)
            .filter(|i| *i != slot && self.lobby.slots[*i].controller != Controller::Empty)
            .map(|i| self.looks[i].colour)
            .collect();
        let (colour, adjusted) = resolve_colour(wanted.colour, &taken);
        self.looks[slot] = Look { colour, ..wanted };
        adjusted.then_some(colour)
    }

    fn looks_to_send(&self) -> Vec<Look> {
        (0..MAX_PLAYERS)
            .map(|i| {
                if self.lobby.slots[i].controller == Controller::Empty {
                    Look::default_for(i)
                } else {
                    self.looks[i]
                }
            })
            .collect()
    }

    fn send_look_notice(&self, id: u64, colour: Colour) {
        let message = format!("That colour is taken: you are {} instead", colour.name());
        self.send(id, ServerMsg::error("LookAdjusted", &message));
    }

    fn set_look(&mut self, id: u64, look: &Value) {
        let Some(slot) = self.conns.get(&id).and_then(|c| c.slot) else {
            return;
        };
        if self.game.is_some() {
            return self.error(id, "WrongPhase", "the look can only change in the lobby");
        }
        if let Some(colour) = self.seat_look(slot, Some(look)) {
            self.send_look_notice(id, colour);
        }
        self.broadcast_lobby();
    }

    fn set_ready(&mut self, id: u64, ready: bool) {
        let Some(slot) = self.conns.get(&id).and_then(|c| c.slot) else {
            return;
        };
        if self.game.is_some() {
            return self.error(id, "WrongPhase", "a match is running");
        }
        self.lobby.slots[slot].ready = ready;
        self.broadcast_lobby();
    }

    fn input(&mut self, id: u64, tick: u32, input: PlayerInput) {
        let Some(slot) = self.conns.get(&id).and_then(|c| c.slot) else {
            return;
        };
        let Some(game) = self.game.as_mut() else {
            return self.error(id, "WrongPhase", "no match is running");
        };
        let now = game.state.tick;
        if tick > now {
            return self.error(id, "BadTick", "input from the future");
        }
        if now - tick > MAX_INPUT_AGE {
            return;
        }
        let merged = match game.pending[slot] {
            Some(old) => PlayerInput {
                mv: input.mv,
                bomb: old.bomb || input.bomb,
                punch: old.punch || input.punch,
            },
            None => input,
        };
        game.pending[slot] = Some(merged);
    }

    fn start(&mut self) {
        if self.game.is_some() || !self.lobby.can_start() {
            return;
        }
        self.matches += 1;
        let seed = self.cfg.seed.unwrap_or_else(|| {
            let t = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos() as u64);
            t ^ self.matches
        });
        let config = self.lobby.settings.to_config(&self.cfg.game);
        let Ok(mut state) = GameState::new(config.clone(), seed, self.lobby.active()) else {
            return;
        };
        let _ = state.set_drops(self.drops.clone());
        let bots = bots_for(&self.lobby, seed);
        self.broadcast(ServerMsg::Start { seed, config });
        self.game = Some(Running {
            state,
            pending: [None; MAX_PLAYERS],
            bots,
        });
        self.next_tick = Instant::now() + self.cfg.tick;
        self.send_snapshots(&[]);
    }

    fn tick(&mut self) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        let mut inputs = [PlayerInput::default(); MAX_PLAYERS];
        let ids = PlayerId::all();
        for (i, input) in inputs.iter_mut().enumerate() {
            *input = match game.bots[i].as_mut() {
                Some(_) if self.bots_paused => PlayerInput::default(),
                Some(bot) => bot.decide(&game.state.observe(ids[i])),
                None => game.pending[i].take().unwrap_or_default(),
            };
        }
        let events = step(&mut game.state, &inputs);
        let phase = game.state.phase;
        self.send_snapshots(&events);
        if let Phase::Ended(outcome) = phase {
            self.broadcast(ServerMsg::Ended { outcome });
            self.game = None;
            for s in &mut self.lobby.slots {
                s.ready = s.controller == Controller::Bot;
            }
            self.close_room_if_empty();
            self.broadcast_lobby();
        }
    }

    fn send_snapshots(&mut self, events: &[eksplode_re_core::Event]) {
        let Some(game) = self.game.as_ref() else {
            return;
        };
        let ids = PlayerId::all();
        for conn in self.conns.values() {
            if let Some(slot) = conn.slot {
                conn.deliver(ServerMsg::Snapshot {
                    tick: game.state.tick,
                    state: Box::new(game.state.observe(ids[slot])),
                    events: events.to_vec(),
                });
            }
        }
    }

    /// Removes a connection; its slot is freed in the lobby or taken over by a bot in a match.
    fn remove(&mut self, id: u64) {
        let Some(conn) = self.conns.remove(&id) else {
            return;
        };
        if let Some(room_id) = conn.room_id.as_ref() {
            self.detach_room(id, room_id);
            return;
        }
        let Some(slot) = conn.slot else {
            return;
        };
        if let Some(game) = self.game.as_mut() {
            self.lobby.slots[slot].controller = Controller::Bot;
            game.bots[slot] = Some(DefaultBot::new(self.matches.wrapping_mul(8) + slot as u64));
            self.broadcast(ServerMsg::PlayerLeft {
                slot: slot as u8,
                replaced_by_bot: true,
            });
        } else {
            let _ = self.lobby.clear(slot);
            self.close_room_if_empty();
            self.broadcast_lobby();
        }
    }

    fn drop_silent(&mut self) {
        let limit = self.cfg.silence_timeout;
        let silent: Vec<u64> = self
            .conns
            .iter()
            .filter(|(_, connection)| {
                let legacy_match = self.game.is_some() && connection.slot.is_some();
                let room_match = connection
                    .room_id
                    .as_ref()
                    .and_then(|room_id| self.rooms.get(room_id))
                    .is_some_and(|room| room.phase == crate::protocol::v2::RoomPhase::Running);
                (legacy_match || room_match) && connection.last_seen.elapsed() > limit
            })
            .map(|(id, _)| *id)
            .collect();
        for id in silent {
            let message = if self
                .conns
                .get(&id)
                .is_some_and(|connection| connection.negotiated_v2)
            {
                ServerMsg::RoomV2(Box::new(crate::protocol::v2::Reply::Error {
                    request_id: None,
                    code: crate::protocol::v2::ErrorCode::Unavailable,
                    message: "no input received".into(),
                }))
            } else {
                ServerMsg::error("Timeout", "no input received")
            };
            self.send(id, message);
            self.remove(id);
        }
    }

    fn error(&mut self, id: u64, code: &str, message: &str) {
        self.send(id, ServerMsg::error(code, message));
        let too_many = self.conns.get_mut(&id).is_some_and(|c| {
            c.errors += 1;
            c.errors >= MAX_ERRORS
        });
        if too_many {
            self.remove(id);
        }
    }

    fn send(&self, id: u64, msg: ServerMsg) {
        if let Some(c) = self.conns.get(&id) {
            c.deliver(msg);
        }
    }

    fn broadcast(&self, msg: ServerMsg) {
        for c in self.conns.values().filter(|c| c.slot.is_some()) {
            c.deliver(msg.clone());
        }
    }

    fn broadcast_lobby(&self) {
        self.broadcast(ServerMsg::Lobby {
            lobby: self.lobby.clone(),
            looks: self.looks_to_send(),
        });
    }

    fn shutdown(&mut self) {
        self.emit_event(EventKind::ServerStopping, EventOutcome::Accepted);
        for connection in self.conns.values() {
            let message = if connection.negotiated_v2 {
                let reply = match connection
                    .room_id
                    .as_ref()
                    .and_then(|room_id| self.rooms.get(room_id))
                {
                    Some(room) => crate::protocol::v2::Reply::RoomClosed {
                        reference: room.reference.clone(),
                        reason: "shutdown".into(),
                    },
                    None => crate::protocol::v2::Reply::Error {
                        request_id: None,
                        code: crate::protocol::v2::ErrorCode::Unavailable,
                        message: "the server stopped".into(),
                    },
                };
                ServerMsg::RoomV2(Box::new(reply))
            } else {
                ServerMsg::error("HostLeft", "the host closed the match")
            };
            connection.deliver(message);
        }
        self.conns.clear();
        self.emit_event(EventKind::ServerStopped, EventOutcome::Completed);
    }
}

fn bots_for(lobby: &Lobby, seed: u64) -> [Option<DefaultBot>; MAX_PLAYERS] {
    [0, 1, 2, 3].map(|i| {
        (lobby.slots[i].controller == Controller::Bot)
            .then(|| DefaultBot::new(seed.wrapping_mul(4).wrapping_add(i as u64)))
    })
}

fn client_address(mut address: SocketAddr) -> SocketAddr {
    if address.ip().is_unspecified() {
        address.set_ip(if address.is_ipv4() {
            std::net::Ipv4Addr::LOCALHOST.into()
        } else {
            std::net::Ipv6Addr::LOCALHOST.into()
        });
    }
    address
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn peer_context_is_supplied_by_the_transport_not_a_player_message() {
        let (sender, receiver) = mpsc::sync_channel(8);
        let handle = HostHandle {
            tx: sender,
            next_id: Arc::new(AtomicU64::new(1)),
            limits: RoomLimits::default(),
        };
        let peer: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let client = handle.connect_peer(false, peer).unwrap();
        match receiver.recv_timeout(Duration::from_secs(1)).unwrap() {
            Ev::New { context, .. } => assert_eq!(
                context,
                ConnectionContext {
                    local: false,
                    peer: Some(peer)
                }
            ),
            _ => panic!("expected a transport connection"),
        }
        drop(client);
        receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        let local = handle.connect(true).unwrap();
        match receiver.recv_timeout(Duration::from_secs(1)).unwrap() {
            Ev::New { context, .. } => assert_eq!(
                context,
                ConnectionContext {
                    local: true,
                    peer: None
                }
            ),
            _ => panic!("expected an in-process connection"),
        }
        drop(local);
    }
}
