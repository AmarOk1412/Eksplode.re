//! Server settings shared by the library, the binary and the terminal client's `host --web-port`.

use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use bomberust_core::Config;

use crate::origin::AllowedOrigins;

/// Everything `bomberust-server` can be configured with.
#[derive(Clone, Debug)]
pub struct ServerOptions {
    /// Address both ports listen on.
    pub bind: IpAddr,
    /// Port of the web page and the WebSocket.
    pub http_port: u16,
    /// Port of the terminal clients.
    pub game_port: u16,
    /// Bots added to the lobby at start-up.
    pub bots: u8,
    pub seed: Option<u64>,
    pub origins: AllowedOrigins,
    /// Open HTTP/WebSocket connections at the same time.
    pub max_connections: usize,
    /// Time a client has to send its request head.
    pub request_timeout: Duration,
    /// A player silent for this long during a match is replaced by a bot.
    pub silence_timeout: Duration,
    pub game: Config,
    /// Developer mode: the lobby owner may pause bots and change the drop chances (testing).
    pub dev: bool,
    /// Duration of one simulation tick.
    pub tick: Duration,
}

impl Default for ServerOptions {
    fn default() -> Self {
        let game = Config::default();
        Self {
            bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
            http_port: 8080,
            game_port: bomberust_net::DEFAULT_PORT,
            bots: 0,
            seed: None,
            origins: AllowedOrigins::SameOrigin,
            max_connections: 64,
            request_timeout: Duration::from_secs(5),
            silence_timeout: Duration::from_secs(10),
            tick: Duration::from_secs(1) / game.tick_hz,
            dev: false,
            game,
        }
    }
}
