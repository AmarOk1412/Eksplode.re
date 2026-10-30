#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use eksplode_re_web_render::commands::{build, build_with_looks, portrait_look, Command, Kind};
use eksplode_re_web_render::interp::Interp;
use eksplode_re_web_render::look::*;
use eksplode_re_web_render::sprites::{player_skin, Rgb};
use serde_json::json;

fn look(colour: usize, gender: Gender, head: Head, face: Face) -> Look {
    Look {
        colour,
        gender,
        head,
        face,
        costume: Costume::None,
    }
}

fn base() -> Look {
    look(4, Gender::Male, Head::None, Face::None)
}

fn quads_with(list: &[Command], rgb: Rgb) -> usize {
    list.iter()
        .filter(|c| c.kind == Kind::Quad && c.color[..3] == rgb && c.color[3] == 1.0)
        .count()
}

#[test]
fn the_names_match_the_documented_options() {
    assert_eq!(
        COLOUR_NAMES,
        ["Blue", "Black", "Red", "Purple", "Green", "Orange", "Yellow", "Pink", "Teal", "White"]
    );
    assert_eq!(GENDER_NAMES, ["Male", "Female"]);
    assert_eq!(HEAD_NAMES, ["None", "Cap", "TopHat", "Crown", "Headphones"]);
    assert_eq!(
        FACE_NAMES,
        ["None", "Glasses", "Sunglasses", "Moustache", "Mask"]
    );
}

#[test]
fn f3_us1_as5_the_default_looks_draw_the_four_characters_of_before() {
    for slot in 0..4u8 {
        assert_eq!(
            skin(&default_for(usize::from(slot))),
            player_skin(slot),
            "slot {slot}"
        );
    }
    assert_eq!(default_for(4), default_for(0));
}

#[test]
fn every_colour_has_its_own_body_and_helmet() {
    let skins: Vec<_> = (0..COLOUR_NAMES.len())
        .map(|c| skin(&look(c, Gender::Male, Head::None, Face::None)))
        .collect();
    for i in 0..skins.len() {
        for j in i + 1..skins.len() {
            assert_ne!(skins[i].body, skins[j].body, "colours {i} and {j}");
            assert_ne!(skins[i].helmet, skins[j].helmet, "colours {i} and {j}");
        }
    }
}

#[test]
fn the_grid_is_fifteen_rows_of_twelve_with_two_empty_rows_above_the_head() {
    let g = grid(&base());
    assert_eq!(g.len(), GRID_ROWS);
    assert_eq!(GRID_ROWS, 15);
    assert!(g.iter().all(|r| r.chars().count() == 12), "{g:?}");
    assert!(g[..OVERHEAD].iter().all(|r| r.chars().all(|c| c == '.')));
    // the base character is unchanged below the overhead
    assert!(g[OVERHEAD].contains('n'));
}

#[test]
fn a_female_look_adds_hair_and_lashes_that_a_male_look_does_not_have() {
    let male = grid(&look(4, Gender::Male, Head::None, Face::None));
    let female = grid(&look(4, Gender::Female, Head::None, Face::None));
    assert_ne!(male, female);
    let count = |g: &[String], ch: char| g.iter().map(|r| r.matches(ch).count()).sum::<usize>();
    assert_eq!(count(&male, 'h'), 0);
    assert!(count(&female, 'h') >= 4, "ponytail");
    assert!(count(&female, 'l') >= 2, "lashes");
}

#[test]
fn every_accessory_changes_the_picture_and_stays_inside_the_sprite() {
    let plain = grid(&base());
    for head in [Head::Cap, Head::TopHat, Head::Crown, Head::Headphones] {
        let g = grid(&look(4, Gender::Male, head, Face::None));
        assert_ne!(g, plain, "{head:?}");
        assert_eq!(g.len(), GRID_ROWS);
        assert!(g.iter().all(|r| r.chars().count() == 12), "{head:?}");
    }
    for face in [Face::Glasses, Face::Sunglasses, Face::Moustache, Face::Mask] {
        let g = grid(&look(4, Gender::Male, Head::None, face));
        assert_ne!(g, plain, "{face:?}");
        assert!(g.iter().all(|r| r.chars().count() == 12), "{face:?}");
    }
    // all eight accessories look different from each other
    let mut all: Vec<Vec<String>> = Vec::new();
    for head in [Head::Cap, Head::TopHat, Head::Crown, Head::Headphones] {
        all.push(grid(&look(4, Gender::Male, head, Face::None)));
    }
    for face in [Face::Glasses, Face::Sunglasses, Face::Moustache, Face::Mask] {
        all.push(grid(&look(4, Gender::Male, Head::None, face)));
    }
    for i in 0..all.len() {
        for j in i + 1..all.len() {
            assert_ne!(all[i], all[j], "accessories {i} and {j}");
        }
    }
}

