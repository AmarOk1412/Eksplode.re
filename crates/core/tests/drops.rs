#![allow(clippy::unwrap_used)]

mod common;

use bomberust_core::drops::{DropTable, DEFAULT_SHARES};
use bomberust_core::{step, Config, CoreError, Event, PowerUpKind, Rng};
use common::*;

const ALL: [PowerUpKind; 7] = [
    PowerUpKind::ExtraBomb,
    PowerUpKind::Range,
    PowerUpKind::Speed,
    PowerUpKind::Punch,
    PowerUpKind::Slow,
    PowerUpKind::HyperSpeed,
    PowerUpKind::SmallBomb,
];

fn only(kind: PowerUpKind) -> DropTable {
    let mut chances = [0.0; 7];
    let i = ALL.iter().position(|k| *k == kind).unwrap();
    chances[i] = 1.0;
    DropTable::new(chances).unwrap()
}

#[test]
fn the_kinds_are_in_the_documented_order_and_three_of_them_are_maluses() {
    assert_eq!(PowerUpKind::ALL, ALL);
    let maluses: Vec<_> = ALL.iter().filter(|k| k.is_malus()).collect();
    assert_eq!(
        maluses,
        [
            &PowerUpKind::Slow,
            &PowerUpKind::HyperSpeed,
            &PowerUpKind::SmallBomb
        ]
    );
}

#[test]
fn every_chance_must_be_finite_and_between_zero_and_one() {
    assert!(DropTable::new([0.0; 7]).is_ok());
    assert!(DropTable::new([1.0; 7]).is_ok());
    for bad in [-0.01, 1.01, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut chances = [0.1; 7];
        chances[3] = bad;
        assert!(
            matches!(DropTable::new(chances), Err(CoreError::InvalidConfig(_))),
            "{bad}"
        );
    }
}

#[test]
fn the_defaults_are_the_overall_rate_times_the_shares() {
    assert_eq!(DEFAULT_SHARES, [0.20, 0.24, 0.24, 0.03, 0.10, 0.08, 0.11]);
    let t = DropTable::defaults(0.3);
    for (i, kind) in ALL.iter().enumerate() {
        let expected = 0.3 * DEFAULT_SHARES[i];
        assert!((t.chance(*kind) - expected).abs() < 1e-6, "{kind:?}");
    }
    assert!((t.total() - 0.3).abs() < 1e-5);
    assert_eq!(DropTable::defaults(0.0).total(), 0.0);
    assert!((DropTable::defaults(1.0).total() - 1.0).abs() < 1e-5);
}

fn counts(t: &DropTable, seed: u64, n: usize) -> ([usize; 7], usize) {
    let mut rng = Rng::new(seed);
    let mut by_kind = [0; 7];
    let mut none = 0;
    for _ in 0..n {
        match t.roll(&mut rng) {
            Some(k) => by_kind[ALL.iter().position(|a| *a == k).unwrap()] += 1,
            None => none += 1,
        }
    }
    (by_kind, none)
}

#[test]
fn f3_us2_as9_over_1000_blocks_every_power_is_within_3_points_of_its_chance() {
    let t = DropTable::defaults(0.3);
    let (by_kind, _) = counts(&t, 12, 1000);
    for (i, kind) in ALL.iter().enumerate() {
        let share = by_kind[i] as f32 / 1000.0;
        assert!(
            (share - t.chance(*kind)).abs() <= 0.03,
            "{kind:?}: {share} vs {}",
            t.chance(*kind)
        );
    }
}

#[test]
fn f3_us2_as9_the_punch_is_at_most_5_percent_of_all_drops() {
    let t = DropTable::defaults(0.3);
    let (by_kind, none) = counts(&t, 99, 10_000);
    let drops = 10_000 - none;
    let punch = by_kind[3];
    assert!(drops > 2500, "{drops}");
    assert!(punch as f32 / drops as f32 <= 0.05, "{punch} of {drops}");
    assert!(punch > 0, "the punch is rare, not impossible");
    // each malus is rarer than each common bonus
    for malus in [4, 5, 6] {
        for bonus in [0, 1, 2] {
            assert!(
                by_kind[malus] < by_kind[bonus],
                "malus {malus} vs bonus {bonus}"
            );
        }
    }
}

#[test]
fn a_chance_of_one_for_one_power_drops_it_every_time_and_zero_never() {
    for (i, kind) in ALL.iter().enumerate() {
        let (by_kind, none) = counts(&only(*kind), 5, 200);
        assert_eq!(by_kind[i], 200, "{kind:?}");
        assert_eq!(none, 0);
        // at 0% it never appears, even when every other power is at 100%
        let mut chances = [1.0; 7];
        chances[i] = 0.0;
        let (by_kind, _) = counts(&DropTable::new(chances).unwrap(), 5, 200);
        assert_eq!(by_kind[i], 0, "{kind:?} never drops at 0%");
    }
    let (_, none) = counts(&DropTable::new([0.0; 7]).unwrap(), 1, 200);
    assert_eq!(none, 200, "all zeros drops nothing");
}

