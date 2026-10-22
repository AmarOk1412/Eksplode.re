//! Which web origins may talk to the game, and the warnings that come with relaxing it.

use std::net::IpAddr;

use crate::options::ServerOptions;

/// Cross-origin policy of the server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AllowedOrigins {
    /// Only pages served by this server itself.
    SameOrigin,
    /// The own origin plus these exact origins (`scheme://host[:port]`).
    List(Vec<String>),
    /// Any web page (unsafe).
    Any,
}

/// May a WebSocket upgrade with this `Origin` be accepted?
///
/// `host` is the `Host` header of the request and `peer` the address it came from. A missing
/// `Origin` (command-line tools, not browsers) is accepted from the local machine only.
pub fn origin_allowed(
    policy: &AllowedOrigins,
    origin: Option<&str>,
    host: Option<&str>,
    peer: IpAddr,
) -> bool {
    if *policy == AllowedOrigins::Any {
        return true;
    }
    let Some(origin) = origin else {
        return peer.is_loopback();
    };
    let origin = origin.to_ascii_lowercase();
    if let Some(host) = host {
        let host = host.to_ascii_lowercase();
        if origin == format!("http://{host}") || origin == format!("https://{host}") {
            return true;
        }
    }
    match policy {
        AllowedOrigins::List(list) => list.iter().any(|o| o.to_ascii_lowercase() == origin),
        _ => false,
    }
}

/// Value of `Access-Control-Allow-Origin` for a static response, if the policy allows one.
pub fn cors_header(policy: &AllowedOrigins, origin: Option<&str>) -> Option<String> {
    match (policy, origin) {
        (AllowedOrigins::Any, _) => Some("*".to_string()),
        (AllowedOrigins::List(list), Some(o)) => list
            .iter()
            .find(|allowed| allowed.eq_ignore_ascii_case(o))
            .map(|_| o.to_string()),
        _ => None,
    }
}

/// Start-up warnings for settings that open the game to more than the local machine.
pub fn warnings(opts: &ServerOptions) -> Vec<String> {
    let mut out = Vec::new();
    if !opts.bind.is_loopback() {
        out.push(format!(
            "WARNING: listening on {}: the game has no encryption and no authentication, so \
             anyone who can reach this machine on these ports can play. Use --bind 127.0.0.1 \
             (the default) or a firewall or tunnel to restrict access.",
            opts.bind
        ));
    }
    match &opts.origins {
        AllowedOrigins::SameOrigin => {}
        AllowedOrigins::List(list) => {
            for origin in list {
                out.push(format!(
                    "WARNING: cross-origin access (CORS) is enabled for {origin}: web pages \
                     from that origin can drive the game."
                ));
            }
        }
        AllowedOrigins::Any => out.push(
            "WARNING: --allow-any-origin: any web page opened by anyone who can reach this \
             server can drive the game (CORS unrestricted). Do not use this on a shared network."
                .to_string(),
        ),
    }
    out
}
