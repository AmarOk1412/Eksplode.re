//! The authoritative host: lobby, 20 Hz simulation, validation and disconnect handling.

use std::collections::HashMap;
use std::io::{BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bomberust_bots::{Bot, DefaultBot};
use bomberust_core::{
    step, Config, Controller, GameState, Lobby, Phase, PlayerId, PlayerInput, MAX_PLAYERS,
};

use crate::client::Client;
use crate::codec::{decode, encode, LineReader, NetError};
use crate::protocol::{valid_name, ClientMsg, RejectReason, ServerMsg, PROTOCOL_VERSION};

/// Connections that cause this many errors are closed.
const MAX_ERRORS: u32 = 10;
/// Inputs older than this many ticks are dropped.
const MAX_INPUT_AGE: u32 = 20;

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
        }
    }
}

enum Ev {
    New {
        id: u64,
        tx: Sender<ServerMsg>,
        local: bool,
    },
    Msg {
        id: u64,
        msg: ClientMsg,
    },
    Bad {
        id: u64,
        err: String,
    },
    Gone {
        id: u64,
    },
    AddBot,
    ClearSlot(usize),
    StartMatch,
    Shutdown,
}

/// A running host. Dropping it ends the match and disconnects everybody.
pub struct Host {
    addr: SocketAddr,
    tx: Sender<Ev>,
    next_id: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    server: Option<JoinHandle<()>>,
}

impl Host {
    /// Binds the listener and starts the server threads.
    pub fn start(cfg: HostConfig) -> Result<Host, NetError> {
        cfg.game
            .validate()
            .map_err(|e| NetError::Protocol(e.to_string()))?;
        let listener = TcpListener::bind(&cfg.bind)?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let (tx, rx) = mpsc::channel();
        let next_id = Arc::new(AtomicU64::new(1));
        let stop = Arc::new(AtomicBool::new(false));

        {
            let (tx, next_id, stop) = (tx.clone(), next_id.clone(), stop.clone());
            thread::spawn(move || accept_loop(listener, tx, next_id, stop));
        }
        let server = thread::spawn(move || Server::new(cfg, rx).run());
        Ok(Host {
            addr,
            tx,
            next_id,
            stop,
            server: Some(server),
        })
    }

    /// Address clients can connect to.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// An in-process connection that has not joined yet: the caller sends `Join` itself.
    ///
    /// `local` marks the connection as the person running the host (slot controller `Local`).
    pub fn connect(&self, local: bool) -> Result<Client, NetError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (stx, srx) = mpsc::channel();
        self.tx
            .send(Ev::New { id, tx: stx, local })
            .map_err(|_| NetError::Closed)?;
        let to_host = self.tx.clone();
        let on_drop = self.tx.clone();
        Ok(Client::from_parts(
            Box::new(move |msg| {
                to_host
                    .send(Ev::Msg { id, msg })
                    .map_err(|_| NetError::Closed)
            }),
            srx,
            Box::new(move || {
                let _ = on_drop.send(Ev::Gone { id });
            }),
        ))
    }

    /// An in-process client for the person running the host; it takes a `Local` slot.
    pub fn local_client(&self, name: &str) -> Result<Client, NetError> {
        let client = self.connect(true)?;
        client.send(ClientMsg::Join {
            version: PROTOCOL_VERSION,
            name: name.to_string(),
        })?;
        Ok(client)
    }

    /// Puts a bot into the first free slot of the lobby.
    pub fn add_bot(&self) {
        let _ = self.tx.send(Ev::AddBot);
    }

    /// Frees a bot or empty slot (human slots are not touched).
    pub fn clear_slot(&self, slot: usize) {
        let _ = self.tx.send(Ev::ClearSlot(slot));
    }

    /// Starts the match with whoever is in the lobby.
    pub fn start_match(&self) {
        let _ = self.tx.send(Ev::StartMatch);
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
    tx: Sender<Ev>,
    next_id: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let id = next_id.fetch_add(1, Ordering::SeqCst);
                if spawn_connection(stream, id, &tx).is_err() {
                    continue;
                }
            }
            Err(_) => thread::sleep(Duration::from_millis(10)),
        }
    }
}

