mod common;

use bomberust_core::{ActiveMalus, Config, Dir, Flight, GameState, Pos, PowerUpKind};
use bomberust_web_render::commands::{build, flatten, portrait, Command, Kind};
use bomberust_web_render::interp::Interp;
use bomberust_web_render::sprites::*;
use bomberust_web_render::view::fit;
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
    let sprite =
        bomberust_web_render::look::runs_for(&bomberust_web_render::look::default_for(2)).len();
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
fn powerups_fall_back_to_their_letter_on_a_tiny_board() {
    let mut s = small(&OPEN, (1, 1), (5, 5));
    for (i, kind) in PowerUpKind::ALL.iter().enumerate() {
        powerup(&mut s, 1 + (i as u8 % 5), 2 + (i as u8 / 5), *kind);
    }
    let o = obs(&s);
    let mut interp = Interp::default();
    interp.update(&o);
    let t = texts(&build(&o, &interp, 1.0, (130.0, 130.0)));
    for letter in ['B', 'R', 'F', 'P', 'K', 'S', 'H', 's'] {
        assert!(t.contains(&letter), "{letter}");
    }
}

fn icon(kind: PowerUpKind) -> Vec<Command> {
    let mut s = small(&OPEN, (1, 1), (5, 5));
    powerup(&mut s, 3, 3, kind);
    let list = commands(&s);
    // everything the board and the players draw is the same for every kind
    let base = commands(&small(&OPEN, (1, 1), (5, 5)));
    list.into_iter().filter(|c| !base.contains(c)).collect()
}

#[test]
fn the_power_icons_are_distinct_per_kind() {
    let mut seen = std::collections::BTreeSet::new();
    for kind in PowerUpKind::ALL {
        let list = icon(kind);
        let key = (
            texts(&list),
            list.iter()
                .map(|c| c.color.map(|v| (v * 255.0) as u32))
                .collect::<Vec<_>>(),
        );
        assert!(
            seen.insert(format!("{key:?}")),
            "{kind:?} looks like another kind"
        );
    }
    assert_eq!(seen.len(), 8);
}

#[test]
fn bonuses_have_a_white_frame_and_maluses_a_dark_red_one() {
    for kind in PowerUpKind::ALL {
        let list = icon(kind);
        let white = !quads_with(&list, WHITE).is_empty();
        let red = !quads_with(&list, MALUS_FRAME).is_empty();
        assert_eq!(white, !kind.is_malus(), "{kind:?}");
        assert_eq!(red, kind.is_malus(), "{kind:?}");
    }
}

fn flying(hop_left: u32) -> GameState {
    let mut s = small(&OPEN, (1, 1), (5, 5));
    bomb(&mut s, 3, 3, 30);
    s.bombs[0].flight = Some(Flight {
        dir: Dir::Right,
        hop_left,
        origin: Pos::new(3, 3),
    });
    s
}

fn body_box(list: &[Command]) -> (f32, f32) {
    let body = quads_with(list, BOMB_BODY);
    let lo_x = body.iter().map(|c| c.points[0].0).fold(f32::MAX, f32::min);
    let lo_y = body.iter().map(|c| c.points[0].1).fold(f32::MAX, f32::min);
    (lo_x, lo_y)
}

#[test]
fn a_bomb_in_flight_is_lifted_along_an_arc_and_has_a_shadow() {
    let hop = Config::default().punch_hop_ticks;
    let landed = {
        let mut s = small(&OPEN, (1, 1), (5, 5));
        bomb(&mut s, 3, 3, 30);
        commands(&s)
    };
    let (x0, y0) = body_box(&landed);
    let scale = fit(7, 7, CANVAS).scale;
    // halfway through the hop: half a tile on and lifted by sin(pi / 2) * 0.5 tile
    let list = commands(&flying(hop - hop / 2));
    let (x1, y1) = body_box(&list);
    assert!((x1 - x0 - 0.5 * scale).abs() <= 2.0, "moved {}", x1 - x0);
    assert!((y0 - y1 - 0.5 * scale).abs() <= 2.0, "lifted {}", y0 - y1);
    assert_eq!(list.len(), landed.len() + 1, "one shadow more");
    // at the start of a hop it sits on the ground
    let (x2, y2) = body_box(&commands(&flying(hop)));
    assert!((x2 - x0).abs() <= 1.0 && (y2 - y0).abs() <= 1.0);
}

