//! Reading room settings from the wire. Decoding is lenient, like looks: a missing or invalid
//! field becomes its default and a value that is not an object gives the defaults, so an old
//! page, a typo or a hostile message never breaks a room.

use std::collections::HashMap;
use std::fmt;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc::{self, Receiver, SyncSender, TryRecvError},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use argon2::{Algorithm, Argon2, Params, Version};
use eksplode_re_bots::{Bot, DefaultBot};
use eksplode_re_core::{
    step, BoardSize, Config, Controller, DropTable, Event, GameState, Lobby, Mode, Phase, PlayerId,
    PlayerInput, RoomSettings,
};
use serde_json::Value;
use subtle::ConstantTimeEq;

use crate::look::{resolve_colour, Look};
use crate::protocol::v2::{
    Admission, ErrorCode, Reply, Request, RequestId, RequestState, RoomPhase, RoomRef, RoomSummary,
    Secret,
};

#[derive(Clone)]
pub struct PasswordVerifier {
    salt: [u8; 16],
    digest: [u8; 32],
}

impl PasswordVerifier {
    pub fn create(password: &Secret, random: &dyn RandomSource) -> Result<Self, ErrorCode> {
        Self::validate(password)?;
        let mut salt = [0; 16];
        random.fill(&mut salt).map_err(|_| ErrorCode::Unavailable)?;
        let digest = Self::derive(password, &salt)?;
        Ok(Self { salt, digest })
    }

    pub fn verify(&self, password: &Secret) -> Result<bool, ErrorCode> {
        Self::validate(password)?;
        let candidate = Self::derive(password, &self.salt)?;
        Ok(bool::from(self.digest.ct_eq(&candidate)))
    }

    pub(crate) fn validate(password: &Secret) -> Result<(), ErrorCode> {
        let value = password.expose();
        if value.is_empty()
            || value.len() > 512
            || value.chars().count() > 128
            || value.chars().any(char::is_control)
        {
            return Err(ErrorCode::BadPassword);
        }
        Ok(())
    }

    fn derive(password: &Secret, salt: &[u8; 16]) -> Result<[u8; 32], ErrorCode> {
        let params = Params::new(19 * 1024, 2, 1, Some(32)).map_err(|_| ErrorCode::Unavailable)?;
        let mut digest = [0; 32];
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(password.expose().as_bytes(), salt, &mut digest)
            .map_err(|_| ErrorCode::Unavailable)?;
        Ok(digest)
    }
}

impl fmt::Debug for PasswordVerifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PasswordVerifier([REDACTED])")
    }
}

#[derive(Debug)]
pub enum PasswordWork {
    Create(Secret),
    Verify {
        verifier: PasswordVerifier,
        password: Secret,
    },
}

#[derive(Debug)]
pub enum PasswordWorkResult {
    Created(PasswordVerifier),
    Verified(bool),
}

#[derive(Debug)]
pub struct PasswordJob {
    pub connection_id: u64,
    pub request_id: RequestId,
    pub work: PasswordWork,
}

#[derive(Debug)]
pub struct PasswordCompletion {
    pub connection_id: u64,
    pub request_id: RequestId,
    pub result: Result<PasswordWorkResult, ErrorCode>,
}

pub struct PasswordWorkers {
    jobs: Option<SyncSender<PasswordJob>>,
    results: Receiver<PasswordCompletion>,
    outstanding: Arc<AtomicUsize>,
    stopped: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl PasswordWorkers {
    pub fn start(random: Arc<dyn RandomSource>) -> Result<Self, ErrorCode> {
        Self::start_with(move |work| match work {
            PasswordWork::Create(password) => PasswordVerifier::create(&password, random.as_ref())
                .map(PasswordWorkResult::Created),
            PasswordWork::Verify { verifier, password } => {
                verifier.verify(&password).map(PasswordWorkResult::Verified)
            }
        })
    }

    fn start_with(
        process: impl Fn(PasswordWork) -> Result<PasswordWorkResult, ErrorCode> + Send + Sync + 'static,
    ) -> Result<Self, ErrorCode> {
        let (jobs, receiver) = mpsc::sync_channel::<PasswordJob>(32);
        let receiver = Arc::new(Mutex::new(receiver));
        let (completed, results) = mpsc::sync_channel(34);
        let process = Arc::new(process);
        let mut pool = Self {
            jobs: Some(jobs),
            results,
            outstanding: Arc::new(AtomicUsize::new(0)),
            stopped: Arc::new(AtomicBool::new(false)),
            threads: Vec::new(),
        };
        for index in 0..2 {
            let receiver = receiver.clone();
            let completed = completed.clone();
            let process = process.clone();
            let stopped = pool.stopped.clone();
            let worker = thread::Builder::new()
                .name(format!("password-{index}"))
                .spawn(move || {
                    while !stopped.load(Ordering::SeqCst) {
                        let job = match receiver.lock() {
                            Ok(receiver) => receiver.recv(),
                            Err(_) => break,
                        };
                        let Ok(job) = job else {
                            break;
                        };
                        if stopped.load(Ordering::SeqCst) {
                            break;
                        }
                        let result = process(job.work);
                        if completed
                            .try_send(PasswordCompletion {
                                connection_id: job.connection_id,
                                request_id: job.request_id,
                                result,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                })
                .map_err(|_| ErrorCode::Unavailable)?;
            pool.threads.push(worker);
        }
        Ok(pool)
    }

    pub fn try_submit(&self, job: PasswordJob) -> Result<(), ErrorCode> {
        let jobs = self.jobs.as_ref().ok_or(ErrorCode::Unavailable)?;
        self.outstanding
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                (count < 34).then_some(count + 1)
            })
            .map_err(|_| ErrorCode::Busy)?;
        if let Err(error) = jobs.try_send(job) {
            self.outstanding.fetch_sub(1, Ordering::SeqCst);
            return Err(match error {
                mpsc::TrySendError::Full(_) => ErrorCode::Busy,
                mpsc::TrySendError::Disconnected(_) => ErrorCode::Unavailable,
            });
        }
        Ok(())
    }

    pub fn try_recv(&self) -> Result<PasswordCompletion, TryRecvError> {
        let result = self.results.try_recv()?;
        self.outstanding.fetch_sub(1, Ordering::SeqCst);
        Ok(result)
    }
}

impl Drop for PasswordWorkers {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.jobs.take();
        for worker in self.threads.drain(..) {
            let _ = worker.join();
        }
    }
}

#[derive(Debug)]
struct AdmissionEntry {
    deadline: Duration,
    finished_at: Option<Duration>,
    state: RequestState,
    reply: Option<Reply>,
}

#[derive(Debug, Default)]
pub struct AdmissionLedger {
    entries: HashMap<RequestId, AdmissionEntry>,
}

#[derive(Debug)]
pub struct ControlBudget {
    tokens: f64,
    updated_at: Duration,
}

impl Default for ControlBudget {
    fn default() -> Self {
        Self {
            tokens: 40.0,
            updated_at: Duration::ZERO,
        }
    }
}

impl ControlBudget {
    pub fn allow(&mut self, now: Duration) -> bool {
        self.tokens =
            (self.tokens + now.saturating_sub(self.updated_at).as_secs_f64() * 20.0).min(40.0);
        self.updated_at = self.updated_at.max(now);
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}

#[derive(Debug, Default)]
pub struct InputBudget {
    window_start: Option<Duration>,
    count: u32,
}

impl InputBudget {
    pub fn allow(&mut self, now: Duration) -> bool {
        if self.window_start.map_or(true, |start| {
            now.saturating_sub(start) >= Duration::from_secs(1)
        }) {
            self.window_start = Some(now);
            self.count = 0;
        }
        if self.count >= 60 {
            return false;
        }
        self.count += 1;
        true
    }
}

impl AdmissionLedger {
    fn prune(&mut self, now: Duration) {
        for (request_id, entry) in &mut self.entries {
            if entry.state == RequestState::Pending && now >= entry.deadline {
                entry.state = RequestState::Expired;
                entry.finished_at = Some(entry.deadline);
                entry.reply = Some(Reply::Error {
                    request_id: Some(request_id.clone()),
                    code: ErrorCode::RequestExpired,
                    message: "admission deadline expired".into(),
                });
            }
        }
        self.entries.retain(|_, entry| {
            entry.finished_at.map_or(true, |finished| {
                now.saturating_sub(finished) < Duration::from_secs(60)
            })
        });
    }

