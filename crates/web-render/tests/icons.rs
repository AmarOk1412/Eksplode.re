#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use eksplode_re_core::{Config, Dir, GameState, PowerUpKind, Slide};
use eksplode_re_web_render::commands::{build, Command, Kind};
use eksplode_re_web_render::interp::Interp;
use eksplode_re_web_render::sprites::{icon, icon_colour, BOMB_BODY};
use eksplode_re_web_render::view::fit;

const CANVAS: (f32, f32) = (800.0, 600.0);

fn frame_of(s: &GameState, canvas: (f32, f32)) -> Vec<Command> {
    let o = obs(s);
    let mut i = Interp::default();
    i.update(&o);
    build(&o, &i, 1.0, canvas)
}

fn only_the_icon(kind: PowerUpKind, canvas: (f32, f32)) -> Vec<Command> {
    let mut s = small(&OPEN, (1, 1), (5, 5));
    powerup(&mut s, 3, 3, kind);
    let base = frame_of(&small(&OPEN, (1, 1), (5, 5)), canvas);
    frame_of(&s, canvas)
        .into_iter()
        .filter(|c| !base.contains(c))
        .collect()
}

#[test]
fn f3_us2_as10_every_power_has_its_own_ten_by_ten_pixel_picture() {
    let mut seen = std::collections::BTreeSet::new();
    for kind in PowerUpKind::ALL {
        let rows = icon(kind);
        assert_eq!(rows.len(), 10, "{kind:?}");
        assert!(rows.iter().all(|r| r.chars().count() == 10), "{kind:?}");
        let pixels = rows
            .iter()
            .flat_map(|r| r.chars())
            .filter(|c| icon_colour(kind, *c).is_some())
            .count();
        assert!((20..=90).contains(&pixels), "{kind:?} has {pixels} pixels");
        assert!(
            seen.insert(rows.join("/")),
            "{kind:?} looks like another kind"
        );
    }
    assert_eq!(seen.len(), 8);
}

#[test]
fn f3_us2_as10_at_normal_size_an_icon_is_pixels_not_a_letter() {
    for kind in PowerUpKind::ALL {
        let list = only_the_icon(kind, CANVAS);
        assert!(
            !list.iter().any(|c| c.kind == Kind::Text),
            "{kind:?} is drawn as a letter"
        );
        // a frame, a background and at least several runs of pixels
        assert!(list.len() >= 10, "{kind:?}: {} commands", list.len());
    }
}

#[test]
fn f11_a_tiny_icon_still_stays_pixels_not_a_letter() {
    let scale = fit(7, 7, (130.0, 130.0)).scale;
    assert!(0.48 * scale < 20.0, "the test canvas must be tiny");
    for kind in PowerUpKind::ALL {
        let list = only_the_icon(kind, (130.0, 130.0));
        assert!(
            !list.iter().any(|c| c.kind == Kind::Text),
            "{kind:?} is drawn as a letter"
        );
    }
}

#[test]
fn f3_us2_as10_icon_pixels_are_whole_canvas_pixels_inside_the_tile() {
    let scale = fit(7, 7, CANVAS).scale;
    for kind in PowerUpKind::ALL {
        for c in only_the_icon(kind, CANVAS) {
            if c.kind != Kind::Quad {
                continue;
            }
            for (x, y) in c.points {
                assert_eq!((x.fract(), y.fract()), (0.0, 0.0), "{kind:?}");
            }
            let w = c.points[1].0 - c.points[0].0;
            assert!(w <= 0.7 * scale, "{kind:?}: a quad {w} px wide");
        }
    }
}

fn slider(slide: Option<Slide>) -> GameState {
    let mut s = small(&OPEN, (1, 1), (5, 5));
    bomb(&mut s, 3, 3, 60);
    s.bombs[0].slide = slide;
    s
}

fn body_x(list: &[Command]) -> f32 {
    list.iter()
        .filter(|c| c.kind == Kind::Quad && c.color[..3] == BOMB_BODY && c.color[3] == 1.0)
        .map(|c| c.points[0].0)
        .fold(f32::MAX, f32::min)
}

#[test]
fn f3_us2_as12_a_sliding_bomb_is_drawn_between_squares_and_only_while_it_can_move() {
    let scale = fit(7, 7, CANVAS).scale;
    let slide_ticks = Config::default().slide_ticks;
    let landed = frame_of(&slider(None), CANVAS);
    // two of three ticks into a step
    let moving = frame_of(
        &slider(Some(Slide {
            dir: Dir::Right,
            step_left: 1,
        })),
        CANVAS,
    );
    let dx = body_x(&moving) - body_x(&landed);
    let progress = 1.0 - 1.0 / slide_ticks as f32;
    let expected = (progress + 1.0 / slide_ticks as f32) * scale; // alpha 1.0 completes the step
    assert!(
        (dx - expected).abs() <= 2.0,
        "moved {dx}, expected {expected}"
    );
    // no square ahead: it stays on its square
    let mut s = slider(Some(Slide {
        dir: Dir::Right,
        step_left: 1,
    }));
    bomb(&mut s, 4, 3, 60);
    let blocked = frame_of(&s, CANVAS);
    let single = body_x(&blocked);
    assert!(
        single <= body_x(&landed) + 1.0,
        "a blocked bomb does not slide into the other"
    );
    // it is not lifted like a flying bomb: the same height as a landed one
    let ys = |l: &[Command]| {
        l.iter()
            .filter(|c| c.kind == Kind::Quad && c.color[..3] == BOMB_BODY)
            .map(|c| c.points[0].1)
            .fold(f32::MAX, f32::min)
    };
    assert!((ys(&moving) - ys(&landed)).abs() <= 1.0);
}
