//! Pixel-art sprites and the colour palette.

use bomberust_core::PowerUpKind;

pub type Rgb = [f32; 3];

pub fn hex(rgb: u32) -> Rgb {
    [
        ((rgb >> 16) & 0xff) as f32 / 255.0,
        ((rgb >> 8) & 0xff) as f32 / 255.0,
        (rgb & 0xff) as f32 / 255.0,
    ]
}

pub const BOARD: Rgb = [0.906, 0.894, 0.898];
pub const BOARD_EDGE: Rgb = [0.78, 0.769, 0.78];
pub const WALL_BASE: Rgb = [0.827, 0.827, 0.839];
pub const WALL_SHADE: Rgb = [0.757, 0.757, 0.773];
pub const WALL_WINDOW: Rgb = [0.98, 0.98, 0.98];
pub const WALL_WINDOW_EDGE: Rgb = [0.72, 0.72, 0.74];
pub const BRICK_BASE: Rgb = [0.851, 0.847, 0.851];
pub const BRICK_LINE: Rgb = [0.55, 0.11, 0.11];
pub const FLAME_OUTER: Rgb = [1.0, 0.42, 0.075];
pub const FLAME_MID: Rgb = [1.0, 0.79, 0.2];
pub const FLAME_CORE: Rgb = [1.0, 0.95, 0.69];
pub const BOMB_BODY: Rgb = [0.09, 0.09, 0.11];
pub const BOMB_SHINE: Rgb = [0.62, 0.62, 0.68];
pub const BOMB_FUSE: Rgb = [0.55, 0.36, 0.2];
pub const SPARK_A: Rgb = [1.0, 0.6, 0.1];
pub const SPARK_B: Rgb = [1.0, 0.9, 0.3];
pub const WHITE: Rgb = [1.0, 1.0, 1.0];
/// Frame of a malus power-up.
pub const MALUS_FRAME: Rgb = [0.42, 0.06, 0.08];
/// Tints of the pulse around a player with a malus.
pub const MALUS_SLOW: Rgb = [0.25, 0.5, 0.95];
pub const MALUS_HYPER: Rgb = [0.98, 0.85, 0.1];
pub const MALUS_SMALL: Rgb = [0.9, 0.2, 0.2];

/// Characters of the front-facing character, top row first; `.` is transparent.
///
/// `n` antenna, `H` helmet, `F` face, `e` eyes, `A` arms, `B` body, `K` belt, `Y` buckle,
/// `S` shoes.
pub const PLAYER: [&str; 13] = [
    ".....nn.....",
    "..HHHHHHHH..",
    "..HFFFFFFH..",
    "..HFeFFeFH..",
    "..HFeFFeFH..",
    "..HFFFFFFH..",
    "..HHHHHHHH..",
    ".AABBBBBBAA.",
    ".ABBBBBBBBA.",
    ".ABBBBBBBBA.",
    "..KKKYYKKK..",
    "..BBB..BBB..",
    "..SS....SS..",
];

/// The head and shoulders rows shown on character cards.
pub const PORTRAIT_ROWS: usize = 7;

/// Round bomb with a fuse and a spark (`o`).
pub const BOMB: [&str; 10] = [
    ".......o..",
    "......f...",
    "...kkkk...",
    "..kkkkkk..",
    ".kwwkkkkk.",
    ".kwkkkkkk.",
    ".kkkkkkkk.",
    ".kkkkkkkk.",
    "..kkkkkk..",
    "...kkkk...",
];

/// The colours of one character.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skin {
    pub nub: Rgb,
    pub helmet: Rgb,
    pub face: Rgb,
    pub eyes: Rgb,
    pub arms: Rgb,
    pub body: Rgb,
    pub belt: Rgb,
    pub buckle: Rgb,
    pub shoes: Rgb,
}

impl Skin {
    pub fn colour_of(&self, ch: char) -> Option<Rgb> {
        Some(match ch {
            'n' => self.nub,
            'H' => self.helmet,
            'F' => self.face,
            'e' => self.eyes,
            'A' => self.arms,
            'B' => self.body,
            'K' => self.belt,
            'Y' => self.buckle,
            'S' => self.shoes,
            _ => return None,
        })
    }
}

/// Slot 0 blue with a white helmet, 1 black with orange eyes, 2 red, 3 purple.
pub fn player_skin(slot: u8) -> Skin {
    let belt = hex(0x1c1c1c);
    let gold = hex(0xe0c020);
    match slot % 4 {
        0 => Skin {
            nub: hex(0xdcdcdc),
            helmet: hex(0xd9d9de),
            face: hex(0xd3a86b),
            eyes: hex(0x7a3a34),
            arms: hex(0xdcdcdc),
            body: hex(0x5468d8),
            belt,
            buckle: gold,
            shoes: hex(0xd79ec4),
        },
        1 => Skin {
            nub: hex(0x151515),
            helmet: hex(0x0b0b0b),
            face: hex(0x2b2b2e),
            eyes: hex(0xf59a0a),
            arms: hex(0x0b0b0b),
            body: hex(0x0b0b0b),
            belt: hex(0x2a2a2a),
            buckle: hex(0xf59a0a),
            shoes: hex(0xf59a0a),
        },
        2 => Skin {
            nub: hex(0xa33030),
            helmet: hex(0xb03434),
            face: hex(0xf3f0ff),
            eyes: hex(0x111111),
            arms: hex(0xb03434),
            body: hex(0xa52c2c),
            belt,
            buckle: gold,
            shoes: hex(0x7d1f1f),
        },
        _ => Skin {
            nub: hex(0xd98af6),
            helmet: hex(0xc77ae6),
            face: hex(0xf3dcff),
            eyes: hex(0xa044c8),
            arms: hex(0xb36ad6),
            body: hex(0xc77ae6),
            belt,
            buckle: gold,
            shoes: hex(0x8a4fb0),
        },
    }
}

