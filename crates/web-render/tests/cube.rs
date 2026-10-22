use bomberust_web_render::cube::{
    badge_char, box_faces, player_color, shade, FACE_LEFT, FACE_RIGHT, FACE_TOP,
};
use bomberust_web_render::iso::project;

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = |i: usize| (a[i] - b[i]) * 255.0;
    (d(0) * d(0) + d(1) * d(1) + d(2) * d(2)).sqrt()
}

#[test]
fn the_palette_is_red_blue_green_yellow() {
    let hex = |r: u8, g: u8, b: u8| {
        [
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
        ]
    };
    assert_eq!(player_color(0), hex(0xdc, 0x32, 0x2f));
    assert_eq!(player_color(1), hex(0x26, 0x8b, 0xd2));
    assert_eq!(player_color(2), hex(0x3c, 0xb4, 0x4b));
    assert_eq!(player_color(3), hex(0xe6, 0xbe, 0x1e));
}

#[test]
fn player_colours_are_pairwise_distinct() {
    for a in 0..4 {
        for b in (a + 1)..4 {
            assert!(dist(player_color(a), player_color(b)) > 100.0, "{a} vs {b}");
        }
    }
}

#[test]
fn faces_use_the_documented_brightness() {
    assert_eq!((FACE_TOP, FACE_LEFT, FACE_RIGHT), (1.0, 0.8, 0.6));
    let c = player_color(1);
    assert_eq!(shade(c, 1.0), c);
    let s = shade(c, 0.8);
    for i in 0..3 {
        assert!((s[i] - c[i] * 0.8).abs() < 1e-6);
    }
}

#[test]
fn the_badge_is_the_slot_number() {
    assert_eq!((0..4).map(badge_char).collect::<String>(), "1234");
}

#[test]
fn a_cube_is_three_connected_faces() {
    let f = box_faces(2.5, 2.5, 0.6, 0.0, 26.0);
    // top is a rhombus: p0 highest, p2 lowest, p1 rightmost, p3 leftmost
    let t = f.top;
    assert!(t[0].1 < t[1].1 && t[0].1 < t[3].1 && t[2].1 > t[1].1 && t[2].1 > t[3].1);
    assert!(t[1].0 > t[0].0 && t[3].0 < t[0].0);
    // the side faces hang from the two front edges of the top face
    assert_eq!(f.left[3], t[3]);
    assert_eq!(f.left[2], t[2]);
    assert_eq!(f.right[3], t[2]);
    assert_eq!(f.right[2], t[1]);
    // and are exactly `h` taller than their bottoms
    assert!((f.left[0].1 - f.left[3].1 - 26.0).abs() < 1e-4);
    assert!((f.right[0].1 - f.right[3].1 - 26.0).abs() < 1e-4);
}

#[test]
fn the_footprint_is_centred_on_the_tile_centre() {
    let f = box_faces(2.5, 2.5, 1.0, 0.0, 0.0);
    let centre = project(2.5, 2.5, 0.0);
    let mid = (
        (f.top[0].0 + f.top[2].0) / 2.0,
        (f.top[0].1 + f.top[2].1) / 2.0,
    );
    assert!((mid.0 - centre.0).abs() < 1e-4 && (mid.1 - centre.1).abs() < 1e-4);
}
