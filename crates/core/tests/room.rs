use bomberust_core::{BoardSize, Config, Mode, RoomSettings};

#[test]
fn defaults_are_the_game_as_it_was() {
    let s = RoomSettings::default();
    assert_eq!(s.name, "");
    assert_eq!(s.mode, Mode::Normal);
    assert_eq!(s.size, BoardSize::Small);
    assert_eq!(s.minutes, 3);
    assert!(s.friendly_fire);
    let c = s.to_config(&Config::default());
    assert_eq!(c, Config::default());
    assert_eq!((c.width, c.height, c.round_limit_ticks), (15, 13, 3600));
}

#[test]
fn sanitize_cleans_the_name_and_the_time() {
    let s = RoomSettings {
        name: "  \u{7}Fri\tday\u{0} room  ".into(),
        minutes: 4,
        ..RoomSettings::default()
    }
    .sanitize();
    assert_eq!(s.name, "Friday room");
    assert_eq!(s.minutes, 3);

    let long = RoomSettings {
        name: "x".repeat(100),
        ..RoomSettings::default()
    }
    .sanitize();
    assert_eq!(long.name.chars().count(), 24);

    let wide = RoomSettings {
        name: "é".repeat(30),
        ..RoomSettings::default()
    }
    .sanitize();
    assert_eq!(wide.name.chars().count(), 24);

    for m in [2, 3, 5, 10] {
        let ok = RoomSettings {
            minutes: m,
            ..RoomSettings::default()
        }
        .sanitize();
        assert_eq!(ok.minutes, m);
    }
}

#[test]
fn a_mode_that_is_not_playable_yet_becomes_normal() {
    for mode in Mode::ALL {
        let s = RoomSettings {
            mode,
            ..RoomSettings::default()
        }
        .sanitize();
        if bomberust_core::PLAYABLE.contains(&mode) {
            assert_eq!(s.mode, mode);
        } else {
            assert_eq!(s.mode, Mode::Normal);
        }
    }
    assert!(bomberust_core::PLAYABLE.contains(&Mode::Normal));
}

#[test]
fn to_config_sets_size_time_and_mode() {
    let base = Config::default();
    let dims = [
        (BoardSize::Small, (15, 13)),
        (BoardSize::Medium, (19, 15)),
        (BoardSize::Large, (25, 19)),
    ];
    for (size, (w, h)) in dims {
        let c = RoomSettings {
            size,
            ..RoomSettings::default()
        }
        .to_config(&base);
        assert_eq!((c.width, c.height), (w, h));
        assert!(c.width % 2 == 1 && c.height % 2 == 1);
        assert_eq!(c.validate(), Ok(()));
    }
    for (minutes, ticks) in [(2, 2400), (3, 3600), (5, 6000), (10, 12000)] {
        let c = RoomSettings {
            minutes,
            ..RoomSettings::default()
        }
        .to_config(&base);
        assert_eq!(c.round_limit_ticks, ticks);
    }
    let c = RoomSettings {
        mode: Mode::Team,
        friendly_fire: false,
        ..RoomSettings::default()
    }
    .to_config(&base);
    assert_eq!(c.mode, Mode::Team);
    assert!(!c.friendly_fire);
}

#[test]
fn a_custom_base_config_is_kept_for_the_default_size_and_time() {
    let base = Config {
        width: 9,
        height: 9,
        round_limit_ticks: 40,
        ..Config::default()
    };
    let c = RoomSettings::default().to_config(&base);
    assert_eq!((c.width, c.height, c.round_limit_ticks), (9, 9, 40));
}

#[test]
fn participants_needed_per_mode() {
    let need = |mode| {
        RoomSettings {
            mode,
            ..RoomSettings::default()
        }
        .participants_needed()
    };
    assert_eq!(need(Mode::Normal), (1, 4));
    assert_eq!(need(Mode::Team), (4, 4));
    assert_eq!(need(Mode::Chaser), (4, 4));
}

#[test]
fn config_validation_covers_the_new_fields() {
    let bad = |f: fn(&mut Config)| {
        let mut c = Config::default();
        f(&mut c);
        assert!(c.validate().is_err());
    };
    bad(|c| c.closing_lead_ticks = 0);
    bad(|c| c.chaser_hearts = 0);
    bad(|c| c.chaser_bombs = 0);
    bad(|c| c.chaser_pillar_mod = 0);
    bad(|c| c.bonus_every_ticks = 0);
    bad(|c| c.chaser_block_density = 1.5);
    bad(|c| c.chaser_block_density = -0.1);
    assert_eq!(Config::default().validate(), Ok(()));
}

#[cfg(feature = "serde")]
#[test]
fn json_round_trip_and_old_configs_still_load() {
    let s = RoomSettings {
        name: "Friday".into(),
        mode: Mode::Chaser,
        size: BoardSize::Large,
        minutes: 10,
        friendly_fire: false,
    };
    let json = serde_json::to_string(&s).unwrap();
    assert!(json.contains("\"chaser\"") && json.contains("\"large\""));
    assert_eq!(serde_json::from_str::<RoomSettings>(&json).unwrap(), s);

    let mut value = serde_json::to_value(Config::default()).unwrap();
    let obj = value.as_object_mut().unwrap();
    for key in [
        "mode",
        "friendly_fire",
        "closing_ticks",
        "closing_lead_ticks",
        "chaser_bombs",
        "chaser_hearts",
        "shield_ticks",
        "chaser_pillar_mod",
        "chaser_block_density",
        "bonus_every_ticks",
        "bonus_cap",
    ] {
        obj.remove(key);
    }
    let old: Config = serde_json::from_value(value).unwrap();
    assert_eq!(old, Config::default());
}