/// The colour that names a player in the page: cards, chips and banners.
pub fn player_color(slot: u8) -> Rgb {
    match slot % 4 {
        0 => hex(0x4d63d8),
        1 => hex(0x0d0d0d),
        2 => hex(0xa83232),
        _ => hex(0xc97df0),
    }
}

/// Blends `a` towards `b` by `t` (0.0 keeps `a`, 1.0 gives `b`).
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// A horizontal run of equally coloured pixels of a sprite.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Run {
    pub col: usize,
    pub row: usize,
    pub len: usize,
    pub color: Rgb,
}

/// Merges the pixels of a template into horizontal runs; characters without a colour are skipped.
pub fn runs<S: AsRef<str>>(
    template: &[S],
    rows: usize,
    colour_of: impl Fn(char) -> Option<Rgb>,
) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for (row, line) in template.iter().take(rows).enumerate() {
        let mut open: Option<Run> = None;
        for (col, ch) in line.as_ref().chars().enumerate() {
            match (colour_of(ch), open.as_mut()) {
                (Some(c), Some(run)) if run.color == c => run.len += 1,
                (Some(c), _) => {
                    out.extend(open.take());
                    open = Some(Run {
                        col,
                        row,
                        len: 1,
                        color: c,
                    });
                }
                (None, _) => out.extend(open.take()),
            }
        }
        out.extend(open);
    }
    out
}

/// Colours of the bomb template; the spark blinks with `blink`.
pub fn bomb_colour(ch: char, blink: bool) -> Option<Rgb> {
    match ch {
        'k' => Some(BOMB_BODY),
        'w' => Some(BOMB_SHINE),
        'f' => Some(BOMB_FUSE),
        'o' => Some(if blink { SPARK_A } else { SPARK_B }),
        _ => None,
    }
}

/// Pixels per side of a power icon template.
pub const ICON_SIZE: usize = 10;

/// Background of the icon of a bonus and of a malus (the frames are white and dark red).
pub const ICON_BONUS_FILL: Rgb = [0.176, 0.227, 0.388];
pub const ICON_MALUS_FILL: Rgb = [0.333, 0.314, 0.376];

/// The pixel picture of a power: extra bomb (a bomb with a plus), roller (a rollerskate), flame,
/// punch (a boxing glove), kick (a boot), slow (a snail), hyper speed (a lightning bolt) and
/// small bomb (a tiny bomb with a minus). `.` is transparent.
pub fn icon(kind: PowerUpKind) -> [&'static str; ICON_SIZE] {
    match kind {
        PowerUpKind::ExtraBomb => [
            "........w.",
            ".......www",
            "..kkkk..w.",
            ".kkkkkk...",
            "kkwkkkkk..",
            "kkwkkkkk..",
            "kkkkkkkk..",
            "kkkkkkkk..",
            ".kkkkkk...",
            "..kkkk....",
        ],
        PowerUpKind::Speed => [
            "..ww......",
            "..ww......",
            "..ww......",
            "..www.....",
            "..wwwwww..",
            "..wwwwwww.",
            ".kkkkkkkk.",
            "..yy..yy..",
            "..yy..yy..",
            "..........",
        ],
        PowerUpKind::Range => [
            "....oo....",
            "...oooo...",
            "...oyoo...",
            "..ooyyoo..",
            "..oyyyyo..",
            ".ooyyyyoo.",
            ".ooyyyyoo.",
            ".oyywwyyo.",
            "..oyyyyo..",
            "...oooo...",
        ],
        PowerUpKind::Punch => [
            "..........",
            "....rrrr..",
            "..wwrrrrr.",
            ".wwwrrwrrr",
            ".wwwrrrrrr",
            ".wwwrrrrrr",
            "..wwrrrrr.",
            "....rrrr..",
            "..........",
            "..........",
        ],
        PowerUpKind::Kick => [
            "..yyyy....",
            "...nn.....",
            "...nn.....",
            "...nn.....",
            "...nnnn...",
            "...nnnnnn.",
            "..nnnnnnnn",
            "..nnnnnnnn",
            "..kkkkkkkk",
            "..........",
        ],
        PowerUpKind::Slow => [
            "k.k.......",
            "k.k..nnnn.",
            "ggg.nnppnn",
            "ggg.npnpnn",
            "gggnnpppnn",
            "gggggnnnn.",
            "gggggggggg",
            ".gggggggg.",
            "..........",
            "..........",
        ],
        PowerUpKind::HyperSpeed => [
            "....yyyy..",
            "...yyyy...",
            "..yyyy....",
            "..yyyyyy..",
            "....yyy...",
            "...yyy....",
            "..yyy.....",
            "..yy......",
            ".yy.......",
            ".y........",
        ],
        PowerUpKind::SmallBomb => [
            "..........",
            "..........",
            "...o......",
            "..f.......",
            ".kkkk.....",
            "kwkkkk....",
            "kkkkkk.www",
            "kkkkkk....",
            ".kkkk.....",
            "..........",
        ],
    }
}

/// The colour of a character of an icon template.
pub fn icon_colour(_kind: PowerUpKind, ch: char) -> Option<Rgb> {
    Some(match ch {
        'k' => hex(0x1c1c22),
        'w' => hex(0xf4f4f4),
        'r' => hex(0xe03a3a),
        'o' => hex(0xf08a1a),
        'y' => hex(0xf7d12a),
        'g' => hex(0x4fbf5a),
        'p' => hex(0x8e5bd6),
        'n' => hex(0x9a6a35),
        'f' => hex(0x8c5a2b),
        _ => return None,
    })
}
