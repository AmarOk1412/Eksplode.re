mod common;

use bomberust_core::{step, Config, Event, Outcome, Phase};
use common::*;

#[test]
fn us1_as7_last_survivor_wins() {
    let starts = [Some((1, 5)), Some((3, 2)), None, None];
    let mut s = state(&OPEN, starts);
    add_bomb(&mut s, 0, 3, 3, 1);
    let ev = step(&mut s, &idle());
    assert_eq!(s.phase, Phase::Ended(Outcome::Winner(pid(0))));
    assert!(ev.contains(&Event::RoundEnded(Outcome::Winner(pid(0)))));
}

#[test]
fn us1_as7_simultaneous_elimination_is_a_draw() {
    let starts = [Some((3, 2)), Some((3, 4)), None, None];
    let mut s = state(&OPEN, starts);
    add_bomb(&mut s, 0, 3, 3, 1);
    let ev = step(&mut s, &idle());
    assert_eq!(s.phase, Phase::Ended(Outcome::Draw));
    assert!(ev.contains(&Event::RoundEnded(Outcome::Draw)));
}

#[test]
fn us1_as7_time_limit_is_a_draw() {
    let c = Config {
        round_limit_ticks: 5,
        ..Config::default()
    };
    let mut s = state_with(c, &OPEN, two_safe());
    for _ in 0..4 {
        step(&mut s, &idle());
        assert_eq!(s.phase, Phase::Playing);
    }
    let ev = step(&mut s, &idle());
    assert_eq!(s.phase, Phase::Ended(Outcome::Draw));
    assert!(ev.contains(&Event::RoundEnded(Outcome::Draw)));
}

#[test]
fn step_after_the_round_ended_does_nothing() {
    let starts = [Some((1, 5)), Some((3, 2)), None, None];
    let mut s = state(&OPEN, starts);
    add_bomb(&mut s, 0, 3, 3, 1);
    step(&mut s, &idle());
    let before = s.clone();
    let ev = step(&mut s, &idle());
    assert!(ev.is_empty());
    assert_eq!(s, before);
}

#[test]
fn three_players_one_eliminated_keeps_playing() {
    let starts = [Some((1, 5)), Some((5, 1)), Some((3, 2)), None];
    let mut s = state(&OPEN, starts);
    add_bomb(&mut s, 0, 3, 3, 1);
    step(&mut s, &idle());
    assert_eq!(s.phase, Phase::Playing);
}

#[test]
fn solo_practice_runs_until_the_player_dies() {
    let mut s = state(&OPEN, only(0, 1, 1));
    for _ in 0..100 {
        step(&mut s, &idle());
    }
    assert_eq!(s.phase, Phase::Playing);
    add_bomb(&mut s, 0, 1, 2, 1);
    step(&mut s, &idle());
    assert_eq!(s.phase, Phase::Ended(Outcome::Draw));
}