#[test]
fn every_character_of_every_grid_has_a_colour() {
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
                let l = look(8, gender, head, face);
                let s = skin(&l);
                for ch in grid(&l)
                    .iter()
                    .flat_map(|r| r.chars())
                    .filter(|c| *c != '.')
                {
                    assert!(colour_of(&l, &s, ch).is_some(), "{ch} in {l:?}");
                }
            }
        }
    }
}

#[test]
fn from_value_is_lenient_like_the_host() {
    let cases = [
        (
            json!({"colour":"Green"}),
            1,
            look(4, Gender::Female, Head::None, Face::None),
        ),
        (
            json!({"colour":7,"gender":null,"head":[],"face":{}}),
            2,
            default_for(2),
        ),
        (
            json!({"colour":"Mauve","gender":"Female","head":"Cap","face":"Monocle"}),
            0,
            look(0, Gender::Female, Head::Cap, Face::None),
        ),
        (json!("Blue"), 3, default_for(3)),
        (json!(null), 1, default_for(1)),
    ];
    for (value, slot, expected) in cases {
        assert_eq!(from_value(&value, slot), expected, "{value}");
    }
}

#[test]
fn f3_us1_as5_a_player_is_drawn_with_the_look_of_its_slot() {
    let s = standard();
    let o = obs(&s);
    let mut i = Interp::default();
    i.update(&o);
    let crown = look(4, Gender::Male, Head::Crown, Face::None);
    let mut looks = [
        default_for(0),
        default_for(1),
        default_for(2),
        default_for(3),
    ];
    looks[1] = crown;
    let with = build_with_looks(&o, &i, 1.0, (800.0, 600.0), &looks);
    let without = build(&o, &i, 1.0, (800.0, 600.0));
    assert!(quads_with(&with, CROWN) > 0, "the crown is drawn");
    assert_eq!(quads_with(&without, CROWN), 0);
    let green = skin(&crown).body;
    assert!(quads_with(&with, green) > 0, "the chosen colour is drawn");
}

#[test]
fn a_portrait_uses_the_given_look() {
    let a = portrait_look(
        &look(4, Gender::Male, Head::None, Face::None),
        (160.0, 226.0),
        7,
    );
    let b = portrait_look(
        &look(4, Gender::Female, Head::Crown, Face::Mask),
        (160.0, 226.0),
        7,
    );
    assert_ne!(a, b);
    assert!(portrait_look(&base(), (0.0, 5.0), 0).is_empty());
}

// ---- 011 US4: the costume, and the gender silhouette kept internal ----

const COSTUMES: [Costume; 4] = [
    Costume::Scarf,
    Costume::Cape,
    Costume::BowTie,
    Costume::Medal,
];

fn dressed(costume: Costume) -> Look {
    Look { costume, ..base() }
}

#[test]
fn f11_us4_the_costume_names_match_the_server() {
    assert_eq!(COSTUME_NAMES, ["None", "Scarf", "Cape", "BowTie", "Medal"]);
    for slot in 0..4 {
        assert_eq!(default_for(slot).costume, Costume::None, "slot {slot}");
    }
}

#[test]
fn f11_us4_as5_a_missing_costume_or_gender_falls_back_to_the_slot_default() {
    // the page no longer sends a gender: the silhouette still alternates with the slot
    for slot in 0..4 {
        let l = from_value(&json!({"colour":"Teal","head":"Cap"}), slot);
        assert_eq!(l.gender, default_for(slot).gender, "slot {slot}");
        assert_eq!(l.costume, Costume::None);
    }
    assert_eq!(default_for(0).gender, Gender::Male);
    assert_eq!(default_for(1).gender, Gender::Female);
    let l = from_value(&json!({"costume":"Cape","gender":"Female"}), 0);
    assert_eq!((l.costume, l.gender), (Costume::Cape, Gender::Female));
    assert_eq!(
        from_value(&json!({"costume":"Toga"}), 2).costume,
        Costume::None
    );
}

#[test]
fn f11_us4_every_costume_changes_the_picture_and_has_its_colours() {
    let plain = grid(&base());
    let mut seen = vec![plain.clone()];
    for costume in COSTUMES {
        let l = dressed(costume);
        let g = grid(&l);
        assert_ne!(g, plain, "{costume:?}");
        assert!(!seen.contains(&g), "{costume:?} looks like another costume");
        seen.push(g.clone());
        let s = skin(&l);
        for view in [View::Down, View::Up, View::Left, View::Right] {
            for frame in [Frame::Idle, Frame::StepA, Frame::StepB] {
                let g = grid_view(&l, view, frame);
                assert_eq!(g.len(), GRID_ROWS);
                assert!(g.iter().all(|r| r.chars().count() == 12), "{costume:?}");
                for ch in g.iter().flat_map(|r| r.chars()).filter(|c| *c != '.') {
                    assert!(colour_of(&l, &s, ch).is_some(), "{ch} in {costume:?}");
                }
            }
        }
    }
}