/// One reader thread feeding the server loop and one writer thread draining its replies.
fn spawn_connection(stream: TcpStream, id: u64, events: &Sender<Ev>) -> Result<(), NetError> {
    stream.set_nonblocking(false)?;
    stream.set_nodelay(true)?;
    let (stx, srx) = mpsc::channel::<ServerMsg>();
    let mut writer = stream.try_clone()?;
    thread::spawn(move || {
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
            local: false,
        })
        .map_err(|_| NetError::Closed)?;

    let events = events.clone();
    thread::spawn(move || {
        let mut lines = LineReader::new(BufReader::new(stream));
        loop {
            match lines.read_line() {
                Ok(Some(line)) => {
                    let ev = match decode::<ClientMsg>(&line) {
                        Ok(msg) => Ev::Msg { id, msg },
                        Err(e) => Ev::Bad {
                            id,
                            err: e.to_string(),
                        },
                    };
                    if events.send(ev).is_err() {
                        return;
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

#[derive(Clone, Copy)]
enum OwnerCommand {
    Start,
    AddBot,
    RemoveBot,
}

struct Conn {
    tx: Sender<ServerMsg>,
    local: bool,
    slot: Option<usize>,
    errors: u32,
    last_seen: Instant,
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
}

impl Server {
    fn new(cfg: HostConfig, rx: Receiver<Ev>) -> Self {
        Self {
            cfg,
            rx,
            lobby: Lobby::new(),
            conns: HashMap::new(),
            game: None,
            next_tick: Instant::now(),
            matches: 0,
        }
    }

    fn run(mut self) {
        loop {
            let timeout = if self.game.is_some() {
                self.next_tick.saturating_duration_since(Instant::now())
            } else {
                Duration::from_millis(50)
            };
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
                self.drop_silent();
            }
        }
    }

    fn handle(&mut self, ev: Ev) {
        match ev {
            Ev::New { id, tx, local } => {
                self.conns.insert(
                    id,
                    Conn {
                        tx,
                        local,
                        slot: None,
                        errors: 0,
                        last_seen: Instant::now(),
                    },
                );
            }
            Ev::Msg { id, msg } => self.on_message(id, msg),
            Ev::Bad { id, err } => self.error(id, "Malformed", &err),
            Ev::Gone { id } => self.remove(id),
            Ev::AddBot => {
                if self.game.is_none() && self.lobby.add_bot().is_ok() {
                    self.broadcast_lobby();
                }
            }
            Ev::ClearSlot(slot) => {
                let removable =
                    self.lobby.slots.get(slot).is_some_and(|s| {
                        matches!(s.controller, Controller::Bot | Controller::Empty)
                    });
                if self.game.is_none() && removable && self.lobby.clear(slot).is_ok() {
                    self.broadcast_lobby();
                }
            }
            Ev::StartMatch => self.start(),
            Ev::Shutdown => {}
        }
    }

    fn on_message(&mut self, id: u64, msg: ClientMsg) {
        let Some(conn) = self.conns.get_mut(&id) else {
            return;
        };
        conn.last_seen = Instant::now();
        let joined = conn.slot.is_some();
        match msg {
            ClientMsg::Join { version, name } => self.join(id, version, &name),
            ClientMsg::Ping { nonce } => self.send(id, ServerMsg::Pong { nonce }),
            _ if !joined => {
                self.send(id, ServerMsg::error("NotJoined", "send Join first"));
                self.remove(id);
            }
            ClientMsg::Leave => self.remove(id),
            ClientMsg::SetReady { ready } => self.set_ready(id, ready),
            ClientMsg::Input { tick, mv, bomb } => self.input(id, tick, PlayerInput { mv, bomb }),
            ClientMsg::Start => self.owner_command(id, OwnerCommand::Start),
            ClientMsg::AddBot => self.owner_command(id, OwnerCommand::AddBot),
            ClientMsg::RemoveBot => self.owner_command(id, OwnerCommand::RemoveBot),
        }
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
            OwnerCommand::Start => self.start(),
            _ if self.game.is_some() => {
                self.error(id, "WrongPhase", "bots can only be changed in the lobby")
            }
            OwnerCommand::AddBot => match self.lobby.add_bot() {
                Ok(_) => self.broadcast_lobby(),
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

    fn join(&mut self, id: u64, version: u32, name: &str) {
        let Some(conn) = self.conns.get(&id) else {
            return;
        };
        if conn.slot.is_some() {
            return self.error(id, "AlreadyJoined", "already in the lobby");
        }
        let local = conn.local;
        let refusal = if version != PROTOCOL_VERSION {
            Some(RejectReason::BadVersion)
        } else if !valid_name(name) {
            Some(RejectReason::BadName)
        } else if self.game.is_some() {
            Some(RejectReason::InMatch)
        } else if self.lobby.first_free().is_none() {
            Some(RejectReason::Full)
        } else {
            None
        };
        if let Some(reason) = refusal {
            self.send(id, ServerMsg::Rejected { reason });
            self.conns.remove(&id);
            return;
        }
        let Some(slot) = self.lobby.first_free() else {
            return;
        };
        let controller = if local {
            Controller::Local
        } else {
            Controller::Remote
        };
        if self.lobby.assign(slot, controller, name).is_err() {
            return;
        }
        if let Some(c) = self.conns.get_mut(&id) {
            c.slot = Some(slot);
        }
        self.send(
            id,
            ServerMsg::Welcome {
                slot: slot as u8,
                config: self.cfg.game.clone(),
                lobby: self.lobby.clone(),
            },
        );
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
        let Ok(state) = GameState::new(self.cfg.game.clone(), seed, self.lobby.active()) else {
            return;
        };
        let bots = bots_for(&self.lobby, seed);
        self.broadcast(ServerMsg::Start {
            seed,
            config: self.cfg.game.clone(),
        });
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
            self.broadcast_lobby();
        }
    }

    fn send_snapshots(&mut self, events: &[bomberust_core::Event]) {
        let Some(game) = self.game.as_ref() else {
            return;
        };
        let ids = PlayerId::all();
        for conn in self.conns.values() {
            if let Some(slot) = conn.slot {
                let _ = conn.tx.send(ServerMsg::Snapshot {
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
            self.broadcast_lobby();
        }
    }

    fn drop_silent(&mut self) {
        let limit = self.cfg.silence_timeout;
        let silent: Vec<u64> = self
            .conns
            .iter()
            .filter(|(_, c)| c.slot.is_some() && c.last_seen.elapsed() > limit)
            .map(|(id, _)| *id)
            .collect();
        for id in silent {
            self.send(id, ServerMsg::error("Timeout", "no input received"));
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
            let _ = c.tx.send(msg);
        }
    }

    fn broadcast(&self, msg: ServerMsg) {
        for c in self.conns.values().filter(|c| c.slot.is_some()) {
            let _ = c.tx.send(msg.clone());
        }
    }

    fn broadcast_lobby(&self) {
        self.broadcast(ServerMsg::Lobby {
            lobby: self.lobby.clone(),
        });
    }

    fn shutdown(&mut self) {
        for c in self.conns.values() {
            let _ =
                c.tx.send(ServerMsg::error("HostLeft", "the host closed the match"));
        }
        self.conns.clear();
    }
}

fn bots_for(lobby: &Lobby, seed: u64) -> [Option<DefaultBot>; MAX_PLAYERS] {
    [0, 1, 2, 3].map(|i| {
        (lobby.slots[i].controller == Controller::Bot)
            .then(|| DefaultBot::new(seed.wrapping_mul(4).wrapping_add(i as u64)))
    })
}