#[test]
fn chances_that_add_up_to_more_than_one_are_scaled_down() {
    let t = DropTable::new([1.0; 7]).unwrap();
    let (by_kind, none) = counts(&t, 3, 7000);
    assert_eq!(
        none, 0,
        "the total is capped at one, so something always drops"
    );
    for (i, n) in by_kind.iter().enumerate() {
        let share = *n as f32 / 7000.0;
        assert!((share - 1.0 / 7.0).abs() < 0.03, "kind {i}: {share}");
    }
    // a sum just above one keeps the proportions
    let t = DropTable::new([0.5, 0.5, 0.5, 0.0, 0.0, 0.0, 0.0]).unwrap();
    let (by_kind, none) = counts(&t, 4, 3000);
    assert_eq!(none, 0);
    assert_eq!(by_kind[3] + by_kind[4] + by_kind[5] + by_kind[6], 0);
}

const BLOCKS: [&str; 7] = [
    "#######", "#.....#", "#.+++.#", "#.....#", "#.+++.#", "#.....#", "#######",
];

#[test]
fn destroyed_blocks_drop_by_the_table_at_the_moment_they_break() {
    let mut s = state(&BLOCKS, two_safe());
    s.set_drops(only(PowerUpKind::Punch)).unwrap();
    add_bomb(&mut s, 0, 3, 3, 1);
    let events = step(&mut s, &idle());
    let destroyed = events
        .iter()
        .filter(|e| matches!(e, Event::BlockDestroyed { .. }))
        .count();
    assert!(destroyed >= 2, "{events:?}");
    let spawned: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::PowerUpSpawned { kind, .. } => Some(*kind),
            _ => None,
        })
        .collect();
    assert_eq!(
        spawned.len(),
        destroyed,
        "at most one item per block, here exactly one"
    );
    assert!(spawned.iter().all(|k| *k == PowerUpKind::Punch));
    assert_eq!(s.powerups.len(), destroyed);
}

#[test]
fn a_table_that_drops_nothing_drops_nothing() {
    let mut s = state(&BLOCKS, two_safe());
    s.set_drops(DropTable::new([0.0; 7]).unwrap()).unwrap();
    add_bomb(&mut s, 0, 3, 3, 1);
    step(&mut s, &idle());
    assert!(s.powerups.is_empty());
}

#[test]
fn the_table_can_change_between_ticks() {
    let mut s = state(&BLOCKS, two_safe());
    s.set_drops(DropTable::new([0.0; 7]).unwrap()).unwrap();
    add_bomb(&mut s, 0, 3, 3, 1);
    step(&mut s, &idle());
    assert!(s.powerups.is_empty());
    s.set_drops(only(PowerUpKind::Slow)).unwrap();
    add_bomb(&mut s, 1, 5, 4, 1);
    let events = step(&mut s, &idle());
    assert!(
        events.iter().all(|e| match e {
            Event::PowerUpSpawned { kind, .. } => *kind == PowerUpKind::Slow,
            _ => true,
        }),
        "{events:?}"
    );
}

#[test]
fn an_invalid_table_is_refused_and_changes_nothing() {
    let mut s = state(&BLOCKS, two_safe());
    let before = s.drops.clone();
    let mut chances = [0.1; 7];
    chances[2] = 2.0;
    assert!(DropTable::new(chances).is_err());
    let bad = DropTable { chances };
    assert!(s.set_drops(bad).is_err());
    assert_eq!(s.drops, before);
}

#[test]
fn the_same_seed_and_inputs_give_the_same_items() {
    let run = |seed: u64| {
        let arena = bomberust_core::Arena::from_rows(&BLOCKS).unwrap();
        let starts = two_safe().map(|s| s.map(|(x, y)| pos(x, y)));
        let mut s =
            bomberust_core::GameState::custom(Config::default(), arena, starts, seed).unwrap();
        s.set_drops(DropTable::defaults(1.0)).unwrap();
        add_bomb(&mut s, 0, 3, 3, 1);
        step(&mut s, &idle());
        s.powerups.clone()
    };
    assert_eq!(run(7), run(7));
    let different = (0..20).any(|seed| run(seed) != run(7));
    assert!(different, "other seeds give other items");
}

#[test]
fn a_new_round_starts_with_the_default_table_of_its_config() {
    let config = Config {
        powerup_chance: 0.5,
        ..Config::default()
    };
    let s = state_with(config, &BLOCKS, two_safe());
    assert_eq!(s.drops, DropTable::defaults(0.5));
}
