//! Command-line argument parsing; see specs/001-bomberman-multiplayer/contracts/cli-interface.md.

use std::fmt;
use std::net::IpAddr;

use eksplode_re_core::{BoardSize, Mode, RoomSettings, MINUTES};
use eksplode_re_net::protocol::{valid_name, DEFAULT_PORT};

pub const USAGE: &str = "\
Usage:
  eksplode-re-cli [OPTIONS]                          play on this machine
  eksplode-re-cli host [--port N] [--bots N] [--name NAME] [--seed N] [--web-port N] [--bind IP] [--dev]
                     [--room-name TEXT] [--mode M] [--size S] [--time MIN] [--friendly-fire on|off] [--no-setup]
  eksplode-re-cli join <ADDR[:PORT]> [--name NAME]

Local options:
  --players <N>   number of local human players, 0-4 (default 1)
  --bots <N>      number of bots, 0-4 (default: fill up to 4 participants)
  --seed <U64>    RNG seed (default: from the system time)
  --lobby         choose the slots on a lobby screen before the game starts
  --mode <MODE>   normal, team or chaser (default normal); team and chaser need 4 participants
  --size <SIZE>   small, medium or large board (default small)
  --time <MIN>    round time in minutes: 2, 3, 5 or 10 (default 3)
  --friendly-fire <on|off>  Team mode: whether explosions hurt teammates (default on)
  -h, --help      show this help
  -V, --version   show the version

Host options:
  --web-port <N>  also serve the web page on this port so browsers can join
  --bind <IP>     address to listen on (default: game port on all interfaces,
                  web page on 127.0.0.1)
  --dev           developer mode: the owner can pause bots and set drop chances in
                  the web page (testing only)
  --room-name <TEXT>  name of the room (at most 24 characters; default: your name plus 's room)
  --mode, --size, --time, --friendly-fire  the room settings, as above
  --no-setup      do not show the room setup screen before the lobby (it is also skipped
                  when a room setting is given)

Network play: the host listens on port 7878 by default; up to 4 participants (humans and bots).
Join accepts host, host:port or a web address such as http://host:8080/ (the host is kept).
Lobby keys: host b add bot, x remove bot, Enter start; clients r ready. The room owner changes
the settings with m mode, s size, t time, f friendly fire. Anyone: q quit.
Keys: P1 arrows+Space, P2 WASD+E, P3 IJKL+O, P4 TFGH+Y; q quit, p pause, r rematch.";

/// Options of a local match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalOptions {
    pub players: u8,
    pub bots: u8,
    pub seed: Option<u64>,
    /// Show the lobby screen first.
    pub lobby: bool,
    /// Mode, board size, time and friendly fire of the match (the name is not used).
    pub room: RoomSettings,
}

/// Options of `host`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostOptions {
    pub port: u16,
    pub bots: u8,
    pub name: String,
    pub seed: Option<u64>,
    /// Also serve the web page on this port (1-65535).
    pub web_port: Option<u16>,
    /// Listening address; `None` keeps the defaults (game port on every interface, web page on loopback).
    pub bind: Option<IpAddr>,
    /// Developer mode: owner-only commands to pause bots and set drop chances.
    pub dev: bool,
    /// The room the host opens; the setup screen starts from it.
    pub room: RoomSettings,
    /// Whether a room setting was given on the command line.
    pub room_given: bool,
    /// Skip the room setup screen.
    pub no_setup: bool,
}

/// Options of `join`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JoinOptions {
    /// `host:port`; the default port is added when missing.
    pub addr: String,
    pub name: String,
}

/// What the user asked the program to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Run(LocalOptions),
    Host(HostOptions),
    Join(JoinOptions),
    Help,
    Version,
}

