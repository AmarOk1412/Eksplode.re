#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use eksplode_re_core::{
    step, Arena, Config, Dir, Event, GameState, PlayerInput, Pos, PowerUp, PowerUpKind,
};

const ROW: &str = "#...........#";

fn rows(width: usize, height: usize) -> Vec<String> {
    let mut v = Vec::new();
    for y in 0..height {
        if y == 0 || y == height - 1 {
            v.push("#".repeat(width));
        } else {
            v.push(format!("#{}#", ".".repeat(width - 2)));
        }
    }
    v
}

/// Builds a width x height arena, then sets the given tiles: (x, y, '#' wall | '+' block).
fn arena_with(width: usize, height: usize, extra: &[(u8, u8, char)]) -> Vec<String> {
    let mut g: Vec<Vec<char>> = rows(width, height)
        .iter()
        .map(|r| r.chars().collect())
        .collect();
    for (x, y, c) in extra {
        g[usize::from(*y)][usize::from(*x)] = *c;
    }
    g.into_iter().map(|r| r.into_iter().collect()).collect()
}

fn punch_input() -> [PlayerInput; 4] {
    let mut i = idle();
    i[0].punch = true;
    i
}

struct Setup {
    width: usize,
    height: usize,
    tiles: Vec<(u8, u8, char)>,
    puncher: (u8, u8),
    facing: Dir,
    bomb: (u8, u8),
    others: Vec<(usize, (u8, u8))>,
    other_bombs: Vec<(u8, u8)>,
}

impl Setup {
    fn new(facing: Dir, puncher: (u8, u8), bomb: (u8, u8)) -> Setup {
        Setup {
            width: 13,
            height: 7,
            tiles: vec![],
            puncher,
            facing,
            bomb,
            others: vec![],
            other_bombs: vec![],
        }
    }

    fn tile(mut self, x: u8, y: u8, c: char) -> Setup {
        self.tiles.push((x, y, c));
        self
    }

    fn player(mut self, slot: usize, at: (u8, u8)) -> Setup {
        self.others.push((slot, at));
        self
    }

    fn other_bomb(mut self, at: (u8, u8)) -> Setup {
        self.other_bombs.push(at);
        self
    }

    fn build(&self) -> GameState {
        let rows = arena_with(self.width, self.height, &self.tiles);
        let refs: Vec<&str> = rows.iter().map(String::as_str).collect();
        let arena = Arena::from_rows(&refs).unwrap();
        let mut starts = [None; 4];
        starts[0] = Some(Pos::new(self.puncher.0, self.puncher.1));
        for (slot, at) in &self.others {
            starts[*slot] = Some(Pos::new(at.0, at.1));
        }
        let mut s = GameState::custom(Config::default(), arena, starts, 1).unwrap();
        s.players[0].has_punch = true;
        s.players[0].facing = self.facing;
        add_bomb(&mut s, 0, self.bomb.0, self.bomb.1, 1000);
        for at in &self.other_bombs {
            add_bomb(&mut s, 1, at.0, at.1, 1000);
        }
        s
    }
}

/// Punches at once and waits for the landing; returns where the punched bomb ended.
fn land(setup: &Setup) -> (Pos, Vec<Event>) {
    let mut s = setup.build();
    let mut all = step(&mut s, &punch_input());
    for _ in 0..80 {
        if s.bombs.iter().all(|b| b.flight.is_none()) {
            break;
        }
        all.extend(step(&mut s, &idle()));
    }
    assert!(
        s.bombs.iter().all(|b| b.flight.is_none()),
        "the bomb never landed"
    );
    let punched = all
        .iter()
        .find_map(|e| match e {
            Event::BombLanded { pos } => Some(*pos),
            _ => None,
        })
        .expect("a landing event");
    (punched, all)
}

fn p(x: u8, y: u8) -> Pos {
    Pos::new(x, y)
}

#[test]
fn f3_us2_as3_a_free_next_square_is_the_landing_square() {
    assert_eq!(land(&Setup::new(Dir::Right, (1, 3), (2, 3))).0, p(3, 3));
}

