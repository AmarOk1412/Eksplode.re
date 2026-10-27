//! Lobby screen: choose who plays which slot before the match starts.

use eksplode_re_core::{Controller, Lobby, PlayerId};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::view::player_color;

/// State of the lobby screen.
pub struct LobbyScreen {
    pub lobby: Lobby,
    pub selected: usize,
    pub start: bool,
    pub quit: bool,
}

impl LobbyScreen {
    pub fn new(lobby: Lobby) -> Self {
        Self {
            lobby,
            selected: 0,
            start: false,
            quit: false,
        }
    }

    pub fn handle_key(&mut self, code: KeyCode) {
        let current = self.lobby.slots[self.selected].controller;
        match code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1).min(self.lobby.slots.len() - 1),
            KeyCode::Char('b' | 'B') => match current {
                Controller::Empty => {
                    let name = format!("Bot {}", self.selected + 1);
                    let _ = self.lobby.assign(self.selected, Controller::Bot, &name);
                }
                Controller::Bot => {
                    let _ = self.lobby.clear(self.selected);
                }
                _ => {}
            },
            KeyCode::Char('h' | 'H') => match current {
                Controller::Empty => {
                    let name = format!("Player {}", self.selected + 1);
                    let _ = self.lobby.assign(self.selected, Controller::Local, &name);
                }
                Controller::Local => {
                    let _ = self.lobby.clear(self.selected);
                }
                _ => {}
            },
            KeyCode::Enter if self.lobby.can_start() => self.start = true,
            KeyCode::Char('q' | 'Q') | KeyCode::Esc => self.quit = true,
            _ => {}
        }
    }
}

fn label(c: Controller) -> &'static str {
    match c {
        Controller::Local => "Human",
        Controller::Remote => "Remote",
        Controller::Bot => "Bot",
        Controller::Empty => "Empty",
    }
}

/// Draws the four slots and the key help.
pub fn render(frame: &mut Frame, screen: &LobbyScreen) {
    let area = frame.area();
    let ids = PlayerId::all();
    let mut lines: Vec<Line> = Vec::new();
    for (i, slot) in screen.lobby.slots.iter().enumerate() {
        let marker = if i == screen.selected { "> " } else { "  " };
        let mut style = Style::default().fg(player_color(ids[i]));
        if i == screen.selected {
            style = style.add_modifier(Modifier::BOLD);
        }
        lines.push(Line::from(vec![
            Span::styled(format!("{marker}Slot {}  ", i + 1), style),
            Span::raw(label(slot.controller)),
        ]));
    }
    lines.push(Line::default());
    lines.push(Line::from("Up/Down select   h human   b bot"));
    lines.push(Line::from(match screen.lobby.start_problem() {
        None => "Enter start   q quit".to_string(),
        Some(problem) => format!("{problem}   q quit"),
    }));
    let height = (lines.len() as u16 + 2).min(area.height);
    let rect = Rect::new(area.x, area.y, area.width, height);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" Eksplode.re lobby ")),
        rect,
    );
}
