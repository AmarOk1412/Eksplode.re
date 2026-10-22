//! Network play on the terminal: a host or client session driven by server messages.

use std::net::SocketAddr;
use std::sync::mpsc::TryRecvError;

use bomberust_core::{Controller, Lobby, Observation, Outcome};
use bomberust_net::protocol::{ClientMsg, RejectReason, ServerMsg};
use bomberust_net::{Client, Host};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::input::{map_key, InputState, KeyAction};
use crate::view::{self, player_color, ViewContext};

/// Where an online session is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Connecting,
    Lobby,
    Playing,
    /// Disconnected, refused or the host left; see the notice.
    Closed,
}

/// One participant's view of a hosted or joined match.
pub struct Online {
    client: Client,
    host: Option<Host>,
    pub slot: Option<usize>,
    pub lobby: Lobby,
    pub stage: Stage,
    pub observation: Option<Observation>,
    pub notice: Option<String>,
    pub quit: bool,
    input: InputState,
    tick: u32,
}

impl Online {
    /// A client of someone else's host.
    pub fn new(client: Client) -> Online {
        Online {
            client,
            host: None,
            slot: None,
            lobby: Lobby::new(),
            stage: Stage::Connecting,
            observation: None,
            notice: None,
            quit: false,
            input: InputState::new(4),
            tick: 0,
        }
    }

    /// The person running the host, playing through an in-process client.
    pub fn hosting(host: Host, client: Client) -> Online {
        let mut o = Online::new(client);
        o.host = Some(host);
        o
    }

    pub fn is_host(&self) -> bool {
        self.host.is_some()
    }

    pub fn host_addr(&self) -> Option<SocketAddr> {
        self.host.as_ref().map(Host::addr)
    }

    /// Applies every message that has arrived.
    pub fn pump(&mut self) {
        loop {
            match self.client.try_recv() {
                Ok(msg) => self.on_message(msg),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if self.stage != Stage::Closed {
                        self.close("connection lost");
                    }
                    break;
                }
            }
        }
    }

    fn close(&mut self, notice: &str) {
        self.stage = Stage::Closed;
        self.notice = Some(notice.to_string());
    }

    fn on_message(&mut self, msg: ServerMsg) {
        match msg {
            ServerMsg::Welcome { slot, lobby, .. } => {
                self.slot = Some(usize::from(slot));
                self.lobby = lobby;
                self.stage = Stage::Lobby;
            }
            ServerMsg::Rejected { reason } => self.close(match reason {
                RejectReason::Full => "join refused: the match is full",
                RejectReason::BadVersion => "join refused: incompatible version",
                RejectReason::BadName => "join refused: invalid name",
                RejectReason::InMatch => "join refused: a match is running",
            }),
            ServerMsg::Lobby { lobby } => {
                self.lobby = lobby;
                if self.stage != Stage::Closed && self.stage != Stage::Playing {
                    self.stage = Stage::Lobby;
                }
            }
            ServerMsg::Start { .. } => {
                self.stage = Stage::Playing;
                self.notice = None;
                self.input.clear();
            }
            ServerMsg::Snapshot { tick, state, .. } => {
                self.tick = tick;
                self.observation = Some(*state);
                if self.stage != Stage::Closed {
                    self.stage = Stage::Playing;
                }
            }
            ServerMsg::PlayerLeft { slot, .. } => {
                self.notice = Some(format!("Player {} left, a bot took over", slot + 1));
            }
            ServerMsg::Ended { outcome } => {
                self.notice = Some(match outcome {
                    Outcome::Winner(id) if Some(id.index()) == self.slot => "You won!".to_string(),
                    Outcome::Winner(id) => format!("Player {} won", id.index() + 1),
                    Outcome::Draw => "Draw".to_string(),
                });
                self.stage = Stage::Lobby;
            }
            ServerMsg::Error { code, message } => {
                if code == "HostLeft" {
                    self.close("the host left");
                } else {
                    self.notice = Some(message);
                }
            }
            ServerMsg::Pong { .. } => {}
        }
    }

    /// Reacts to a key press for the current stage.
    pub fn handle_key(&mut self, code: KeyCode) {
        let action = map_key(code);
        if action == Some(KeyAction::Quit) {
            self.quit = true;
            return;
        }
        match self.stage {
            Stage::Playing => {
                if let Some(KeyAction::Player { action, .. }) = action {
                    self.input.press(0, action);
                }
            }
            Stage::Lobby => self.lobby_key(code, action),
            Stage::Connecting | Stage::Closed => {}
        }
    }

    fn lobby_key(&mut self, code: KeyCode, action: Option<KeyAction>) {
        if let Some(host) = &self.host {
            match code {
                KeyCode::Char('b') => host.add_bot(),
                KeyCode::Char('x') => {
                    if let Some(i) = (0..4)
                        .rev()
                        .find(|i| self.lobby.slots[*i].controller == Controller::Bot)
                    {
                        host.clear_slot(i);
                    }
                }
                KeyCode::Enter => host.start_match(),
                _ => {}
            }
        } else if action == Some(KeyAction::Rematch) {
            let ready = self.slot.is_some_and(|s| self.lobby.slots[s].ready);
            let _ = self.client.send(ClientMsg::SetReady { ready: !ready });
        }
    }

    /// Sends this tick's input to the host while a match runs.
    pub fn send_input(&mut self) {
        if self.stage != Stage::Playing {
            return;
        }
        let i = self.input.take()[0];
        let _ = self.client.send(ClientMsg::Input {
            tick: self.tick,
            mv: i.mv,
            bomb: i.bomb,
        });
    }

    /// Slot descriptions for the status lines of the game view.
    pub fn context(&self) -> ViewContext {
        let tags = [0, 1, 2, 3].map(|i| match self.lobby.slots[i].controller {
            _ if Some(i) == self.slot => Some("you".to_string()),
            Controller::Local => Some("host".to_string()),
            Controller::Remote => Some("remote".to_string()),
            Controller::Bot => Some("bot".to_string()),
            Controller::Empty => None,
        });
        ViewContext {
            paused: false,
            tags,
            notice: self.notice.clone(),
        }
    }
}

