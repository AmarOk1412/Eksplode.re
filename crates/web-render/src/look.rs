//! A player's look as the renderer understands it: colour, gender look and accessories.
//!
//! This mirrors the look of the server (`bomberust_net::look`): the names are the wire names, and
//! reading a look from JSON is as lenient as the host's, so a bad entry only ever gives a default.

use serde_json::Value;

use crate::sprites::{hex, mix, player_skin, runs, Rgb, Run, Skin, PLAYER};

pub const COLOUR_NAMES: [&str; 10] = [
    "Blue", "Black", "Red", "Purple", "Green", "Orange", "Yellow", "Pink", "Teal", "White",
];
pub const GENDER_NAMES: [&str; 2] = ["Male", "Female"];
pub const HEAD_NAMES: [&str; 5] = ["None", "Cap", "TopHat", "Crown", "Headphones"];
pub const FACE_NAMES: [&str; 5] = ["None", "Glasses", "Sunglasses", "Moustache", "Mask"];

/// Rows of the character plus the empty rows above the head that hats may use.
pub const GRID_ROWS: usize = 15;
pub const OVERHEAD: usize = 2;
const WIDTH: usize = 12;

pub const HAIR_LIGHT: f32 = 0.5;
pub const CAP: Rgb = [0.784, 0.455, 0.165];
pub const HAT: Rgb = [0.078, 0.078, 0.078];
pub const HAT_BAND: Rgb = [0.702, 0.149, 0.18];
pub const CROWN: Rgb = [0.91, 0.753, 0.125];
pub const HEADBAND: Rgb = [0.235, 0.235, 0.235];
pub const CUP: Rgb = [0.549, 0.549, 0.588];
pub const FRAME: Rgb = [0.102, 0.102, 0.102];
pub const SUNGLASSES: Rgb = [0.039, 0.039, 0.039];
pub const MOUSTACHE: Rgb = [0.353, 0.227, 0.118];
pub const MASK: Rgb = [0.659, 0.831, 0.933];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gender {
    Male,
    Female,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Head {
    None,
    Cap,
    TopHat,
    Crown,
    Headphones,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    None,
    Glasses,
    Sunglasses,
    Moustache,
    Mask,
}

const GENDERS: [Gender; 2] = [Gender::Male, Gender::Female];
const HEADS: [Head; 5] = [
    Head::None,
    Head::Cap,
    Head::TopHat,
    Head::Crown,
    Head::Headphones,
];
const FACES: [Face; 5] = [
    Face::None,
    Face::Glasses,
    Face::Sunglasses,
    Face::Moustache,
    Face::Mask,
];

/// A look; `colour` indexes [`COLOUR_NAMES`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    pub colour: usize,
    pub gender: Gender,
    pub head: Head,
    pub face: Face,
}

/// The look of a slot nobody customised: blue, black, red and purple characters.
pub fn default_for(slot: usize) -> Look {
    Look {
        colour: slot % 4,
        gender: if slot % 2 == 0 {
            Gender::Male
        } else {
            Gender::Female
        },
        head: Head::None,
        face: Face::None,
    }
}

/// Reads a look from loose JSON; a missing, wrongly typed or unknown field becomes the default of
/// `slot` for that field, a value that is not an object gives the full default.
pub fn from_value(value: &Value, slot: usize) -> Look {
    let default = default_for(slot);
    let Some(map) = value.as_object() else {
        return default;
    };
    let text = |key: &str| map.get(key).and_then(Value::as_str);
    let find =
        |names: &[&str], key: &str| text(key).and_then(|t| names.iter().position(|n| *n == t));
    Look {
        colour: find(&COLOUR_NAMES, "colour").unwrap_or(default.colour),
        gender: find(&GENDER_NAMES, "gender").map_or(default.gender, |i| GENDERS[i]),
        head: find(&HEAD_NAMES, "head").map_or(default.head, |i| HEADS[i]),
        face: find(&FACE_NAMES, "face").map_or(default.face, |i| FACES[i]),
    }
}

const BODIES: [u32; 6] = [0x3a9d4a, 0xe8761a, 0xe0c020, 0xf08cc0, 0x1f9d9d, 0xe8e8ee];

/// The colours of a character of this look's colour.
pub fn skin(look: &Look) -> Skin {
    let colour = look.colour % COLOUR_NAMES.len();
    if colour < 4 {
        return player_skin(colour as u8);
    }
    let body = hex(BODIES[colour - 4]);
    let white = [1.0, 1.0, 1.0];
    let black = [0.0, 0.0, 0.0];
    let helmet = mix(body, white, 0.55);
    Skin {
        nub: helmet,
        helmet,
        face: mix(body, white, 0.85),
        eyes: mix(body, black, 0.65),
        arms: helmet,
        body,
        belt: hex(0x1c1c1c),
        buckle: hex(0xe0c020),
        shoes: mix(body, black, 0.4),
    }
}

