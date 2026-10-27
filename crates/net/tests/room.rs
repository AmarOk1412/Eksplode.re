#![allow(clippy::unwrap_used, clippy::expect_used)]

use eksplode_re_core::{BoardSize, Mode, RoomSettings};
use eksplode_re_net::room::from_wire;
use serde_json::json;

#[test]
fn a_valid_object_is_read_as_it_is() {
    let s = from_wire(&json!({
        "name": "Friday room", "mode": "team", "size": "medium", "minutes": 5, "friendly_fire": false
    }));
    assert_eq!(s.name, "Friday room");
    assert_eq!(s.mode, Mode::Team);
    assert_eq!(s.size, BoardSize::Medium);
    assert_eq!(s.minutes, 5);
    assert!(!s.friendly_fire);
}

#[test]
fn anything_that_is_not_an_object_gives_the_defaults() {
    for v in [
        json!(null),
        json!(7),
        json!("team"),
        json!([1, 2]),
        json!(true),
    ] {
        assert_eq!(from_wire(&v), RoomSettings::default(), "{v}");
    }
}

#[test]
fn missing_wrong_and_unknown_fields_fall_back_per_field() {
    let s = from_wire(&json!({
        "name": 5, "mode": 3, "size": ["large"], "minutes": "5", "friendly_fire": "no",
        "extra": {"deep": [1, 2, 3]}
    }));
    assert_eq!(s, RoomSettings::default());

    let s = from_wire(&json!({"mode": "battle", "size": "huge", "minutes": 4}));
    assert_eq!(s.mode, Mode::Normal);
    assert_eq!(s.size, BoardSize::Small);
    assert_eq!(s.minutes, 3);

    let s = from_wire(&json!({"minutes": 999999999999u64}));
    assert_eq!(s.minutes, 3);
    let s = from_wire(&json!({"minutes": -2}));
    assert_eq!(s.minutes, 3);
    let s = from_wire(&json!({"minutes": 2.5}));
    assert_eq!(s.minutes, 3);

    let s = from_wire(&json!({"size": "large", "name": "Only these"}));
    assert_eq!(s.size, BoardSize::Large);
    assert_eq!(s.name, "Only these");
    assert_eq!(s.minutes, 3);
}

#[test]
fn the_name_is_cleaned_and_cut() {
    let s = from_wire(&json!({"name": "  \u{7}Hi\u{0}\nthere  "}));
    assert_eq!(s.name, "Hithere");
    let s = from_wire(&json!({"name": "x".repeat(60_000)}));
    assert_eq!(s.name.chars().count(), 24);
    let s = from_wire(&json!({"name": "\u{1}\u{2}\t "}));
    assert_eq!(s.name, "");
}

#[test]
fn the_wire_names_are_lowercase_strings() {
    let s = RoomSettings {
        name: "A".into(),
        mode: Mode::Chaser,
        size: BoardSize::Large,
        minutes: 10,
        friendly_fire: false,
    };
    let v = serde_json::to_value(&s).unwrap();
    assert_eq!(
        v,
        json!({"name":"A","mode":"chaser","size":"large","minutes":10,"friendly_fire":false})
    );
    assert_eq!(from_wire(&v), s);
    for (mode, name) in [
        (Mode::Normal, "normal"),
        (Mode::Team, "team"),
        (Mode::Chaser, "chaser"),
    ] {
        assert_eq!(serde_json::to_value(mode).unwrap(), json!(name));
    }
    for (size, name) in [
        (BoardSize::Small, "small"),
        (BoardSize::Medium, "medium"),
        (BoardSize::Large, "large"),
    ] {
        assert_eq!(serde_json::to_value(size).unwrap(), json!(name));
    }
}
