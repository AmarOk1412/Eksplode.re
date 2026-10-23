//! How a player looks: colour, gender look and accessories. Purely cosmetic; no rule depends on it.
//!
//! The wire form is a JSON object such as
//! `{"colour":"Blue","gender":"Male","head":"Cap","face":"None"}`. Decoding is lenient so an old
//! page, a typo or a hostile message only ever produces a default look for its sender.

use serde::{Deserialize, Serialize};
use serde_json::Value;

macro_rules! options {
    ($(#[$meta:meta])* $name:ident { $($variant:ident),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            /// Every choice, in the order the editor shows them.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The name used on the wire.
            pub fn name(self) -> &'static str {
                match self {
                    $($name::$variant => stringify!($variant)),+
                }
            }

            /// The choice with this exact wire name.
            pub fn from_name(name: &str) -> Option<$name> {
                Self::ALL.iter().copied().find(|c| c.name() == name)
            }
        }
    };
}

options! {
    /// The main colour of a character.
    Colour { Blue, Black, Red, Purple, Green, Orange, Yellow, Pink, Teal, White }
}
options! {
    /// The gender look: a cosmetic choice of hair and face details.
    Gender { Male, Female }
}
options! {
    /// Something worn on the head.
    Head { None, Cap, TopHat, Crown, Headphones }
}
options! {
    /// Something worn on the face.
    Face { None, Glasses, Sunglasses, Moustache, Mask }
}

/// A player's chosen appearance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Look {
    pub colour: Colour,
    pub gender: Gender,
    pub head: Head,
    pub face: Face,
}

impl Look {
    /// The look a slot gets when nobody chose one: blue, black, red and purple characters.
    pub fn default_for(slot: usize) -> Look {
        let (colour, gender) = match slot % 4 {
            0 => (Colour::Blue, Gender::Male),
            1 => (Colour::Black, Gender::Female),
            2 => (Colour::Red, Gender::Male),
            _ => (Colour::Purple, Gender::Female),
        };
        Look {
            colour,
            gender,
            head: Head::None,
            face: Face::None,
        }
    }

    /// Reads a look from loose JSON. A missing, wrongly typed or unknown field becomes the
    /// default of `slot` for that field; a value that is not an object gives the full default.
    pub fn from_wire(value: &Value, slot: usize) -> Look {
        let default = Look::default_for(slot);
        let Some(map) = value.as_object() else {
            return default;
        };
        let text = |key: &str| map.get(key).and_then(Value::as_str);
        Look {
            colour: text("colour")
                .and_then(Colour::from_name)
                .unwrap_or(default.colour),
            gender: text("gender")
                .and_then(Gender::from_name)
                .unwrap_or(default.gender),
            head: text("head")
                .and_then(Head::from_name)
                .unwrap_or(default.head),
            face: text("face")
                .and_then(Face::from_name)
                .unwrap_or(default.face),
        }
    }
}

/// Gives `requested` unless another participant has it, otherwise the next free colour in palette
/// order (wrapping). The flag says whether the colour was changed.
pub fn resolve_colour(requested: Colour, taken: &[Colour]) -> (Colour, bool) {
    if !taken.contains(&requested) {
        return (requested, false);
    }
    let all = Colour::ALL;
    let start = all.iter().position(|c| *c == requested).unwrap_or(0);
    for step in 1..=all.len() {
        let candidate = all[(start + step) % all.len()];
        if !taken.contains(&candidate) {
            return (candidate, true);
        }
    }
    (requested, false)
}