#[test]
fn a_player_with_a_malus_gets_a_coloured_pulse() {
    let halo = |kind: Option<PowerUpKind>, tint: Rgb| {
        let mut s = small(&OPEN, (1, 1), (5, 5));
        s.players[0].malus = kind.map(|kind| ActiveMalus {
            kind,
            ticks_left: 50,
        });
        commands(&s)
            .iter()
            .filter(|c| c.color[..3] == tint && c.color[3] > 0.0 && c.color[3] < 1.0)
            .count()
    };
    assert!(halo(Some(PowerUpKind::Slow), MALUS_SLOW) > 0);
    assert!(halo(Some(PowerUpKind::HyperSpeed), MALUS_HYPER) > 0);
    assert!(halo(Some(PowerUpKind::SmallBomb), MALUS_SMALL) > 0);
    assert_eq!(halo(None, MALUS_SLOW), 0);
    assert_eq!(halo(Some(PowerUpKind::Slow), MALUS_HYPER), 0);
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

/// Quads of the warning on one square: the red frame pieces and the dark block shadow.
fn warning_quads(list: &[Command]) -> (Vec<&Command>, Vec<&Command>) {
    let frame = list
        .iter()
        .filter(|c| c.kind == Kind::Quad && c.color[..3] == WARNING_FRAME)
        .collect();
    let shadow = list
        .iter()
        .filter(|c| c.kind == Kind::Quad && c.color[..3] == WARNING_SHADOW)
        .collect();
    (frame, shadow)
}

fn width_of(c: &Command) -> f32 {
    c.points[1].0 - c.points[0].0
}

#[test]
fn f4_us2_as6_an_announced_square_is_a_red_frame_with_a_growing_block_shadow() {
    let mut quiet = standard();
    let none = commands(&quiet);
    assert!(
        warning_quads(&none).0.is_empty(),
        "nothing announced, nothing red"
    );

    let lead = quiet.config.closing_lead_ticks;
    quiet.closing.falling.push(bomberust_core::Falling {
        pos: bomberust_core::Pos::new(7, 6),
        ticks_left: lead,
    });
    let early = commands(&quiet);
    let (frame, shadow_early) = warning_quads(&early);
    assert_eq!(frame.len(), 4, "a frame of four bars");
    assert_eq!(shadow_early.len(), 1);
    assert!(frame.iter().all(|c| c.color[3] > 0.0 && c.color[3] <= 1.0));

    quiet.closing.falling[0].ticks_left = 2;
    let late = commands(&quiet);
    let (_, shadow_late) = warning_quads(&late);
    assert!(
        width_of(shadow_late[0]) > width_of(shadow_early[0]) * 1.5,
        "the block shadow grows as the landing comes closer: {} vs {}",
        width_of(shadow_late[0]),
        width_of(shadow_early[0])
    );
    assert_eq!(
        commands(&quiet),
        late,
        "the same observation draws the same frame"
    );
}

#[test]
fn f4_us2_as6_a_landed_block_is_an_ordinary_wall() {
    let mut s = standard();
    let before = quads_with(&commands(&s), WALL_BASE).len();
    s.arena
        .set_tile(bomberust_core::Pos::new(7, 6), bomberust_core::Tile::Wall);
    s.arena
        .set_tile(bomberust_core::Pos::new(7, 5), bomberust_core::Tile::Wall);
    let after = quads_with(&commands(&s), WALL_BASE).len();
    assert!(after > before, "{before} -> {after}");
}

#[test]
fn announced_squares_are_drawn_below_players_and_bombs() {
    let mut s = standard();
    s.closing.falling.push(bomberust_core::Falling {
        pos: s.players[0].pos,
        ticks_left: 10,
    });
    let list = commands(&s);
    let (frame, _) = warning_quads(&list);
    let helmet = quads_with(&list, player_skin(0).helmet);
    assert!(!helmet.is_empty());
    let top_frame = frame.iter().map(|c| c.layer).fold(f32::MIN, f32::max);
    let low_player = helmet.iter().map(|c| c.layer).fold(f32::MAX, f32::min);
    assert!(top_frame < low_player, "the warning is under the player");
}
