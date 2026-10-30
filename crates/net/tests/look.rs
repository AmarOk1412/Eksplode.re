#![allow(clippy::unwrap_used, clippy::expect_used)]

use eksplode_re_net::look::{resolve_colour, Colour, Costume, Face, Head, Look};
use serde_json::json;

#[test]
fn f3_us1_as7_defaults_for_bots_and_terminal_players() {
    let expected = [Colour::Blue, Colour::Black, Colour::Red, Colour::Purple];
    for (slot, colour) in expected.into_iter().enumerate() {
        let look = Look::default_for(slot);
        assert_eq!(look.colour, colour, "slot {slot}");
        assert_eq!(
            (look.head, look.face, look.costume),
            (Head::None, Face::None, Costume::None)
        );
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
    let heads: Vec<&str> = Head::ALL.iter().map(|c| c.name()).collect();
    assert_eq!(heads, ["None", "Cap", "TopHat", "Crown", "Headphones"]);
    let faces: Vec<&str> = Face::ALL.iter().map(|c| c.name()).collect();
    assert_eq!(
        faces,
        ["None", "Glasses", "Sunglasses", "Moustache", "Mask"]
    );
    let costumes: Vec<&str> = Costume::ALL.iter().map(|c| c.name()).collect();
    assert_eq!(costumes, ["None", "Scarf", "Cape", "BowTie", "Medal"]);
    assert_eq!(Costume::default(), Costume::None);
}

#[test]
fn a_complete_look_round_trips_through_json() {
    let look = Look {
        colour: Colour::Teal,
        head: Head::Crown,
        face: Face::Sunglasses,
        costume: Costume::Cape,
    };
    let text = serde_json::to_string(&look).unwrap();
    assert_eq!(
        text,
        r#"{"colour":"Teal","head":"Crown","face":"Sunglasses","costume":"Cape"}"#
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
            (Colour::Green, Head::None, Face::None, Costume::None),
        ),
        // wrong types
        (
            json!({"colour":7,"head":[],"face":{},"costume":3}),
            2,
            (Colour::Red, Head::None, Face::None, Costume::None),
        ),
        // unknown values keep the valid fields
        (
            json!({"colour":"Mauve","head":"Cap","face":"Monocle","costume":"Toga"}),
            0,
            (Colour::Blue, Head::Cap, Face::None, Costume::None),
        ),
        // case matters: wire names are exact
        (
            json!({"colour":"green","costume":"scarf"}),
            3,
            (Colour::Purple, Head::None, Face::None, Costume::None),
        ),
        // extra fields are ignored
        (
            json!({"colour":"Pink","hat":"big","size":3,"costume":"Medal"}),
            0,
            (Colour::Pink, Head::None, Face::None, Costume::Medal),
        ),
    ];
    for (value, slot, (colour, head, face, costume)) in cases {
        let look = Look::from_wire(&value, slot);
        assert_eq!(
            (look.colour, look.head, look.face, look.costume),
            (colour, head, face, costume),
            "{value}"
        );
    }
}

#[test]
fn f11_us4_as5_a_legacy_look_with_a_gender_still_decodes_with_a_default_costume() {
    let legacy = json!({"colour":"Pink","gender":"Female","head":"Cap","face":"Glasses"});
    let expected = Look {
        colour: Colour::Pink,
        head: Head::Cap,
        face: Face::Glasses,
        costume: Costume::None,
    };
    assert_eq!(Look::from_wire(&legacy, 1), expected, "lenient reader");
    assert_eq!(
        serde_json::from_value::<Look>(legacy).unwrap(),
        expected,
        "the typed reader of Welcome/Lobby looks ignores the old key too"
    );
    let sent = serde_json::to_value(Look::default_for(1)).unwrap();
    assert!(sent.get("gender").is_none(), "{sent}");
    assert_eq!(sent["costume"], "None");
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
