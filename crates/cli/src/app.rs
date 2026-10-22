//! Local game session and the terminal event loop.

use std::fmt;
use std::io;
use std::time::{Duration, Instant};

use bomberust_core::{Config, CoreError, Event, GameState, Phase, MAX_PLAYERS};
use ratatui::crossterm::event::{self, Event as TermEvent, KeyCode, KeyEventKind};

use crate::args::LocalOptions;
use crate::input::{map_key, InputState, KeyAction};
use crate::view::{self, ViewContext};

/// Errors that end the program with exit code 1.
#[derive(Debug)]
pub enum AppError {
    Core(CoreError),
    Io(io::Error),
    Unsupported(&'static str),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Core(e) => write!(f, "{e}"),
            AppError::Io(e) => write!(f, "terminal error: {e}"),
            AppError::Unsupported(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<CoreError> for AppError {
    fn from(e: CoreError) -> Self {
        AppError::Core(e)
    }
}

impl From<io::Error> for AppError {
    fn from(e: io::Error) -> Self {
        AppError::Io(e)
    }
}

/// A local match: the game state plus everything between keyboard and simulation.
pub struct Session {
    config: Config,
    seed: u64,
    active: [bool; MAX_PLAYERS],
    humans: usize,
    pub state: GameState,
    input: InputState,
    pub paused: bool,
    pub quit: bool,
}

impl Session {
    /// Humans take the first slots, bots the following ones.
    pub fn new(opts: &LocalOptions, seed: u64) -> Result<Session, CoreError> {
        let config = Config::default();
        let humans = usize::from(opts.players);
        let total = humans + usize::from(opts.bots);
        let active = [0, 1, 2, 3].map(|i| i < total);
        let state = GameState::new(config.clone(), seed, active)?;
        Ok(Session {
            input: InputState::new(config.base_move_cooldown),
            config,
            seed,
            active,
            humans,
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
            KeyAction::Player { slot, action } if slot < self.humans && !self.paused => {
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
        let inputs = self.input.take();
        bomberust_core::step(&mut self.state, &inputs)
    }

    /// Slot descriptions for the status lines.
    pub fn context(&self) -> ViewContext {
        let tags = [0, 1, 2, 3].map(|i| {
            if !self.active[i] {
                None
            } else if i < self.humans {
                Some("human".to_string())
            } else {
                Some("bot".to_string())
            }
        });
        ViewContext {
            paused: self.paused,
            tags,
        }
    }

    fn rematch(&mut self) {
        self.seed = self.seed.wrapping_add(1);
        if let Ok(state) = GameState::new(self.config.clone(), self.seed, self.active) {
            self.state = state;
            self.input.clear();
        }
    }
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
    if opts.bots > 0 {
        return Err(AppError::Unsupported(
            "bots are not available in this build yet",
        ));
    }
    let mut session = Session::new(opts, seed)?;
    let mut terminal = ratatui::try_init()?;
    let _guard = TerminalGuard;

    let tick = Duration::from_secs(1) / session.config.tick_hz;
    let mut next = Instant::now() + tick;
    while !session.quit {
        let ctx = session.context();
        terminal.draw(|f| view::render(f, &session.state, &ctx))?;

        let timeout = next.saturating_duration_since(Instant::now());
        if event::poll(timeout)? {
            if let TermEvent::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Release {
                    session.handle_key(key.code);
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
