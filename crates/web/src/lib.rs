//! Web server of Bomberust: serves the page and bridges WebSocket players to the match host.
//!
//! One process runs the authoritative match (`bomberust-net`) with its TCP port for terminal
//! clients, plus an HTTP server on a second port. Browser tabs connect to `/ws` and are attached
//! to the same host as in-process clients, so they are peers of terminal players and bots.

pub mod addr;
pub mod args;
pub mod assets;
pub mod http;
pub mod options;
pub mod origin;
mod ws;

use std::fmt;
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use bomberust_net::{Host, HostConfig, HostHandle, NetError};

pub use options::ServerOptions;

/// Why the server could not start.
#[derive(Debug)]
pub enum ServerError {
    /// A port could not be opened.
    Bind {
        what: &'static str,
        addr: SocketAddr,
        source: std::io::Error,
    },
    Net(NetError),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerError::Bind { what, addr, source } => {
                write!(f, "cannot listen on {addr} ({what}): {source}")
            }
            ServerError::Net(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ServerError {}

/// A running server. Dropping it stops the web server and, when it owns one, the match host.
pub struct Server {
    host: Option<Host>,
    opts: ServerOptions,
    http_addr: SocketAddr,
    game_addr: SocketAddr,
    stop: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
}

impl Server {
    /// Starts the match host and the web server.
    pub fn start(opts: ServerOptions) -> Result<Server, ServerError> {
        let game_bind = SocketAddr::new(opts.bind, opts.game_port);
        let host = Host::start(HostConfig {
            bind: game_bind.to_string(),
            tick: opts.tick,
            seed: opts.seed,
            game: opts.game.clone(),
            silence_timeout: opts.silence_timeout,
            dev: opts.dev,
            defaults: opts.room.clone(),
        })
        .map_err(|e| match e {
            NetError::Io(source) => ServerError::Bind {
                what: "game port",
                addr: game_bind,
                source,
            },
            other => ServerError::Net(other),
        })?;
        for _ in 0..opts.bots {
            host.add_bot();
        }
        let game_addr = host.addr();
        let mut server = Self::serve(host.handle(), game_addr, opts)?;
        server.host = Some(host);
        Ok(server)
    }

    /// Adds the web page to a host that someone else owns (for example the terminal host).
    pub fn attach(
        handle: HostHandle,
        game_addr: SocketAddr,
        opts: ServerOptions,
    ) -> Result<Server, ServerError> {
        Self::serve(handle, game_addr, opts)
    }

    fn serve(
        handle: HostHandle,
        game_addr: SocketAddr,
        opts: ServerOptions,
    ) -> Result<Server, ServerError> {
        let bind = SocketAddr::new(opts.bind, opts.http_port);
        let listener = TcpListener::bind(bind).map_err(|source| ServerError::Bind {
            what: "web port",
            addr: bind,
            source,
        })?;
        let http_addr = listener.local_addr().map_err(|source| ServerError::Bind {
            what: "web port",
            addr: bind,
            source,
        })?;
        let stop = Arc::new(AtomicBool::new(false));
        let ctx = Arc::new(http::Ctx {
            origins: opts.origins.clone(),
            max_connections: opts.max_connections,
            request_timeout: opts.request_timeout,
            stop: stop.clone(),
            active: Arc::new(AtomicUsize::new(0)),
            host: handle,
        });
        let accept = thread::spawn(move || http::serve(listener, ctx));
        Ok(Server {
            host: None,
            opts,
            http_addr,
            game_addr,
            stop,
            accept: Some(accept),
        })
    }

    /// Where the page and the WebSocket are served.
    pub fn http_addr(&self) -> SocketAddr {
        self.http_addr
    }

    /// Where terminal clients connect.
    pub fn game_addr(&self) -> SocketAddr {
        self.game_addr
    }

    /// The text printed when the server is ready.
    pub fn banner(&self) -> String {
        addr::banner(&self.opts, self.http_addr, self.game_addr)
    }

    /// Tells every client the host is leaving and stops the servers.
    pub fn shutdown(self) {}
}

impl Drop for Server {
    fn drop(&mut self) {
        // The host goes first so every client receives `HostLeft` before the sockets close.
        drop(self.host.take());
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.accept.take() {
            let _ = h.join();
        }
    }
}
