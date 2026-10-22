//! Command-line argument parsing; see specs/001-bomberman-multiplayer/contracts/cli-interface.md.

use std::fmt;

use bomberust_net::protocol::{valid_name, DEFAULT_PORT};

pub const USAGE: &str = "\
Usage:
  bomberust-cli [OPTIONS]                          play on this machine
  bomberust-cli host [--port N] [--bots N] [--name NAME] [--seed N]
  bomberust-cli join <ADDR[:PORT]> [--name NAME]

Local options:
  --players <N>   number of local human players, 0-4 (default 1)
  --bots <N>      number of bots, 0-4 (default: fill up to 4 participants)
  --seed <U64>    RNG seed (default: from the system time)
  --lobby         choose the slots on a lobby screen before the game starts
  -h, --help      show this help
  -V, --version   show the version

Network play: the host listens on port 7878 by default; up to 4 participants (humans and bots).
Lobby keys: host b add bot, x remove bot, Enter start; clients r ready. Anyone: q quit.
Keys: P1 arrows+Space, P2 WASD+E, P3 IJKL+O, P4 TFGH+Y; q quit, p pause, r rematch.";

/// Options of a local match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalOptions {
    pub players: u8,
    pub bots: u8,
    pub seed: Option<u64>,
    /// Show the lobby screen first.
    pub lobby: bool,
}

/// Options of `host`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostOptions {
    pub port: u16,
    pub bots: u8,
    pub name: String,
    pub seed: Option<u64>,
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
    InvalidValue { option: String, value: String },
    BadParticipants { players: u8, bots: u8 },
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
            UsageError::BadParticipants { players, bots } => write!(
                f,
                "players ({players}) + bots ({bots}) must be between 1 and 4"
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
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "--players" => players = count(&arg, it.next(), 4)?,
            "--bots" => bots = Some(count(&arg, it.next(), 4)?),
            "--lobby" => lobby = true,
            "--seed" => seed = Some(number(&arg, it.next())?),
            _ => return Err(UsageError::UnknownOption(arg)),
        }
    }
    let bots = bots.unwrap_or(4 - players);
    let total = u16::from(players) + u16::from(bots);
    let min = if lobby { 0 } else { 1 };
    if !(min..=4).contains(&total) {
        return Err(UsageError::BadParticipants { players, bots });
    }
    Ok(Command::Run(LocalOptions {
        players,
        bots,
        seed,
        lobby,
    }))
}

fn parse_host(mut it: impl Iterator<Item = String>) -> Result<Command, UsageError> {
    let mut opts = HostOptions {
        port: DEFAULT_PORT,
        bots: 0,
        name: "Host".to_string(),
        seed: None,
    };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--port" => opts.port = number(&arg, it.next())?,
            "--bots" => opts.bots = count(&arg, it.next(), 3)?,
            "--name" => opts.name = name(&arg, it.next())?,
            "--seed" => opts.seed = Some(number(&arg, it.next())?),
            _ => return Err(UsageError::UnknownOption(arg)),
        }
    }
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
    let addr = if addr.contains(':') {
        addr
    } else {
        format!("{addr}:{DEFAULT_PORT}")
    };
    Ok(Command::Join(JoinOptions { addr, name: player }))
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
