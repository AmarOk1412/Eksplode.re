use bomberust_web_render::sprites::*;
use bomberust_web_render::view::{fit, MARGIN};

#[test]
fn fit_centres_the_arena_with_a_whole_number_scale() {
    let v = fit(15, 13, (800.0, 600.0));
    assert_eq!(v.scale.fract(), 0.0);
    let (x0, y0) = v.project(0.0, 0.0);
    let (x1, y1) = v.project(15.0, 13.18);
    assert!(x0 >= MARGIN - 1.0 && y0 >= MARGIN - 1.0);
    assert!(x1 <= 800.0 - MARGIN + 1.0 && y1 <= 600.0 - MARGIN + 1.0);
    assert!(
        ((x0 + (800.0 - x1)) / 2.0 - x0).abs() <= 1.0,
        "centred horizontally"
    );
}

#[test]
fn project_and_unproject_are_inverse() {
    let v = fit(15, 13, (800.0, 600.0));
    let (px, py) = v.project(4.25, 7.5);
    let (x, y) = v.unproject(px, py);
    assert!((x - 4.25).abs() < 1e-3 && (y - 7.5).abs() < 1e-3);
}

#[test]
fn neighbouring_rectangles_share_their_edges() {
    let v = fit(15, 13, (801.0, 599.0));
    for t in 0..14 {
        let a = v.rect(t as f32, 2.0, t as f32 + 1.0, 3.0);
        let b = v.rect(t as f32 + 1.0, 2.0, t as f32 + 2.0, 3.0);
        assert_eq!(a[1].0, b[0].0, "tile {t}");
    }
}

#[test]
fn rectangles_are_never_thinner_than_a_pixel() {
    let v = fit(15, 13, (800.0, 600.0));
    let r = v.rect(1.0, 1.0, 1.0001, 1.0001);
    assert!(r[1].0 - r[0].0 >= 1.0 && r[2].1 - r[1].1 >= 1.0);
}

#[test]
fn odd_canvases_give_finite_views() {
    for canvas in [(0.0, 0.0), (f32::NAN, 5.0), (-4.0, 3.0), (1.0, 1.0)] {
        let v = fit(15, 13, canvas);
        assert!(v.scale.is_finite() && v.scale >= 1.0);
        assert!(v.offset_x.is_finite() && v.offset_y.is_finite());
    }
}

#[test]
fn every_character_row_is_twelve_pixels_wide() {
    assert_eq!(PLAYER.len(), 13);
    assert!(PLAYER.iter().all(|r| r.chars().count() == 12));
    assert!(BOMB.iter().all(|r| r.chars().count() == 10));
    assert!(PORTRAIT_ROWS < PLAYER.len());
}

#[test]
fn runs_merge_equal_neighbours_and_skip_transparent_pixels() {
    let colour = |c: char| match c {
        'a' => Some([1.0, 0.0, 0.0]),
        'b' => Some([0.0, 1.0, 0.0]),
        _ => None,
    };
    let r = runs(&["aab.b", "..a.."], 2, colour);
    let spans: Vec<(usize, usize, usize)> = r.iter().map(|r| (r.row, r.col, r.len)).collect();
    assert_eq!(spans, vec![(0, 0, 2), (0, 2, 1), (0, 4, 1), (1, 2, 1)]);
    assert_eq!(
        runs(&["aa", "bb"], 1, colour).len(),
        1,
        "rows limits the output"
    );
}

#[test]
fn the_four_characters_use_different_colours() {
    let skins: Vec<Skin> = (0..4).map(player_skin).collect();
    for i in 0..4 {
        for j in i + 1..4 {
            assert_ne!(skins[i].helmet, skins[j].helmet, "{i} {j}");
            assert_ne!(skins[i].body, skins[j].body, "{i} {j}");
        }
        assert_ne!(player_color(i as u8), player_color(((i + 1) % 4) as u8));
    }
    assert_eq!(player_skin(4), player_skin(0));
}

#[test]
fn every_template_character_has_a_colour() {
    let skin = player_skin(0);
    for row in PLAYER {
        for ch in row.chars().filter(|c| *c != '.') {
            assert!(skin.colour_of(ch).is_some(), "{ch}");
        }
    }
    for row in BOMB {
        for ch in row.chars().filter(|c| *c != '.') {
            assert!(bomb_colour(ch, true).is_some(), "{ch}");
        }
    }
}

#[test]
fn the_bomb_spark_blinks() {
    assert_ne!(bomb_colour('o', true), bomb_colour('o', false));
}
