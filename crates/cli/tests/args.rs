use bomberust_cli::args::{parse, Command, LocalOptions, UsageError};

fn args(list: &[&str]) -> Result<Command, UsageError> {
    parse(list.iter().map(|s| s.to_string()))
}

#[test]
fn defaults_to_one_human() {
    assert_eq!(
        args(&[]),
        Ok(Command::Run(LocalOptions {
            players: 1,
            bots: 0,
            seed: None
        }))
    );
}

#[test]
fn parses_players_bots_and_seed() {
    assert_eq!(
        args(&["--players", "2", "--bots", "2", "--seed", "42"]),
        Ok(Command::Run(LocalOptions {
            players: 2,
            bots: 2,
            seed: Some(42)
        }))
    );
}

#[test]
fn too_many_participants_is_a_usage_error() {
    assert_eq!(
        args(&["--players", "3", "--bots", "2"]),
        Err(UsageError::BadParticipants {
            players: 3,
            bots: 2
        })
    );
}

#[test]
fn nobody_playing_is_a_usage_error() {
    assert!(matches!(
        args(&["--players", "0"]),
        Err(UsageError::BadParticipants {
            players: 0,
            bots: 0
        })
    ));
}

#[test]
fn bots_only_is_allowed() {
    assert!(args(&["--players", "0", "--bots", "4"]).is_ok());
}

#[test]
fn rejects_bad_values_and_unknown_options() {
    assert!(matches!(
        args(&["--players", "x"]),
        Err(UsageError::InvalidValue { .. })
    ));
    assert!(matches!(
        args(&["--players", "9"]),
        Err(UsageError::InvalidValue { .. })
    ));
    assert!(matches!(
        args(&["--seed", "-1"]),
        Err(UsageError::InvalidValue { .. })
    ));
    assert!(matches!(
        args(&["--bots"]),
        Err(UsageError::MissingValue(_))
    ));
    assert!(matches!(
        args(&["--nope"]),
        Err(UsageError::UnknownOption(_))
    ));
}

#[test]
fn help_and_version() {
    assert_eq!(args(&["--help"]), Ok(Command::Help));
    assert_eq!(args(&["-h"]), Ok(Command::Help));
    assert_eq!(args(&["--version"]), Ok(Command::Version));
}
