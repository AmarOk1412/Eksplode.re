//! A player's look as the renderer understands it: colour, accessories and costume.
//!
//! This mirrors the look of the server (`eksplode_re_net::look`): the names are the wire names, and
//! reading a look from JSON is as lenient as the host's, so a bad entry only ever gives a default.
//! The body silhouette (`gender`) is no longer a player choice (feature 011): the page and the
//! host do not send it, so it falls back to the slot default, alternating between the slots.

use eksplode_re_core::Dir;
use serde_json::Value;

use crate::sprites::{hex, mix, player_skin, runs, Rgb, Run, Skin, PLAYER};

pub const COLOUR_NAMES: [&str; 10] = [
    "Blue", "Black", "Red", "Purple", "Green", "Orange", "Yellow", "Pink", "Teal", "White",
];
pub const GENDER_NAMES: [&str; 2] = ["Male", "Female"];
pub const HEAD_NAMES: [&str; 5] = ["None", "Cap", "TopHat", "Crown", "Headphones"];
pub const FACE_NAMES: [&str; 5] = ["None", "Glasses", "Sunglasses", "Moustache", "Mask"];
pub const COSTUME_NAMES: [&str; 5] = ["None", "Scarf", "Cape", "BowTie", "Medal"];

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
pub const SCARF: Rgb = [0.851, 0.192, 0.192];
pub const CAPE: Rgb = [0.42, 0.13, 0.55];
pub const BOWTIE: Rgb = [0.11, 0.11, 0.24];
pub const RIBBON: Rgb = [0.18, 0.4, 0.78];
pub const MEDAL: Rgb = [0.83, 0.62, 0.17];

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

/// Something worn on the body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Costume {
    None,
    Scarf,
    Cape,
    BowTie,
    Medal,
}

const GENDERS: [Gender; 2] = [Gender::Male, Gender::Female];
const COSTUMES: [Costume; 5] = [
    Costume::None,
    Costume::Scarf,
    Costume::Cape,
    Costume::BowTie,
    Costume::Medal,
];
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
    /// The body silhouette; not a player choice, see the module documentation.
    pub gender: Gender,
    pub head: Head,
    pub face: Face,
    pub costume: Costume,
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
        costume: Costume::None,
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
        costume: find(&COSTUME_NAMES, "costume").map_or(default.costume, |i| COSTUMES[i]),
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

/// Which way a character looks: `Right` is `Left` mirrored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Down,
    Up,
    Left,
    Right,
}

impl View {
    /// The view of a character that faces `dir`.
    pub fn from_dir(dir: Dir) -> View {
        match dir {
            Dir::Down => View::Down,
            Dir::Up => View::Up,
            Dir::Left => View::Left,
            Dir::Right => View::Right,
        }
    }
}

/// The pose of a character: standing, or one of the two steps of the walk (the feet and arms
/// swap between `StepA` and `StepB`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frame {
    Idle,
    StepA,
    StepB,
}

/// The side view, facing left: `n` antenna, `H` helmet, `F` face, `e` eye, `B` body, `K` belt,
/// `Y` buckle, `S` shoes (the arm `A` is added by the frame).
const SIDE: [&str; 13] = [
    ".....nn.....",
    "...HHHHHH...",
    "..HFFHHHHH..",
    "..HFeFHHHH..",
    "..HFeFHHHH..",
    "..HFFFHHHH..",
    "..HHHHHHHH..",
    "...BBBBBB...",
    "...BBBBBB...",
    "...BBBBBB...",
    "...KYKKKK...",
    "....BBB.....",
    "...SSSS.....",
];

fn rows_into(g: &mut [Vec<char>], at: usize, rows: &[&str]) {
    for (r, line) in rows.iter().enumerate() {
        for (c, ch) in line.chars().enumerate().take(WIDTH) {
            if let Some(cell) = g.get_mut(r + at).and_then(|row| row.get_mut(c)) {
                *cell = ch;
            }
        }
    }
}

