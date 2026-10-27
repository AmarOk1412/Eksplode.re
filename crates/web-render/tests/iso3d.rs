#![allow(clippy::unwrap_used)]

use eksplode_re_web_render::commands::{Command, Kind};
use eksplode_re_web_render::iso3d::*;
use eksplode_re_web_render::look::{default_for, Face, Gender, Head, Look};
use eksplode_re_web_render::sprites::{player_skin, Rgb};

fn look(colour: usize, head: Head, face: Face) -> Look {
    Look {
        colour,
        gender: Gender::Male,
        head,
        face,
    }
}

fn plain() -> Look {
    look(0, Head::None, Face::None)
}

fn quads_with(list: &[Command], rgb: Rgb) -> Vec<&Command> {
    list.iter()
        .filter(|c| c.kind == Kind::Quad && c.color[..3] == rgb && c.color[3] == 1.0)
        .collect()
}

#[test]
fn the_projection_is_two_to_one_and_round_trips() {
    // one tile step along x moves right and down by 2:1
    let o = project([0.0, 0.0, 0.0]);
    let x = project([1.0, 0.0, 0.0]);
    let y = project([0.0, 1.0, 0.0]);
    assert_eq!((x.0 - o.0, x.1 - o.1), (1.0, 0.5));
    assert_eq!((y.0 - o.0, y.1 - o.1), (-1.0, 0.5));
    // up is up on the screen
    assert_eq!(project([0.0, 0.0, 1.0]).1, o.1 - 1.0);
    for (px, py) in [(0.0, 0.0), (1.5, -2.0), (-3.25, 4.75), (10.0, 10.0)] {
        let s = project([px, py, 0.0]);
        let (rx, ry) = unproject_ground(s);
        assert!(
            (rx - px).abs() < 1e-5 && (ry - py).abs() < 1e-5,
            "{px},{py}"
        );
    }
}

#[test]
fn f3_us1_as3_the_commands_stay_inside_the_rectangle() {
    for (w, h) in [(160.0, 226.0), (64.0, 64.0), (300.0, 120.0), (40.0, 400.0)] {
        for phase in [0.0, 0.5, 1.0, 3.0, 6.5, 100.0] {
            let list = portrait3d(&plain(), (w, h), phase);
            assert!(!list.is_empty(), "{w}x{h}");
            for c in &list {
                if c.kind == Kind::Text {
                    continue;
                }
                for (px, py) in c.points {
                    assert!(
                        (-0.01..=w + 0.01).contains(&px) && (-0.01..=h + 0.01).contains(&py),
                        "{w}x{h} phase {phase}: ({px}, {py})"
                    );
                }
            }
        }
    }
}

#[test]
fn invalid_sizes_give_nothing() {
    for size in [
        (0.0, 100.0),
        (100.0, 0.0),
        (-5.0, 100.0),
        (100.0, -1.0),
        (f32::NAN, 100.0),
        (100.0, f32::INFINITY),
    ] {
        assert!(portrait3d(&plain(), size, 0.0).is_empty(), "{size:?}");
    }
    // a bad phase still draws something finite
    for phase in [f32::NAN, f32::INFINITY, -3.0] {
        let list = portrait3d(&plain(), (100.0, 140.0), phase);
        assert!(!list.is_empty());
        assert!(list
            .iter()
            .all(|c| c.points.iter().all(|p| p.0.is_finite() && p.1.is_finite())));
    }
}

#[test]
fn the_same_phase_gives_the_same_commands() {
    let a = portrait3d(&plain(), (160.0, 226.0), 1.25);
    let b = portrait3d(&plain(), (160.0, 226.0), 1.25);
    assert_eq!(a, b);
}

#[test]
fn characters_differ_per_slot_colour_and_accessory() {
    let size = (160.0, 226.0);
    let slots: Vec<_> = (0..4)
        .map(|s| portrait3d(&default_for(s), size, 0.0))
        .collect();
    for i in 0..4 {
        for j in i + 1..4 {
            assert_ne!(slots[i], slots[j], "slot {i} and {j}");
        }
    }
    let blue = portrait3d(&look(0, Head::None, Face::None), size, 0.0);
    let green = portrait3d(&look(4, Head::None, Face::None), size, 0.0);
    assert_ne!(blue, green);
    for head in [Head::Cap, Head::TopHat, Head::Crown, Head::Headphones] {
        assert_ne!(
            portrait3d(&look(0, head, Face::None), size, 0.0),
            blue,
            "{head:?}"
        );
    }
    for face in [Face::Glasses, Face::Sunglasses, Face::Moustache, Face::Mask] {
        assert_ne!(
            portrait3d(&look(0, Head::None, face), size, 0.0),
            blue,
            "{face:?}"
        );
    }
    let female = Look {
        gender: Gender::Female,
        ..look(0, Head::None, Face::None)
    };
    assert_ne!(portrait3d(&female, size, 0.0), blue);
}

