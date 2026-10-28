//! Local game session and the terminal event loops.

use std::fmt;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use eksplode_re_bots::{Bot, DefaultBot};
use eksplode_re_core::{
    Config, Controller, CoreError, Event, GameState, Lobby, LobbyError, Phase, PlayerId,
    MAX_PLAYERS,
};
use eksplode_re_net::protocol::RejectReason;
use eksplode_re_net::{Client, Host, HostConfig, NetError};
use eksplode_re_web::{Server, ServerError, ServerOptions};
use ratatui::crossterm::event::{self, Event as TermEvent, KeyCode};
use ratatui::DefaultTerminal;

use crate::args::{HostOptions, JoinOptions, LocalOptions};
use crate::input::{
    default_key_bindings, map_key_with_bindings, normalize, InputState, KeyAction, KeyBindings,
};
use crate::lobby::{self, LobbyScreen};
use crate::online::{self, Online};
use crate::setup::{self, SetupScreen};
use crate::view::{self, ViewContext};

/// Errors that end the program with exit code 1.
#[derive(Debug)]
pub enum AppError {
    Core(CoreError),
    Lobby(LobbyError),
    Net(NetError),
    Web(ServerError),
    /// A join that failed; the text is the whole explanation.
    Join(String),
    Io(io::Error),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Core(e) => write!(f, "{e}"),
            AppError::Lobby(e) => write!(f, "{e}"),
            AppError::Net(e) => write!(f, "{e}"),
            AppError::Web(e) => write!(f, "{e}"),
            AppError::Join(m) => write!(f, "{m}"),
            AppError::Io(e) => write!(f, "terminal error: {e}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<CoreError> for AppError {
    fn from(e: CoreError) -> Self {
        AppError::Core(e)
    }
}

impl From<LobbyError> for AppError {
    fn from(e: LobbyError) -> Self {
        AppError::Lobby(e)
    }
}

impl From<NetError> for AppError {
    fn from(e: NetError) -> Self {
        AppError::Net(e)
    }
}

impl From<ServerError> for AppError {
    fn from(e: ServerError) -> Self {
        AppError::Web(e)
    }
}

impl From<io::Error> for AppError {
    fn from(e: io::Error) -> Self {
        AppError::Io(e)
    }
}

/// A local match: the game state plus everything between keyboard, bots and simulation.
pub struct Session {
    config: Config,
    seed: u64,
    controllers: [Controller; MAX_PLAYERS],
    key_bindings: [KeyBindings; MAX_PLAYERS],
    active: [bool; MAX_PLAYERS],
    bots: [Option<DefaultBot>; MAX_PLAYERS],
    pub state: GameState,
    input: InputState,
    pub paused: bool,
    pub quit: bool,
}

impl Session {
    /// Humans take the first slots, bots the following ones.
    pub fn new(opts: &LocalOptions, seed: u64) -> Result<Session, CoreError> {
        let mut lobby = Lobby::from_counts(opts.players, opts.bots)
            .map_err(|e| CoreError::InvalidSlots(e.to_string()))?;
        lobby.settings = opts.room.clone().sanitize();
        Self::from_lobby_with_bindings(&lobby, seed, opts.key_bindings)
    }

    /// Starts a round with the slots chosen in `lobby`.
    pub fn from_lobby(lobby: &Lobby, seed: u64) -> Result<Session, CoreError> {
        Self::from_lobby_with_bindings(lobby, seed, default_key_bindings())
    }

    fn from_lobby_with_bindings(
        lobby: &Lobby,
        seed: u64,
        key_bindings: [KeyBindings; MAX_PLAYERS],
    ) -> Result<Session, CoreError> {
        let config = lobby.settings.to_config(&Config::default());
        let state = GameState::new(config.clone(), seed, lobby.active())?;
        let controllers = [0, 1, 2, 3].map(|i| lobby.slots[i].controller);
        Ok(Session {
            input: InputState::new(config.base_move_cooldown),
            bots: make_bots(&controllers, seed),
            active: lobby.active(),
            config,
            seed,
            controllers,
            key_bindings,
            state,
            paused: false,
            quit: false,
        })
    }

    /// Reacts to a key press.
    pub fn handle_key(&mut self, code: KeyCode) {
        let Some(action) = map_key_with_bindings(code, &self.key_bindings) else {
            return;
        };
        match action {
            KeyAction::Quit => self.quit = true,
            KeyAction::Pause if self.state.phase == Phase::Playing => {
                self.paused = !self.paused;
                self.input.clear();
            }
            KeyAction::Rematch if matches!(self.state.phase, Phase::Ended(_)) => self.rematch(),
            KeyAction::Player { slot, action }
                if self.controllers[slot] == Controller::Local && !self.paused =>
            {
                self.input.press(slot, action);
            }
            _ => {}
        }
    }

