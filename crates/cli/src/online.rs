//! Network play on the terminal: a host or client session driven by server messages.

use std::net::SocketAddr;
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

use eksplode_re_core::{Controller, Lobby, Observation, Outcome, RoomSettings};
use eksplode_re_net::protocol::v2::{Reply, Request, RequestId, RoomRef, RoomSummary, Scope};
use eksplode_re_net::protocol::{ClientMsg, RejectReason, ServerMsg};
use eksplode_re_net::{Client, Host};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use crate::input::{map_key_for_player, normalize, InputState, KeyAction, KeyBindings};
use crate::setup::SetupScreen;
use crate::view::{self, player_color, team_letter, team_members, ViewContext};

/// Where an online session is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Connecting,
    Directory,
    Creating,
    Password,
    ConfirmLeave,
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
    /// Where browsers can join, when the host also serves the web page.
    pub web_url: Option<String>,
    rejection: Option<(RejectReason, Instant)>,
    key_bindings: KeyBindings,
    input: InputState,
    tick: u32,
    pub rooms: Vec<RoomSummary>,
    pub selected_room: usize,
    pub room_ref: Option<RoomRef>,
    player_name: String,
    request_sequence: u64,
    directory_request: Option<RequestId>,
    owner_slot: Option<usize>,
    revision: u64,
    capacity: u8,
    password_required: bool,
    password_input: String,
    password_prompt: bool,
    password_update: bool,
    setup: Option<SetupScreen>,
    initial_room: Option<RoomSettings>,
    requested_room_id: Option<String>,
    creation_defaults: RoomSettings,
    leave_return: Stage,
}

impl Online {
    /// A client of someone else's host.
    pub fn new(client: Client) -> Online {
        Self::with_key_bindings(client, KeyBindings::default_for_slot(0))
    }

    /// A joining client with custom local gameplay keys.
    pub fn with_key_bindings(client: Client, key_bindings: KeyBindings) -> Online {
        Online {
            client,
            host: None,
            slot: None,
            lobby: Lobby::new(),
            stage: Stage::Connecting,
            observation: None,
            notice: None,
            quit: false,
            web_url: None,
            rejection: None,
            key_bindings,
            input: InputState::new(4),
            tick: 0,
            rooms: Vec::new(),
            selected_room: 0,
            room_ref: None,
            player_name: "Player".into(),
            request_sequence: 0,
            directory_request: None,
            owner_slot: None,
            revision: 0,
            capacity: 4,
            password_required: false,
            password_input: String::new(),
            password_prompt: false,
            password_update: false,
            setup: None,
            initial_room: None,
            requested_room_id: None,
            creation_defaults: RoomSettings::default(),
            leave_return: Stage::Lobby,
        }
    }

    pub fn directory(client: Client, name: String, key_bindings: KeyBindings) -> Online {
        let mut online = Self::with_key_bindings(client, key_bindings);
        online.player_name = name;
        online
    }

    pub fn hosting_room(
        host: Host,
        client: Client,
        name: String,
        settings: RoomSettings,
    ) -> Online {
        let mut online = Self::directory(client, name, KeyBindings::default_for_slot(0));
        online.host = Some(host);
        online.initial_room = Some(settings);
        if online
            .client
            .send_v2(Request::Hello { version: 2 })
            .is_err()
        {
            online.close("connection lost");
        }
        online
    }

    pub fn with_room_id(mut self, room_id: Option<String>) -> Self {
        self.requested_room_id = room_id;
        self
    }

    fn control(&mut self, make: impl FnOnce(RequestId) -> Request) {
        let Some(sequence) = self.request_sequence.checked_add(1) else {
            self.close("request counter exhausted");
            return;
        };
        self.request_sequence = sequence;
        let Ok(request_id) = RequestId::new(format!("terminal-{sequence}")) else {
            self.close("invalid request identifier");
            return;
        };
        if self.client.send_v2(make(request_id)).is_err() {
            self.close("connection lost");
        }
    }