#[test]
fn f3_us2_as3_a_bomb_hops_over_blocks_and_walls_and_lands_on_the_first_free_square() {
    // 1 block, 2 blocks, 3 blocks, a pillar wall, a block then a wall
    let cases: Vec<(Setup, Pos)> = vec![
        (
            Setup::new(Dir::Right, (1, 3), (2, 3)).tile(3, 3, '+'),
            p(4, 3),
        ),
        (
            Setup::new(Dir::Right, (1, 3), (2, 3))
                .tile(3, 3, '+')
                .tile(4, 3, '+'),
            p(5, 3),
        ),
        (
            Setup::new(Dir::Right, (1, 3), (2, 3))
                .tile(3, 3, '+')
                .tile(4, 3, '+')
                .tile(5, 3, '+'),
            p(6, 3),
        ),
        (
            Setup::new(Dir::Right, (1, 3), (2, 3)).tile(3, 3, '#'),
            p(4, 3),
        ),
        (
            Setup::new(Dir::Right, (1, 3), (2, 3))
                .tile(3, 3, '+')
                .tile(4, 3, '#'),
            p(5, 3),
        ),
        // other directions
        (
            Setup::new(Dir::Left, (6, 3), (5, 3)).tile(4, 3, '+'),
            p(3, 3),
        ),
        (
            Setup::new(Dir::Down, (3, 1), (3, 2)).tile(3, 3, '+'),
            p(3, 4),
        ),
        (Setup::new(Dir::Up, (3, 5), (3, 4)).tile(3, 3, '#'), p(3, 2)),
        (Setup::new(Dir::Up, (3, 5), (3, 4)), p(3, 3)),
        (
            Setup::new(Dir::Left, (9, 3), (8, 3))
                .tile(7, 3, '+')
                .tile(6, 3, '+'),
            p(5, 3),
        ),
    ];
    for (i, (setup, expected)) in cases.into_iter().enumerate() {
        assert_eq!(land(&setup).0, expected, "case {i}");
    }
}

#[test]
fn f3_us2_as3_a_bomb_hops_over_players_and_other_bombs_too() {
    let cases: Vec<(Setup, Pos)> = vec![
        (
            Setup::new(Dir::Right, (1, 3), (2, 3)).player(1, (3, 3)),
            p(4, 3),
        ),
        (
            Setup::new(Dir::Right, (1, 3), (2, 3)).other_bomb((3, 3)),
            p(4, 3),
        ),
        (
            Setup::new(Dir::Right, (1, 3), (2, 3))
                .tile(3, 3, '+')
                .player(1, (4, 3))
                .other_bomb((5, 3)),
            p(6, 3),
        ),
        (
            Setup::new(Dir::Right, (1, 3), (2, 3))
                .player(1, (3, 3))
                .player(2, (4, 3)),
            p(5, 3),
        ),
        (
            Setup::new(Dir::Down, (3, 1), (3, 2))
                .player(1, (3, 3))
                .other_bomb((3, 4)),
            p(3, 5),
        ),
    ];
    for (i, (setup, expected)) in cases.into_iter().enumerate() {
        assert_eq!(land(&setup).0, expected, "case {i}");
    }
}

#[test]
fn a_flame_or_a_power_up_on_the_landing_square_does_not_stop_the_bomb() {
    let mut s = Setup::new(Dir::Right, (1, 3), (2, 3)).build();
    s.powerups.push(PowerUp {
        pos: p(3, 3),
        kind: PowerUpKind::Speed,
    });
    s.flames.push(eksplode_re_core::Flame {
        pos: p(3, 3),
        ticks_left: 50,
        owners: 0,
    });
    step(&mut s, &punch_input());
    for _ in 0..10 {
        step(&mut s, &idle());
    }
    assert_eq!(s.bombs[0].pos, p(3, 3));
    assert_eq!(s.powerups.len(), 1, "the power-up stays on the board");
}

#[test]
fn f3_us2_as4_at_the_edge_the_bomb_lands_on_the_nearest_free_square_back_toward_its_origin() {
    // a 7 wide arena: x = 1..=5 is open; the wall is at x = 6
    let mut a = Setup::new(Dir::Right, (3, 3), (4, 3));
    a.width = 7;
    a = a.tile(5, 3, '+');
    // the block at 5 and the wall at 6 are obstacles, the edge follows: back to the origin
    assert_eq!(land(&a).0, p(4, 3));
    let mut b = Setup::new(Dir::Right, (4, 3), (5, 3));
    b.width = 7;
    assert_eq!(land(&b).0, p(5, 3), "the origin itself is a candidate");
}