    pub fn begin(
        &mut self,
        request_id: RequestId,
        now: Duration,
    ) -> Result<RequestState, ErrorCode> {
        self.prune(now);
        if let Some(entry) = self.entries.get(&request_id) {
            return Ok(entry.state);
        }
        if self.entries.len() >= 8
            || self
                .entries
                .values()
                .any(|entry| entry.state == RequestState::Pending)
        {
            return Err(ErrorCode::Busy);
        }
        self.entries.insert(
            request_id,
            AdmissionEntry {
                deadline: now
                    .checked_add(Duration::from_secs(5))
                    .unwrap_or(Duration::MAX),
                finished_at: None,
                state: RequestState::Pending,
                reply: None,
            },
        );
        Ok(RequestState::Pending)
    }

    pub fn status(
        &mut self,
        request_id: &RequestId,
        now: Duration,
    ) -> (RequestState, Option<Reply>) {
        self.prune(now);
        self.entries
            .get(request_id)
            .map(|entry| (entry.state, entry.reply.clone()))
            .unwrap_or((RequestState::Unknown, None))
    }

    pub fn pending_deadline(&mut self, request_id: &RequestId, now: Duration) -> Option<Duration> {
        self.prune(now);
        self.entries
            .get(request_id)
            .filter(|entry| entry.state == RequestState::Pending)
            .map(|entry| entry.deadline)
    }

    pub fn cancel(
        &mut self,
        request_id: &RequestId,
        now: Duration,
    ) -> (RequestState, Option<Reply>) {
        self.prune(now);
        if let Some(entry) = self.entries.get_mut(request_id) {
            if entry.state == RequestState::Pending {
                entry.state = RequestState::Cancelled;
                entry.finished_at = Some(now);
                entry.reply = Some(Reply::Error {
                    request_id: Some(request_id.clone()),
                    code: ErrorCode::Cancelled,
                    message: "admission cancelled".into(),
                });
            }
        }
        self.status(request_id, now)
    }

    pub fn finish(
        &mut self,
        request_id: &RequestId,
        now: Duration,
        reply: Reply,
        committed: bool,
    ) -> Result<(), ErrorCode> {
        self.prune(now);
        let entry = self
            .entries
            .get_mut(request_id)
            .ok_or(ErrorCode::RequestExpired)?;
        match entry.state {
            RequestState::Pending => {}
            RequestState::Expired => return Err(ErrorCode::RequestExpired),
            RequestState::Cancelled => return Err(ErrorCode::Cancelled),
            _ => return Err(ErrorCode::Busy),
        }
        entry.state = if committed {
            RequestState::Committed
        } else {
            RequestState::Rejected
        };
        entry.finished_at = Some(now);
        entry.reply = Some(reply);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomError {
    RandomUnavailable,
    InvalidServerId,
    InvalidProof,
    InvalidLimits,
    InvalidSlot,
    SlotUnavailable,
    CounterExhausted,
}

impl fmt::Display for RoomError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RandomUnavailable => "secure randomness unavailable",
            Self::InvalidServerId => "server ID must contain 1 to 64 ASCII identifier characters",
            Self::InvalidProof => "invalid reconnect proof",
            Self::InvalidLimits => "room or connection limit is out of range",
            Self::InvalidSlot => "participant slot must be between zero and three",
            Self::SlotUnavailable => "participant slot is already occupied",
            Self::CounterExhausted => "room counter exhausted",
        })
    }
}

impl std::error::Error for RoomError {}

