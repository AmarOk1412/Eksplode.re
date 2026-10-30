#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use eksplode_re_core::{Dir, GameState};
use eksplode_re_web_render::commands::{build, Command, Kind};
use eksplode_re_web_render::interp::Interp;
use eksplode_re_web_render::look::*;
use eksplode_re_web_render::sprites::{player_skin, Rgb};

const CANVAS: (f32, f32) = (800.0, 600.0);
const VIEWS: [View; 4] = [View::Down, View::Up, View::Left, View::Right];
const FRAMES: [Frame; 3] = [Frame::Idle, Frame::StepA, Frame::StepB];

fn look(gender: Gender, head: Head, face: Face) -> Look {
    Look {
        colour: 0,
        gender,
        head,
        face,
        costume: Costume::None,
    }
}

fn plain() -> Look {
    look(Gender::Male, Head::None, Face::None)
}

fn count(g: &[String], ch: char) -> usize {
    g.iter()
        .flat_map(|r| r.chars())
        .filter(|c| *c == ch)
        .count()
}

fn row(g: &[String], r: usize) -> Vec<char> {
    g[r].chars().collect()
}

#[test]
fn every_view_and_frame_is_a_full_picture_in_the_same_grid() {
    for head in [
        Head::None,
        Head::Cap,
        Head::TopHat,
        Head::Crown,
        Head::Headphones,
    ] {
        for face in [
            Face::None,
            Face::Glasses,
            Face::Sunglasses,
            Face::Moustache,
            Face::Mask,
        ] {
            for gender in [Gender::Male, Gender::Female] {
                for view in VIEWS {
                    for frame in FRAMES {
                        let g = grid_view(&look(gender, head, face), view, frame);
                        assert_eq!(g.len(), GRID_ROWS);
                        assert!(
                            g.iter().all(|r| r.chars().count() == 12),
                            "{view:?} {frame:?}"
                        );
                        assert!(count(&g, 'B') >= 10, "{view:?} {frame:?} has a body");
                        assert!(count(&g, 'S') >= 2, "{view:?} {frame:?} has shoes");
                    }
                }
            }
        }
    }
    // the old front picture is the front idle view
    assert_eq!(grid(&plain()), grid_view(&plain(), View::Down, Frame::Idle));
}

#[test]
fn f3_us1_as8_the_view_follows_the_direction() {
    assert_eq!(View::from_dir(Dir::Down), View::Down);
    assert_eq!(View::from_dir(Dir::Up), View::Up);
    assert_eq!(View::from_dir(Dir::Left), View::Left);
    assert_eq!(View::from_dir(Dir::Right), View::Right);
}

#[test]
fn f3_us1_as8_the_back_has_no_face_and_the_side_shows_one_eye() {
    let front = grid_view(&plain(), View::Down, Frame::Idle);
    let back = grid_view(&plain(), View::Up, Frame::Idle);
    let side = grid_view(&plain(), View::Left, Frame::Idle);
    assert_eq!(count(&front, 'e'), 4, "two eyes of two pixels");
    assert_eq!(count(&back, 'e'), 0);
    assert_eq!(count(&back, 'F'), 0, "no face from behind");
    assert_eq!(count(&back, 'Y'), 0, "no buckle from behind");
    assert_eq!(count(&side, 'e'), 2, "one eye of two pixels");
    assert!(count(&side, 'F') > 0, "the profile of the face");
    assert!(count(&side, 'F') < count(&front, 'F'));
    // three different pictures
    assert_ne!(front, back);
    assert_ne!(front, side);
    assert_ne!(back, side);
}

#[test]
fn f3_us1_as8_right_is_the_mirror_of_left() {
    for head in [Head::None, Head::Cap, Head::Headphones] {
        for face in [Face::None, Face::Glasses, Face::Moustache] {
            for frame in FRAMES {
                let l = grid_view(&look(Gender::Female, head, face), View::Left, frame);
                let r = grid_view(&look(Gender::Female, head, face), View::Right, frame);
                for (a, b) in l.iter().zip(&r) {
                    assert_eq!(a.chars().rev().collect::<String>(), *b);
                }
            }
        }
    }
}

#[test]
fn f3_us1_as8_face_accessories_are_seen_from_the_front_and_the_side_but_not_from_behind() {
    let cases = [
        (Face::Glasses, 'k'),
        (Face::Sunglasses, 's'),
        (Face::Moustache, 'm'),
        (Face::Mask, 'x'),
    ];
    for (face, ch) in cases {
        let l = look(Gender::Male, Head::None, face);
        assert!(
            count(&grid_view(&l, View::Down, Frame::Idle), ch) > 0,
            "{face:?} front"
        );
        assert!(
            count(&grid_view(&l, View::Left, Frame::Idle), ch) > 0,
            "{face:?} side"
        );
        assert_eq!(
            count(&grid_view(&l, View::Up, Frame::Idle), ch),
            0,
            "{face:?} back"
        );
        assert_ne!(
            grid_view(&l, View::Left, Frame::Idle),
            grid_view(&plain(), View::Left, Frame::Idle)
        );
    }
}

