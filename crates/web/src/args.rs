//! Command-line options of `bomberust-server`.

use std::fmt;
use std::net::IpAddr;

use crate::options::ServerOptions;
use crate::origin::AllowedOrigins;

pub const USAGE: &str = "\
Usage: bomberust-server [OPTIONS]

Options:
  --bind <IP>              address to listen on (default 127.0.0.1)
  --http-port <N>          web page and WebSocket port (default 8080)
  --game-port <N>          terminal client port (default 7878)
  --bots <N>               bots to add to the lobby, 0-3 (default 0)
  --seed <U64>             fixed RNG seed for every match (default: random)
  --allow-origin <ORIGIN>  accept WebSocket connections from this web origin (repeatable)
  --allow-any-origin       accept WebSocket connections from any web origin (unsafe)
  --max-connections <N>    open HTTP/WebSocket connections (default 64, minimum 4)
  --dev                    developer mode: owner-only commands to pause bots and set drop chances (testing only)
  -h, --help               show this help
  -V, --version            show the version

By default only the page served by this server may connect, and only from this machine.
The game has no encryption and no authentication: see docs/SECURITY.md.";

/// What the user asked the program to do.
#[derive(Clone, Debug)]
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
    InvalidValue { option: String, value: String },
}

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UsageError::UnknownOption(o) => write!(f, "unknown option '{o}'"),
            UsageError::MissingValue(o) => write!(f, "option '{o}' needs a value"),
            UsageError::InvalidValue { option, value } => {
                write!(f, "invalid value '{value}' for '{option}'")
            }
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
