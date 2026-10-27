//! Command-line options of `eksplode-re-server`.

use std::fmt;
use std::net::IpAddr;

use eksplode_re_core::{BoardSize, Mode, MINUTES};

use crate::options::ServerOptions;
use crate::origin::AllowedOrigins;

pub const USAGE: &str = "\
Usage: eksplode-re-server [OPTIONS]

Options:
  --bind <IP>              address to listen on (default 0.0.0.0)
  --http-port <N>          web page and WebSocket port (default 8080)
  --game-port <N>          terminal client port (default 7878)
  --bots <N>               bots to add to the lobby, 0-3 (default 0)
  --seed <U64>             fixed RNG seed for every match (default: random)
  --allow-origin <ORIGIN>  accept WebSocket connections from this web origin (repeatable)
  --allow-any-origin       accept WebSocket connections from any web origin (unsafe)
  --max-connections <N>    open HTTP/WebSocket connections (default 64, minimum 4)
  --room-name <TEXT>       default room name, at most 24 characters (default: the owner's name plus 's room)
  --mode <MODE>            default mode: normal, team or chaser (default normal)
  --size <SIZE>            default board size: small, medium or large (default small)
  --time <MIN>             default round time in minutes: 2, 3, 5 or 10 (default 3)
  --friendly-fire <on|off>  default friendly fire for Team mode (default on)
  --dev                    developer mode: owner-only commands to pause bots and set drop chances (testing only)
  -h, --help               show this help
  -V, --version            show the version

By default only the page served by this server may connect, and only from this machine.
The game has no encryption and no authentication: see docs/SECURITY.md.";

/// What the user asked the program to do.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)] // built once at start-up
pub enum Command {
    Run(ServerOptions),
    Help,
    Version,
}

impl PartialEq for Command {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Command::Help, Command::Help) | (Command::Version, Command::Version)
        )
    }
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
        }
    }
}

impl std::error::Error for UsageError {}

/// Parses the arguments after the program name.
pub fn parse<I>(args: I) -> Result<Command, UsageError>
where
    I: IntoIterator<Item = String>,
{
    let mut opts = ServerOptions::default();
    let mut listed = Vec::new();
    let mut any = false;
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "--bind" => opts.bind = parse_value::<IpAddr>(&arg, it.next())?,
            "--http-port" => opts.http_port = port(&arg, it.next())?,
            "--game-port" => opts.game_port = port(&arg, it.next())?,
            "--bots" => {
                let v = value(&arg, it.next())?;
                opts.bots = match v.parse::<u8>() {
                    Ok(n) if n <= 3 => n,
                    _ => return Err(invalid(&arg, v)),
                };
            }
            "--seed" => opts.seed = Some(parse_value(&arg, it.next())?),
            "--allow-origin" => {
                let v = value(&arg, it.next())?;
                let has_scheme = v.split_once("://").is_some_and(|(s, rest)| {
                    !s.is_empty() && !rest.is_empty() && !rest.contains('/')
                });
                if !has_scheme {
                    return Err(invalid(&arg, v));
                }
                listed.push(v);
            }
            "--allow-any-origin" => any = true,
            "--dev" => opts.dev = true,
            "--room-name" => opts.room.name = value(&arg, it.next())?,
            "--mode" => {
                let v = value(&arg, it.next())?;
                opts.room.mode =
                    Mode::from_name(&v).ok_or_else(|| choice(&arg, v, "normal, team, chaser"))?;
            }
            "--size" => {
                let v = value(&arg, it.next())?;
                opts.room.size = BoardSize::from_name(&v)
                    .ok_or_else(|| choice(&arg, v, "small, medium, large"))?;
            }
            "--time" => {
                let v = value(&arg, it.next())?;
                opts.room.minutes = match v.parse::<u8>() {
                    Ok(m) if MINUTES.contains(&m) => m,
                    _ => return Err(choice(&arg, v, "2, 3, 5, 10")),
                };
            }
            "--friendly-fire" => {
                let v = value(&arg, it.next())?;
                opts.room.friendly_fire = match v.as_str() {
                    "on" => true,
                    "off" => false,
                    _ => return Err(choice(&arg, v, "on, off")),
                };
            }
            "--max-connections" => {
                let v = value(&arg, it.next())?;
                opts.max_connections = match v.parse::<usize>() {
                    Ok(n) if n >= 4 => n,
                    _ => return Err(invalid(&arg, v)),
                };
            }
            _ => return Err(UsageError::UnknownOption(arg)),
        }
    }
    opts.room = opts.room.clone().clean();
    opts.origins = if any {
        AllowedOrigins::Any
    } else if listed.is_empty() {
        AllowedOrigins::SameOrigin
    } else {
        AllowedOrigins::List(listed)
    };
    Ok(Command::Run(opts))
}

fn value(option: &str, v: Option<String>) -> Result<String, UsageError> {
    v.ok_or_else(|| UsageError::MissingValue(option.to_string()))
}

fn choice(option: &str, v: String, allowed: &'static str) -> UsageError {
    UsageError::InvalidChoice {
        option: option.to_string(),
        value: v,
        allowed,
    }
}

fn invalid(option: &str, v: String) -> UsageError {
    UsageError::InvalidValue {
        option: option.to_string(),
        value: v,
    }
}

fn parse_value<T: std::str::FromStr>(option: &str, v: Option<String>) -> Result<T, UsageError> {
    let v = value(option, v)?;
    v.parse().map_err(|_| invalid(option, v))
}

/// A port from 1 to 65535.
fn port(option: &str, v: Option<String>) -> Result<u16, UsageError> {
    let v = value(option, v)?;
    match v.parse::<u16>() {
        Ok(p) if p != 0 => Ok(p),
        _ => Err(invalid(option, v)),
    }
}
