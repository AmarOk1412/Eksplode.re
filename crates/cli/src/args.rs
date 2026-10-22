//! Command-line argument parsing; see specs/001-bomberman-multiplayer/contracts/cli-interface.md.

use std::fmt;

pub const USAGE: &str = "\
Usage: bomberust-cli [OPTIONS]

Options:
  --players <N>   number of local human players, 0-4 (default 1)
  --bots <N>      number of bots, 0-4 (default: fill up to 4 participants)
  --seed <U64>    RNG seed (default: from the system time)
  --lobby         choose the slots on a lobby screen before the game starts
  -h, --help      show this help
  -V, --version   show the version

players + bots must be between 1 and 4 (0 is fine with --lobby).
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

/// What the user asked the program to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Run(LocalOptions),
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
        }
    }
}

impl std::error::Error for UsageError {}

/// Parses the arguments after the program name.
pub fn parse<I>(args: I) -> Result<Command, UsageError>
where
    I: IntoIterator<Item = String>,
{
    let mut players = 1;
    let mut bots = None;
    let mut seed = None;
    let mut lobby = false;
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "--players" => players = count(&arg, it.next())?,
            "--bots" => bots = Some(count(&arg, it.next())?),
            "--lobby" => lobby = true,
            "--seed" => {
                let v = it
                    .next()
                    .ok_or_else(|| UsageError::MissingValue(arg.clone()))?;
                seed = Some(v.parse().map_err(|_| UsageError::InvalidValue {
                    option: arg.clone(),
                    value: v,
                })?);
            }
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

fn count(option: &str, value: Option<String>) -> Result<u8, UsageError> {
    let v = value.ok_or_else(|| UsageError::MissingValue(option.to_string()))?;
    match v.parse::<u8>() {
        Ok(n) if n <= 4 => Ok(n),
        _ => Err(UsageError::InvalidValue {
            option: option.to_string(),
            value: v,
        }),
    }
}
