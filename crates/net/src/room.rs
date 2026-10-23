//! Reading room settings from the wire. Decoding is lenient, like looks: a missing or invalid
//! field becomes its default and a value that is not an object gives the defaults, so an old
//! page, a typo or a hostile message never breaks a room.

use bomberust_core::{BoardSize, Mode, RoomSettings};
use serde_json::Value;

/// The settings in `value`; never fails. The mode is not checked against the playable modes
/// (the host does that with [`RoomSettings::sanitize`]).
pub fn from_wire(value: &Value) -> RoomSettings {
    let Some(obj) = value.as_object() else {
        return RoomSettings::default();
    };
    let defaults = RoomSettings::default();
    let field = |name: &str| obj.get(name).cloned();
    RoomSettings {
        name: obj
            .get("name")
            .and_then(Value::as_str)
            .map_or(defaults.name, str::to_string),
        mode: field("mode")
            .and_then(|v| serde_json::from_value::<Mode>(v).ok())
            .unwrap_or(defaults.mode),
        size: field("size")
            .and_then(|v| serde_json::from_value::<BoardSize>(v).ok())
            .unwrap_or(defaults.size),
        minutes: obj
            .get("minutes")
            .and_then(Value::as_u64)
            .and_then(|m| u8::try_from(m).ok())
            .unwrap_or(defaults.minutes),
        friendly_fire: obj
            .get("friendly_fire")
            .and_then(Value::as_bool)
            .unwrap_or(defaults.friendly_fire),
    }
    .clean()
}
