#![allow(clippy::unwrap_used, clippy::expect_used)]

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

mod network {
    use super::*;
    use bomberust_cli::args::{HostOptions, JoinOptions};

    fn host(port: u16, bots: u8, name: &str, seed: Option<u64>) -> Result<Command, UsageError> {
        Ok(Command::Host(HostOptions {
            port,
            bots,
            name: name.into(),
            seed,
            web_port: None,
            bind: None,
        }))
    }

    fn join(addr: &str, name: &str) -> Result<Command, UsageError> {
        Ok(Command::Join(JoinOptions {
            addr: addr.into(),
            name: name.into(),
        }))
    }

    #[test]
    fn host_defaults() {
        assert_eq!(args(&["host"]), host(7878, 0, "Host", None));
    }

    #[test]
    fn host_options() {
        assert_eq!(
            args(&["host", "--port", "9000", "--bots", "2", "--name", "Ann", "--seed", "5"]),
            host(9000, 2, "Ann", Some(5))
        );
    }

    #[test]
    fn us3_as1_host_accepts_a_web_port_and_a_bind_address() {
        let Ok(Command::Host(o)) = args(&["host", "--web-port", "8080", "--bind", "127.0.0.1"])
        else {
            panic!("must parse");
        };
        assert_eq!(o.web_port, Some(8080));
        assert_eq!(o.bind, Some("127.0.0.1".parse().unwrap()));
        assert_eq!((o.port, o.bots, o.name.as_str()), (7878, 0, "Host"));
        let Ok(Command::Host(o)) = args(&["host", "--bind", "::1"]) else {
            panic!("must parse");
        };
        assert_eq!(o.bind, Some("::1".parse().unwrap()));
    }

    #[test]
    fn invalid_web_ports_and_addresses_are_usage_errors() {
        for bad in [
            ["host", "--web-port", "0"],
            ["host", "--web-port", "70000"],
            ["host", "--web-port", "x"],
            ["host", "--bind", "nope"],
            ["host", "--bind", "300.1.1.1"],
        ] {
            assert!(
                matches!(args(&bad), Err(UsageError::InvalidValue { .. })),
                "{bad:?}"
            );
        }
        assert!(matches!(
            args(&["host", "--web-port"]),
            Err(UsageError::MissingValue(_))
        ));
    }

    #[test]
    fn join_accepts_web_style_addresses_and_keeps_only_the_host() {
        // the port of a web address is the web port, not the game port
        for (given, want) in [
            ("http://192.168.1.5:8080/", "192.168.1.5:7878"),
            ("https://play.example", "play.example:7878"),
            ("ws://host.lan/ws", "host.lan:7878"),
            ("http://[::1]:8080/", "[::1]:7878"),
        ] {
            assert_eq!(args(&["join", given]), join(want, "Player"), "{given}");
        }
    }

    #[test]
    fn host_rejects_bad_values() {
        assert!(matches!(
            args(&["host", "--port", "70000"]),
            Err(UsageError::InvalidValue { .. })
        ));
        assert!(matches!(
            args(&["host", "--bots", "4"]),
            Err(UsageError::InvalidValue { .. })
        ));
        assert!(matches!(
            args(&["host", "--name", ""]),
            Err(UsageError::InvalidValue { .. })
        ));
        assert!(matches!(
            args(&["host", "--name", "this name is much too long"]),
            Err(UsageError::InvalidValue { .. })
        ));
        assert!(matches!(
            args(&["host", "--players", "2"]),
            Err(UsageError::UnknownOption(_))
        ));
    }

    #[test]
    fn join_needs_an_address_and_adds_the_default_port() {
        assert_eq!(args(&["join"]), Err(UsageError::MissingAddress));
        assert_eq!(
            args(&["join", "192.168.1.5"]),
            join("192.168.1.5:7878", "Player")
        );
        assert_eq!(
            args(&["join", "host.lan:9000", "--name", "Bob"]),
            join("host.lan:9000", "Bob")
        );
        assert_eq!(
            args(&["join", "--name", "Bob", "1.2.3.4:1"]),
            join("1.2.3.4:1", "Bob")
        );
    }

    #[test]
    fn join_rejects_extra_arguments() {
        assert!(matches!(
            args(&["join", "a", "b"]),
            Err(UsageError::UnknownOption(_))
        ));
        assert!(matches!(
            args(&["join", "a", "--bots", "1"]),
            Err(UsageError::UnknownOption(_))
        ));
    }
}
