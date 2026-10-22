//! Local game session and the terminal event loops.

use std::fmt;
use std::io;
use std::time::{Duration, Instant};

use bomberust_bots::{Bot, DefaultBot};
use bomberust_core::{
    Config, Controller, CoreError, Event, GameState, Lobby, LobbyError, Phase, PlayerId,
    MAX_PLAYERS,
};
use bomberust_net::{Client, Host, HostConfig, NetError};
use ratatui::crossterm::event::{self, Event as TermEvent, KeyCode};
use ratatui::DefaultTerminal;

use crate::args::{HostOptions, JoinOptions, LocalOptions};
use crate::input::{map_key, normalize, InputState, KeyAction};
use crate::lobby::{self, LobbyScreen};
use crate::online::{self, Online};
use crate::view::{self, ViewContext};

/// Errors that end the program with exit code 1.
#[derive(Debug)]
pub enum AppError {
    Core(CoreError),
    Lobby(LobbyError),
    Net(NetError),
    Io(io::Error),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Core(e) => write!(f, "{e}"),
            AppError::Lobby(e) => write!(f, "{e}"),
            AppError::Net(e) => write!(f, "{e}"),
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
        let lobby = Lobby::from_counts(opts.players, opts.bots)
            .map_err(|e| CoreError::InvalidSlots(e.to_string()))?;
        Self::from_lobby(&lobby, seed)
    }

    /// Starts a round with the slots chosen in `lobby`.
    pub fn from_lobby(lobby: &Lobby, seed: u64) -> Result<Session, CoreError> {
        let config = Config::default();
        let state = GameState::new(config.clone(), seed, lobby.active())?;
        let controllers = [0, 1, 2, 3].map(|i| lobby.slots[i].controller);
        Ok(Session {
            input: InputState::new(config.base_move_cooldown),
            bots: make_bots(&controllers, seed),
            active: lobby.active(),
            config,
            seed,
            controllers,
            state,
            paused: false,
            quit: false,
        })
    }

    /// Reacts to a key press.
    pub fn handle_key(&mut self, code: KeyCode) {
        let Some(action) = map_key(code) else {
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
        bomberust_core::step(&mut self.state, &inputs)
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
    let mut terminal = ratatui::try_init()?;
    let _guard = TerminalGuard;

    if opts.lobby {
        match lobby_loop(&mut terminal, lobby)? {
            Some(chosen) => lobby = chosen,
            None => return Ok(()),
        }
    }
    game_loop(&mut terminal, Session::from_lobby(&lobby, seed)?)
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

/// Hosts a match: listens on the port and plays as the first participant.
pub fn run_host(opts: &HostOptions) -> Result<(), AppError> {
    let host = Host::start(HostConfig {
        bind: format!("0.0.0.0:{}", opts.port),
        seed: opts.seed,
        ..HostConfig::default()
    })?;
    for _ in 0..opts.bots {
        host.add_bot();
    }
    let client = host.local_client(&opts.name)?;
    online_loop(Online::hosting(host, client))
}

/// Joins a match hosted elsewhere.
pub fn run_join(opts: &JoinOptions) -> Result<(), AppError> {
    let client = Client::connect(&opts.addr, &opts.name)?;
    online_loop(Online::new(client))
}

fn online_loop(mut online: Online) -> Result<(), AppError> {
    let mut terminal = ratatui::try_init()?;
    let _guard = TerminalGuard;
    let tick = Duration::from_secs(1) / Config::default().tick_hz;
    let mut next = Instant::now() + tick;
    while !online.quit {
        online.pump();
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
