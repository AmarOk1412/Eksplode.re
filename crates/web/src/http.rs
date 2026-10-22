//! A small HTTP/1.1 layer: five fixed routes, strict limits, one WebSocket endpoint.

use std::io::{Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use bomberust_net::HostHandle;

use crate::assets::{self, CSP};
use crate::origin::{cors_header, origin_allowed, AllowedOrigins};
use crate::ws;

/// Longest accepted request head (request line plus headers).
pub const MAX_HEAD: usize = 8 * 1024;
const MAX_HEADERS: usize = 64;
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Shared state of every connection thread.
pub struct Ctx {
    pub origins: AllowedOrigins,
    pub max_connections: usize,
    pub request_timeout: Duration,
    pub stop: Arc<AtomicBool>,
    pub active: Arc<AtomicUsize>,
    pub host: HostHandle,
}

/// A parsed request head.
#[derive(Debug)]
pub struct Request {
    pub method: String,
    /// The path without query string or fragment.
    pub path: String,
    pub headers: Vec<(String, String)>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn wants_websocket(&self) -> bool {
        self.header("upgrade")
            .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
    }
}

/// Why a request head could not be read.
#[derive(Debug, PartialEq, Eq)]
pub enum HeadError {
    TooLarge,
    /// Closed, timed out or failed before a full head arrived.
    Incomplete,
}

fn find_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

/// Reads until the blank line that ends the head; returns everything read so far.
pub fn read_head(stream: &mut impl Read) -> Result<Vec<u8>, HeadError> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        if let Some(end) = find_end(&buf) {
            return if end > MAX_HEAD {
                Err(HeadError::TooLarge)
            } else {
                Ok(buf)
            };
        }
        if buf.len() > MAX_HEAD {
            return Err(HeadError::TooLarge);
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return Err(HeadError::Incomplete),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
}

/// Parses a head read by [`read_head`]; `None` means "400 Bad Request".
pub fn parse_head(raw: &[u8]) -> Option<Request> {
    let end = find_end(raw)?;
    let text = std::str::from_utf8(&raw[..end - 4]).ok()?;
    let mut lines = text.split("\r\n");
    let mut parts = lines.next()?.split(' ');
    let (method, target, version) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || !matches!(version, "HTTP/1.0" | "HTTP/1.1") {
        return None;
    }
    if method.is_empty() || !method.bytes().all(|b| b.is_ascii_uppercase()) {
        return None;
    }
    if !target.starts_with('/') {
        return None;
    }
    let path = target.split(['?', '#']).next().unwrap_or("/").to_string();
    let mut headers = Vec::new();
    for line in lines {
        let (name, value) = line.split_once(':')?;
        if name.is_empty() || name.contains(' ') || headers.len() >= MAX_HEADERS {
            return None;
        }
        headers.push((name.to_string(), value.trim().to_string()));
    }
    Some(Request {
        method: method.to_string(),
        path,
        headers,
    })
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        426 => "Upgrade Required",
        431 => "Request Header Fields Too Large",
        503 => "Service Unavailable",
        _ => "Error",
    }
}

fn respond(
    stream: &mut TcpStream,
    status: u16,
    headers: &[(&str, String)],
    body: &[u8],
    head_only: bool,
) {
    let mut out = format!("HTTP/1.1 {status} {}\r\n", reason(status));
    for (k, v) in headers {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    out.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    ));
    let mut bytes = out.into_bytes();
    if !head_only {
        bytes.extend_from_slice(body);
    }
    let _ = stream.write_all(&bytes);
    let _ = stream.flush();
}

fn plain(stream: &mut TcpStream, status: u16, text: &str, extra: &[(&str, String)]) {
    let mut headers = vec![("Content-Type", "text/plain; charset=utf-8".to_string())];
    headers.extend(extra.iter().map(|(k, v)| (*k, v.clone())));
    respond(stream, status, &headers, text.as_bytes(), false);
}

/// Accepts connections until `ctx.stop` is set.
pub fn serve(listener: TcpListener, ctx: Arc<Ctx>) {
    let _ = listener.set_nonblocking(true);
    while !ctx.stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((mut stream, peer)) => {
                let _ = stream.set_nonblocking(false);
                if ctx.active.fetch_add(1, Ordering::SeqCst) >= ctx.max_connections {
                    ctx.active.fetch_sub(1, Ordering::SeqCst);
                    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
                    plain(&mut stream, 503, "too many connections", &[]);
                    continue;
                }
                let ctx = ctx.clone();
                thread::spawn(move || {
                    let _guard = ActiveGuard(ctx.active.clone());
                    handle(stream, peer.ip(), &ctx);
                });
            }
            Err(_) => thread::sleep(Duration::from_millis(10)),
        }
    }
}

struct ActiveGuard(Arc<AtomicUsize>);

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

fn handle(mut stream: TcpStream, peer: IpAddr, ctx: &Ctx) {
    let _ = stream.set_read_timeout(Some(ctx.request_timeout));
    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
    let raw = match read_head(&mut stream) {
        Ok(raw) => raw,
        Err(HeadError::TooLarge) => return plain(&mut stream, 431, "request head too large", &[]),
        Err(HeadError::Incomplete) => return,
    };
    let Some(req) = parse_head(&raw) else {
        return plain(&mut stream, 400, "bad request", &[]);
    };

    if req.path == "/ws" {
        if !req.wants_websocket() {
            let extra = [("Upgrade", "websocket".to_string())];
            return plain(&mut stream, 426, "use a WebSocket client", &extra);
        }
        if !origin_allowed(&ctx.origins, req.header("origin"), req.header("host"), peer) {
            return plain(&mut stream, 403, "origin not allowed", &[]);
        }
        return ws::run(stream, raw, ctx);
    }

    let head_only = match req.method.as_str() {
        "GET" => false,
        "HEAD" => true,
        _ => {
            let extra = [("Allow", "GET, HEAD".to_string())];
            return plain(&mut stream, 405, "method not allowed", &extra);
        }
    };
    let Some(asset) = assets::lookup(&req.path) else {
        return plain(&mut stream, 404, "not found", &[]);
    };
    let mut headers = vec![
        ("Content-Type", asset.content_type.to_string()),
        ("Cache-Control", "no-cache".to_string()),
        ("X-Content-Type-Options", "nosniff".to_string()),
        ("Referrer-Policy", "no-referrer".to_string()),
        ("Content-Security-Policy", CSP.to_string()),
    ];
    let origin = req.header("origin");
    if let Some(value) = cors_header(&ctx.origins, origin) {
        headers.push(("Access-Control-Allow-Origin", value));
        if value_varies(&ctx.origins) {
            headers.push(("Vary", "Origin".to_string()));
        }
    }
    respond(&mut stream, 200, &headers, asset.body, head_only);
}

fn value_varies(policy: &AllowedOrigins) -> bool {
    matches!(policy, AllowedOrigins::List(_))
}