    /// Advances the game by one tick unless paused.
    pub fn tick(&mut self) -> Vec<Event> {
        if self.paused {
            return Vec::new();
        }
        let mut inputs = self.input.take();
        let ids = PlayerId::all();
        for (i, bot) in self.bots.iter_mut().enumerate() {
            if let Some(bot) = bot {
                inputs[i] = bot.decide(&self.state.observe(ids[i]));
            }
        }
        eksplode_re_core::step(&mut self.state, &inputs)
    }

    /// Slot descriptions for the status lines.
    pub fn context(&self) -> ViewContext {
        let tags = [0, 1, 2, 3].map(|i| match self.controllers[i] {
            Controller::Local => Some("human".to_string()),
            Controller::Remote => Some("remote".to_string()),
            Controller::Bot => Some("bot".to_string()),
            Controller::Empty => None,
        });
        ViewContext {
            paused: self.paused,
            tags,
            notice: None,
        }
    }

    fn rematch(&mut self) {
        self.seed = self.seed.wrapping_add(1);
        if let Ok(state) = GameState::new(self.config.clone(), self.seed, self.active) {
            self.state = state;
            self.bots = make_bots(&self.controllers, self.seed);
            self.input.clear();
        }
    }
}

fn make_bots(
    controllers: &[Controller; MAX_PLAYERS],
    seed: u64,
) -> [Option<DefaultBot>; MAX_PLAYERS] {
    [0, 1, 2, 3].map(|i| {
        (controllers[i] == Controller::Bot)
            .then(|| DefaultBot::new(seed.wrapping_mul(4).wrapping_add(i as u64)))
    })
}

/// Restores the terminal when dropped, also on early returns and panics.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

/// Runs a local match in the terminal until the player quits.
pub fn run(opts: &LocalOptions, seed: u64) -> Result<(), AppError> {
    let mut lobby = Lobby::from_counts(opts.players, opts.bots)?;
    lobby.settings = opts.room.clone().sanitize();
    let mut terminal = ratatui::try_init()?;
    let _guard = TerminalGuard;

    if opts.lobby {
        match lobby_loop(&mut terminal, lobby)? {
            Some(chosen) => lobby = chosen,
            None => return Ok(()),
        }
    }
    game_loop(
        &mut terminal,
        Session::from_lobby_with_bindings(&lobby, seed, opts.key_bindings)?,
    )
}

/// Returns the chosen lobby, or `None` when the user quit.
fn lobby_loop(terminal: &mut DefaultTerminal, lobby: Lobby) -> Result<Option<Lobby>, AppError> {
    let mut screen = LobbyScreen::new(lobby);
    while !screen.start && !screen.quit {
        terminal.draw(|f| lobby::render(f, &screen))?;
        if let TermEvent::Key(key) = event::read()? {
            if let Some(code) = normalize(key) {
                screen.handle_key(code);
            }
        }
    }
    Ok(screen.start.then_some(screen.lobby))
}

fn game_loop(terminal: &mut DefaultTerminal, mut session: Session) -> Result<(), AppError> {
    let tick = Duration::from_secs(1) / session.config.tick_hz;
    let mut next = Instant::now() + tick;
    while !session.quit {
        let ctx = session.context();
        let obs = session.state.observe(PlayerId::all()[0]);
        terminal.draw(|f| view::render(f, &obs, &ctx))?;

        let timeout = next.saturating_duration_since(Instant::now());
        if event::poll(timeout)? {
            if let TermEvent::Key(key) = event::read()? {
                if let Some(code) = normalize(key) {
                    session.handle_key(code);
                }
            }
        }
        if Instant::now() >= next {
            session.tick();
            next += tick;
        }
    }
    Ok(())
}

/// How long a refusal stays on screen before the program exits with an error.
const REJECTION_PAUSE: Duration = Duration::from_secs(2);

/// Lines printed to stderr before the terminal switches screens when the host exposes the game.
pub fn host_warnings(opts: &HostOptions) -> Vec<String> {
    let game_ip = opts.bind.unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    let web_ip = opts.bind.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let mut ports = format!("port {}", opts.port);
    if let (Some(web), false) = (opts.web_port, web_ip.is_loopback()) {
        ports.push_str(&format!(" and web port {web}"));
    }
    let mut out = Vec::new();
    if !game_ip.is_loopback() {
        out.push(format!(
            "WARNING: listening on {game_ip}: the game has no encryption and no authentication, \
             so anyone who can reach this machine on {ports} can play. Use --bind 127.0.0.1 to \
             play only on this machine, or a firewall or tunnel to restrict access."
        ));
    }
    if opts.dev {
        out.push(eksplode_re_net::DEV_WARNING.to_string());
    }
    out
}

/// The address browsers use for a web server listening on `addr`.
pub fn web_url(addr: SocketAddr) -> String {
    let ip = match addr.ip() {
        IpAddr::V4(v4) if v4.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(v6) if v6.is_unspecified() => IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    match ip {
        IpAddr::V4(v4) => format!("http://{v4}:{}/", addr.port()),
        IpAddr::V6(v6) => format!("http://[{v6}]:{}/", addr.port()),
    }
}

/// The one-line explanation of a refused join.
pub fn join_failure(reason: RejectReason) -> AppError {
    AppError::Join(
        match reason {
            RejectReason::Full => "the match is full (4 players); try again when someone leaves",
            RejectReason::BadVersion => {
                "the host runs an incompatible version; update both to the same release"
            }
            RejectReason::BadName => "the host refused the name; use 1 to 16 printable characters",
            RejectReason::InMatch => "a match is running; wait for it to finish, then join",
        }
        .to_string(),
    )
}

fn connect_failure(addr: &str, e: NetError) -> AppError {
    let reason = match &e {
        NetError::Io(io) if io.kind() == io::ErrorKind::ConnectionRefused => {
            "connection refused; check that the host is running and the address and port are right"
                .to_string()
        }
        NetError::Io(io) => io.to_string().replace('\n', " "),
        other => other.to_string().replace('\n', " "),
    };
    AppError::Join(format!("cannot reach {addr}: {reason}"))
}

/// Hosts a match: listens on the port and plays as the first participant, optionally also
/// serving the web page for browsers.
pub fn run_host(opts: &HostOptions) -> Result<(), AppError> {
    for line in host_warnings(opts) {
        eprintln!("{line}");
    }
    let room = if opts.room_given || opts.no_setup {
        opts.room.clone().sanitize()
    } else {
        match setup_room(opts.room.clone())? {
            Some(room) => room,
            None => return Ok(()),
        }
    };
    let game_ip = opts.bind.unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    let host = Host::start(HostConfig {
        bind: SocketAddr::new(game_ip, opts.port).to_string(),
        seed: opts.seed,
        dev: opts.dev,
        defaults: room.clone(),
        default_bots: opts.bots,
        server_id: opts.server_id.clone(),
        limits: opts.limits,
        ..HostConfig::default()
    })?;
    let web = match opts.web_port {
        Some(port) => Some(Server::attach(
            host.handle(),
            host.addr(),
            ServerOptions {
                bind: opts.bind.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST)),
                http_port: port,
                game_port: host.addr().port(),
                seed: opts.seed,
                dev: opts.dev,
                room: room.clone(),
                server_id: opts.server_id.clone(),
                max_rooms: opts.limits.max_rooms,
                max_connections: usize::from(opts.limits.max_connections),
                ..ServerOptions::default()
            },
        )?),
        None => None,
    };
    let client = host.connect(true)?;
    let mut online = Online::hosting_room(host, client, opts.name.clone(), room);
    online.web_url = web.as_ref().map(|s| web_url(s.http_addr()));
    let result = online_loop(online);
    // the match host is gone by now, so every browser has been told before the page stops
    drop(web);
    result
}