    pub fn refresh_rooms(&mut self) {
        self.rooms.clear();
        self.selected_room = 0;
        let mut request = None;
        self.control(|request_id| {
            request = Some(request_id.clone());
            Request::ListRooms {
                request_id,
                scope: Scope::Local,
                cursor: None,
            }
        });
        self.directory_request = request;
    }

    fn create_room(&mut self, settings: RoomSettings) {
        let name = self.player_name.clone();
        self.stage = Stage::Connecting;
        self.control(|request_id| Request::create_room(request_id, name, &settings, 4));
    }

    fn join_room(&mut self, password: Option<eksplode_re_net::protocol::v2::Secret>) {
        let Some(room) = self.rooms.get(self.selected_room).cloned() else {
            self.stage = Stage::Directory;
            return;
        };
        let name = self.player_name.clone();
        self.stage = Stage::Connecting;
        self.notice = None;
        self.password_prompt = password.is_some();
        self.control(|request_id| Request::JoinRoom {
            request_id,
            server_epoch: room.reference.server_epoch,
            room_id: room.reference.room_id,
            name,
            password,
            look: None,
        });
    }

    fn on_room_message(&mut self, reply: Reply) {
        match reply {
            Reply::Hello {
                version: 2,
                capabilities,
                server_epoch,
                defaults,
                ..
            } if capabilities.iter().any(|capability| capability == "rooms") => {
                self.creation_defaults = defaults;
                if let Some(settings) = self.initial_room.take() {
                    self.create_room(settings);
                } else if let Some(room_id) = self.requested_room_id.take() {
                    let name = self.player_name.clone();
                    self.control(|request_id| Request::JoinRoom {
                        request_id,
                        server_epoch,
                        room_id,
                        name,
                        password: None,
                        look: None,
                    });
                } else {
                    self.stage = Stage::Directory;
                    self.refresh_rooms();
                }
            }
            Reply::Hello { .. } => self.close("room protocol upgrade required"),
            Reply::Rooms {
                request_id,
                rooms,
                next_cursor,
                ..
            } if self.directory_request.as_ref() == Some(&request_id) => {
                self.rooms.extend(rooms);
                if let Some(cursor) = next_cursor {
                    let mut request = None;
                    self.control(|request_id| {
                        request = Some(request_id.clone());
                        Request::ListRooms {
                            request_id,
                            scope: Scope::Local,
                            cursor: Some(cursor),
                        }
                    });
                    self.directory_request = request;
                } else {
                    self.directory_request = None;
                    if self.room_ref.is_none() && self.stage != Stage::Creating {
                        self.stage = Stage::Directory;
                    }
                }
            }
            Reply::Welcome { admission, .. } => {
                self.rejection = None;
                self.password_input.clear();
                self.password_prompt = false;
                self.password_update = false;
                self.room_ref = Some(admission.reference);
                self.owner_slot = admission.owner_slot.map(usize::from);
                self.revision = admission.revision;
                self.capacity = admission.capacity;
                self.password_required = admission.password_required;
                self.on_message(ServerMsg::Welcome {
                    slot: admission.slot,
                    config: admission.config,
                    lobby: admission.lobby,
                    looks: admission.looks,
                    dev: admission.dev,
                });
                self.notice = None;
            }
            Reply::RoomUpdated {
                reference,
                revision,
                owner_slot,
                capacity,
                password_required,
                lobby,
                looks,
                ..
            } if self.room_ref.as_ref() == Some(&reference) => {
                self.owner_slot = owner_slot.map(usize::from);
                self.revision = revision;
                self.capacity = capacity;
                self.password_required = password_required;
                self.on_message(ServerMsg::Lobby { lobby, looks });
            }
            Reply::Start {
                reference,
                seed,
                config,
            } if self.room_ref.as_ref() == Some(&reference) => {
                self.on_message(ServerMsg::Start { seed, config })
            }
            Reply::Snapshot {
                reference,
                tick,
                state,
                events,
            } if self.room_ref.as_ref() == Some(&reference) => {
                self.on_message(ServerMsg::Snapshot {
                    tick,
                    state,
                    events,
                })
            }
            Reply::PlayerLeft {
                reference,
                slot,
                replaced_by_bot,
            } if self.room_ref.as_ref() == Some(&reference) => {
                self.on_message(ServerMsg::PlayerLeft {
                    slot,
                    replaced_by_bot,
                })
            }
            Reply::Ended { reference, outcome } if self.room_ref.as_ref() == Some(&reference) => {
                self.on_message(ServerMsg::Ended { outcome })
            }
            Reply::RoomClosed { reference, reason }
                if self.room_ref.as_ref() == Some(&reference) && reason == "shutdown" =>
            {
                self.room_ref = None;
                self.close("the host left");
            }
            Reply::LeftRoom { reference, .. }
            | Reply::Removed { reference, .. }
            | Reply::RoomClosed { reference, .. }
                if self.room_ref.as_ref() == Some(&reference) =>
            {
                self.room_ref = None;
                self.slot = None;
                self.lobby = Lobby::new();
                self.observation = None;
                self.input.clear();
                self.notice = None;
                self.stage = Stage::Directory;
                self.refresh_rooms();
            }
            Reply::Error { code, message, .. } => {
                self.notice = Some(format!("{code:?}: {message}"));
                if self.room_ref.is_none() && self.stage == Stage::Connecting {
                    use eksplode_re_net::protocol::v2::ErrorCode;
                    if code == ErrorCode::BadPassword && self.password_prompt {
                        self.password_input.clear();
                        self.stage = Stage::Password;
                        return;
                    }
                    self.rejection = match code {
                        ErrorCode::Full => Some((RejectReason::Full, Instant::now())),
                        ErrorCode::BadName => Some((RejectReason::BadName, Instant::now())),
                        ErrorCode::InMatch => Some((RejectReason::InMatch, Instant::now())),
                        ErrorCode::BadVersion => Some((RejectReason::BadVersion, Instant::now())),
                        _ => None,
                    };
                    self.password_prompt = false;
                    self.stage = Stage::Directory;
                }
            }
            _ => {}
        }
    }

