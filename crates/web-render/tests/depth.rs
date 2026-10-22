mod common;

use bomberust_core::PowerUpKind;
use bomberust_web_render::depth::{draw_list, Item, Layer, Sprite};
use bomberust_web_render::interp::Interp;
use common::*;

fn items(s: &bomberust_core::GameState) -> Vec<Item> {
    let o = obs(s);
    let mut i = Interp::default();
    i.update(&o);
    draw_list(&o, &i, 1.0, 0.0)
}

#[test]
fn us1_as3_layers_are_sorted_back_to_front_and_rows_top_to_bottom() {
    let list = items(&standard());
    assert!(
        list.windows(2).all(|w| w[0].key() <= w[1].key()),
        "objects must be sorted by layer, then row"
    );
    let layers: Vec<Layer> = list.iter().map(|i| i.layer).collect();
    assert!(layers.windows(2).all(|w| w[0] <= w[1]));
}

#[test]
fn us1_as3_a_player_lower_on_the_screen_covers_one_above() {
    let s = small(&OPEN, (2, 1), (2, 4));
    let list = items(&s);
    let at = |slot: u8| {
        list.iter()
            .position(|i| i.sprite == Sprite::Player { slot })
            .unwrap()
    };
    assert!(at(0) < at(1), "the player in row 1 is drawn before row 4");
}

#[test]
fn on_one_tile_the_layers_are_stacked_in_order() {
    let mut s = small(&OPEN, (1, 1), (5, 5));
    flame(&mut s, 3, 3);
    bomb(&mut s, 3, 3, 10);
    powerup(&mut s, 3, 3, PowerUpKind::Range);
    let list = items(&s);
    let layers: Vec<Layer> = list
        .iter()
        .filter(|i| i.pos == (3.5, 3.5))
        .map(|i| i.layer)
        .collect();
    assert_eq!(layers, vec![Layer::Flame, Layer::PowerUp, Layer::Bomb]);
}

#[test]
fn equal_rows_are_drawn_left_to_right() {
    let mut s = small(&OPEN, (1, 1), (5, 5));
    powerup(&mut s, 4, 3, PowerUpKind::Speed);
    powerup(&mut s, 2, 3, PowerUpKind::Speed);
    let spots: Vec<(f32, f32)> = items(&s)
        .iter()
        .filter(|i| i.layer == Layer::PowerUp)
        .map(|i| i.pos)
        .collect();
    assert_eq!(spots, vec![(2.5, 3.5), (4.5, 3.5)]);
}

#[test]
fn eliminated_players_are_not_listed() {
    let mut s = small(&OPEN, (1, 1), (5, 5));
    s.players[1].alive = false;
    let players = items(&s)
        .iter()
        .filter(|i| matches!(i.sprite, Sprite::Player { .. }))
        .count();
    assert_eq!(players, 1);
}

#[test]
fn walls_and_blocks_are_listed_and_the_floor_is_not() {
    let rows = [
        "#######", "#.....#", "#..+..#", "#.....#", "#.....#", "#.....#", "#######",
    ];
    let list = items(&small(&rows, (1, 1), (5, 5)));
    let count = |f: fn(&Sprite) -> bool| list.iter().filter(|i| f(&i.sprite)).count();
    assert_eq!(count(|s| matches!(s, Sprite::Wall)), 24, "7x7 border");
    assert_eq!(count(|s| matches!(s, Sprite::Block)), 1);
}

#[test]
fn power_ups_bob_by_less_than_a_tile() {
    let mut s = small(&OPEN, (1, 1), (5, 5));
    powerup(&mut s, 3, 3, PowerUpKind::Speed);
    let o = obs(&s);
    let mut i = Interp::default();
    i.update(&o);
    for phase in 0..40 {
        let list = draw_list(&o, &i, 1.0, phase as f32);
        let p = list.iter().find(|i| i.layer == Layer::PowerUp).unwrap();
        assert!(p.lift.abs() <= 0.06, "{}", p.lift);
    }
}