/// The legs and arms of the front and back views for a frame.
fn frame_front(g: &mut [Vec<char>], frame: Frame) {
    let at = |r: usize| r + OVERHEAD;
    match frame {
        Frame::Idle => {}
        Frame::StepA => {
            for r in 7..=9 {
                put(g, at(r), 1..=1, '.');
                put(g, at(r), 10..=10, '.');
            }
            put(g, at(8), 1..=1, 'A');
            put(g, at(9), 1..=1, 'A');
            put(g, at(10), 1..=1, 'A');
            put(g, at(7), 10..=10, 'A');
            put(g, at(8), 10..=10, 'A');
            rows_into(g, at(11), &["..BBB..BSS..", "..SS........"]);
        }
        Frame::StepB => {
            for r in 7..=9 {
                put(g, at(r), 1..=1, '.');
                put(g, at(r), 10..=10, '.');
            }
            put(g, at(7), 1..=1, 'A');
            put(g, at(8), 1..=1, 'A');
            put(g, at(8), 10..=10, 'A');
            put(g, at(9), 10..=10, 'A');
            put(g, at(10), 10..=10, 'A');
            rows_into(g, at(11), &["..SSB..BBB..", "........SS.."]);
        }
    }
}

/// The legs and the near arm of the side view for a frame.
fn frame_side(g: &mut [Vec<char>], frame: Frame) {
    let at = |r: usize| r + OVERHEAD;
    let arm = match frame {
        Frame::Idle => 5,
        Frame::StepA => 4,
        Frame::StepB => 6,
    };
    for r in 7..=9 {
        put(g, at(r), arm..=arm + 1, 'A');
    }
    if frame != Frame::Idle {
        rows_into(g, at(11), &["...BB..BB...", "..SSS..SSS.."]);
    }
}

/// The picture of a look seen from `view` in `frame`, as characters: `GRID_ROWS` rows of 12, `.`
/// is empty. Colour, gender look and accessories are the same in every view; what changes is what
/// can be seen (no face from behind, one eye from the side, the cap brim on the side the
/// character looks to).
pub fn grid_view(look: &Look, view: View, frame: Frame) -> Vec<String> {
    if view == View::Right {
        return grid_view(look, View::Left, frame)
            .into_iter()
            .map(|row| row.chars().rev().collect())
            .collect();
    }
    let mut g: Vec<Vec<char>> = vec![vec!['.'; WIDTH]; GRID_ROWS];
    let side = view == View::Left;
    if side {
        rows_into(&mut g, OVERHEAD, &SIDE);
        frame_side(&mut g, frame);
    } else {
        let rows: Vec<&str> = PLAYER.to_vec();
        rows_into(&mut g, OVERHEAD, &rows);
        frame_front(&mut g, frame);
    }
    let female = look.gender == Gender::Female;
    if view == View::Up {
        // the back of the head: hair for the female look, the helmet otherwise; no buckle
        let back = if female { 'h' } else { 'H' };
        for cell in g[4..=7].iter_mut().flat_map(|row| row[3..=8].iter_mut()) {
            if matches!(*cell, 'F' | 'e') {
                *cell = back;
            }
        }
        put(&mut g, 12, 5..=6, 'K');
    }
    if female {
        for row in 4..=8 {
            put(&mut g, row, 10..=10, 'h');
        }
        match view {
            View::Down => {
                put(&mut g, 4, 4..=4, 'l');
                put(&mut g, 4, 7..=7, 'l');
            }
            View::Left => put(&mut g, 4, 4..=4, 'l'),
            _ => {}
        }
    }
    costume_into(&mut g, look.costume, view);
    match look.head {
        Head::None => {}
        Head::Cap => {
            put(&mut g, 2, 3..=8, 'c');
            put(&mut g, 3, 2..=9, 'c');
            match view {
                View::Down => put(&mut g, 4, 8..=10, 'c'),
                View::Left => put(&mut g, 4, 1..=3, 'c'),
                _ => {}
            }
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
            if side {
                for row in 5..=7 {
                    put(&mut g, row, 5..=6, 'q');
                }
            } else {
                for row in 5..=7 {
                    put(&mut g, row, 1..=1, 'q');
                    put(&mut g, row, 10..=10, 'q');
                }
            }
        }
    }
    match (look.face, view) {
        (_, View::Up) | (Face::None, _) => {}
        (Face::Glasses, View::Left) => {
            put(&mut g, 4, 3..=5, 'k');
            put(&mut g, 7, 3..=5, 'k');
            put(&mut g, 5, 3..=3, 'k');
            put(&mut g, 6, 3..=3, 'k');
            put(&mut g, 5, 5..=9, 'k');
        }
        (Face::Sunglasses, View::Left) => {
            put(&mut g, 5, 3..=9, 's');
            put(&mut g, 6, 3..=5, 's');
        }
        (Face::Moustache, View::Left) => put(&mut g, 7, 3..=4, 'm'),
        (Face::Mask, View::Left) => {
            put(&mut g, 7, 3..=5, 'x');
            put(&mut g, 8, 3..=5, 'x');
        }
        (Face::Glasses, _) => {
            put(&mut g, 4, 3..=8, 'k');
            put(&mut g, 7, 3..=8, 'k');
            for row in 5..=6 {
                put(&mut g, row, 3..=3, 'k');
                put(&mut g, row, 8..=8, 'k');
            }
            put(&mut g, 5, 5..=6, 'k');
        }
        (Face::Sunglasses, _) => {
            put(&mut g, 5, 3..=8, 's');
            put(&mut g, 6, 3..=8, 's');
        }
        (Face::Moustache, _) => put(&mut g, 7, 3..=8, 'm'),
        (Face::Mask, _) => {
            put(&mut g, 7, 3..=8, 'x');
            put(&mut g, 8, 3..=8, 'x');
        }
    }
    g.into_iter().map(|row| row.into_iter().collect()).collect()
}

