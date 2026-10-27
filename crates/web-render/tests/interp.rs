#![allow(clippy::unwrap_used)]

use eksplode_re_core::{Arena, Config, GameState, Observation, PlayerId, Pos};
use eksplode_re_web_render::interp::Interp;

fn obs_at(tick: u32, p0: (u8, u8)) -> Observation {
    let rows = [
        "#######", "#.....#", "#.....#", "#.....#", "#.....#", "#.....#", "#######",
    ];
    let arena = Arena::from_rows(&rows).unwrap();
    let starts = [Some(Pos::new(p0.0, p0.1)), Some(Pos::new(5, 5)), None, None];
    let mut s = GameState::custom(Config::default(), arena, starts, 1).unwrap();
    s.tick = tick;
    s.observe(PlayerId::new(0).unwrap())
}

#[test]
fn the_first_snapshot_has_no_motion() {
    let mut i = Interp::default();
    i.update(&obs_at(5, (1, 1)));
    for alpha in [0.0, 0.5, 1.0] {
        assert_eq!(i.position(0, alpha), (1.0, 1.0));
    }
}

#[test]
fn positions_lerp_between_the_previous_and_current_tick() {
    let mut i = Interp::default();
    i.update(&obs_at(5, (1, 1)));
    i.update(&obs_at(6, (2, 1)));
    assert_eq!(i.position(0, 0.0), (1.0, 1.0));
    assert_eq!(i.position(0, 0.5), (1.5, 1.0));
    assert_eq!(i.position(0, 1.0), (2.0, 1.0));
}

#[test]
fn alpha_is_clamped_and_never_leaves_the_tile_pair() {
    let mut i = Interp::default();
    i.update(&obs_at(5, (1, 1)));
    i.update(&obs_at(6, (1, 2)));
    assert_eq!(i.position(0, -3.0), (1.0, 1.0));
    assert_eq!(i.position(0, 7.0), (1.0, 2.0));
    assert_eq!(i.position(0, f32::NAN), (1.0, 1.0));
    for k in 0..=20 {
        let (x, y) = i.position(0, k as f32 / 20.0);
        assert!((1.0..=2.0).contains(&y) && x == 1.0);
    }
}

#[test]
fn the_same_tick_does_not_reset_the_motion() {
    let mut i = Interp::default();
    i.update(&obs_at(5, (1, 1)));
    i.update(&obs_at(6, (2, 1)));
    i.update(&obs_at(6, (2, 1)));
    assert_eq!(i.position(0, 0.0), (1.0, 1.0));
}

#[test]
fn a_new_match_jumps_instead_of_sliding() {
    let mut i = Interp::default();
    i.update(&obs_at(500, (5, 5)));
    i.update(&obs_at(0, (1, 1)));
    assert_eq!(i.position(0, 0.0), (1.0, 1.0));
}

#[test]
fn unknown_slots_are_at_the_origin() {
    let i = Interp::default();
    assert_eq!(i.position(9, 0.5), (0.0, 0.0));
}
