use bomberust_cli::args::{parse, Command, LocalOptions, UsageError};

fn args(list: &[&str]) -> Result<Command, UsageError> {
    parse(list.iter().map(|s| s.to_string()))
}

fn opts(players: u8, bots: u8, seed: Option<u64>, lobby: bool) -> Result<Command, UsageError> {
    Ok(Command::Run(LocalOptions {
        players,
        bots,
        seed,
        lobby,
    }))
}

#[test]
fn defaults_to_one_human_and_three_bots() {
    assert_eq!(args(&[]), opts(1, 3, None, false));
}

#[test]
fn bots_default_to_filling_the_match() {
    assert_eq!(args(&["--players", "2"]), opts(2, 2, None, false));
    assert_eq!(args(&["--players", "4"]), opts(4, 0, None, false));
    assert_eq!(args(&["--players", "0"]), opts(0, 4, None, false));
}

#[test]
fn parses_players_bots_seed_and_lobby() {
    assert_eq!(
        args(&["--players", "2", "--bots", "1", "--seed", "42", "--lobby"]),
        opts(2, 1, Some(42), true)
    );
    assert_eq!(args(&["--bots", "0"]), opts(1, 0, None, false));
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
    assert_eq!(
        args(&["--players", "0", "--bots", "0"]),
        Err(UsageError::BadParticipants {
            players: 0,
            bots: 0
        })
    );
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

#[test]
fn an_empty_start_is_fine_with_the_lobby() {
    assert_eq!(
        args(&["--players", "0", "--bots", "0", "--lobby"]),
        opts(0, 0, None, true)
    );
}
