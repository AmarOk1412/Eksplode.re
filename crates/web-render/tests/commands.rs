mod common;

use bomberust_core::{GameState, PowerUpKind};
use bomberust_web_render::commands::{build, flatten, portrait, Command, Kind};
use bomberust_web_render::interp::Interp;
use bomberust_web_render::sprites::*;
use common::*;

const CANVAS: (f32, f32) = (800.0, 600.0);

fn commands(s: &GameState) -> Vec<Command> {
    let o = obs(s);
    let mut i = Interp::default();
    i.update(&o);
    build(&o, &i, 1.0, CANVAS)
}

fn quads_with(list: &[Command], rgb: Rgb) -> Vec<&Command> {
    list.iter()
        .filter(|c| c.kind == Kind::Quad && c.color[..3] == rgb && c.color[3] == 1.0)
        .collect()
}

fn texts(list: &[Command]) -> Vec<char> {
    list.iter()
        .filter(|c| c.kind == Kind::Text)
        .map(|c| char::from(c.text as u8))
        .collect()
}

#[test]
fn the_buffer_has_a_stride_of_sixteen() {
    assert_eq!(Command::STRIDE, 16);
    let list = commands(&standard());
    let flat = flatten(&list);
    assert_eq!(flat.len(), list.len() * 16);
    assert_eq!(
        flat[0],
        Kind::Quad as u8 as f32,
        "the first command is the board"
    );
}

#[test]
fn the_board_comes_first_with_a_front_edge() {
    let list = commands(&standard());
    assert_eq!(list[0].color[..3], BOARD_EDGE);
    assert_eq!(list[1].color[..3], BOARD);
    assert!(
        list[0].points[2].1 > list[1].points[2].1,
        "the edge sticks out below"
    );
}

#[test]
fn us1_as3_each_player_is_a_sprite_in_their_own_colours() {
    let list = commands(&standard());
    for slot in 0..4u8 {
        let skin = player_skin(slot);
        for (what, c) in [
            ("helmet", skin.helmet),
            ("body", skin.body),
            ("eyes", skin.eyes),
        ] {
            assert!(!quads_with(&list, c).is_empty(), "slot {slot} {what}");
        }
    }
    let helmets: Vec<Rgb> = (0..4).map(|s| player_skin(s).helmet).collect();
    for i in 0..4 {
        for j in i + 1..4 {
            assert_ne!(helmets[i], helmets[j], "slots {i} and {j} look different");
        }
    }
}

#[test]
fn walls_are_grey_tiles_with_a_white_window() {
    let list = commands(&standard());
    // 15x13 arena: 52 border walls + 30 pillars
    assert_eq!(quads_with(&list, WALL_WINDOW).len(), 82);
    assert_eq!(quads_with(&list, WALL_BASE).len(), 82);
}

#[test]
fn blocks_are_bricks() {
    let rows = [
        "#######", "#.....#", "#..+..#", "#..+..#", "#.....#", "#.....#", "#######",
    ];
    let list = commands(&small(&rows, (1, 1), (5, 5)));
    assert_eq!(quads_with(&list, BRICK_BASE).len(), 2);
    // four courses and four joints per block
    assert_eq!(quads_with(&list, BRICK_LINE).len(), 16);
}

#[test]
fn eliminated_players_are_not_drawn() {
    let alive = commands(&standard());
    let mut s = standard();
    s.players[2].alive = false;
    let dead = commands(&s);
    let skin = player_skin(2);
    let sprite = runs(&PLAYER, PLAYER.len(), |c| skin.colour_of(c)).len();
    assert_eq!(
        alive.len() - dead.len(),
        sprite + 1,
        "sprite plus its shadow"
    );
}