/// Draws the lobby, the game or the closing message.
pub fn render(frame: &mut Frame, online: &Online) {
    match (&online.observation, online.stage) {
        (Some(obs), Stage::Playing) => view::render(frame, obs, &online.context()),
        _ => render_lobby(frame, online),
    }
}

fn render_lobby(frame: &mut Frame, online: &Online) {
    let area = frame.area();
    let ids = bomberust_core::PlayerId::all();
    let mut lines: Vec<Line> = Vec::new();
    if let Some(addr) = online.host_addr() {
        lines.push(Line::from(format!("Hosting on port {}", addr.port())));
        lines.push(Line::from(format!(
            "Others join with: bomberust-cli join <your-ip>:{}",
            addr.port()
        )));
        lines.push(Line::default());
    }
    for (i, slot) in online.lobby.slots.iter().enumerate() {
        let who = match slot.controller {
            Controller::Empty => "Empty".to_string(),
            Controller::Bot => "Bot".to_string(),
            _ => {
                let ready = if slot.ready { " [ready]" } else { "" };
                format!("{}{ready}", slot.name)
            }
        };
        let me = if Some(i) == online.slot { " (you)" } else { "" };
        lines.push(Line::from(vec![
            Span::styled(
                format!("Slot {}  ", i + 1),
                Style::default()
                    .fg(player_color(ids[i]))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("{who}{me}")),
        ]));
    }
    lines.push(Line::default());
    if let Some(n) = &online.notice {
        lines.push(Line::from(Span::styled(
            n.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )));
    }
    lines.push(Line::from(match (online.stage, online.is_host()) {
        (Stage::Connecting, _) => "Connecting...   q quit",
        (Stage::Closed, _) => "q quit",
        (_, true) => "b add bot   x remove bot   Enter start   q quit",
        (_, false) => "r ready   q quit   (waiting for the host to start)",
    }));
    let height = (lines.len() as u16 + 2).min(area.height);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" Bomberust online ")),
        Rect::new(area.x, area.y, area.width, height),
    );
}