    fn send_action(&mut self, message: ClientMsg) {
        let Some(reference) = self.room_ref.as_ref() else {
            return;
        };
        let room_id = reference.room_id.clone();
        if let ClientMsg::Input {
            tick,
            mv,
            bomb,
            punch,
        } = message
        {
            if self
                .client
                .send_v2(Request::Input {
                    room_id,
                    tick,
                    mv,
                    bomb,
                    punch,
                })
                .is_err()
            {
                self.close("connection lost");
            }
            return;
        }
        let revision = self.revision;
        let capacity = self.capacity;
        self.control(|request_id| match message {
            ClientMsg::Start => Request::Start {
                request_id,
                room_id,
            },
            ClientMsg::AddBot => Request::AddBot {
                request_id,
                room_id,
                slot: None,
            },
            ClientMsg::RemoveBot => Request::RemoveBot {
                request_id,
                room_id,
                slot: None,
            },
            ClientMsg::SetReady { ready } => Request::SetReady {
                request_id,
                room_id,
                ready,
            },
            ClientMsg::SetLook { look } => Request::SetLook {
                request_id,
                room_id,
                look,
            },
            ClientMsg::SetRoom { settings } => Request::SetRoom {
                request_id,
                room_id,
                expected_revision: revision,
                settings,
                capacity,
                password: None,
            },
            ClientMsg::Leave => Request::LeaveRoom {
                request_id,
                room_id,
            },
            _ => Request::Ping { nonce: 0 },
        });
    }

    pub fn is_host(&self) -> bool {
        self.host.is_some()
    }