#[test]
fn f3_us2_as5_a_punch_without_the_power_or_a_bomb_or_on_a_flying_bomb_does_nothing() {
    // no power
    let mut s = Setup::new(Dir::Right, (1, 3), (2, 3)).build();
    s.players[0].has_punch = false;
    let events = step(&mut s, &punch_input());
    assert!(s.bombs[0].flight.is_none());
    assert!(!events
        .iter()
        .any(|e| matches!(e, Event::BombPunched { .. })));
    assert_eq!(s.bombs[0].pos, p(2, 3));
    // no bomb in front: facing a free square
    let mut s = Setup::new(Dir::Right, (1, 3), (2, 3)).build();
    s.players[0].facing = Dir::Down;
    let events = step(&mut s, &punch_input());
    assert!(!events
        .iter()
        .any(|e| matches!(e, Event::BombPunched { .. })));
    // the bomb is two squares away, not next to the player
    let mut s = Setup::new(Dir::Right, (1, 3), (3, 3)).build();
    let events = step(&mut s, &punch_input());
    assert!(!events
        .iter()
        .any(|e| matches!(e, Event::BombPunched { .. })));
    // already in flight: a second punch changes nothing
    let mut s = Setup::new(Dir::Right, (1, 3), (2, 3))
        .tile(3, 3, '+')
        .tile(4, 3, '+')
        .build();
    let first = step(&mut s, &punch_input());
    assert!(first.iter().any(|e| matches!(e, Event::BombPunched { .. })));
    let flight = s.bombs[0].flight;
    let second = step(&mut s, &punch_input());
    assert!(!second
        .iter()
        .any(|e| matches!(e, Event::BombPunched { .. })));
    assert!(flight.is_some());
}

#[test]
fn the_bomb_hops_one_square_every_two_ticks_with_events() {
    let mut s = Setup::new(Dir::Right, (1, 3), (2, 3))
        .tile(3, 3, '+')
        .tile(4, 3, '+')
        .tile(5, 3, '+')
        .build();
    assert_eq!(s.config.punch_hop_ticks, 2);
    let mut hops = Vec::new();
    let mut punched = 0;
    let mut landed = Vec::new();
    for tick in 0..14u32 {
        let input = if tick == 0 { punch_input() } else { idle() };
        for e in step(&mut s, &input) {
            match e {
                Event::BombPunched { player, from, dir } => {
                    assert_eq!((player.index(), from, dir), (0, p(2, 3), Dir::Right));
                    punched += 1;
                }
                Event::BombHopped { pos } => hops.push((tick, pos)),
                Event::BombLanded { pos } => landed.push((tick, pos)),
                _ => {}
            }
        }
    }
    assert_eq!(punched, 1);
    assert_eq!(
        hops.iter().map(|h| h.1).collect::<Vec<_>>(),
        [p(3, 3), p(4, 3), p(5, 3), p(6, 3)]
    );
    assert_eq!(hops.iter().map(|h| h.0).collect::<Vec<_>>(), [2, 4, 6, 8]);
    assert_eq!(landed, vec![(8, p(6, 3))]);
}

#[test]
fn a_player_stepping_onto_the_landing_square_during_the_flight_moves_the_landing_on() {
    let mut s = Setup::new(Dir::Right, (1, 3), (2, 3))
        .player(1, (3, 2))
        .build();
    let mut input = punch_input();
    input[1].mv = Some(Dir::Down);
    step(&mut s, &input);
    assert_eq!(
        s.players[1].pos,
        p(3, 3),
        "the other player is now on the landing square"
    );
    for _ in 0..10 {
        step(&mut s, &idle());
    }
    assert_eq!(s.bombs[0].pos, p(4, 3));
    assert!(s.bombs[0].flight.is_none());
}

