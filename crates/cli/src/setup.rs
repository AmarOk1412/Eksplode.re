//! Room setup screen: the terminal host sets the room before the lobby opens.

use eksplode_re_core::{Mode, RoomSettings};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

const NAME: usize = 0;
const MODE: usize = 1;
const SIZE: usize = 2;
const TIME: usize = 3;
const FRIENDLY_FIRE: usize = 4;
const CREATE: usize = 5;
/// The longest name that can be typed, in characters.
const MAX_NAME: usize = eksplode_re_core::room::MAX_NAME_CHARS;

/// State of the setup screen.
pub struct SetupScreen {
    pub settings: RoomSettings,
    /// Index into [`SetupScreen::visible`]; the last row is "Create room".
    pub selected: usize,
    /// The owner confirmed: open the lobby.
    pub done: bool,
    /// The owner went back: leave without opening a room.
    pub quit: bool,
}

impl SetupScreen {
    pub fn new(settings: RoomSettings) -> Self {
        Self {
            settings: settings.sanitize(),
            selected: 0,
            done: false,
            quit: false,
        }
    }

    /// The rows on screen; friendly fire only matters in Team mode.
    fn visible(&self) -> Vec<usize> {
        let mut rows = vec![NAME, MODE, SIZE, TIME];
        if self.settings.mode == Mode::Team {
            rows.push(FRIENDLY_FIRE);
        }
        rows.push(CREATE);
        rows
    }

    /// Number of rows on screen.
    pub fn rows(&self) -> usize {
        self.visible().len()
    }

    fn row(&self) -> usize {
        let rows = self.visible();
        rows[self.selected.min(rows.len() - 1)]
    }

    pub fn handle_key(&mut self, code: KeyCode) {
        let step = |code: KeyCode| if code == KeyCode::Left { -1 } else { 1 };
        match code {
            KeyCode::Esc => self.quit = true,
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1).min(self.rows() - 1),
            KeyCode::Left | KeyCode::Right => match self.row() {
                MODE => self.settings.step_mode(step(code)),
                SIZE => self.settings.step_size(step(code)),
                TIME => self.settings.step_minutes(step(code)),
                FRIENDLY_FIRE => self.settings.friendly_fire = !self.settings.friendly_fire,
                _ => {}
            },
            KeyCode::Enter if self.row() == CREATE => self.done = true,
            KeyCode::Enter => self.selected = (self.selected + 1).min(self.rows() - 1),
            KeyCode::Backspace if self.row() == NAME => {
                self.settings.name.pop();
            }
            KeyCode::Char(c)
                if self.row() == NAME
                    && !c.is_control()
                    && self.settings.name.chars().count() < MAX_NAME =>
            {
                self.settings.name.push(c);
            }
            _ => {}
        }
        // moving away from Team mode can hide the selected row
        self.selected = self.selected.min(self.rows() - 1);
    }
}

fn mode_hint(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "everybody against everybody, the board closes in at the end",
        Mode::Team => "2 vs 2, 4 players",
        Mode::Chaser => "one chaser against three runners, 4 players",
    }
}

/// Draws the setup screen.
pub fn render(frame: &mut Frame, screen: &SetupScreen) {
    let area = frame.area();
    let s = &screen.settings;
    let (w, h) = s.size.dimensions();
    let mut lines = vec![Line::default()];
    for (i, row) in screen.visible().into_iter().enumerate() {
        let selected = i == screen.selected;
        let marker = if selected { "> " } else { "  " };
        let (label, value, hint) = match row {
            NAME => (
                "Name",
                format!("{}{}", s.name, if selected { "_" } else { "" }),
                String::new(),
            ),
            MODE => (
                "Mode",
                format!("< {} >", s.mode.label()),
                mode_hint(s.mode).to_string(),
            ),
            SIZE => (
                "Board size",
                format!("< {} >", s.size.label()),
                format!("{w} x {h}"),
            ),
            TIME => (
                "Round time",
                format!("< {} min >", s.minutes),
                String::new(),
            ),
            FRIENDLY_FIRE => (
                "Friendly fire",
                format!("< {} >", if s.friendly_fire { "on" } else { "off" }),
                String::new(),
            ),
            _ => ("[ Create room ]", String::new(), String::new()),
        };
        let style = if selected {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let text = if row == CREATE {
            format!("{marker}{label}")
        } else {
            format!("{marker}{label:<14} {value:<16} {hint}")
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    lines.push(Line::default());
    lines.push(Line::from(
        "Up/Down select   Left/Right change   Enter confirm   Esc back",
    ));
    let height = (lines.len() as u16 + 2).min(area.height);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" ROOM SETUP ")),
        Rect::new(area.x, area.y, area.width, height),
    );
}