/// The costume on the body rows (9 to 13 of the grid): `r` scarf, `v` cape, `w` bow tie, `i`
/// ribbon and `d` medal. The bow tie and the medal are worn in front, so they are not seen from
/// behind; the cape hangs behind, so the front only shows its edges.
fn costume_into(g: &mut [Vec<char>], costume: Costume, view: View) {
    match (costume, view) {
        (Costume::None, _) => {}
        (Costume::Scarf, View::Left) => {
            put(g, 9, 3..=8, 'r');
            put(g, 10, 8..=9, 'r');
            put(g, 11, 9..=9, 'r');
        }
        (Costume::Scarf, View::Up) => {
            put(g, 9, 3..=8, 'r');
            put(g, 10, 4..=4, 'r');
        }
        (Costume::Scarf, _) => {
            put(g, 9, 3..=8, 'r');
            put(g, 10, 7..=7, 'r');
            put(g, 11, 7..=7, 'r');
        }
        (Costume::Cape, View::Left) => {
            for row in 9..=13 {
                put(g, row, 9..=9, 'v');
            }
            put(g, 9, 8..=8, 'v');
        }
        (Costume::Cape, View::Up) => {
            for row in 9..=12 {
                put(g, row, 2..=9, 'v');
            }
        }
        (Costume::Cape, _) => {
            put(g, 9, 2..=2, 'v');
            put(g, 9, 9..=9, 'v');
            for row in 12..=13 {
                put(g, row, 1..=1, 'v');
                put(g, row, 10..=10, 'v');
            }
        }
        (Costume::BowTie | Costume::Medal, View::Up) => {}
        (Costume::BowTie, View::Left) => put(g, 9, 3..=3, 'w'),
        (Costume::BowTie, _) => {
            put(g, 9, 4..=7, 'w');
            put(g, 10, 4..=4, 'w');
            put(g, 10, 7..=7, 'w');
        }
        (Costume::Medal, View::Left) => {
            put(g, 9, 3..=3, 'i');
            put(g, 10, 3..=3, 'd');
        }
        (Costume::Medal, _) => {
            put(g, 9, 5..=6, 'i');
            put(g, 10, 5..=6, 'd');
        }
    }
}

/// The picture of a look as characters, seen from the front and standing.
pub fn grid(look: &Look) -> Vec<String> {
    grid_view(look, View::Down, Frame::Idle)
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
        'r' => SCARF,
        'v' => CAPE,
        'w' => BOWTIE,
        'i' => RIBBON,
        'd' => MEDAL,
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

/// The runs of the first `rows` rows of a look seen from `view` in `frame`.
pub fn runs_view(look: &Look, view: View, frame: Frame, rows: usize) -> Vec<Run> {
    let skin = skin(look);
    runs(&grid_view(look, view, frame), rows, |c| {
        colour_of(look, &skin, c)
    })
}

/// Like [`runs_for`] but only the first `rows` rows of the picture (including the overhead).
pub fn runs_of(look: &Look, rows: usize) -> Vec<Run> {
    let skin = skin(look);
    runs(&grid(look), rows, |c| colour_of(look, &skin, c))
}
