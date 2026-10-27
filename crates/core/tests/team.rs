#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use eksplode_re_core::{step, Config, Controller, Flame, GameState, Lobby, Mode, Outcome, Phase};

fn team_config() -> Config {
    Config {
        mode: Mode::Team,
        ..Config::default()
    }
}

/// Team A: slots 0 and 1 on the left, team B: slots 2 and 3 on the right.
const FOUR: [Option<(u8, u8)>; 4] = [Some((1, 1)), Some((1, 5)), Some((5, 1)), Some((5, 5))];

fn team_state(config: Config) -> GameState {
    state_with(config, &OPEN, FOUR)
}

fn flame(s: &mut GameState, x: u8, y: u8, owners: u8) {
    s.flames.push(Flame {
        pos: pos(x, y),
        ticks_left: 3,
        owners,
    });
}

fn alive(s: &GameState) -> [bool; 4] {
    [0, 1, 2, 3].map(|i| s.players[i].alive)
}

#[test]
fn f4_us3_as1_teams_are_fixed_by_slot_and_four_participants_are_needed() {
    let s = team_state(team_config());
    assert_eq!(
        [0, 1, 2, 3].map(|i| s.players[i].team),
        [Some(0), Some(0), Some(1), Some(1)],
        "slots 1 and 2 against slots 3 and 4"
    );
    let normal = team_state(Config::default());
    assert!(normal.players.iter().all(|p| p.team.is_none()));
    assert!(s.observe(pid(0)).players.iter().all(|p| p.team.is_some()));

    let mut lobby = Lobby::new();
    lobby.settings.mode = Mode::Team;
    for i in 0..3 {
        lobby.assign(i, Controller::Remote, "p").unwrap();
    }
    assert!(!lobby.can_start());
    let problem = lobby.start_problem().unwrap();
    assert!(
        problem.contains("Team") && problem.contains("4") && problem.contains("1 missing"),
        "{problem}"
    );
    assert!(problem.contains("bots"), "{problem}");
    lobby.add_bot().unwrap();
    assert!(lobby.can_start());
    assert_eq!(lobby.start_problem(), None);
}

#[test]
fn f4_us3_as2_a_team_wins_when_the_other_team_is_eliminated() {
    // both members of team B burn: team A wins
    let mut s = team_state(team_config());
    flame(&mut s, 5, 1, 0);
    flame(&mut s, 5, 5, 0);
    let ev = step(&mut s, &idle());
    assert_eq!(alive(&s), [true, true, false, false]);
    assert_eq!(s.phase, Phase::Ended(Outcome::TeamWin(0)));
    assert!(ev.contains(&eksplode_re_core::Event::RoundEnded(Outcome::TeamWin(0))));

    // one member of each team burns: the round goes on
    let mut s = team_state(team_config());
    flame(&mut s, 1, 5, 0);
    flame(&mut s, 5, 5, 0);
    step(&mut s, &idle());
    assert_eq!(alive(&s), [true, false, true, false]);
    assert_eq!(s.phase, Phase::Playing);

    // the winning team does not need both members: one of them is eliminated as well
    let mut s = team_state(team_config());
    flame(&mut s, 1, 5, 0);
    flame(&mut s, 5, 1, 0);
    flame(&mut s, 5, 5, 0);
    step(&mut s, &idle());
    assert_eq!(alive(&s), [true, false, false, false]);
    assert_eq!(s.phase, Phase::Ended(Outcome::TeamWin(0)));

    // and team B can win too
    let mut s = team_state(team_config());
    flame(&mut s, 1, 1, 0);
    flame(&mut s, 1, 5, 0);
    step(&mut s, &idle());
    assert_eq!(s.phase, Phase::Ended(Outcome::TeamWin(1)));
}

#[test]
fn f4_us3_as3_both_teams_eliminated_together_is_a_draw() {
    let mut s = team_state(team_config());
    for (x, y) in [(1, 1), (1, 5), (5, 1), (5, 5)] {
        flame(&mut s, x, y, 0);
    }
    step(&mut s, &idle());
    assert_eq!(alive(&s), [false; 4]);
    assert_eq!(s.phase, Phase::Ended(Outcome::Draw));

    // the last member of each team in the same tick
    let mut s = team_state(team_config());
    flame(&mut s, 1, 5, 0);
    flame(&mut s, 5, 5, 0);
    step(&mut s, &idle());
    flame(&mut s, 1, 1, 0);
    flame(&mut s, 5, 1, 0);
    step(&mut s, &idle());
    assert_eq!(s.phase, Phase::Ended(Outcome::Draw));
}

/// Players: 0 at (1,3) in the blast of its own bomb, 1 (teammate) at (3,2), 2 (enemy) at (4,3),
/// 3 (enemy) far away. The bomb of player 0 sits on (3,3) and goes off at once.
fn blast_setup(config: Config) -> GameState {
    let starts = [Some((1, 3)), Some((3, 2)), Some((4, 3)), Some((5, 5))];
    let mut s = state_with(config, &OPEN, starts);
    add_bomb(&mut s, 0, 3, 3, 1);
    s
}

