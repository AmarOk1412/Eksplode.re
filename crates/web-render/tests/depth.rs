mod common;

use bomberust_core::PowerUpKind;
use bomberust_web_render::depth::{draw_list, Layer, Sprite};
use bomberust_web_render::interp::Interp;
use common::*;

fn items(s: &bomberust_core::GameState) -> Vec<bomberust_web_render::depth::Item> {
    let o = obs(s);
    let mut i = Interp::default();
    i.update(&o);
    draw_list(&o, &i, 1.0, 0.0)
}

#[test]
fn us1_as3_floors_come_first() {
    let list = items(&standard());
    let first_object = list.iter().position(|i| i.layer != Layer::Floor).unwrap();
    assert!(first_object > 0);
    assert!(list[..first_object].iter().all(|i| i.layer == Layer::Floor));
    assert!(list[first_object..].iter().all(|i| i.layer != Layer::Floor));
}

#[test]
fn us1_as3_objects_are_sorted_back_to_front() {
    let list = items(&standard());
    let keys: Vec<f32> = list
        .iter()
        .filter(|i| i.layer != Layer::Floor)
        .map(|i| i.pos.0 + i.pos.1)
        .collect();
    assert!(
        keys.windows(2).all(|w| w[0] <= w[1]),
        "objects must be sorted by x + y"
    );
}

#[test]
fn us1_as3_a_player_behind_a_block_is_drawn_before_it() {
    let rows = [
        "#######", "#.....#", "#.....#", "#..+..#", "#.....#", "#.....#", "#######",
    ];
    let s = small(&rows, (2, 2), (4, 4));
    let list = items(&s);
    let at = |pred: &dyn Fn(&Sprite) -> bool| list.iter().position(|i| pred(&i.sprite)).unwrap();
    let block = at(&|s| matches!(s, Sprite::Block));
    let behind = at(&|s| matches!(s, Sprite::Player { slot: 0 }));
    let front = at(&|s| matches!(s, Sprite::Player { slot: 1 }));
    assert!(
        behind < block,
        "player at (2,2) is behind the block at (3,3)"
    );
    assert!(front > block, "player at (4,4) is in front of the block");
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
    assert_eq!(
        layers,
        vec![Layer::Floor, Layer::Flame, Layer::PowerUp, Layer::Bomb]
    );
}

#[test]
fn ties_keep_the_generation_order() {
    let mut s = small(&OPEN, (1, 1), (5, 5));
    powerup(&mut s, 1, 3, PowerUpKind::Speed);
    powerup(&mut s, 3, 1, PowerUpKind::Speed);
    let list = items(&s);
    let spots: Vec<(f32, f32)> = list
        .iter()
        .filter(|i| i.layer == Layer::PowerUp)
        .map(|i| i.pos)
        .collect();
    // equal x + y: the order of the observation (insertion order) is kept
    assert_eq!(spots, vec![(1.5, 3.5), (3.5, 1.5)]);
}

#[test]
fn eliminated_players_are_not_listed() {
    let mut s = small(&OPEN, (1, 1), (5, 5));
    s.players[1].alive = false;
    let list = items(&s);
    let players = list
        .iter()
        .filter(|i| matches!(i.sprite, Sprite::Player { .. }))
        .count();
    assert_eq!(players, 1);
}

#[test]
fn walls_are_boxes_and_the_floor_has_no_wall_tiles() {
    let s = small(&OPEN, (1, 1), (5, 5));
    let list = items(&s);
    let walls = list
        .iter()
        .filter(|i| matches!(i.sprite, Sprite::Wall))
        .count();
    assert_eq!(walls, 24, "7x7 border");
    let floors = list
        .iter()
        .filter(|i| matches!(i.sprite, Sprite::Floor { .. }))
        .count();
    assert_eq!(floors, 25);
}