#[test]
fn bombs_pulse_with_the_fuse() {
    let width = |ticks| {
        let mut s = small(&OPEN, (1, 1), (5, 5));
        bomb(&mut s, 3, 3, ticks);
        let list = commands(&s);
        let body = quads_with(&list, BOMB_BODY);
        assert!(!body.is_empty());
        let (lo, hi) = body.iter().fold((f32::MAX, f32::MIN), |(lo, hi), c| {
            let xs = c.points.map(|p| p.0);
            (lo.min(xs[0]), hi.max(xs[1]))
        });
        hi - lo
    };
    let sizes: std::collections::BTreeSet<u32> = (1..40).map(|t| width(t) as u32).collect();
    assert!(sizes.len() >= 2, "{sizes:?}");
}

#[test]
fn flames_fill_their_tile_in_three_layers() {
    let mut s = small(&OPEN, (1, 1), (5, 5));
    flame(&mut s, 3, 3);
    let list = commands(&s);
    for c in [FLAME_OUTER, FLAME_MID, FLAME_CORE] {
        assert_eq!(quads_with(&list, c).len(), 1);
    }
}

#[test]
fn powerups_show_their_letter() {
    let mut s = small(&OPEN, (1, 1), (5, 5));
    powerup(&mut s, 2, 2, PowerUpKind::ExtraBomb);
    powerup(&mut s, 3, 2, PowerUpKind::Range);
    powerup(&mut s, 4, 2, PowerUpKind::Speed);
    let t = texts(&commands(&s));
    for letter in ['B', 'R', 'S'] {
        assert!(t.contains(&letter), "{letter}");
    }
}

#[test]
fn commands_are_ordered_back_to_front() {
    let list = commands(&standard());
    assert!(list.windows(2).all(|w| w[0].layer <= w[1].layer));
}

#[test]
fn the_same_input_gives_the_same_buffer() {
    let s = standard();
    assert_eq!(flatten(&commands(&s)), flatten(&commands(&s)));
}

#[test]
fn pixels_are_snapped_to_whole_canvas_pixels() {
    for c in commands(&standard())
        .iter()
        .filter(|c| c.kind == Kind::Quad)
    {
        for (x, y) in c.points {
            assert_eq!((x.fract(), y.fract()), (0.0, 0.0), "{c:?}");
        }
    }
}

#[test]
fn everything_lands_inside_the_canvas() {
    for c in commands(&standard())
        .iter()
        .filter(|c| c.kind == Kind::Quad)
    {
        for (x, y) in c.points {
            assert!(
                (0.0..=CANVAS.0).contains(&x) && (0.0..=CANVAS.1).contains(&y),
                "{c:?}"
            );
        }
    }
}

#[test]
fn a_tiny_canvas_does_not_break_anything() {
    let o = obs(&standard());
    let mut i = Interp::default();
    i.update(&o);
    let list = build(&o, &i, 0.5, (0.0, 0.0));
    assert!(list
        .iter()
        .all(|c| c.points.iter().all(|p| p.0.is_finite() && p.1.is_finite())));
}

#[test]
fn portraits_fit_their_canvas_and_differ_per_slot() {
    let full = portrait(0, (160.0, 226.0), 0);
    let head = portrait(0, (160.0, 226.0), PORTRAIT_ROWS);
    assert!(full.len() > head.len() && head.len() > 10);
    for c in &full {
        for (x, y) in c.points {
            assert!(
                (0.0..=160.0).contains(&x) && (0.0..=226.0).contains(&y),
                "{c:?}"
            );
        }
    }
    let colours = |slot| -> Vec<[f32; 4]> {
        portrait(slot, (100.0, 100.0), 0)
            .iter()
            .map(|c| c.color)
            .collect()
    };
    assert_ne!(colours(0), colours(1));
    assert_ne!(colours(2), colours(3));
    assert_eq!(colours(0), colours(4), "slots wrap around");
}

#[test]
fn portraits_of_odd_sizes_are_empty_or_finite() {
    assert!(portrait(0, (0.0, 10.0), 0).is_empty());
    assert!(portrait(0, (f32::NAN, 10.0), 0).is_empty());
    assert!(portrait(0, (-3.0, 10.0), 0).is_empty());
    let tiny = portrait(1, (3.0, 3.0), 99);
    assert!(tiny
        .iter()
        .all(|c| c.points.iter().all(|p| p.0.is_finite())));
}