pub trait RandomSource: Send + Sync {
    fn fill(&self, output: &mut [u8]) -> Result<(), RoomError>;
}

#[derive(Debug, Default)]
pub struct OsRandom;

impl RandomSource for OsRandom {
    fn fill(&self, output: &mut [u8]) -> Result<(), RoomError> {
        getrandom::getrandom(output).map_err(|_| RoomError::RandomUnavailable)
    }
}

pub trait ServiceClock: Send + Sync {
    fn now(&self) -> Duration;
    fn timestamp_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0)
    }
}

#[derive(Debug)]
pub struct MonotonicClock(Instant);

impl Default for MonotonicClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}

impl ServiceClock for MonotonicClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
}

pub fn random_id(random: &dyn RandomSource) -> Result<String, RoomError> {
    let mut bytes = [0; 16];
    random.fill(&mut bytes)?;
    Ok(hex(&bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerIdentity {
    pub server_id: String,
    pub server_epoch: String,
}

impl ServerIdentity {
    pub fn new(server_id: Option<&str>, random: &dyn RandomSource) -> Result<Self, RoomError> {
        if server_id.is_some_and(|identifier| !valid_server_id(identifier)) {
            return Err(RoomError::InvalidServerId);
        }
        let server_epoch = random_id(random)?;
        let server_id = server_id
            .map(str::to_string)
            .unwrap_or_else(|| format!("server-{server_epoch}"));
        Ok(Self {
            server_id,
            server_epoch,
        })
    }
}

pub fn valid_server_id(identifier: &str) -> bool {
    !identifier.is_empty()
        && identifier.len() <= 64
        && identifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

#[derive(Clone)]
pub struct RejoinProof([u8; 32]);

impl RejoinProof {
    pub fn generate(random: &dyn RandomSource) -> Result<Self, RoomError> {
        let mut bytes = [0; 32];
        random.fill(&mut bytes)?;
        Ok(Self(bytes))
    }

    pub fn from_secret(secret: &Secret) -> Result<Self, RoomError> {
        let text = secret.expose();
        if text.len() != 64
            || !text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(RoomError::InvalidProof);
        }
        let mut bytes = [0; 32];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16)
                .map_err(|_| RoomError::InvalidProof)?;
        }
        Ok(Self(bytes))
    }

    pub fn matches(&self, candidate: &Secret) -> bool {
        Self::from_secret(candidate)
            .map(|candidate| bool::from(self.0.ct_eq(&candidate.0)))
            .unwrap_or(false)
    }

    pub fn to_secret(&self) -> Secret {
        Secret::new(hex(&self.0))
    }
}

impl fmt::Debug for RejoinProof {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoomRevision(u64);

impl RoomRevision {
    pub fn new(value: u64) -> Self {
        Self(value)
    }
    pub fn get(&self) -> u64 {
        self.0
    }
    pub fn advance(&mut self) -> Result<u64, RoomError> {
        self.0 = self.0.checked_add(1).ok_or(RoomError::CounterExhausted)?;
        Ok(self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MembershipStatus {
    Connected,
    Reserved,
}

#[derive(Clone, Debug)]
pub struct HumanMembership {
    pub membership_id: String,
    pub slot: u8,
    pub join_sequence: u64,
    pub connection_id: Option<u64>,
    pub status: MembershipStatus,
    pub reserved_until: Option<Duration>,
    pub proof: RejoinProof,
}

#[derive(Clone, Debug, Default)]
pub struct RoomMembers {
    pub owner_membership_id: Option<String>,
    entries: HashMap<String, HumanMembership>,
    next_sequence: u64,
}

impl RoomMembers {
    pub fn admit(
        &mut self,
        slot: u8,
        connection_id: u64,
        random: &dyn RandomSource,
    ) -> Result<&HumanMembership, RoomError> {
        if slot >= 4 {
            return Err(RoomError::InvalidSlot);
        }
        if self
            .entries
            .values()
            .any(|member| member.slot == slot || member.connection_id == Some(connection_id))
        {
            return Err(RoomError::SlotUnavailable);
        }
        let next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(RoomError::CounterExhausted)?;
        let membership_id = random_id(random)?;
        if self.entries.contains_key(&membership_id) {
            return Err(RoomError::RandomUnavailable);
        }
        let proof = RejoinProof::generate(random)?;
        let member = HumanMembership {
            membership_id: membership_id.clone(),
            slot,
            join_sequence: self.next_sequence,
            connection_id: Some(connection_id),
            status: MembershipStatus::Connected,
            reserved_until: None,
            proof,
        };
        self.next_sequence = next_sequence;
        if self.owner_membership_id.is_none() {
            self.owner_membership_id = Some(membership_id.clone());
        }
        self.entries.insert(membership_id.clone(), member);
        self.entries
            .get(&membership_id)
            .ok_or(RoomError::RandomUnavailable)
    }

    pub fn get(&self, membership_id: &str) -> Option<&HumanMembership> {
        self.entries.get(membership_id)
    }
    pub fn get_mut(&mut self, membership_id: &str) -> Option<&mut HumanMembership> {
        self.entries.get_mut(membership_id)
    }
    pub fn values(&self) -> impl Iterator<Item = &HumanMembership> {
        self.entries.values()
    }

    pub fn reserve(&mut self, connection_id: u64, now: Duration) -> bool {
        let Some(member) = self
            .entries
            .values_mut()
            .find(|member| member.connection_id == Some(connection_id))
        else {
            return false;
        };
        member.connection_id = None;
        member.status = MembershipStatus::Reserved;
        member.reserved_until = Some(
            now.checked_add(Duration::from_secs(60))
                .unwrap_or(Duration::MAX),
        );
        true
    }

    pub fn remove(&mut self, membership_id: &str) -> Option<HumanMembership> {
        let removed = self.entries.remove(membership_id)?;
        if self.owner_membership_id.as_deref() == Some(membership_id) {
            self.owner_membership_id = self
                .entries
                .values()
                .filter(|member| {
                    member.status == MembershipStatus::Connected && member.connection_id.is_some()
                })
                .min_by_key(|member| (member.join_sequence, member.slot))
                .map(|member| member.membership_id.clone());
        }
        Some(removed)
    }

    pub fn expire(&mut self, now: Duration) -> Vec<HumanMembership> {
        let expired: Vec<String> = self
            .entries
            .values()
            .filter(|member| {
                member.status == MembershipStatus::Reserved
                    && member
                        .reserved_until
                        .is_some_and(|deadline| now >= deadline)
            })
            .map(|member| member.membership_id.clone())
            .collect();
        expired
            .into_iter()
            .filter_map(|membership_id| self.remove(&membership_id))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoomLimits {
    pub max_rooms: u16,
    pub max_connections: u16,
}

pub struct RoomRuntime {
    pub reference: RoomRef,
    pub revision: RoomRevision,
    pub members: RoomMembers,
    pub lobby: Lobby,
    pub capacity: u8,
    pub phase: RoomPhase,
    pub looks: [Look; 4],
    config: Config,
    dev: bool,
    fixed_seed: Option<u64>,
    matches: RoomRevision,
    running: Option<RoomGame>,
    next_tick: Duration,
    drops: DropTable,
    bots_paused: bool,
    pub(crate) password: Option<PasswordVerifier>,
}

struct RoomGame {
    state: GameState,
    pending: [Option<PlayerInput>; 4],
    bots: [Option<DefaultBot>; 4],
}

impl RoomRuntime {
    pub fn new(
        reference: RoomRef,
        settings: RoomSettings,
        capacity: u8,
        config: Config,
        dev: bool,
        fixed_seed: Option<u64>,
    ) -> Result<Self, ErrorCode> {
        if !(1..=4).contains(&capacity) || !eksplode_re_core::MINUTES.contains(&settings.minutes) {
            return Err(ErrorCode::BadRoom);
        }
        if settings.mode != Mode::Normal && capacity != 4 {
            return Err(ErrorCode::ModeNeedsPlayers);
        }
        let drops = DropTable::defaults(config.powerup_chance);
        Ok(Self {
            reference,
            revision: RoomRevision::new(0),
            members: RoomMembers::default(),
            lobby: Lobby {
                settings,
                ..Lobby::new()
            },
            capacity,
            phase: RoomPhase::Lobby,
            looks: [0, 1, 2, 3].map(Look::default_for),
            config,
            dev,
            fixed_seed,
            matches: RoomRevision::default(),
            running: None,
            next_tick: Duration::ZERO,
            drops,
            bots_paused: false,
            password: None,
        })
    }

    pub fn admission_candidate(&self) -> Result<Self, ErrorCode> {
        if self.phase != RoomPhase::Lobby {
            return Err(ErrorCode::InMatch);
        }
        let mut candidate = Self::new(
            self.reference.clone(),
            self.lobby.settings.clone(),
            self.capacity,
            self.config.clone(),
            self.dev,
            self.fixed_seed,
        )?;
        candidate.revision = self.revision;
        candidate.members = self.members.clone();
        candidate.lobby = self.lobby.clone();
        candidate.looks = self.looks;
        candidate.matches = self.matches;
        candidate.drops = self.drops.clone();
        candidate.bots_paused = self.bots_paused;
        candidate.password = self.password.clone();
        Ok(candidate)
    }

    fn slot(&self, connection_id: u64) -> Result<usize, ErrorCode> {
        self.members
            .values()
            .find(|member| member.connection_id == Some(connection_id))
            .map(|member| usize::from(member.slot))
            .ok_or(ErrorCode::NotInRoom)
    }

    pub(crate) fn owner(&self, connection_id: u64) -> Result<usize, ErrorCode> {
        let slot = self.slot(connection_id)?;
        let owner = self
            .members
            .owner_membership_id
            .as_deref()
            .and_then(|owner| self.members.get(owner));
        if owner.is_some_and(|owner| owner.connection_id == Some(connection_id)) {
            Ok(slot)
        } else {
            Err(ErrorCode::NotOwner)
        }
    }

    fn fanout(&self, reply: Reply) -> Vec<(u64, Reply)> {
        self.members
            .values()
            .filter_map(|member| {
                member
                    .connection_id
                    .map(|connection| (connection, reply.clone()))
            })
            .collect()
    }

    fn snapshots(&self, events: &[Event]) -> Vec<(u64, Reply)> {
        let Some(game) = self.running.as_ref() else {
            return Vec::new();
        };
        self.members
            .values()
            .filter_map(|member| {
                let connection = member.connection_id?;
                let player = PlayerId::new(member.slot).ok()?;
                Some((
                    connection,
                    Reply::Snapshot {
                        reference: self.reference.clone(),
                        tick: game.state.tick,
                        state: Box::new(game.state.observe(player)),
                        events: events.to_vec(),
                    },
                ))
            })
            .collect()
    }

    pub fn command(
        &mut self,
        connection_id: u64,
        request: Request,
        now: Duration,
        tick: Duration,
        random: &dyn RandomSource,
    ) -> Result<Vec<(u64, Reply)>, ErrorCode> {
        let slot = self.slot(connection_id)?;
        match request {
            Request::Start { .. } => {
                self.owner(connection_id)?;
                if self.phase != RoomPhase::Lobby {
                    return Err(ErrorCode::InMatch);
                }
                if self.lobby.start_problem().is_some() {
                    return Err(ErrorCode::ModeNeedsPlayers);
                }
                let mut next_match = self.matches;
                next_match.advance().map_err(|_| ErrorCode::Unavailable)?;
                let mut revision = self.revision;
                revision.advance().map_err(|_| ErrorCode::Unavailable)?;
                let seed = match self.fixed_seed {
                    Some(seed) => seed,
                    None => {
                        let mut bytes = [0; 8];
                        random
                            .fill(&mut bytes)
                            .map_err(|_| ErrorCode::Unavailable)?;
                        u64::from_le_bytes(bytes)
                    }
                };
                let config = self.lobby.settings.to_config(&self.config);
                let mut state = GameState::new(config.clone(), seed, self.lobby.active())
                    .map_err(|_| ErrorCode::BadRoom)?;
                state
                    .set_drops(self.drops.clone())
                    .map_err(|_| ErrorCode::BadRoom)?;
                let bots = std::array::from_fn(|index| {
                    (self.lobby.slots[index].controller == Controller::Bot)
                        .then(|| DefaultBot::new(seed.wrapping_mul(4).wrapping_add(index as u64)))
                });
                self.running = Some(RoomGame {
                    state,
                    pending: [None; 4],
                    bots,
                });
                self.phase = RoomPhase::Running;
                self.next_tick = now.checked_add(tick).unwrap_or(Duration::MAX);
                self.matches = next_match;
                self.revision = revision;
                let mut replies = self.fanout(Reply::Start {
                    reference: self.reference.clone(),
                    seed,
                    config,
                });
                replies.extend(self.snapshots(&[]));
                replies.extend(self.fanout(self.updated()));
                return Ok(replies);
            }
            Request::AddBot { .. } => {
                self.owner(connection_id)?;
                if self.phase != RoomPhase::Lobby {
                    return Err(ErrorCode::InMatch);
                }
                if self.lobby.participants() >= usize::from(self.capacity) {
                    return Err(ErrorCode::Full);
                }
                self.revision
                    .advance()
                    .map_err(|_| ErrorCode::Unavailable)?;
                let bot = self.lobby.add_bot().map_err(|_| ErrorCode::Full)?;
                let wanted = Look::default_for(bot);
                let taken: Vec<_> = self
                    .lobby
                    .slots
                    .iter()
                    .enumerate()
                    .filter(|(index, item)| *index != bot && item.controller != Controller::Empty)
                    .map(|(index, _)| self.looks[index].colour)
                    .collect();
                let (colour, _) = resolve_colour(wanted.colour, &taken);
                self.looks[bot] = Look { colour, ..wanted };
            }
            Request::RemoveBot { .. } => {
                self.owner(connection_id)?;
                if self.phase != RoomPhase::Lobby {
                    return Err(ErrorCode::InMatch);
                }
                if let Some(bot) = self.lobby.last_bot() {
                    self.revision
                        .advance()
                        .map_err(|_| ErrorCode::Unavailable)?;
                    self.lobby.clear(bot).map_err(|_| ErrorCode::BadRoom)?;
                }
            }
            Request::SetReady { ready, .. } => {
                if self.phase != RoomPhase::Lobby {
                    return Err(ErrorCode::InMatch);
                }
                self.revision
                    .advance()
                    .map_err(|_| ErrorCode::Unavailable)?;
                self.lobby.slots[slot].ready = ready;
            }
            Request::SetLook { look, .. } => {
                if self.phase != RoomPhase::Lobby {
                    return Err(ErrorCode::InMatch);
                }
                self.revision
                    .advance()
                    .map_err(|_| ErrorCode::Unavailable)?;
                let wanted = Look::from_wire(&look, slot);
                let taken: Vec<_> = self
                    .lobby
                    .slots
                    .iter()
                    .enumerate()
                    .filter(|(index, item)| *index != slot && item.controller != Controller::Empty)
                    .map(|(index, _)| self.looks[index].colour)
                    .collect();
                let (colour, _) = resolve_colour(wanted.colour, &taken);
                self.looks[slot] = Look { colour, ..wanted };
                let mut replies = self.fanout(self.updated());
                if let Some(notice) = self.look_notice(connection_id, Some(&look)) {
                    replies.push((connection_id, notice));
                }
                return Ok(replies);
            }
            Request::SetRoom {
                expected_revision,
                settings,
                capacity,
                password,
                ..
            } => {
                self.owner(connection_id)?;
                if self.phase != RoomPhase::Lobby {
                    return Err(ErrorCode::InMatch);
                }
                if expected_revision != self.revision.get() {
                    return Err(ErrorCode::StaleRevision);
                }
                if password
                    .as_ref()
                    .is_some_and(|password| !password.expose().is_empty())
                {
                    return Err(ErrorCode::Unsupported);
                }
                if usize::from(capacity) < self.lobby.participants() {
                    return Err(ErrorCode::CapacityConflict);
                }
                let mut settings: RoomSettings =
                    serde_json::from_value(settings).map_err(|_| ErrorCode::BadRoom)?;
                if settings.name.chars().any(char::is_control) {
                    return Err(ErrorCode::BadRoom);
                }
                settings.name = settings.name.trim().to_string();
                if settings.name.is_empty() || settings.name.chars().count() > 24 {
                    return Err(ErrorCode::BadRoom);
                }
                Self::new(
                    self.reference.clone(),
                    settings.clone(),
                    capacity,
                    self.config.clone(),
                    self.dev,
                    self.fixed_seed,
                )?;
                self.revision
                    .advance()
                    .map_err(|_| ErrorCode::Unavailable)?;
                self.lobby.settings = settings;
                self.capacity = capacity;
                if password.is_some() {
                    self.password = None;
                }
            }
            Request::Input {
                tick: input_tick,
                mv,
                bomb,
                punch,
                ..
            } => {
                let game = self.running.as_mut().ok_or(ErrorCode::InMatch)?;
                if input_tick > game.state.tick {
                    return Err(ErrorCode::BadRoom);
                }
                if game.state.tick - input_tick > 20 {
                    return Ok(Vec::new());
                }
                let previous = game.pending[slot].unwrap_or_default();
                game.pending[slot] = Some(PlayerInput {
                    mv,
                    bomb: previous.bomb || bomb,
                    punch: previous.punch || punch,
                });
                return Ok(Vec::new());
            }
            request @ (Request::DevPauseBots { .. }
            | Request::DevSetDrops { .. }
            | Request::DevResetDrops { .. }) => {
                self.owner(connection_id)?;
                if !self.dev {
                    return Err(ErrorCode::Unsupported);
                }
                match request {
                    Request::DevPauseBots { paused, .. } => self.bots_paused = paused,
                    Request::DevSetDrops { chances, .. } => {
                        let chances: [f32; 8] =
                            chances.try_into().map_err(|_| ErrorCode::BadRoom)?;
                        self.drops = DropTable::new(chances).map_err(|_| ErrorCode::BadRoom)?;
                    }
                    _ => self.drops = DropTable::defaults(self.config.powerup_chance),
                }
                if let Some(game) = self.running.as_mut() {
                    game.state
                        .set_drops(self.drops.clone())
                        .map_err(|_| ErrorCode::BadRoom)?;
                }
                return Ok(self.fanout(Reply::DevState {
                    reference: self.reference.clone(),
                    paused: self.bots_paused,
                    drops: self.drops.chances,
                }));
            }
            _ => return Err(ErrorCode::Unsupported),
        }
        Ok(self.fanout(self.updated()))
    }

    pub fn next_tick(&self) -> Option<Duration> {
        self.running.as_ref().map(|_| self.next_tick)
    }

    pub fn dev_state(&self) -> Reply {
        Reply::DevState {
            reference: self.reference.clone(),
            paused: self.bots_paused,
            drops: self.drops.chances,
        }
    }

    pub fn advance(&mut self, now: Duration, tick: Duration) -> Vec<(u64, Reply)> {
        if self.running.is_none() || now < self.next_tick {
            return Vec::new();
        }
        self.next_tick = self.next_tick.checked_add(tick).unwrap_or(Duration::MAX);
        if now
            > self
                .next_tick
                .checked_add(tick.saturating_mul(10))
                .unwrap_or(Duration::MAX)
        {
            self.next_tick = now.checked_add(tick).unwrap_or(Duration::MAX);
        }
        let Some(game) = self.running.as_mut() else {
            return Vec::new();
        };
        let inputs = std::array::from_fn(|slot| match game.bots[slot].as_mut() {
            Some(_) if self.bots_paused => PlayerInput::default(),
            Some(bot) => bot.decide(&game.state.observe(PlayerId::all()[slot])),
            None => game.pending[slot].take().unwrap_or_default(),
        });
        let events = step(&mut game.state, &inputs);
        let phase = game.state.phase;
        let mut replies = self.snapshots(&events);
        if let Phase::Ended(outcome) = phase {
            replies.extend(self.fanout(Reply::Ended {
                reference: self.reference.clone(),
                outcome,
            }));
            self.running = None;
            self.phase = RoomPhase::Lobby;
            for slot in &mut self.lobby.slots {
                slot.ready = slot.controller == Controller::Bot;
            }
            let _ = self.revision.advance();
            replies.extend(self.fanout(self.updated()));
        }
        replies
    }

    pub(crate) fn admission_slot(&self, name: &str) -> Result<usize, ErrorCode> {
        if !crate::protocol::valid_name(name) {
            return Err(ErrorCode::BadName);
        }
        if self.phase != RoomPhase::Lobby {
            return Err(ErrorCode::InMatch);
        }
        self.lobby
            .slots
            .iter()
            .take(usize::from(self.capacity))
            .position(|slot| slot.controller == Controller::Empty)
            .ok_or(ErrorCode::Full)
    }

    pub fn admit(
        &mut self,
        connection_id: u64,
        name: &str,
        local: bool,
        look: Option<&Value>,
        random: &dyn RandomSource,
    ) -> Result<Admission, ErrorCode> {
        let slot = self.admission_slot(name)?;
        let mut revision = self.revision;
        revision.advance().map_err(|_| ErrorCode::Unavailable)?;
        let membership_id = self
            .members
            .admit(slot as u8, connection_id, random)
            .map_err(|_| ErrorCode::Unavailable)?
            .membership_id
            .clone();
        let controller = if local {
            Controller::Local
        } else {
            Controller::Remote
        };
        if self.lobby.assign(slot, controller, name).is_err() {
            self.members.remove(&membership_id);
            return Err(ErrorCode::BadRoom);
        }
        let wanted = look
            .map(|value| Look::from_wire(value, slot))
            .unwrap_or_else(|| Look::default_for(slot));
        let taken: Vec<_> = self
            .lobby
            .slots
            .iter()
            .enumerate()
            .filter(|(index, item)| *index != slot && item.controller != Controller::Empty)
            .map(|(index, _)| self.looks[index].colour)
            .collect();
        let (colour, _) = resolve_colour(wanted.colour, &taken);
        self.looks[slot] = Look { colour, ..wanted };
        self.revision = revision;
        self.admission(&membership_id)
    }

    pub fn look_notice(&self, connection_id: u64, look: Option<&Value>) -> Option<Reply> {
        let look = look?;
        let slot = self.slot(connection_id).ok()?;
        let wanted = Look::from_wire(look, slot);
        let assigned = self.looks[slot].colour;
        (wanted.colour != assigned).then(|| Reply::Error {
            request_id: None,
            code: ErrorCode::LookAdjusted,
            message: format!("That colour is taken; your colour is {}", assigned.name()),
        })
    }

    pub fn admission(&self, membership_id: &str) -> Result<Admission, ErrorCode> {
        let member = self
            .members
            .get(membership_id)
            .ok_or(ErrorCode::NotInRoom)?;
        let owner_slot = self
            .members
            .owner_membership_id
            .as_deref()
            .and_then(|owner| self.members.get(owner))
            .map(|owner| owner.slot);
        Ok(Admission {
            reference: self.reference.clone(),
            membership_id: membership_id.into(),
            slot: member.slot,
            resume_proof: member.proof.to_secret(),
            reconnect_window_seconds: 60,
            owner_membership_id: self.members.owner_membership_id.clone(),
            owner_slot,
            revision: self.revision.get(),
            capacity: self.capacity,
            password_required: self.password.is_some(),
            config: self.lobby.settings.to_config(&self.config),
            lobby: self.lobby.clone(),
            looks: self.looks.to_vec(),
            dev: self.dev,
        })
    }

    pub fn summary(&self) -> RoomSummary {
        let connected_humans = self
            .members
            .values()
            .filter(|member| member.status == MembershipStatus::Connected)
            .count() as u8;
        let reserved_humans = self
            .members
            .values()
            .filter(|member| member.status == MembershipStatus::Reserved)
            .count() as u8;
        let occupied = self.lobby.participants() as u8;
        RoomSummary {
            reference: self.reference.clone(),
            settings: self.lobby.settings.clone(),
            capacity: self.capacity,
            occupied,
            connected_humans,
            reserved_humans,
            bots: occupied.saturating_sub(connected_humans + reserved_humans),
            phase: self.phase,
            password_required: self.password.is_some(),
            revision: self.revision.get(),
            game_addr: None,
            web_origin: None,
        }
    }

    pub fn updated(&self) -> Reply {
        let owner_slot = self
            .members
            .owner_membership_id
            .as_deref()
            .and_then(|owner| self.members.get(owner))
            .map(|owner| owner.slot);
        let memberships = self.members.values().map(|member| serde_json::json!({
            "membership_id": member.membership_id, "slot": member.slot,
            "connected": member.connection_id.is_some(), "reserved": member.status == MembershipStatus::Reserved
        })).collect();
        Reply::RoomUpdated {
            reference: self.reference.clone(),
            revision: self.revision.get(),
            owner_membership_id: self.members.owner_membership_id.clone(),
            owner_slot,
            capacity: self.capacity,
            password_required: self.password.is_some(),
            memberships,
            lobby: self.lobby.clone(),
            looks: self.looks.to_vec(),
        }
    }

    pub fn remove_connection(&mut self, connection_id: u64) {
        let membership = self
            .members
            .values()
            .find(|member| member.connection_id == Some(connection_id))
            .map(|member| member.membership_id.clone());
        if let Some(membership) = membership {
            if let Some(member) = self.members.remove(&membership) {
                let slot = usize::from(member.slot);
                if let Some(game) = self.running.as_mut() {
                    self.lobby.slots[slot].controller = Controller::Bot;
                    game.bots[slot] = Some(DefaultBot::new(
                        self.matches.get().wrapping_mul(8).wrapping_add(slot as u64),
                    ));
                } else {
                    let _ = self.lobby.clear(slot);
                }
                let _ = self.revision.advance();
            }
        }
    }
}

impl Default for RoomLimits {
    fn default() -> Self {
        Self {
            max_rooms: 32,
            max_connections: 256,
        }
    }
}

impl RoomLimits {
    pub fn validate(&self) -> Result<(), RoomError> {
        if !(1..=128).contains(&self.max_rooms) || !(4..=1024).contains(&self.max_connections) {
            return Err(RoomError::InvalidLimits);
        }
        Ok(())
    }
}

/// The settings in `value`; never fails. The mode is not checked against the playable modes
/// (the host does that with [`RoomSettings::sanitize`]).
pub fn from_wire(value: &Value) -> RoomSettings {
    let Some(obj) = value.as_object() else {
        return RoomSettings::default();
    };
    let defaults = RoomSettings::default();
    let field = |name: &str| obj.get(name).cloned();
    RoomSettings {
        name: obj
            .get("name")
            .and_then(Value::as_str)
            .map_or(defaults.name, str::to_string),
        mode: field("mode")
            .and_then(|v| serde_json::from_value::<Mode>(v).ok())
            .unwrap_or(defaults.mode),
        size: field("size")
            .and_then(|v| serde_json::from_value::<BoardSize>(v).ok())
            .unwrap_or(defaults.size),
        minutes: obj
            .get("minutes")
            .and_then(Value::as_u64)
            .and_then(|m| u8::try_from(m).ok())
            .unwrap_or(defaults.minutes),
        friendly_fire: obj
            .get("friendly_fire")
            .and_then(Value::as_bool)
            .unwrap_or(defaults.friendly_fire),
    }
    .clean()
}

#[cfg(test)]
mod password_tests {
    use super::*;

    #[test]
    fn password_workers_bound_running_queued_and_unread_results() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc, Arc, Mutex,
        };
        let (entered_tx, entered_rx) = mpsc::sync_channel(2);
        let (release_tx, release_rx) = mpsc::sync_channel(2);
        let release_rx = Mutex::new(release_rx);
        let entered = Arc::new(AtomicUsize::new(0));
        let observed = entered.clone();
        let pool = PasswordWorkers::start_with(move |_| {
            let index = observed.fetch_add(1, Ordering::SeqCst);
            if index < 2 {
                entered_tx.send(()).unwrap();
                release_rx
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap();
            }
            Ok(PasswordWorkResult::Verified(true))
        })
        .unwrap();
        let job = |index: u64| PasswordJob {
            connection_id: index + 10,
            request_id: RequestId::new(format!("password-{index}")).unwrap(),
            work: PasswordWork::Create(Secret::new("worker-password-canary")),
        };
        pool.try_submit(job(0)).unwrap();
        pool.try_submit(job(1)).unwrap();
        for _ in 0..2 {
            entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        for index in 2..34 {
            pool.try_submit(job(index)).unwrap();
        }
        assert_eq!(entered.load(Ordering::SeqCst), 2);
        assert_eq!(pool.try_submit(job(34)), Err(ErrorCode::Busy));
        release_tx.send(()).unwrap();
        release_tx.send(()).unwrap();
        assert_eq!(
            pool.try_submit(job(34)),
            Err(ErrorCode::Busy),
            "unread outcomes retain their bounded slots"
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut completed = std::collections::HashSet::new();
        while completed.len() < 34 {
            match pool.try_recv() {
                Ok(completion) => {
                    assert_eq!(
                        completion.request_id.as_str(),
                        format!("password-{}", completion.connection_id - 10)
                    );
                    assert!(matches!(
                        completion.result,
                        Ok(PasswordWorkResult::Verified(true))
                    ));
                    assert!(completed.insert(completion.connection_id));
                }
                Err(mpsc::TryRecvError::Empty) => std::thread::yield_now(),
                Err(error) => panic!("completion channel closed: {error}"),
            }
            assert!(Instant::now() < deadline, "worker results did not complete");
        }
        pool.try_submit(job(34)).unwrap();
    }

    #[test]
    fn password_workers_hash_off_thread_and_redact_jobs_and_results() {
        use std::sync::{mpsc::TryRecvError, Arc};
        let pool = PasswordWorkers::start(Arc::new(OsRandom)).unwrap();
        let password = Secret::new("worker-private-canary");
        let job = PasswordJob {
            connection_id: 7,
            request_id: RequestId::new("private-create").unwrap(),
            work: PasswordWork::Create(password.clone()),
        };
        assert!(!format!("{job:?}").contains(password.expose()));
        pool.try_submit(job).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let result = loop {
            match pool.try_recv() {
                Ok(completion) => break completion,
                Err(TryRecvError::Empty) => std::thread::yield_now(),
                Err(error) => panic!("completion channel closed: {error}"),
            }
            assert!(
                Instant::now() < deadline,
                "password hashing did not complete"
            );
        };
        assert_eq!(result.connection_id, 7);
        assert_eq!(result.request_id.as_str(), "private-create");
        assert!(!format!("{result:?}").contains(password.expose()));
        match result.result.unwrap() {
            PasswordWorkResult::Created(verifier) => assert!(verifier.verify(&password).unwrap()),
            _ => panic!("expected a private verifier"),
        }
    }

    #[test]
    fn password_workers_keep_serving_after_validation_and_verification_failures() {
        use std::sync::{mpsc::TryRecvError, Arc};
        let pool = PasswordWorkers::start(Arc::new(OsRandom)).unwrap();
        let verifier =
            PasswordVerifier::create(&Secret::new("correct-password"), &OsRandom).unwrap();
        let work = [
            PasswordWork::Create(Secret::new("")),
            PasswordWork::Verify {
                verifier: verifier.clone(),
                password: Secret::new("incorrect-password"),
            },
            PasswordWork::Verify {
                verifier,
                password: Secret::new("correct-password"),
            },
        ];
        for (index, work) in work.into_iter().enumerate() {
            pool.try_submit(PasswordJob {
                connection_id: 42,
                request_id: RequestId::new(format!("verify-{index}")).unwrap(),
                work,
            })
            .unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut results = std::collections::HashMap::new();
        while results.len() < 3 {
            match pool.try_recv() {
                Ok(completion) => {
                    assert_eq!(completion.connection_id, 42);
                    results.insert(
                        completion.request_id.as_str().to_string(),
                        completion.result,
                    );
                }
                Err(TryRecvError::Empty) => std::thread::yield_now(),
                Err(error) => panic!("completion channel closed: {error}"),
            }
            assert!(Instant::now() < deadline, "worker results did not complete");
        }
        assert!(matches!(
            results.remove("verify-0").unwrap(),
            Err(ErrorCode::BadPassword)
        ));
        assert!(matches!(
            results.remove("verify-1").unwrap(),
            Ok(PasswordWorkResult::Verified(false))
        ));
        assert!(matches!(
            results.remove("verify-2").unwrap(),
            Ok(PasswordWorkResult::Verified(true))
        ));
    }

    #[test]
    fn password_workers_discard_queued_work_on_shutdown() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc, Arc, Mutex,
        };
        let (entered_tx, entered_rx) = mpsc::sync_channel(2);
        let (release_tx, release_rx) = mpsc::sync_channel(2);
        let release_rx = Mutex::new(release_rx);
        let processed = Arc::new(AtomicUsize::new(0));
        let observed = processed.clone();
        let submitter = std::thread::current().id();
        let pool = PasswordWorkers::start_with(move |_| {
            assert_ne!(std::thread::current().id(), submitter);
            observed.fetch_add(1, Ordering::SeqCst);
            entered_tx.send(()).unwrap();
            release_rx
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(2))
                .unwrap();
            Ok(PasswordWorkResult::Verified(true))
        })
        .unwrap();
        for index in 0..2 {
            pool.try_submit(PasswordJob {
                connection_id: 1,
                request_id: RequestId::new(format!("active-{index}")).unwrap(),
                work: PasswordWork::Create(Secret::new("shutdown-canary")),
            })
            .unwrap();
        }
        for _ in 0..2 {
            entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        for index in 0..32 {
            pool.try_submit(PasswordJob {
                connection_id: 1,
                request_id: RequestId::new(format!("queued-{index}")).unwrap(),
                work: PasswordWork::Create(Secret::new("shutdown-canary")),
            })
            .unwrap();
        }
        let stopped = pool.stopped.clone();
        let shutdown = std::thread::spawn(move || drop(pool));
        let deadline = Instant::now() + Duration::from_secs(2);
        while !stopped.load(Ordering::SeqCst) {
            assert!(Instant::now() < deadline, "shutdown did not start");
            std::thread::yield_now();
        }
        release_tx.send(()).unwrap();
        release_tx.send(()).unwrap();
        shutdown.join().unwrap();
        assert_eq!(processed.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn salts_change_digests_and_hashes_use_the_approved_argon2id_policy() {
        struct Salt(u8);
        impl RandomSource for Salt {
            fn fill(&self, output: &mut [u8]) -> Result<(), RoomError> {
                output.fill(self.0);
                Ok(())
            }
        }
        let password = Secret::new("password-policy-canary");
        let first = PasswordVerifier::create(&password, &Salt(1)).unwrap();
        let second = PasswordVerifier::create(&password, &Salt(2)).unwrap();
        assert_ne!(first.salt, second.salt);
        assert_ne!(first.digest, second.digest);
        let mut expected = [0; 32];
        let params = Params::new(19_456, 2, 1, Some(32)).unwrap();
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(password.expose().as_bytes(), &first.salt, &mut expected)
            .unwrap();
        assert_eq!(first.digest, expected);
    }
}
