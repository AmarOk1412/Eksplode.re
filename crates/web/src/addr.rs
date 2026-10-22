//! The start-up banner and the addresses it shows.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

use crate::options::ServerOptions;

/// The address this machine uses to reach other machines, if it has one.
///
/// A UDP "connection" only selects a route; no packet is sent.
pub fn primary_lan_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

fn host_part(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => format!("[{v6}]"),
    }
}

/// Addresses a client can use: the bound one, or loopback plus the LAN address for wildcards.
fn client_ips(bound: IpAddr) -> Vec<IpAddr> {
    if !bound.is_unspecified() {
        return vec![bound];
    }
    let mut ips = vec![IpAddr::V4(Ipv4Addr::LOCALHOST)];
    ips.extend(primary_lan_ip());
    ips
}

/// What `bomberust-server` prints when it is ready.
pub fn banner(_opts: &ServerOptions, http: SocketAddr, game: SocketAddr) -> String {
    let ips = client_ips(http.ip());
    let mut out = String::from("Bomberust server running\n");
    for (i, ip) in ips.iter().enumerate() {
        let label = if i == 0 {
            "  Web page:        "
        } else {
            "                   "
        };
        out.push_str(&format!(
            "{label}http://{}:{}/\n",
            host_part(*ip),
            http.port()
        ));
    }
    for (i, ip) in ips.iter().enumerate() {
        let label = if i == 0 {
            "  Terminal client: "
        } else {
            "                   "
        };
        out.push_str(&format!(
            "{label}bomberust-cli join {}:{}\n",
            host_part(*ip),
            game.port()
        ));
    }
    out.push_str("  Stop:            Ctrl+C\n");
    out
}