#[test]
fn f3_us1_as8_hats_and_headphones_are_seen_from_every_side_and_the_cap_brim_points_ahead() {
    for (head, ch) in [
        (Head::Cap, 'c'),
        (Head::TopHat, 't'),
        (Head::Crown, 'g'),
        (Head::Headphones, 'p'),
    ] {
        for view in VIEWS {
            let g = grid_view(&look(Gender::Male, head, Face::None), view, Frame::Idle);
            assert!(count(&g, ch) > 0, "{head:?} {view:?}");
        }
    }
    let cap = look(Gender::Male, Head::Cap, Face::None);
    let left = grid_view(&cap, View::Left, Frame::Idle);
    let right = grid_view(&cap, View::Right, Frame::Idle);
    let back = grid_view(&cap, View::Up, Frame::Idle);
    assert!(
        row(&left, 4)[..4].contains(&'c'),
        "the brim is on the left when facing left"
    );
    assert!(
        row(&right, 4)[8..].contains(&'c'),
        "and on the right when facing right"
    );
    assert!(!row(&back, 4).contains(&'c'), "no brim from behind");
}

#[test]
fn f3_us1_as8_gender_and_colour_survive_every_view() {
    let female = look(Gender::Female, Head::None, Face::None);
    for view in VIEWS {
        let g = grid_view(&female, view, Frame::Idle);
        assert!(count(&g, 'h') > 0, "{view:?} has hair");
        assert_ne!(g, grid_view(&plain(), view, Frame::Idle), "{view:?}");
    }
    assert!(
        count(&grid_view(&female, View::Down, Frame::Idle), 'l') > 0,
        "lashes from the front"
    );
    assert_eq!(count(&grid_view(&female, View::Up, Frame::Idle), 'l'), 0);
    // the same skin colours are used in every view
    for colour in [0usize, 4, 9] {
        let l = Look { colour, ..plain() };
        let base: std::collections::BTreeSet<String> =
            runs_view(&l, View::Down, Frame::Idle, GRID_ROWS)
                .iter()
                .map(|r| format!("{:?}", r.color))
                .collect();
        for view in [View::Up, View::Left] {
            let other: std::collections::BTreeSet<String> =
                runs_view(&l, view, Frame::Idle, GRID_ROWS)
                    .iter()
                    .map(|r| format!("{:?}", r.color))
                    .collect();
            assert!(
                other.is_subset(&base) || base.is_subset(&other),
                "colour {colour} {view:?}"
            );
        }
    }
}

#[test]
fn f3_us1_as9_the_walk_frames_differ_from_standing_and_from_each_other() {
    for view in VIEWS {
        let idle = grid_view(&plain(), view, Frame::Idle);
        let a = grid_view(&plain(), view, Frame::StepA);
        let b = grid_view(&plain(), view, Frame::StepB);
        assert_ne!(idle, a, "{view:?}");
        assert_ne!(idle, b, "{view:?}");
        assert_ne!(a, b, "{view:?}");
        // only the legs and arms move: the head is the same in every frame
        assert_eq!(
            idle[..9],
            a[..9].to_vec()[..],
            "{view:?} head rows stay still"
        );
        assert_eq!(
            idle[..9],
            b[..9].to_vec()[..],
            "{view:?} head rows stay still"
        );
    }
}

// --- choosing view and frame while the match runs --------------------------------------------

fn at_tick(s: &mut GameState, tick: u32, x: u8) -> eksplode_re_core::Observation {
    s.tick = tick;
    s.players[0].pos = eksplode_re_core::Pos::new(x, 3);
    obs(s)
}

#[test]
fn f3_us1_as9_a_player_who_just_moved_walks_and_one_who_stands_idles() {
    let mut s = small(&OPEN, (1, 3), (5, 5));
    let mut i = Interp::default();
    i.update(&at_tick(&mut s, 10, 1));
    assert_eq!(i.pose(0, 0.0), Frame::Idle, "a player that never moved");
    i.update(&at_tick(&mut s, 11, 2));
    assert_eq!(i.pose(0, 0.0), Frame::StepA, "the first step");
    assert_eq!(i.pose(1, 0.0), Frame::Idle, "somebody else stands");
    // halfway through the interval the feet pass each other
    i.update(&at_tick(&mut s, 13, 2));
    assert_eq!(i.pose(0, 0.0), Frame::Idle);
    // the next step uses the other foot
    i.update(&at_tick(&mut s, 15, 3));
    assert_eq!(i.pose(0, 0.0), Frame::StepB);
    i.update(&at_tick(&mut s, 19, 4));
    assert_eq!(i.pose(0, 0.0), Frame::StepA, "and the cycle goes on");
    // standing for a while goes back to idle
    i.update(&at_tick(&mut s, 40, 4));
    assert_eq!(i.pose(0, 0.5), Frame::Idle);
}