/// A mistake in the command line; the process exits with code 2.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UsageError {
    UnknownOption(String),
    MissingValue(String),
    InvalidValue {
        option: String,
        value: String,
    },
    /// A value outside a short list of choices; `allowed` names them.
    InvalidChoice {
        option: String,
        value: String,
        allowed: &'static str,
    },
    BadParticipants {
        players: u8,
        bots: u8,
    },
    /// The mode needs more participants than the command line gives.
    ModeNeedsPlayers {
        mode: &'static str,
        needed: u8,
    },
    MissingAddress,
}

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UsageError::UnknownOption(o) => write!(f, "unknown option '{o}'"),
            UsageError::MissingValue(o) => write!(f, "option '{o}' needs a value"),
            UsageError::InvalidValue { option, value } => {
                write!(f, "invalid value '{value}' for '{option}'")
            }
            UsageError::InvalidChoice {
                option,
                value,
                allowed,
            } => write!(
                f,
                "invalid value '{value}' for '{option}' (allowed: {allowed})"
            ),
            UsageError::BadParticipants { players, bots } => write!(
                f,
                "players ({players}) + bots ({bots}) must be between 1 and 4"
            ),
            UsageError::ModeNeedsPlayers { mode, needed } => write!(
                f,
                "mode '{mode}' needs {needed} participants: add players or bots"
            ),
            UsageError::MissingAddress => write!(f, "join needs the address of the host"),
        }
    }
}

impl std::error::Error for UsageError {}

/// Parses the arguments after the program name.
pub fn parse<I>(args: I) -> Result<Command, UsageError>
where
    I: IntoIterator<Item = String>,
{
    let mut it = args.into_iter().peekable();
    match it.peek().map(String::as_str) {
        Some("host") => {
            it.next();
            parse_host(it)
        }
        Some("join") => {
            it.next();
            parse_join(it)
        }
        _ => parse_local(it),
    }
}

fn parse_local(mut it: impl Iterator<Item = String>) -> Result<Command, UsageError> {
    let mut players = 1;
    let mut bots = None;
    let mut seed = None;
    let mut lobby = false;
    let mut room = RoomSettings::default();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "--players" => players = count(&arg, it.next(), 4)?,
            "--bots" => bots = Some(count(&arg, it.next(), 4)?),
            "--lobby" => lobby = true,
            "--seed" => seed = Some(number(&arg, it.next())?),
            _ if room_option(&arg, &mut it, &mut room)? => {}
            _ => return Err(UsageError::UnknownOption(arg)),
        }
    }
    let bots = bots.unwrap_or(4 - players);
    let total = u16::from(players) + u16::from(bots);
    let min = if lobby { 0 } else { 1 };
    if !(min..=4).contains(&total) {
        return Err(UsageError::BadParticipants { players, bots });
    }
    let (needed, _) = room.participants_needed();
    if !lobby && total < u16::from(needed) {
        return Err(UsageError::ModeNeedsPlayers {
            mode: room.mode.name(),
            needed,
        });
    }
    Ok(Command::Run(LocalOptions {
        players,
        bots,
        seed,
        lobby,
        room,
    }))
}

fn parse_host(mut it: impl Iterator<Item = String>) -> Result<Command, UsageError> {
    let mut opts = HostOptions {
        port: DEFAULT_PORT,
        bots: 0,
        name: "Host".to_string(),
        seed: None,
        web_port: None,
        bind: None,
        dev: false,
        room: RoomSettings::default(),
        room_given: false,
        no_setup: false,
    };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--port" => opts.port = number(&arg, it.next())?,
            "--bots" => opts.bots = count(&arg, it.next(), 3)?,
            "--name" => opts.name = name(&arg, it.next())?,
            "--seed" => opts.seed = Some(number(&arg, it.next())?),
            "--web-port" => opts.web_port = Some(web_port(&arg, it.next())?),
            "--bind" => opts.bind = Some(number(&arg, it.next())?),
            "--dev" => opts.dev = true,
            "--no-setup" => opts.no_setup = true,
            "--room-name" => {
                opts.room.name = value(&arg, it.next())?;
                opts.room_given = true;
            }
            _ if room_option(&arg, &mut it, &mut opts.room)? => opts.room_given = true,
            _ => return Err(UsageError::UnknownOption(arg)),
        }
    }
    opts.room = opts.room.clean();
    Ok(Command::Host(opts))
}