#[test]
fn head_faces_are_shaded_left_right_and_top() {
    assert_eq!((SHADE_TOP, SHADE_LEFT, SHADE_RIGHT), (1.0, 0.8, 0.6));
    let helmet = player_skin(0).helmet;
    let list = portrait3d(&plain(), (200.0, 280.0), 0.0);
    for s in [SHADE_TOP, SHADE_LEFT, SHADE_RIGHT] {
        assert!(!quads_with(&list, shade(helmet, s)).is_empty(), "shade {s}");
    }
}

#[test]
fn the_pixel_face_is_drawn_on_the_front_right_face_as_skewed_quads() {
    let skin = player_skin(0);
    let list = portrait3d(&plain(), (200.0, 280.0), 0.0);
    // the front-right face is shaded 0.6; the eyes sit on it
    let eyes = quads_with(&list, shade(skin.eyes, SHADE_RIGHT));
    assert!(eyes.len() >= 2, "{}", eyes.len());
    for c in eyes {
        let p = c.points;
        // the top edge falls from the left to the right ... in a 2:1 projection moving
        // along the face goes down by half a pixel per pixel, so the edge is not level
        assert!((p[0].1 - p[1].1).abs() > 0.01, "{p:?}");
        // vertical edges stay vertical
        assert!((p[0].0 - p[3].0).abs() < 0.01, "{p:?}");
    }
    // not on the left face
    assert!(quads_with(&list, shade(skin.eyes, SHADE_LEFT)).is_empty());
}

#[test]
fn the_vertical_bob_is_small_and_has_a_two_second_period() {
    assert_eq!(BOB_PERIOD, 2.0);
    const _: () = assert!(BOB_AMPLITUDE <= 0.04);
    let h = 226.0;
    let mut lowest = f32::MAX;
    let mut highest = f32::MIN;
    for i in 0..200 {
        let t = i as f32 * 0.01;
        let b = bob_offset(t, h);
        assert!(
            (b - bob_offset(t + BOB_PERIOD, h)).abs() < 1e-3,
            "period at {t}"
        );
        lowest = lowest.min(b);
        highest = highest.max(b);
    }
    assert!(highest - lowest <= 0.04 * h + 1e-3, "{}", highest - lowest);
    assert!(highest - lowest > 0.01 * h, "it must actually move");
    // the whole picture moves with it, the shadow stays put
    let a = portrait3d(&plain(), (160.0, h), 0.0);
    let b = portrait3d(&plain(), (160.0, h), 1.0);
    assert_eq!(a.len(), b.len());
    assert!(a.iter().zip(&b).any(|(p, q)| p.points != q.points));
    let ground = a.first().unwrap();
    assert_eq!(
        ground.points,
        b.first().unwrap().points,
        "the shadow does not bob"
    );
    assert!(ground.color[3] < 1.0);
}

#[test]
fn the_character_turns_every_six_seconds() {
    assert_eq!(YAW_PERIOD, 6.0);
    assert!(!faces_left(0.0) && !faces_left(5.9));
    assert!(faces_left(6.0) && faces_left(11.9));
    assert!(!faces_left(12.0));
    let skin = player_skin(0);
    let right = portrait3d(&plain(), (200.0, 280.0), 0.0);
    let left = portrait3d(&plain(), (200.0, 280.0), 6.0);
    assert!(!quads_with(&right, shade(skin.eyes, SHADE_RIGHT)).is_empty());
    assert!(!quads_with(&left, shade(skin.eyes, SHADE_LEFT)).is_empty());
    assert!(quads_with(&left, shade(skin.eyes, SHADE_RIGHT)).is_empty());
}

#[test]
fn commands_are_ordered_back_to_front() {
    let list = portrait3d(&look(0, Head::Crown, Face::Glasses), (160.0, 226.0), 2.0);
    for pair in list.windows(2) {
        assert!(pair[0].layer <= pair[1].layer);
    }
}