    pub fn host_addr(&self) -> Option<SocketAddr> {
        self.host.as_ref().map(Host::addr)
    }

    /// The reason the host refused this join, once it has been on screen for `shown_for`.
    pub fn rejection_shown(&self, shown_for: Duration) -> Option<RejectReason> {
        self.rejection
            .filter(|(_, since)| since.elapsed() >= shown_for)
            .map(|(reason, _)| reason)
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
            ServerMsg::Rejected { reason } => {
                self.rejection = Some((reason, Instant::now()));
                self.close(match reason {
                    RejectReason::Full => "join refused: the match is full",
                    RejectReason::BadVersion => "join refused: incompatible version",
                    RejectReason::BadName => "join refused: invalid name",
                    RejectReason::InMatch => "join refused: a match is running",
                });
            }
            ServerMsg::Lobby { lobby, .. } => {
                self.lobby = lobby;
                if self.stage != Stage::Closed
                    && self.stage != Stage::Playing
                    && self.stage != Stage::ConfirmLeave
                {
                    self.stage = Stage::Lobby;
                }
            }
            ServerMsg::Start { .. } => {
                if self.stage == Stage::ConfirmLeave {
                    self.leave_return = Stage::Playing;
                } else {
                    self.stage = Stage::Playing;
                }
                self.notice = None;
                self.input.clear();
            }
            ServerMsg::Snapshot { tick, state, .. } => {
                self.tick = tick;
                self.observation = Some(*state);
                if self.stage != Stage::Closed && self.stage != Stage::ConfirmLeave {
                    self.stage = Stage::Playing;
                }
            }
            ServerMsg::PlayerLeft { slot, .. } => {
                self.notice = Some(format!("Player {} left, a bot took over", slot + 1));
            }
            ServerMsg::Ended { outcome } => {
                self.notice = None;
                if self.stage == Stage::ConfirmLeave {
                    self.leave_return = Stage::Lobby;
                } else {
                    self.stage = Stage::Lobby;
                }
            }
            ServerMsg::Error { code, message } => {
                if code == "HostLeft" {
                    self.close("the host left");
                } else {
                    self.notice = Some(message);
                }
            }
            ServerMsg::RoomV2(reply) => self.on_room_message(*reply),
            ServerMsg::Pong { .. } | ServerMsg::DevState { .. } | ServerMsg::RoomInfo { .. } => {}
        }
    }

    /// Reacts to a key press for the current stage.
    pub fn handle_key(&mut self, code: KeyCode) {
        if self.stage == Stage::Creating {
            if let Some(screen) = self.setup.as_mut() {
                screen.handle_key(code);
                if screen.quit {
                    self.stage = Stage::Directory;
                    self.setup = None;
                } else if screen.done {
                    let settings = screen.settings.clone();
                    self.setup = None;
                    self.create_room(settings);
                }
            }
            return;
        }
        if self.stage == Stage::Password {
            match code {
                KeyCode::Esc => {
                    self.password_input.clear();
                    let updating_room = self.password_update;
                    self.password_prompt = false;
                    self.password_update = false;
                    self.notice = None;
                    self.stage = if updating_room {
                        Stage::Lobby
                    } else {
                        Stage::Directory
                    };
                }
                KeyCode::Enter => {
                    let password = std::mem::take(&mut self.password_input);
                    if self.password_update {
                        self.password_prompt = false;
                        self.password_update = false;
                        self.stage = Stage::Lobby;
                        self.update_room_options(
                            self.lobby.settings.clone(),
                            self.capacity,
                            Some(eksplode_re_net::protocol::v2::Secret::new(password)),
                        );
                    } else {
                        self.join_room(Some(eksplode_re_net::protocol::v2::Secret::new(password)));
                    }
                }
                KeyCode::Backspace => {
                    self.password_input.pop();
                }
                KeyCode::Char(character)
                    if !character.is_control()
                        && self.password_input.chars().count() < 128
                        && self.password_input.len() + character.len_utf8() <= 512 =>
                {
                    self.password_input.push(character);
                }
                _ => {}
            }
            return;
        }
        let action = map_key_for_player(code, self.key_bindings, 0);
        if action == Some(KeyAction::Quit) {
            self.send_action(ClientMsg::Leave);
            self.quit = true;
            return;
        }
        if code == KeyCode::Esc && matches!(self.stage, Stage::Lobby | Stage::Playing) {
            self.leave_return = self.stage;
            self.stage = Stage::ConfirmLeave;
            self.input.clear();
            return;
        }
        match self.stage {
            Stage::Directory => match code {
                KeyCode::Up => self.selected_room = self.selected_room.saturating_sub(1),
                KeyCode::Down => {
                    self.selected_room =
                        (self.selected_room + 1).min(self.rooms.len().saturating_sub(1))
                }
                KeyCode::Char('r') => self.refresh_rooms(),
                KeyCode::Char('c') => {
                    let mut settings = self.creation_defaults.clone();
                    if settings.name.is_empty() {
                        settings.name = format!("{}'s room", self.player_name)
                            .chars()
                            .take(24)
                            .collect();
                    }
                    self.setup = Some(SetupScreen::new(settings));
                    self.stage = Stage::Creating;
                }
                KeyCode::Enter => {
                    if let Some(room) = self.rooms.get(self.selected_room).cloned() {
                        if room.password_required {
                            self.password_input.clear();
                            self.password_prompt = true;
                            self.notice = None;
                            self.stage = Stage::Password;
                        } else {
                            self.join_room(None);
                        }
                    }
                }
                _ => {}
            },
            Stage::ConfirmLeave => match code {
                KeyCode::Esc => self.stage = self.leave_return,
                KeyCode::Enter => self.send_action(ClientMsg::Leave),
                _ => {}
            },
            Stage::Playing => {
                if let Some(KeyAction::Player { action, .. }) = action {
                    self.input.press(0, action);
                }
            }
            Stage::Lobby => self.lobby_key(code, action),
            Stage::Connecting | Stage::Closed | Stage::Creating | Stage::Password => {}
        }
    }

    /// Reacts to a raw terminal key event (Enter in all its forms, modifiers dropped).
    pub fn handle_event(&mut self, ev: KeyEvent) {
        if let Some(code) = normalize(ev) {
            self.handle_key(code);
        }
    }

    /// Lobby commands go to the host as messages; the host decides who may do what and
    /// answers refusals with an `Error` that ends up in the notice.
    fn lobby_key(&mut self, code: KeyCode, action: Option<KeyAction>) {
        match code {
            KeyCode::Char('c' | 'C') => {
                if self.owner_slot != self.slot {
                    self.notice = Some("Only the room owner can change capacity".into());
                    return;
                }
                if self.lobby.settings.mode != eksplode_re_core::Mode::Normal {
                    self.notice = Some("Team and Chaser rooms require four players".into());
                    return;
                }
                // bots only fill seats (011): the host drops them when the capacity shrinks,
                // so only humans set the smallest capacity
                let humans = self
                    .lobby
                    .slots
                    .iter()
                    .filter(|slot| {
                        matches!(
                            slot.controller,
                            eksplode_re_core::Controller::Local
                                | eksplode_re_core::Controller::Remote
                        )
                    })
                    .count()
                    .min(4) as u8;
                let capacity = if self.capacity > humans.max(1) {
                    self.capacity - 1
                } else {
                    4
                };
                self.update_room_options(self.lobby.settings.clone(), capacity, None);
                return;
            }
            KeyCode::Char('p' | 'P') => {
                if self.owner_slot == self.slot {
                    self.password_input.clear();
                    self.password_prompt = true;
                    self.password_update = true;
                    self.notice = None;
                    self.stage = Stage::Password;
                } else {
                    self.notice = Some("Only the room owner can change its password".into());
                }
                return;
            }
            _ => {}
        }
        let msg = match code {
            KeyCode::Char(c @ ('m' | 's' | 't' | 'f' | 'd' | 'M' | 'S' | 'T' | 'F' | 'D')) => {
                let mut settings = self.lobby.settings.clone();
                match c.to_ascii_lowercase() {
                    'm' => settings.step_mode(1),
                    's' => settings.step_size(1),
                    't' => settings.step_minutes(1),
                    'd' => settings.step_bot_difficulty(1),
                    _ => settings.friendly_fire = !settings.friendly_fire,
                }
                ClientMsg::set_room(&settings)
            }
            KeyCode::Char('b') => ClientMsg::AddBot,
            KeyCode::Char('x') => ClientMsg::RemoveBot,
            KeyCode::Enter => ClientMsg::Start,
            _ if action == Some(KeyAction::Rematch) => {
                let ready = self.slot.is_some_and(|s| self.lobby.slots[s].ready);
                ClientMsg::SetReady { ready: !ready }
            }
            _ => return,
        };
        self.notice = None;
        self.send_action(msg);
    }

    fn update_room_options(
        &mut self,
        settings: RoomSettings,
        capacity: u8,
        password: Option<eksplode_re_net::protocol::v2::Secret>,
    ) {
        if self
            .client
            .set_room_options(&settings, capacity, password)
            .is_err()
        {
            self.notice = Some("room update could not be sent".into());
        }
    }

    /// Sends this tick's input to the host while a match runs.
    pub fn send_input(&mut self) {
        if self.stage != Stage::Playing {
            return;
        }
        let i = self.input.take()[0];
        self.send_action(ClientMsg::Input {
            tick: self.tick,
            mv: i.mv,
            bomb: i.bomb,
            punch: i.punch,
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
    if online.stage == Stage::Creating {
        if let Some(screen) = online.setup.as_ref() {
            crate::setup::render(frame, screen);
            return;
        }
    }
    if online.stage == Stage::Directory {
        render_directory(frame, online);
        return;
    }
    if online.stage == Stage::Password {
        render_password(frame, online);
        return;
    }
    if online.stage == Stage::ConfirmLeave {
        let area = frame.area();
        frame.render_widget(
            Paragraph::new("Leave this room?\nEnter confirm   Esc cancel")
                .block(Block::bordered().title(" Leave room ")),
            area,
        );
        return;
    }
    match (&online.observation, online.stage) {
        (Some(obs), Stage::Playing) => view::render(frame, obs, &online.context()),
        _ => render_lobby(frame, online),
    }
}

fn render_directory(frame: &mut Frame, online: &Online) {
    let area = frame.area();
    if area.width < 40 || area.height < 10 {
        frame.render_widget(Paragraph::new("Terminal too small: need 40 x 10"), area);
        return;
    }
    let mut lines = vec![
        Line::from(format!("Player: {}", online.player_name)),
        Line::default(),
    ];
    let count = usize::from(area.height.saturating_sub(9)).max(1);
    let start = online.selected_room.saturating_sub(count.saturating_sub(1));
    if online.rooms.is_empty() {
        lines.push(Line::from("No rooms available"));
    }
    for (index, room) in online.rooms.iter().enumerate().skip(start).take(count) {
        let selected = if index == online.selected_room {
            ">"
        } else {
            " "
        };
        // bots only keep seats warm (011): count the humans, show the bots apart
        let humans = room.connected_humans.saturating_add(room.reserved_humans);
        let bots = match room.bots {
            0 => String::new(),
            1 => " +1 bot".to_string(),
            n => format!(" +{n} bots"),
        };
        let label = format!(
            "{selected} {} [{}] | {} | {}/{}{} | {:?} | {} | {}",
            room.settings.name,
            room.reference.room_id.chars().take(8).collect::<String>(),
            room.settings.mode.label(),
            humans,
            room.capacity,
            bots,
            room.phase,
            if room.password_required {
                "password required"
            } else {
                "open"
            },
            room.reference.server_id
        );
        lines.push(Line::from(
            label
                .chars()
                .take(usize::from(area.width.saturating_sub(2)))
                .collect::<String>(),
        ));
    }
    if let Some(selected) = online.rooms.get(online.selected_room) {
        lines.push(Line::from(format!("Room: {}", selected.reference.room_id)));
    }
    lines.push(Line::default());
    if let Some(notice) = online.notice.as_ref() {
        lines.push(Line::from(notice.as_str()));
    }
    lines.push(Line::from(
        "Up/Down select   Enter join   c create   r refresh   q quit",
    ));
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" Rooms ")),
        area,
    );
}