fn parse_join(mut it: impl Iterator<Item = String>) -> Result<Command, UsageError> {
    let mut addr = None;
    let mut player = "Player".to_string();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--name" => player = name(&arg, it.next())?,
            a if a.starts_with('-') => return Err(UsageError::UnknownOption(arg)),
            _ if addr.is_none() => addr = Some(arg),
            _ => return Err(UsageError::UnknownOption(arg)),
        }
    }
    let addr = addr.ok_or(UsageError::MissingAddress)?;
    let addr = match strip_scheme(&addr) {
        Some(host) => format!("{host}:{DEFAULT_PORT}"),
        None if addr.contains(':') => addr,
        None => format!("{addr}:{DEFAULT_PORT}"),
    };
    Ok(Command::Join(JoinOptions { addr, name: player }))
}

/// For a web address (`http://host:8080/page`) the host alone: its port is the web port, not
/// the game port. `None` when the address has no scheme.
fn strip_scheme(addr: &str) -> Option<&str> {
    let rest = ["http://", "https://", "ws://", "wss://"]
        .iter()
        .find_map(|s| addr.strip_prefix(s))?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    if authority.starts_with('[') {
        let end = authority.find(']')?;
        return Some(&authority[..=end]);
    }
    Some(authority.split(':').next().unwrap_or(authority))
}

fn web_port(option: &str, v: Option<String>) -> Result<u16, UsageError> {
    let v = value(option, v)?;
    match v.parse::<u16>() {
        Ok(p) if p > 0 => Ok(p),
        _ => Err(invalid(option, v)),
    }
}

/// Reads `--mode`, `--size`, `--time` and `--friendly-fire`; `false` when `arg` is another option.
fn room_option(
    arg: &str,
    it: &mut impl Iterator<Item = String>,
    room: &mut RoomSettings,
) -> Result<bool, UsageError> {
    match arg {
        "--mode" => {
            let v = value(arg, it.next())?;
            room.mode =
                Mode::from_name(&v).ok_or_else(|| choice(arg, v, "normal, team, chaser"))?;
        }
        "--size" => {
            let v = value(arg, it.next())?;
            room.size =
                BoardSize::from_name(&v).ok_or_else(|| choice(arg, v, "small, medium, large"))?;
        }
        "--time" => {
            let v = value(arg, it.next())?;
            room.minutes = match v.parse::<u8>() {
                Ok(m) if MINUTES.contains(&m) => m,
                _ => return Err(choice(arg, v, "2, 3, 5, 10")),
            };
        }
        "--friendly-fire" => {
            let v = value(arg, it.next())?;
            room.friendly_fire = match v.as_str() {
                "on" => true,
                "off" => false,
                _ => return Err(choice(arg, v, "on, off")),
            };
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn choice(option: &str, v: String, allowed: &'static str) -> UsageError {
    UsageError::InvalidChoice {
        option: option.to_string(),
        value: v,
        allowed,
    }
}

fn value(option: &str, v: Option<String>) -> Result<String, UsageError> {
    v.ok_or_else(|| UsageError::MissingValue(option.to_string()))
}

fn invalid(option: &str, v: String) -> UsageError {
    UsageError::InvalidValue {
        option: option.to_string(),
        value: v,
    }
}

fn number<T: std::str::FromStr>(option: &str, v: Option<String>) -> Result<T, UsageError> {
    let v = value(option, v)?;
    v.parse().map_err(|_| invalid(option, v))
}

fn count(option: &str, v: Option<String>, max: u8) -> Result<u8, UsageError> {
    let v = value(option, v)?;
    match v.parse::<u8>() {
        Ok(n) if n <= max => Ok(n),
        _ => Err(invalid(option, v)),
    }
}

fn name(option: &str, v: Option<String>) -> Result<String, UsageError> {
    let v = value(option, v)?;
    if valid_name(&v) {
        Ok(v)
    } else {
        Err(invalid(option, v))
    }
}
