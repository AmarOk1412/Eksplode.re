//! Network play: wire protocol, authoritative host and client.

pub mod client;
pub mod codec;
pub mod host;
pub mod protocol;

pub use client::Client;
pub use codec::NetError;
pub use host::{Host, HostConfig, HostHandle};
pub use protocol::{ClientMsg, RejectReason, ServerMsg, DEFAULT_PORT, PROTOCOL_VERSION};
