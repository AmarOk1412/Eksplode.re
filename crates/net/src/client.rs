//! Client side: a connection to a host (TCP or in-process) seen as a message channel.

use std::collections::VecDeque;
use std::io::{BufReader, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::codec::{decode, encode, LineReader, NetError};
use crate::protocol::{ClientMsg, RejectReason, ServerMsg};

type SendFn = Box<dyn Fn(ClientMsg) -> Result<(), NetError> + Send>;

/// A joined (or joining) connection. Dropping it disconnects.
pub struct Client {
    send: SendFn,
    rx: Receiver<ServerMsg>,
    on_drop: Option<Box<dyn FnOnce() + Send>>,
    room: Mutex<Option<ClientRoom>>,
    request_sequence: AtomicU64,
    queued: Mutex<VecDeque<ServerMsg>>,
}

#[derive(Clone)]
struct ClientRoom {
    reference: crate::protocol::v2::RoomRef,
    revision: u64,
    capacity: u8,
}

impl Client {
    /// Connects and negotiates protocol v2 without taking a seat in a room.
    pub fn connect(addr: &str) -> Result<Client, NetError> {
        Self::connect_unjoined(addr)
    }

    pub fn connect_unjoined(addr: &str) -> Result<Client, NetError> {
        let client = Self::transport(addr)?;
        client.send_v2(crate::protocol::v2::Request::Hello {
            version: crate::protocol::v2::VERSION,
        })?;
        Ok(client)
    }

    fn transport(addr: &str) -> Result<Client, NetError> {
        let stream = TcpStream::connect(addr)?;
        stream.set_nodelay(true)?;
        let (tx, rx) = mpsc::sync_channel(128);

        let reader = stream.try_clone()?;
        let reader_control = stream.try_clone()?;
        reader.set_read_timeout(Some(Duration::from_secs(2)))?;
        thread::spawn(move || {
            let mut lines = LineReader::new(BufReader::new(reader));
            let mut awaiting_hello = true;
            loop {
                let line = match lines.read_line() {
                    Ok(Some(line)) => line,
                    _ => {
                        if awaiting_hello {
                            let _ = tx.send(ServerMsg::Rejected {
                                reason: RejectReason::BadVersion,
                            });
                        }
                        break;
                    }
                };
                if awaiting_hello {
                    match decode::<crate::protocol::v2::Reply>(&line) {
                        Ok(reply @ crate::protocol::v2::Reply::Hello { version: 2, .. }) => {
                            awaiting_hello = false;
                            let _ = reader_control.set_read_timeout(None);
                            if tx.send(ServerMsg::RoomV2(Box::new(reply))).is_err() {
                                break;
                            }
                        }
                        Ok(
                            reply @ crate::protocol::v2::Reply::Error {
                                code:
                                    crate::protocol::v2::ErrorCode::ServerFull
                                    | crate::protocol::v2::ErrorCode::Unavailable,
                                ..
                            },
                        ) => {
                            let _ = tx.send(ServerMsg::RoomV2(Box::new(reply)));
                            break;
                        }
                        _ => {
                            let _ = tx.send(ServerMsg::Rejected {
                                reason: RejectReason::BadVersion,
                            });
                            break;
                        }
                    }
                    continue;
                }
                let message = decode::<crate::protocol::v2::Reply>(&line)
                    .map(|reply| ServerMsg::RoomV2(Box::new(reply)));
                if let Ok(msg) = message {
                    if tx.send(msg).is_err() {
                        break;
                    }
                }
            }
            let _ = reader_control.shutdown(std::net::Shutdown::Both);
        });

        let closer = stream.try_clone()?;
        let writer = Arc::new(Mutex::new(stream));
        let send: SendFn = Box::new(move |msg| {
            let bytes = encode(&msg)?;
            let mut s = writer
                .lock()
                .map_err(|_| NetError::Protocol("writer poisoned".into()))?;
            s.write_all(&bytes)?;
            Ok(())
        });
        let client = Client {
            send,
            rx,
            on_drop: Some(Box::new(move || {
                let _ = closer.shutdown(std::net::Shutdown::Both);
            })),
            room: Mutex::new(None),
            request_sequence: AtomicU64::new(0),
            queued: Mutex::new(VecDeque::new()),
        };
        Ok(client)
    }

    pub(crate) fn from_parts(
        send: SendFn,
        rx: Receiver<ServerMsg>,
        on_drop: Box<dyn FnOnce() + Send>,
    ) -> Client {
        Client {
            send,
            rx,
            on_drop: Some(on_drop),
            room: Mutex::new(None),
            request_sequence: AtomicU64::new(0),
            queued: Mutex::new(VecDeque::new()),
        }
    }

    /// Sends a message to the host.
    pub fn send(&self, msg: ClientMsg) -> Result<(), NetError> {
        use crate::protocol::v2::{Request, RequestId, Scope};
        if matches!(msg, ClientMsg::RoomV2(_)) {
            return (self.send)(msg);
        }
        let binding = self
            .room
            .lock()
            .map_err(|_| NetError::Protocol("room state unavailable".into()))?
            .clone();
        if let Some(binding) = binding {
            // try_update (the non-deprecated replacement) needs Rust 1.95, newer than this
            // workspace's declared MSRV (1.75); keep fetch_update until the MSRV moves.
            #[allow(deprecated)]
            let sequence = self
                .request_sequence
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |sequence| {
                    sequence.checked_add(1)
                })
                .map_err(|_| NetError::Protocol("request counter exhausted".into()))?;
            let request_id = RequestId::new(format!("sdk-{sequence}"))
                .map_err(|error| NetError::Protocol(error.into()))?;
            let room_id = binding.reference.room_id;
            let request = match msg {
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
                    expected_revision: binding.revision,
                    settings,
                    capacity: binding.capacity,
                    password: None,
                },
                ClientMsg::Input {
                    tick,
                    mv,
                    bomb,
                    punch,
                } => Request::Input {
                    room_id,
                    tick,
                    mv,
                    bomb,
                    punch,
                },
                ClientMsg::Leave => Request::LeaveRoom {
                    request_id,
                    room_id,
                },
                ClientMsg::Ping { nonce } => Request::Ping { nonce },
                ClientMsg::RoomQuery => Request::ListRooms {
                    request_id,
                    scope: Scope::Local,
                    cursor: None,
                },
                ClientMsg::DevPauseBots { paused } => Request::DevPauseBots {
                    request_id,
                    room_id,
                    paused,
                },
                ClientMsg::DevSetDrops { chances } => Request::DevSetDrops {
                    request_id,
                    room_id,
                    chances,
                },
                ClientMsg::DevResetDrops => Request::DevResetDrops {
                    request_id,
                    room_id,
                },
                _ => {
                    return Err(NetError::Protocol(
                        "admission requires an explicit v2 room request".into(),
                    ))
                }
            };
            return (self.send)(ClientMsg::RoomV2(Box::new(request)));
        }
        (self.send)(msg)
    }

    pub fn send_v2(&self, request: crate::protocol::v2::Request) -> Result<(), NetError> {
        self.send(ClientMsg::RoomV2(Box::new(request)))
    }

    pub fn create_room(
        &self,
        name: &str,
        settings: &eksplode_re_core::RoomSettings,
        capacity: u8,
    ) -> Result<crate::protocol::v2::RoomRef, NetError> {
        let request_id = self.admission_id()?;
        self.send_v2(crate::protocol::v2::Request::create_room(
            request_id.clone(),
            name.into(),
            settings,
            capacity,
        ))?;
        self.await_admission(&request_id)
    }

    pub fn join_room(
        &self,
        reference: &crate::protocol::v2::RoomRef,
        name: &str,
    ) -> Result<crate::protocol::v2::RoomRef, NetError> {
        let request_id = self.admission_id()?;
        self.send_v2(crate::protocol::v2::Request::JoinRoom {
            request_id: request_id.clone(),
            server_epoch: reference.server_epoch.clone(),
            room_id: reference.room_id.clone(),
            name: name.into(),
            password: None,
            look: None,
        })?;
        self.await_admission(&request_id)
    }

    pub fn set_room_options(
        &self,
        settings: &eksplode_re_core::RoomSettings,
        capacity: u8,
        password: Option<crate::protocol::v2::Secret>,
    ) -> Result<(), NetError> {
        use crate::protocol::v2::{Request, RequestId};
        let binding = self
            .room
            .lock()
            .map_err(|_| NetError::Protocol("room state unavailable".into()))?
            .clone()
            .ok_or_else(|| NetError::Protocol("room settings require an admitted room".into()))?;
        // try_update (the non-deprecated replacement) needs Rust 1.95, newer than this
        // workspace's declared MSRV (1.75); keep fetch_update until the MSRV moves.
        #[allow(deprecated)]
        let sequence = self
            .request_sequence
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |sequence| {
                sequence.checked_add(1)
            })
            .map_err(|_| NetError::Protocol("request counter exhausted".into()))?;
        let request_id = RequestId::new(format!("sdk-{sequence}"))
            .map_err(|error| NetError::Protocol(error.into()))?;
        let settings = serde_json::to_value(settings)
            .map_err(|error| NetError::Malformed(error.to_string()))?;
        self.send_v2(Request::SetRoom {
            request_id,
            room_id: binding.reference.room_id,
            expected_revision: binding.revision,
            settings,
            capacity,
            password,
        })
    }

    fn admission_id(&self) -> Result<crate::protocol::v2::RequestId, NetError> {
        // try_update (the non-deprecated replacement) needs Rust 1.95, newer than this
        // workspace's declared MSRV (1.75); keep fetch_update until the MSRV moves.
        #[allow(deprecated)]
        let sequence = self
            .request_sequence
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |sequence| {
                sequence.checked_add(1)
            })
            .map_err(|_| NetError::Protocol("request counter exhausted".into()))?;
        crate::protocol::v2::RequestId::new(format!("admission-{sequence}"))
            .map_err(|error| NetError::Protocol(error.into()))
    }

    fn await_admission(
        &self,
        request_id: &crate::protocol::v2::RequestId,
    ) -> Result<crate::protocol::v2::RoomRef, NetError> {
        use crate::protocol::v2::Reply;
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut queued = VecDeque::new();
        let result = loop {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                break Err(NetError::PendingAdmission {
                    request_id: request_id.as_str().into(),
                });
            };
            let message = match self.rx.recv_timeout(remaining) {
                Ok(message) => self.received(message),
                Err(_) => {
                    break Err(NetError::PendingAdmission {
                        request_id: request_id.as_str().into(),
                    })
                }
            };
            let outcome = match &message {
                ServerMsg::RoomV2(reply) => match reply.as_ref() {
                    Reply::Welcome {
                        request_id: original,
                        admission,
                    } if original == request_id => Some(Ok(admission.reference.clone())),
                    Reply::Error {
                        request_id: Some(original),
                        code,
                        ..
                    } if original == request_id => {
                        Some(Err(NetError::Protocol(format!("{code:?}"))))
                    }
                    _ => None,
                },
                ServerMsg::Rejected { reason } => {
                    Some(Err(NetError::Protocol(format!("{reason:?}"))))
                }
                _ => None,
            };
            queued.push_back(message);
            if let Some(outcome) = outcome {
                break outcome;
            }
            if queued.len() >= 128 {
                break Err(NetError::PendingAdmission {
                    request_id: request_id.as_str().into(),
                });
            }
        };
        self.queued
            .lock()
            .map_err(|_| NetError::Protocol("reply queue unavailable".into()))?
            .extend(queued);
        result
    }

    /// Next message if one is waiting; `Disconnected` once the host is gone.
    pub fn try_recv(&self) -> Result<ServerMsg, TryRecvError> {
        if let Ok(mut queued) = self.queued.lock() {
            if let Some(message) = queued.pop_front() {
                return Ok(message);
            }
        }
        self.rx.try_recv().map(|message| self.received(message))
    }

    /// Waits up to `timeout` for the next message.
    pub fn recv_timeout(&self, timeout: Duration) -> Result<ServerMsg, RecvTimeoutError> {
        if let Ok(mut queued) = self.queued.lock() {
            if let Some(message) = queued.pop_front() {
                return Ok(message);
            }
        }
        self.rx
            .recv_timeout(timeout)
            .map(|message| self.received(message))
    }

    fn received(&self, message: ServerMsg) -> ServerMsg {
        use crate::protocol::v2::Reply;
        if let ServerMsg::RoomV2(reply) = &message {
            if let Ok(mut room) = self.room.lock() {
                match reply.as_ref() {
                    Reply::Welcome { admission, .. } | Reply::Resumed { admission, .. } => {
                        *room = Some(ClientRoom {
                            reference: admission.reference.clone(),
                            revision: admission.revision,
                            capacity: admission.capacity,
                        });
                    }
                    Reply::RoomUpdated {
                        reference,
                        revision,
                        capacity,
                        ..
                    } => {
                        if let Some(binding) = room
                            .as_mut()
                            .filter(|binding| binding.reference == *reference)
                        {
                            binding.revision = *revision;
                            binding.capacity = *capacity;
                        }
                    }
                    Reply::LeftRoom { reference, .. }
                    | Reply::Removed { reference, .. }
                    | Reply::RoomClosed { reference, .. }
                        if room
                            .as_ref()
                            .is_some_and(|binding| binding.reference == *reference) =>
                    {
                        *room = None
                    }
                    _ => {}
                }
            }
        }
        message
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Some(f) = self.on_drop.take() {
            f();
        }
    }
}
