mod common;

use common::*;
use eksplode_re_core::{step, Config, Event, Outcome, Phase};

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

/// 7x7 open arena, pattern from tick 50: squares land at 55, 57, ... (every 2 ticks); the last
/// two squares of the spiral, (2,3) and (3,3), land at ticks 101 and 103.
fn pattern_config() -> Config {
    Config {
        round_limit_ticks: 100,
        closing_ticks: 50,
        closing_lead_ticks: 5,
        ..Config::default()
    }
}

fn run_until(s: &mut eksplode_re_core::GameState, tick: u32) {
    while s.tick < tick && s.phase == Phase::Playing {
        step(s, &idle());
    }
}

#[test]
fn f4_us2_as3_the_round_goes_on_past_the_time_while_two_players_live() {
    let mut s = state_with(
        pattern_config(),
        &OPEN,
        [Some((2, 3)), Some((3, 3)), None, None],
    );
    run_until(&mut s, 101);
    assert_eq!(s.tick, 101, "past the limit of 100");
    assert_eq!(
        s.phase,
        Phase::Playing,
        "no draw by time while two players are alive"
    );
}

#[test]
fn f4_us2_as4_the_covered_board_ends_the_round_with_the_last_survivor_or_a_draw() {
    // the first to be covered dies, the other wins
    let mut s = state_with(
        pattern_config(),
        &OPEN,
        [Some((2, 3)), Some((3, 3)), None, None],
    );
    let mut events = Vec::new();
    while s.phase == Phase::Playing {
        events.extend(step(&mut s, &idle()));
    }
    assert_eq!(s.phase, Phase::Ended(Outcome::Winner(pid(1))));
    assert!(
        s.tick <= 100 + 5 + 1,
        "ended within the pattern time and the lead: {}",
        s.tick
    );
    assert!(events.contains(&Event::PlayerEliminated { player: pid(0) }));

    // two players eliminated by the same block and explosion: a draw
    let mut s = state_with(
        pattern_config(),
        &OPEN,
        [Some((2, 3)), Some((3, 3)), None, None],
    );
    run_until(&mut s, 101);
    s.flames.push(eksplode_re_core::Flame {
        pos: pos(3, 3),
        ticks_left: 2,
        owners: 0,
    });
    step(&mut s, &idle());
    assert_eq!(s.phase, Phase::Ended(Outcome::Draw));
}

#[test]
fn f4_us2_as5_one_player_left_before_the_pattern_ends_the_round_at_once_and_it_never_starts() {
    let starts = [Some((1, 5)), Some((3, 2)), None, None];
    let mut s = state_with(pattern_config(), &OPEN, starts);
    add_bomb(&mut s, 0, 3, 3, 1);
    let ev = step(&mut s, &idle());
    assert_eq!(s.phase, Phase::Ended(Outcome::Winner(pid(0))));
    assert!(!ev.contains(&Event::PatternStarted));
    assert!(!s.closing.started);
    assert!(s.closing.falling.is_empty());
}

#[test]
fn a_practice_round_of_one_keeps_the_time_draw_and_never_runs_the_pattern() {
    let mut s = state_with(pattern_config(), &OPEN, [Some((3, 3)), None, None, None]);
    let mut events = Vec::new();
    while s.phase == Phase::Playing {
        events.extend(step(&mut s, &idle()));
    }
    assert_eq!(s.tick, 100);
    assert_eq!(s.phase, Phase::Ended(Outcome::Draw));
    assert!(!events.contains(&Event::PatternStarted));
}

#[test]
fn a_pattern_that_is_not_shorter_than_the_limit_or_is_off_leaves_the_time_draw() {
    for closing_ticks in [0, 100, 500] {
        let c = Config {
            round_limit_ticks: 100,
            closing_ticks,
            ..Config::default()
        };
        let mut s = state_with(c, &OPEN, two_safe());
        while s.phase == Phase::Playing {
            step(&mut s, &idle());
        }
        assert_eq!(s.tick, 100, "closing_ticks {closing_ticks}");
        assert_eq!(s.phase, Phase::Ended(Outcome::Draw));
    }
}
