#![allow(clippy::unwrap_used, clippy::expect_used)]

use eksplode_re_cli::args::{parse, Command, LocalOptions, UsageError};
use eksplode_re_cli::input::{default_key_bindings, KeyBindings};
use ratatui::crossterm::event::KeyCode;

fn args(list: &[&str]) -> Result<Command, UsageError> {
    parse(list.iter().map(|s| s.to_string()))
}

/// `local` is now an explicit subcommand (bare invocation defaults to `host`, see
/// `defaults_to_hosting_a_game`), so local-play option tests route through it.
fn local(list: &[&str]) -> Result<Command, UsageError> {
    let mut full = vec!["local"];
    full.extend_from_slice(list);
    args(&full)
}

fn opts(players: u8, bots: u8, seed: Option<u64>, lobby: bool) -> Result<Command, UsageError> {
    Ok(Command::Run(LocalOptions {
        players,
        bots,
        seed,
        lobby,
        room: Default::default(),
        key_bindings: default_key_bindings(),
    }))
}

#[test]
fn defaults_to_hosting_a_game() {
    assert!(matches!(args(&[]), Ok(Command::Host(_))));
}

#[test]
fn local_defaults_to_one_human_and_three_bots_and_shows_the_lobby_first() {
    assert_eq!(local(&[]), opts(1, 3, None, true));
}

#[test]
fn bots_default_to_filling_the_match() {
    assert_eq!(local(&["--players", "2"]), opts(2, 2, None, true));
    assert_eq!(local(&["--players", "4"]), opts(4, 0, None, true));
    assert_eq!(local(&["--players", "0"]), opts(0, 4, None, true));
}

#[test]
fn parses_players_bots_seed_and_lobby() {
    assert_eq!(
        local(&["--players", "2", "--bots", "1", "--seed", "42", "--lobby"]),
        opts(2, 1, Some(42), true)
    );
    assert_eq!(local(&["--bots", "0"]), opts(1, 0, None, true));
}

#[test]
fn instant_skips_the_lobby_screen() {
    assert_eq!(
        local(&["--players", "2", "--bots", "2", "--instant"]),
        opts(2, 2, None, false)
    );
    // the last of --lobby/--instant on the command line wins
    assert_eq!(local(&["--instant", "--lobby"]), opts(1, 3, None, true));
    assert_eq!(local(&["--lobby", "--instant"]), opts(1, 3, None, false));
}

#[test]
fn too_many_participants_is_a_usage_error() {
    assert_eq!(
        local(&["--players", "3", "--bots", "2"]),
        Err(UsageError::BadParticipants {
            players: 3,
            bots: 2
        })
    );
}

#[test]
fn nobody_playing_instantly_is_a_usage_error() {
    assert_eq!(
        local(&["--players", "0", "--bots", "0", "--instant"]),
        Err(UsageError::BadParticipants {
            players: 0,
            bots: 0
        })
    );
}

#[test]
fn nobody_playing_yet_is_fine_since_the_lobby_shows_by_default() {
    assert_eq!(
        local(&["--players", "0", "--bots", "0"]),
        opts(0, 0, None, true)
    );
}