/// Shows the room setup screen; `None` when the owner went back.
fn setup_room(
    start: eksplode_re_core::RoomSettings,
) -> Result<Option<eksplode_re_core::RoomSettings>, AppError> {
    let mut terminal = ratatui::try_init()?;
    let _guard = TerminalGuard;
    let mut screen = SetupScreen::new(start);
    while !screen.done && !screen.quit {
        terminal.draw(|f| setup::render(f, &screen))?;
        if let TermEvent::Key(key) = event::read()? {
            if let Some(code) = normalize(key) {
                screen.handle_key(code);
            }
        }
    }
    Ok(screen.done.then_some(screen.settings))
}

/// Joins a match hosted elsewhere.
pub fn run_join(opts: &JoinOptions) -> Result<(), AppError> {
    let client =
        Client::connect_unjoined(&opts.addr).map_err(|e| connect_failure(&opts.addr, e))?;
    online_loop(
        Online::directory(client, opts.name.clone(), opts.key_bindings)
            .with_room_id(opts.room_id.clone()),
    )
}

fn online_loop(mut online: Online) -> Result<(), AppError> {
    let mut terminal = ratatui::try_init()?;
    let _guard = TerminalGuard;
    let tick = Duration::from_secs(1) / Config::default().tick_hz;
    let mut next = Instant::now() + tick;
    while !online.quit {
        online.pump();
        if let Some(reason) = online.rejection_shown(REJECTION_PAUSE) {
            return Err(join_failure(reason));
        }
        terminal.draw(|f| online::render(f, &online))?;
        let timeout = next.saturating_duration_since(Instant::now());
        if event::poll(timeout)? {
            if let TermEvent::Key(key) = event::read()? {
                online.handle_event(key);
            }
        }
        if Instant::now() >= next {
            online.send_input();
            next += tick;
        }
    }
    Ok(())
}