fn render_password(frame: &mut Frame, online: &Online) {
    let area = frame.area();
    let room_name = online
        .rooms
        .get(online.selected_room)
        .map(|room| room.settings.name.as_str())
        .unwrap_or("selected room");
    let mut lines = vec![
        Line::from(format!("Room: {room_name}")),
        Line::from(format!(
            "Password: {}",
            "*".repeat(online.password_input.chars().count())
        )),
    ];
    if let Some(notice) = online.notice.as_ref() {
        lines.push(Line::from(notice.as_str()));
    }
    lines.push(Line::default());
    lines.push(Line::from(if online.password_update {
        "Type new password   Enter save (blank removes)   Esc cancel"
    } else {
        "Type password   Enter join   Esc cancel"
    }));
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" Room password ")),
        area,
    );
}

fn render_lobby(frame: &mut Frame, online: &Online) {
    let area = frame.area();
    let ids = eksplode_re_core::PlayerId::all();
    let mut lines: Vec<Line> = Vec::new();
    if let Some(addr) = online.host_addr() {
        lines.push(Line::from(format!("Hosting on port {}", addr.port())));
        lines.push(Line::from(format!(
            "Others join with: eksplode-re-cli join <your-ip>:{}",
            addr.port()
        )));
        if let Some(url) = &online.web_url {
            lines.push(Line::from(format!("Web page: {url}")));
        }
        lines.push(Line::default());
    }
    if online.stage != Stage::Connecting {
        lines.push(Line::from(Span::styled(
            online.lobby.settings.label(),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        if online.lobby.settings.mode == eksplode_re_core::Mode::Chaser {
            lines.push(Line::from(
                "one chaser, three runners: roles are drawn at the start",
            ));
        } else {
            lines.push(Line::default());
        }
        if online.password_required {
            lines.push(Line::from("Password: required"));
        } else {
            lines.push(Line::from("Password: open"));
        }
        lines.push(Line::from(format!(
            "Capacity: {}/{}",
            online.lobby.participants(),
            online.capacity
        )));
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
        let team = if online.lobby.settings.mode == eksplode_re_core::Mode::Team {
            format!(
                "  [team {}]",
                team_letter(eksplode_re_core::team::team_of(i))
            )
        } else {
            String::new()
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("Slot {}  ", i + 1),
                Style::default()
                    .fg(player_color(ids[i]))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("{who}{me}{team}")),
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
        _ => "Enter start (owner)   b add bot   x remove bot   r ready   q quit",
    }));
    if online.stage == Stage::Lobby {
        if online.owner_slot == online.slot {
            lines.push(Line::from(
                "Owner controls: c capacity   p password   m mode   s size   t time   f fire",
            ));
        } else {
            let owner_name = online
                .owner_slot
                .and_then(|slot| online.lobby.slots.get(slot))
                .map(|slot| slot.name.as_str())
                .filter(|name| !name.is_empty())
                .unwrap_or("the room owner");
            lines.push(Line::from(format!(
                "Only {owner_name} can manage room settings"
            )));
        }
    }
    let height = (lines.len() as u16 + 2).min(area.height);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" Eksplode.re online ")),
        Rect::new(area.x, area.y, area.width, height),
    );
}