#[test]
fn f4_us3_as4_friendly_fire_on_hurts_teammates_like_anybody() {
    let mut s = blast_setup(team_config());
    assert!(s.config.friendly_fire, "the default is on");
    step(&mut s, &idle());
    assert_eq!(
        alive(&s),
        [false, false, false, true],
        "everybody in the blast dies"
    );
}

#[test]
fn f4_us3_as4_friendly_fire_off_spares_teammates_but_not_the_owner_or_opponents() {
    let config = Config {
        friendly_fire: false,
        ..team_config()
    };
    let mut s = blast_setup(config);
    step(&mut s, &idle());
    assert!(s.players[1].alive, "the teammate in the blast survives");
    assert!(
        !s.players[0].alive,
        "a player is still hurt by their own bomb"
    );
    assert!(!s.players[2].alive, "an opponent in the blast dies");
    assert!(s.players[3].alive);
    assert_eq!(s.phase, Phase::Playing, "one of each team is left");
}

#[test]
fn friendly_fire_off_chain_reactions_still_work_and_use_the_owner_of_each_bomb() {
    let config = Config {
        friendly_fire: false,
        ..team_config()
    };
    let starts = [Some((1, 3)), Some((3, 2)), Some((5, 1)), Some((5, 5))];
    let mut s = state_with(config, &OPEN, starts);
    // player 0's bomb sets off player 1's bomb at (5,3): its flames reach (5,5) where an
    // opponent stands, but its owner's teammate is out of reach
    add_bomb(&mut s, 0, 3, 3, 1);
    add_bomb(&mut s, 1, 5, 3, 50);
    let ev = step(&mut s, &idle());
    let exploded = ev
        .iter()
        .filter(|e| matches!(e, eksplode_re_core::Event::BombExploded { .. }))
        .count();
    assert_eq!(exploded, 2, "the chain reaction goes off");
    assert!(
        !s.players[3].alive,
        "the opponent in the second bomb's flames dies"
    );
    assert!(
        s.players[1].alive,
        "the teammate of the first bomb's owner survives it"
    );
    let f = s.flames.iter().find(|f| f.pos == pos(5, 5)).unwrap();
    assert!(
        f.owned_by(pid(1)) && !f.owned_by(pid(0)),
        "the flame belongs to the second bomb"
    );
}

#[test]
fn a_flame_lit_by_a_teammate_and_an_opponent_still_hurts_with_friendly_fire_off() {
    let config = Config {
        friendly_fire: false,
        ..team_config()
    };
    // owners: player 0 (a teammate of player 1) and player 2 (an opponent)
    let mut s = state_with(config.clone(), &OPEN, FOUR);
    flame(&mut s, 1, 5, 0b0101);
    step(&mut s, &idle());
    assert!(!s.players[1].alive);

    // only the teammate lit it: spared
    let mut s = state_with(config.clone(), &OPEN, FOUR);
    flame(&mut s, 1, 5, 0b0001);
    step(&mut s, &idle());
    assert!(s.players[1].alive);

    // a flame of unknown origin hurts everybody
    let mut s = state_with(config, &OPEN, FOUR);
    flame(&mut s, 1, 5, 0);
    step(&mut s, &idle());
    assert!(!s.players[1].alive);
}

#[test]
fn normal_mode_ignores_the_friendly_fire_setting() {
    let config = Config {
        friendly_fire: false,
        ..Config::default()
    };
    let mut s = state_with(config, &OPEN, FOUR);
    flame(&mut s, 1, 5, 0b0001);
    step(&mut s, &idle());
    assert!(!s.players[1].alive, "no teams, so every flame hurts");
}

#[test]
fn the_end_pattern_runs_in_team_mode_and_ends_in_a_team_win() {
    let config = Config {
        round_limit_ticks: 100,
        closing_ticks: 50,
        closing_lead_ticks: 5,
        ..team_config()
    };
    // team B stands on early squares of the spiral, team A on the last two
    let starts = [Some((3, 3)), Some((2, 3)), Some((1, 1)), Some((5, 5))];
    let mut s = state_with(config, &OPEN, starts);
    let mut started = false;
    while s.phase == Phase::Playing {
        started |= step(&mut s, &idle()).contains(&eksplode_re_core::Event::PatternStarted);
    }
    assert!(started);
    assert_eq!(s.phase, Phase::Ended(Outcome::TeamWin(0)));
    assert!(s.players[0].alive && s.players[1].alive);
}

#[test]
fn two_players_of_one_team_alone_do_not_end_the_round_at_once() {
    // a Team room needs four, but the rules must not declare a winner at tick zero when only
    // one team took part
    let starts = [Some((1, 1)), Some((1, 5)), None, None];
    let mut s = state_with(team_config(), &OPEN, starts);
    step(&mut s, &idle());
    assert_eq!(s.phase, Phase::Playing);
}