#[test]
fn f3_us1_as9_the_pace_follows_the_speed() {
    // after one step, count the ticks the foot stays out: it is longer for a slower player
    let out = |cooldown: u32| {
        let mut s = small(&OPEN, (1, 3), (5, 5));
        s.players[0].move_cooldown = cooldown;
        let mut i = Interp::default();
        i.update(&at_tick(&mut s, 10, 1));
        i.update(&at_tick(&mut s, 11, 2));
        (0..12)
            .filter(|k| {
                i.update(&at_tick(&mut s, 11 + k, 2));
                i.pose(0, 0.0) != Frame::Idle
            })
            .count()
    };
    assert!(out(8) > out(4), "{} vs {}", out(8), out(4));
}

#[test]
fn f3_us1_as9_a_new_match_starts_idle() {
    let mut s = small(&OPEN, (1, 3), (5, 5));
    let mut i = Interp::default();
    i.update(&at_tick(&mut s, 10, 1));
    i.update(&at_tick(&mut s, 11, 2));
    assert_eq!(i.pose(0, 0.0), Frame::StepA);
    i.update(&at_tick(&mut s, 0, 1));
    assert_eq!(i.pose(0, 0.0), Frame::Idle);
}

fn quads_with(list: &[Command], rgb: Rgb) -> Vec<&Command> {
    list.iter()
        .filter(|c| c.kind == Kind::Quad && c.color[..3] == rgb && c.color[3] == 1.0)
        .collect()
}

fn draw(s: &mut GameState, ticks: &[(u32, u8)], alpha: f32) -> Vec<Command> {
    let mut i = Interp::default();
    let mut last = None;
    for (t, x) in ticks {
        let o = at_tick(s, *t, *x);
        i.update(&o);
        last = Some(o);
    }
    build(&last.unwrap(), &i, alpha, CANVAS)
}

#[test]
fn f3_us1_as8_the_match_draws_the_view_of_the_facing() {
    let eyes = player_skin(0).eyes;
    let mut counts = Vec::new();
    for dir in [Dir::Down, Dir::Up, Dir::Left, Dir::Right] {
        let mut s = small(&OPEN, (1, 3), (5, 5));
        s.players[0].facing = dir;
        let list = draw(&mut s, &[(10, 1)], 1.0);
        counts.push(quads_with(&list, eyes).len());
    }
    assert_eq!(
        counts,
        [4, 0, 2, 2],
        "eyes seen when facing down, up, left and right"
    );
    // a new round faces down
    let s = small(&OPEN, (1, 3), (5, 5));
    assert_eq!(s.players[0].facing, Dir::Down);
}

#[test]
fn f3_us1_as9_a_walking_sprite_bobs_and_changes_its_feet() {
    let helmet = player_skin(0).helmet;
    let top = |list: &[Command]| {
        quads_with(list, helmet)
            .iter()
            .map(|c| c.points[0].1)
            .fold(f32::MAX, f32::min)
    };
    let mut s = small(&OPEN, (1, 3), (5, 5));
    let standing = draw(&mut s, &[(10, 2)], 1.0);
    let mut s = small(&OPEN, (1, 3), (5, 5));
    // the same position, but reached by a step just now
    let walking = draw(&mut s, &[(10, 2), (11, 2)], 0.0);
    let mut s = small(&OPEN, (1, 3), (5, 5));
    let stepping = draw(&mut s, &[(10, 1), (11, 2)], 1.0);
    assert!(
        top(&stepping) < top(&standing),
        "a step frame is one pixel higher"
    );
    assert_eq!(top(&walking), top(&standing), "no movement means no bob");
    assert_ne!(stepping, standing);
}

#[test]
fn f3_us1_as9_nothing_leaves_the_canvas_in_any_view() {
    for dir in [Dir::Down, Dir::Up, Dir::Left, Dir::Right] {
        let mut s = small(&OPEN, (1, 1), (5, 5));
        s.players[0].facing = dir;
        for list in [
            draw(&mut s, &[(10, 1), (11, 2)], 0.5),
            draw(&mut s, &[(10, 1)], 1.0),
        ] {
            for c in list {
                for (x, y) in c.points {
                    assert!(
                        (0.0..=CANVAS.0).contains(&x) && (0.0..=CANVAS.1).contains(&y),
                        "{dir:?}"
                    );
                }
            }
        }
    }
}
