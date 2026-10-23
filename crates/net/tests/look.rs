#![allow(clippy::unwrap_used, clippy::expect_used)]

use bomberust_net::look::{resolve_colour, Colour, Face, Gender, Head, Look};
use serde_json::json;

#[test]
fn f3_us1_as7_defaults_for_bots_and_terminal_players() {
    let expected = [
        (Colour::Blue, Gender::Male),
        (Colour::Black, Gender::Female),
        (Colour::Red, Gender::Male),
        (Colour::Purple, Gender::Female),
    ];
    for (slot, (colour, gender)) in expected.into_iter().enumerate() {
        let look = Look::default_for(slot);
        assert_eq!(look.colour, colour, "slot {slot}");
        assert_eq!(look.gender, gender, "slot {slot}");
        assert_eq!((look.head, look.face), (Head::None, Face::None));
    }
    assert_eq!(
        Look::default_for(4),
        Look::default_for(0),
        "slots wrap around"
    );
}

#[test]
fn the_palette_and_the_options_are_exactly_the_documented_ones() {
    let colours: Vec<&str> = Colour::ALL.iter().map(|c| c.name()).collect();
    assert_eq!(
        colours,
        ["Blue", "Black", "Red", "Purple", "Green", "Orange", "Yellow", "Pink", "Teal", "White"]
    );
    let genders: Vec<&str> = Gender::ALL.iter().map(|c| c.name()).collect();
    assert_eq!(genders, ["Male", "Female"]);
    let heads: Vec<&str> = Head::ALL.iter().map(|c| c.name()).collect();
    assert_eq!(heads, ["None", "Cap", "TopHat", "Crown", "Headphones"]);
    let faces: Vec<&str> = Face::ALL.iter().map(|c| c.name()).collect();
    assert_eq!(
        faces,
        ["None", "Glasses", "Sunglasses", "Moustache", "Mask"]
    );
}

#[test]
fn a_complete_look_round_trips_through_json() {
    let look = Look {
        colour: Colour::Teal,
        gender: Gender::Female,
        head: Head::Crown,
        face: Face::Sunglasses,
    };
    let text = serde_json::to_string(&look).unwrap();
    assert_eq!(
        text,
        r#"{"colour":"Teal","gender":"Female","head":"Crown","face":"Sunglasses"}"#
    );
    assert_eq!(
        Look::from_wire(&serde_json::from_str(&text).unwrap(), 0),
        look
    );
}

#[test]
fn missing_wrong_and_unknown_fields_become_the_slot_default_one_by_one() {
    let cases = [
        // a missing field
        (
            json!({"colour":"Green"}),
            1,
            (Colour::Green, Gender::Female, Head::None, Face::None),
        ),
        // wrong types
        (
            json!({"colour":7,"gender":null,"head":[],"face":{}}),
            2,
            (Colour::Red, Gender::Male, Head::None, Face::None),
        ),
        // unknown values keep the valid fields
        (
            json!({"colour":"Mauve","gender":"Female","head":"Cap","face":"Monocle"}),
            0,
            (Colour::Blue, Gender::Female, Head::Cap, Face::None),
        ),
        // case matters: wire names are exact
        (
            json!({"colour":"green"}),
            3,
            (Colour::Purple, Gender::Female, Head::None, Face::None),
        ),
        // extra fields are ignored
        (
            json!({"colour":"Pink","hat":"big","size":3}),
            0,
            (Colour::Pink, Gender::Male, Head::None, Face::None),
        ),
    ];
    for (value, slot, (colour, gender, head, face)) in cases {
        let look = Look::from_wire(&value, slot);
        assert_eq!(
            (look.colour, look.gender, look.head, look.face),
            (colour, gender, head, face),
            "{value}"
        );
    }
}

#[test]
fn anything_that_is_not_an_object_gives_the_full_default() {
    for value in [
        json!(null),
        json!(5),
        json!("Blue"),
        json!([1, 2]),
        json!(true),
    ] {
        assert_eq!(Look::from_wire(&value, 1), Look::default_for(1), "{value}");
    }
    let huge = json!({"colour": "x".repeat(60_000)});
    assert_eq!(Look::from_wire(&huge, 2), Look::default_for(2));
}

#[test]
fn a_taken_colour_moves_to_the_next_free_one_in_palette_order() {
    assert_eq!(
        resolve_colour(Colour::Red, &[Colour::Blue]),
        (Colour::Red, false)
    );
    assert_eq!(
        resolve_colour(Colour::Blue, &[Colour::Blue]),
        (Colour::Black, true)
    );
    assert_eq!(
        resolve_colour(Colour::Blue, &[Colour::Blue, Colour::Black, Colour::Red]),
        (Colour::Purple, true)
    );
    // wraps around the end of the palette
    assert_eq!(
        resolve_colour(Colour::White, &[Colour::White]),
        (Colour::Blue, true)
    );
}