#[test]
fn rejects_bad_values_and_unknown_options() {
    assert!(matches!(
        local(&["--players", "x"]),
        Err(UsageError::InvalidValue { .. })
    ));
    assert!(matches!(
        local(&["--players", "9"]),
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
        local(&["--players", "0", "--bots", "0", "--lobby"]),
        opts(0, 0, None, true)
    );
}

mod network {
    use super::*;
    use eksplode_re_cli::args::{HostOptions, JoinOptions};

    fn host(port: u16, bots: u8, name: &str, seed: Option<u64>) -> Result<Command, UsageError> {
        Ok(Command::Host(HostOptions {
            port,
            bots,
            name: name.into(),
            seed,
            web_port: None,
            bind: None,
            dev: false,
            room: Default::default(),
            room_given: false,
            no_setup: false,
            server_id: None,
            limits: Default::default(),
        }))
    }

    #[test]
    fn f3_us4_as6_host_accepts_dev_and_it_is_off_by_default() {
        let Ok(Command::Host(o)) = args(&["host", "--dev"]) else {
            panic!("host --dev must parse");
        };
        assert!(o.dev);
        let Ok(Command::Host(o)) = args(&["host", "--web-port", "8080", "--dev", "--bots", "1"])
        else {
            panic!("host --dev with other options must parse");
        };
        assert!(o.dev && o.bots == 1 && o.web_port == Some(8080));
        let Ok(Command::Host(o)) = args(&["host"]) else {
            panic!();
        };
        assert!(!o.dev);
        assert!(eksplode_re_cli::args::USAGE.contains("--dev"));
    }

    fn join(addr: &str, name: &str) -> Result<Command, UsageError> {
        Ok(Command::Join(JoinOptions {
            addr: addr.into(),
            name: name.into(),
            key_bindings: KeyBindings::default_for_slot(0),
            room_id: None,
        }))
    }

    #[test]
    fn joining_a_specific_room_is_explicit_and_bounded() {
        let room_id = "ab".repeat(16);
        let Ok(Command::Join(options)) = args(&["join", "127.0.0.1:7878", "--room-id", &room_id])
        else {
            panic!("explicit room selection must parse");
        };
        assert_eq!(options.room_id.as_deref(), Some(room_id.as_str()));
        for invalid in ["".to_string(), "not-a-room".into(), "ab".repeat(33)] {
            assert!(args(&["join", "127.0.0.1", "--room-id", &invalid]).is_err());
        }
    }

    #[test]
    fn host_defaults() {
        assert_eq!(args(&["host"]), host(7878, 0, "Host", None));
    }

    #[test]
    fn terminal_host_room_identity_and_limits_match_the_server() {
        let Ok(Command::Host(options)) = args(&[
            "host",
            "--server-id",
            "alpha",
            "--max-rooms",
            "128",
            "--max-connections",
            "1024",
        ]) else {
            panic!("room host options must parse");
        };
        assert_eq!(options.server_id.as_deref(), Some("alpha"));
        assert_eq!(
            (options.limits.max_rooms, options.limits.max_connections),
            (128, 1024)
        );
        for (flag, value) in [
            ("--server-id", "bad name"),
            ("--max-rooms", "0"),
            ("--max-rooms", "129"),
            ("--max-connections", "3"),
            ("--max-connections", "1025"),
        ] {
            assert!(args(&["host", flag, value]).is_err());
        }
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
        let Ok(Command::Host(o)) = args(&["host", "--web-port", "8080", "--bind", "0.0.0.0"])
        else {
            panic!("must parse");
        };
        assert_eq!(o.web_port, Some(8080));
        assert_eq!(o.bind, Some("0.0.0.0".parse().unwrap()));
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

    #[test]
    fn terminal_key_options_parse_six_bindings_and_keep_omitted_slot_defaults() {
        let Ok(Command::Run(defaults)) = local(&[]) else {
            panic!("expected default local options");
        };
        let Ok(Command::Run(configured)) = local(&["--p1-keys", "1,2,3,4,5,6"]) else {
            panic!("expected configured local options");
        };
        assert_eq!(configured.key_bindings[0].up, KeyCode::Char('1'));
        assert_eq!(configured.key_bindings[0].down, KeyCode::Char('2'));
        assert_eq!(configured.key_bindings[0].left, KeyCode::Char('3'));
        assert_eq!(configured.key_bindings[0].right, KeyCode::Char('4'));
        assert_eq!(configured.key_bindings[0].bomb, KeyCode::Char('5'));
        assert_eq!(configured.key_bindings[0].punch, KeyCode::Char('6'));
        assert_eq!(configured.key_bindings[1], defaults.key_bindings[1]);
    }

    #[test]
    fn joining_player_can_configure_only_their_own_keyboard_bindings() {
        let Ok(Command::Join(join)) =
            args(&["join", "host.lan", "--keys", "Up,Down,Left,Right,Space,z"])
        else {
            panic!("expected a joining client");
        };
        assert_eq!(join.key_bindings.up, KeyCode::Up);
        assert_eq!(join.key_bindings.bomb, KeyCode::Char(' '));
        assert_eq!(join.key_bindings.punch, KeyCode::Char('z'));
    }

    #[test]
    fn invalid_or_conflicting_key_binding_options_are_usage_errors() {
        for bad in [
            &["--p1-keys", "1,2,3"][..],
            &["--p1-keys", "1,2,3,4,5,Escape"][..],
            &["--p1-keys", "q,2,3,4,5,6"][..],
            &["--p1-keys", "1,2,3,4,5,6", "--p2-keys", "1,7,8,9,0,a"][..],
            &["join", "host", "--keys", "1,1,2,3,4,5"][..],
        ] {
            assert!(args(bad).is_err(), "accepted invalid bindings {bad:?}");
        }
    }
}

mod room_options {
    use super::*;
    use eksplode_re_cli::args::{HostOptions, USAGE};
    use eksplode_re_core::{BoardSize, Mode, RoomSettings};

    fn host_opts(list: &[&str]) -> HostOptions {
        let mut full = vec!["host"];
        full.extend_from_slice(list);
        match args(&full) {
            Ok(Command::Host(o)) => o,
            other => panic!("expected host options, got {other:?}"),
        }
    }

    fn local_opts(list: &[&str]) -> LocalOptions {
        match local(list) {
            Ok(Command::Run(o)) => o,
            other => panic!("expected local options, got {other:?}"),
        }
    }

    #[test]
    fn host_takes_every_room_setting() {
        let o = host_opts(&[
            "--room-name",
            "Friday",
            "--mode",
            "team",
            "--size",
            "medium",
            "--time",
            "5",
            "--friendly-fire",
            "off",
        ]);
        assert_eq!(o.room.name, "Friday");
        assert_eq!(o.room.mode, Mode::Team);
        assert_eq!(o.room.size, BoardSize::Medium);
        assert_eq!(o.room.minutes, 5);
        assert!(!o.room.friendly_fire);
        assert!(o.room_given);
        assert!(!o.no_setup);
    }

    #[test]
    fn host_without_settings_keeps_the_defaults_and_shows_the_setup() {
        let o = host_opts(&[]);
        assert_eq!(o.room, RoomSettings::default());
        assert!(!o.room_given && !o.no_setup);
        assert!(host_opts(&["--no-setup"]).no_setup);
        assert!(!host_opts(&["--no-setup"]).room_given);
    }

    #[test]
    fn the_local_game_takes_mode_size_time_and_friendly_fire() {
        let o = local_opts(&["--mode", "chaser", "--size", "large", "--time", "2"]);
        assert_eq!(o.room.mode, Mode::Chaser);
        assert_eq!(o.room.size, BoardSize::Large);
        assert_eq!(o.room.minutes, 2);
        let o = local_opts(&["--friendly-fire", "off"]);
        assert!(!o.room.friendly_fire);
    }

    #[test]
    fn the_local_game_has_no_room_name_and_no_setup_option() {
        for opt in ["--room-name", "--no-setup"] {
            assert!(matches!(
                local(&[opt, "x"]),
                Err(UsageError::UnknownOption(_))
            ));
        }
    }

    #[test]
    fn a_mode_that_needs_four_participants_is_checked_for_the_local_game() {
        assert!(matches!(
            local(&[
                "--mode",
                "team",
                "--players",
                "1",
                "--bots",
                "1",
                "--instant"
            ]),
            Err(UsageError::ModeNeedsPlayers { needed: 4, .. })
        ));
        // the lobby screen lets you add participants before starting, so it is not checked here
        assert!(local(&["--mode", "team", "--players", "1", "--bots", "1"]).is_ok());
        assert!(local(&["--mode", "team", "--players", "2", "--bots", "2"]).is_ok());
        assert!(local(&["--mode", "team", "--lobby", "--players", "0", "--bots", "0"]).is_ok());
    }

    #[test]
    fn bad_values_list_the_allowed_ones() {
        for (list, allowed) in [
            (&["--mode", "battle"][..], "normal, team, chaser"),
            (&["--size", "huge"], "small, medium, large"),
            (&["--time", "4"], "2, 3, 5, 10"),
            (&["--friendly-fire", "maybe"], "on, off"),
        ] {
            let err = args(list).unwrap_err();
            assert!(err.to_string().contains(allowed), "{list:?}: {err}");
            let mut host = vec!["host"];
            host.extend_from_slice(list);
            let err = args(&host).unwrap_err();
            assert!(err.to_string().contains(allowed), "host {list:?}: {err}");
        }
        assert!(matches!(
            args(&["host", "--room-name"]),
            Err(UsageError::MissingValue(_))
        ));
    }

    #[test]
    fn the_help_text_mentions_every_option() {
        for flag in [
            "--room-name",
            "--mode",
            "--size",
            "--time",
            "--friendly-fire",
            "--no-setup",
        ] {
            assert!(USAGE.contains(flag), "{flag} missing from --help");
        }
    }
}