fn put(grid: &mut [Vec<char>], row: usize, cols: std::ops::RangeInclusive<usize>, ch: char) {
    for col in cols {
        if let Some(cell) = grid.get_mut(row).and_then(|r| r.get_mut(col)) {
            *cell = ch;
        }
    }
}

/// The picture of a look as characters: `GRID_ROWS` rows of 12, `.` is empty.
pub fn grid(look: &Look) -> Vec<String> {
    let mut g: Vec<Vec<char>> = vec![vec!['.'; WIDTH]; GRID_ROWS];
    for (r, line) in PLAYER.iter().enumerate() {
        for (c, ch) in line.chars().enumerate().take(WIDTH) {
            g[r + OVERHEAD][c] = ch;
        }
    }
    if look.gender == Gender::Female {
        put(&mut g, 4, 10..=10, 'h');
        put(&mut g, 5, 10..=10, 'h');
        put(&mut g, 6, 10..=10, 'h');
        put(&mut g, 7, 10..=10, 'h');
        put(&mut g, 8, 10..=10, 'h');
        put(&mut g, 4, 4..=4, 'l');
        put(&mut g, 4, 7..=7, 'l');
    }
    match look.head {
        Head::None => {}
        Head::Cap => {
            put(&mut g, 2, 3..=8, 'c');
            put(&mut g, 3, 2..=9, 'c');
            put(&mut g, 4, 8..=10, 'c');
        }
        Head::TopHat => {
            put(&mut g, 0, 4..=7, 't');
            put(&mut g, 1, 4..=7, 't');
            put(&mut g, 2, 4..=7, 'b');
            put(&mut g, 3, 2..=9, 't');
        }
        Head::Crown => {
            for col in [3, 5, 6, 8] {
                put(&mut g, 1, col..=col, 'g');
            }
            put(&mut g, 2, 3..=8, 'g');
        }
        Head::Headphones => {
            put(&mut g, 2, 3..=8, 'p');
            put(&mut g, 3, 2..=2, 'p');
            put(&mut g, 3, 9..=9, 'p');
            for row in 5..=7 {
                put(&mut g, row, 1..=1, 'q');
                put(&mut g, row, 10..=10, 'q');
            }
        }
    }
    match look.face {
        Face::None => {}
        Face::Glasses => {
            put(&mut g, 4, 3..=8, 'k');
            put(&mut g, 7, 3..=8, 'k');
            for row in 5..=6 {
                put(&mut g, row, 3..=3, 'k');
                put(&mut g, row, 8..=8, 'k');
            }
            put(&mut g, 5, 5..=6, 'k');
        }
        Face::Sunglasses => {
            put(&mut g, 5, 3..=8, 's');
            put(&mut g, 6, 3..=8, 's');
        }
        Face::Moustache => put(&mut g, 7, 3..=8, 'm'),
        Face::Mask => {
            put(&mut g, 7, 3..=8, 'x');
            put(&mut g, 8, 3..=8, 'x');
        }
    }
    g.into_iter().map(|row| row.into_iter().collect()).collect()
}

/// The colour of a grid character for this look.
pub fn colour_of(look: &Look, skin: &Skin, ch: char) -> Option<Rgb> {
    if let Some(c) = skin.colour_of(ch) {
        return Some(c);
    }
    Some(match ch {
        'h' => mix(skin.body, [0.0, 0.0, 0.0], HAIR_LIGHT),
        'l' => skin.eyes,
        'c' => CAP,
        't' => HAT,
        'b' => HAT_BAND,
        'g' => CROWN,
        'p' => HEADBAND,
        'q' => CUP,
        'k' => FRAME,
        's' => SUNGLASSES,
        'm' => MOUSTACHE,
        'x' => MASK,
        _ => {
            let _ = look;
            return None;
        }
    })
}

/// The horizontal runs of pixels of the whole picture of a look.
pub fn runs_for(look: &Look) -> Vec<Run> {
    runs_of(look, GRID_ROWS)
}

/// Like [`runs_for`] but only the first `rows` rows of the picture (including the overhead).
pub fn runs_of(look: &Look, rows: usize) -> Vec<Run> {
    let skin = skin(look);
    runs(&grid(look), rows, |c| colour_of(look, &skin, c))
}
