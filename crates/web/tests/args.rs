use std::net::{IpAddr, Ipv4Addr};

use eksplode_re_web::args::{parse, Command, UsageError};
use eksplode_re_web::origin::AllowedOrigins;
use eksplode_re_web::ServerOptions;

fn args(list: &[&str]) -> Result<Command, UsageError> {
    parse(list.iter().map(|s| s.to_string()))
}

fn opts(list: &[&str]) -> ServerOptions {
    match args(list) {
        Ok(Command::Run(o)) => o,
        other => panic!("expected options, got {other:?}"),
    }
}

#[test]
fn defaults() {
    let o = opts(&[]);
    assert_eq!(o.bind, IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert_eq!(o.http_port, 8080);
    assert_eq!(o.game_port, 7878);
    assert_eq!(o.bots, 0);
    assert_eq!(o.seed, None);
    assert_eq!(o.origins, AllowedOrigins::SameOrigin);
    assert_eq!(o.max_connections, 64);
}

#[test]
fn every_option_is_parsed() {
    let o = opts(&[
        "--bind",
        "0.0.0.0",
        "--http-port",
        "9000",
        "--game-port",
        "9001",
        "--bots",
        "2",
        "--seed",
        "7",
        "--max-connections",
        "10",
    ]);
    assert_eq!(o.bind, "0.0.0.0".parse::<IpAddr>().unwrap());
    assert_eq!(
        (o.http_port, o.game_port, o.bots, o.seed, o.max_connections),
        (9000, 9001, 2, Some(7), 10)
    );
}

#[test]
fn origins_are_collected() {
    assert_eq!(
        opts(&["--allow-origin", "http://a.example"]).origins,
        AllowedOrigins::List(vec!["http://a.example".into()])
    );
    assert_eq!(opts(&["--allow-any-origin"]).origins, AllowedOrigins::Any);
    assert_eq!(
        opts(&["--allow-any-origin", "--allow-origin", "http://a.example"]).origins,
        AllowedOrigins::Any,
        "any wins over a list"
    );
}

#[test]
fn ports_must_be_1_to_65535() {
    for bad in ["0", "65536", "-1", "abc", ""] {
        assert!(
            matches!(
                args(&["--http-port", bad]),
                Err(UsageError::InvalidValue { .. })
            ),
            "{bad}"
        );
        assert!(
            matches!(
                args(&["--game-port", bad]),
                Err(UsageError::InvalidValue { .. })
            ),
            "{bad}"
        );
    }
    assert!(args(&["--http-port", "1"]).is_ok() && args(&["--game-port", "65535"]).is_ok());
}

#[test]
fn bots_are_0_to_3() {
    assert_eq!(opts(&["--bots", "3"]).bots, 3);
    assert!(matches!(
        args(&["--bots", "4"]),
        Err(UsageError::InvalidValue { .. })
    ));
}

#[test]
fn bind_must_be_an_ip_address() {
    assert!(opts(&["--bind", "::1"]).bind.is_ipv6());
    assert!(matches!(
        args(&["--bind", "localhost"]),
        Err(UsageError::InvalidValue { .. })
    ));
}

#[test]
fn origins_need_a_scheme() {
    for bad in ["example.com", "localhost:3000", "//a.example", ""] {
        assert!(
            matches!(
                args(&["--allow-origin", bad]),
                Err(UsageError::InvalidValue { .. })
            ),
            "{bad}"
        );
    }
    assert!(args(&["--allow-origin", "https://a.example:8443"]).is_ok());
}

#[test]
fn max_connections_is_at_least_four() {
    assert!(matches!(
        args(&["--max-connections", "3"]),
        Err(UsageError::InvalidValue { .. })
    ));
    assert!(args(&["--max-connections", "4"]).is_ok());
}

#[test]
fn usage_errors() {
    assert!(matches!(
        args(&["--nope"]),
        Err(UsageError::UnknownOption(_))
    ));
    assert!(matches!(
        args(&["--bots"]),
        Err(UsageError::MissingValue(_))
    ));
    assert!(matches!(
        args(&["--seed", "x"]),
        Err(UsageError::InvalidValue { .. })
    ));
}

#[test]
fn help_and_version() {
    assert_eq!(args(&["--help"]), Ok(Command::Help));
    assert_eq!(args(&["-h"]), Ok(Command::Help));
    assert_eq!(args(&["--version"]), Ok(Command::Version));
    assert_eq!(args(&["-V"]), Ok(Command::Version));
}

#[test]
fn f3_us4_as1_dev_mode_is_off_by_default_and_parses_alone_or_with_other_options() {
    assert!(!opts(&[]).dev);
    assert!(opts(&["--dev"]).dev);
    let o = opts(&["--bots", "2", "--dev", "--http-port", "9000"]);
    assert!(o.dev);
    assert_eq!((o.bots, o.http_port), (2, 9000));
    assert!(eksplode_re_web::args::USAGE.contains("--dev"));
}

#[test]
fn f4_room_options_set_the_defaults_of_the_creation_menu() {
    use eksplode_re_core::{BoardSize, Mode};
    let o = opts(&[
        "--room-name",
        "Friday",
        "--mode",
        "team",
        "--size",
        "large",
        "--time",
        "10",
        "--friendly-fire",
        "off",
    ]);
    assert_eq!(o.room.name, "Friday");
    assert_eq!(o.room.mode, Mode::Team);
    assert_eq!(o.room.size, BoardSize::Large);
    assert_eq!(o.room.minutes, 10);
    assert!(!o.room.friendly_fire);
    assert_eq!(opts(&[]).room, eksplode_re_core::RoomSettings::default());
}

#[test]
fn f4_invalid_room_options_list_the_allowed_values() {
    for (list, allowed) in [
        (["--mode", "battle"], "normal, team, chaser"),
        (["--size", "huge"], "small, medium, large"),
        (["--time", "4"], "2, 3, 5, 10"),
        (["--time", "x"], "2, 3, 5, 10"),
        (["--friendly-fire", "maybe"], "on, off"),
    ] {
        let err = args(&list).unwrap_err();
        assert!(
            err.to_string().contains(allowed),
            "{list:?}: {err} must list {allowed}"
        );
    }
    assert_eq!(
        args(&["--mode"]).unwrap_err(),
        UsageError::MissingValue("--mode".into())
    );
}

#[test]
fn f4_a_long_room_name_is_cut_like_the_menu_does() {
    let o = opts(&["--room-name", &"x".repeat(80)]);
    assert_eq!(o.room.name.chars().count(), 24);
}