#[test]
fn a_bomb_in_flight_does_not_block_movement() {
    let mut s = Setup::new(Dir::Right, (1, 3), (2, 3))
        .tile(3, 3, '+')
        .tile(4, 3, '+')
        .player(1, (2, 2))
        .build();
    step(&mut s, &punch_input());
    // the bomb has left (2, 3); a player may walk onto it while the bomb is in the air
    let mut input = idle();
    input[1].mv = Some(Dir::Down);
    step(&mut s, &input);
    assert_eq!(s.players[1].pos, p(2, 3));
    assert!(s.bombs[0].flight.is_some());
}

#[test]
fn the_fuse_keeps_burning_and_the_bomb_explodes_on_time_at_its_landing_square() {
    let mut s = Setup::new(Dir::Right, (1, 3), (2, 3))
        .tile(3, 3, '+')
        .build();
    s.bombs[0].ticks_left = 8;
    let mut exploded_at = None;
    for tick in 0..12u32 {
        let input = if tick == 0 { punch_input() } else { idle() };
        for e in step(&mut s, &input) {
            if let Event::BombExploded { pos, .. } = e {
                exploded_at = Some((tick, pos));
            }
        }
    }
    assert_eq!(
        exploded_at,
        Some((7, p(4, 3))),
        "fuse of 8 ticks, landing at (4, 3)"
    );
}

#[test]
fn a_bomb_whose_fuse_ends_in_flight_explodes_over_the_square_it_is_on() {
    let mut s = Setup::new(Dir::Right, (1, 3), (2, 3))
        .player(1, (3, 3))
        .build();
    s.bombs[0].ticks_left = 3;
    step(&mut s, &punch_input());
    let mut exploded = None;
    for _ in 0..6 {
        for e in step(&mut s, &idle()) {
            if let Event::BombExploded { pos, .. } = e {
                exploded = Some(pos);
            }
        }
    }
    // the first hop at tick 2 put it over (3, 3), which holds a player, so it was still flying
    assert_eq!(exploded, Some(p(3, 3)));
    assert!(!s.players[1].alive, "the player under it burns");
}

#[test]
fn flames_do_not_trigger_a_bomb_in_flight() {
    let mut s = Setup::new(Dir::Right, (1, 3), (2, 3))
        .tile(3, 3, '+')
        .tile(4, 3, '+')
        .tile(5, 3, '+')
        .player(1, (6, 1))
        .build();
    step(&mut s, &punch_input());
    step(&mut s, &idle());
    step(&mut s, &idle()); // first hop: the bomb is over (3, 3)
    assert!(s.bombs[0].flight.is_some());
    // another bomb explodes right next to it
    add_bomb(&mut s, 1, 3, 2, 1);
    let events = step(&mut s, &idle());
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::BombExploded { pos, .. } if *pos == p(3, 2))));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::BombExploded { owner, .. } if owner.index() == 0)),
        "the bomb in the air stays"
    );
    assert!(s.bombs.iter().any(|b| b.owner.index() == 0));
}

#[test]
fn a_landed_punched_bomb_takes_part_in_chain_reactions_again() {
    let mut s = Setup::new(Dir::Right, (1, 3), (2, 3))
        .other_bomb((5, 3))
        .player(1, (9, 1))
        .build();
    s.bombs[0].ticks_left = 8;
    step(&mut s, &punch_input()); // lands on (3, 3) after two ticks, two squares from the other bomb
    let mut exploded = Vec::new();
    for _ in 0..12 {
        for e in step(&mut s, &idle()) {
            if let Event::BombExploded { pos, .. } = e {
                exploded.push(pos);
            }
        }
    }
    assert!(exploded.contains(&p(3, 3)));
    assert!(
        exploded.contains(&p(5, 3)),
        "the flames of the landed bomb set off the other one"
    );
}

#[test]
fn a_punch_is_deterministic() {
    let run = || {
        let mut s = Setup::new(Dir::Right, (1, 3), (2, 3))
            .tile(3, 3, '+')
            .build();
        let mut all = step(&mut s, &punch_input());
        for _ in 0..20 {
            all.extend(step(&mut s, &idle()));
        }
        (all, s.bombs.clone())
    };
    assert_eq!(run(), run());
    let _ = ROW;
}
