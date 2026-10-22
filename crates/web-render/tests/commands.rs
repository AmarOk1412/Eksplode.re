mod common;

use bomberust_core::{GameState, PowerUpKind};
use bomberust_web_render::commands::{build, flatten, Command, Kind};
use bomberust_web_render::cube::{
    badge_char, player_color, shade, FACE_LEFT, FACE_RIGHT, FACE_TOP, WALL_LEFT, WALL_RIGHT,
    WALL_TOP,
};
use bomberust_web_render::interp::Interp;
use common::*;

const CANVAS: (f32, f32) = (800.0, 600.0);

fn commands(s: &GameState) -> Vec<Command> {
    let o = obs(s);
    let mut i = Interp::default();
    i.update(&o);
    build(&o, &i, 1.0, CANVAS)
}

fn quads_with_rgb(list: &[Command], rgb: [f32; 3]) -> usize {
    list.iter()
        .filter(|c| c.kind == Kind::Quad && c.color[..3] == rgb && c.color[3] == 1.0)
        .count()
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
        "the first command is a floor quad"
    );
}

#[test]
fn us1_as3_each_player_is_one_cube_in_their_own_colour_with_a_badge() {
    let list = commands(&standard());
    for slot in 0..4u8 {
        let c = player_color(slot);
        for f in [FACE_TOP, FACE_LEFT, FACE_RIGHT] {
            assert_eq!(
                quads_with_rgb(&list, shade(c, f)),
                1,
                "slot {slot} face {f}"
            );
        }
        assert!(texts(&list).contains(&badge_char(slot)), "badge {slot}");
    }
    assert_eq!(texts(&list).len(), 4);
}

#[test]
fn walls_are_three_face_boxes() {
    let list = commands(&standard());
    // 15x13 arena: 52 border walls + 30 pillars
    for c in [WALL_TOP, WALL_LEFT, WALL_RIGHT] {
        assert_eq!(quads_with_rgb(&list, c), 82);
    }
}

#[test]
fn eliminated_players_are_not_drawn() {
    let mut s = standard();
    s.players[2].alive = false;
    let list = commands(&s);
    assert_eq!(quads_with_rgb(&list, shade(player_color(2), FACE_TOP)), 0);
    assert!(!texts(&list).contains(&badge_char(2)));
    assert!(texts(&list).contains(&badge_char(1)));
}

#[test]
fn bombs_are_circles_that_pulse_with_the_fuse() {
    let radius = |ticks| {
        let mut s = small(&OPEN, (1, 1), (5, 5));
        bomb(&mut s, 3, 3, ticks);
        let circles: Vec<Command> = commands(&s)
            .into_iter()
            .filter(|c| c.kind == Kind::Circle)
            .collect();
        assert_eq!(circles.len(), 1);
        circles[0].size
    };
    assert!(radius(10) > 0.0);
    assert_ne!(radius(10), radius(11));
}

#[test]
fn flames_are_translucent() {
    let mut s = small(&OPEN, (1, 1), (5, 5));
    flame(&mut s, 3, 3);
    let list = commands(&s);
    assert!(list
        .iter()
        .any(|c| c.kind == Kind::Quad && c.color[3] < 1.0 && c.color[3] > 0.0));
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
fn everything_lands_inside_the_canvas() {
    let list = commands(&standard());
    for c in &list {
        if c.kind == Kind::Quad || c.kind == Kind::Triangle {
            for (x, y) in c.points {
                assert!(
                    (0.0..=CANVAS.0).contains(&x) && (0.0..=CANVAS.1).contains(&y),
                    "{c:?}"
                );
            }
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
