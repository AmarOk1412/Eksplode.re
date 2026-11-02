mod common;

use common::*;
use eksplode_re_core::{step, Config, Event, Outcome, Phase};

#[test]
fn f13_observation_suppresses_contradictory_kart_state() {
    let mut state = state(&OPEN, two_safe());
    state.players[0].kart = Some(eksplode_re_core::KartState {
        index: 0,
        entered_tick: 0,
        cooldown_left: 0,
        travel: None,
        pending_return: None,
    });
    state.players[1].alive = false;
    state.players[1].kart = state.players[0].kart;
    state.players[1].kart.as_mut().unwrap().index = u16::MAX;
    state.players[1].immobilized_left = u32::MAX;
    let observation = state.observe(pid(0));
    assert!(observation
        .players
        .iter()
        .all(|player| player.kart.is_none()));
    assert_eq!(observation.players[1].immobilized_left, 0);
    assert!(
        state.players[0].kart.is_some(),
        "observing does not mutate authority"
    );
}

fn earn_return(state: &mut eksplode_re_core::GameState) {
    state.config.fuse_ticks = 3;
    state.players[1].pos = pos(1, 2);
    throw_from_top(state);
    for _ in 0..3 {
        step(state, &idle());
    }
}

#[test]
fn f13_us3_as1_current_kart_bomb_kill_returns_its_owner_safely() {
    let mut state = kart_state();
    earn_return(&mut state);
    assert!(state.players[0].alive);
    assert!(state.players[0].kart.is_none());
    assert!(!state.players[1].alive);
    assert_eq!(state.players[0].kills, 1);
    assert_eq!(state.players[0].pos, pos(2, 2));
    assert!(!state
        .flames
        .iter()
        .any(|flame| flame.pos == state.players[0].pos));
    assert!(state
        .flames
        .iter()
        .any(|flame| flame.revenge_stints[0] == Some(0)));
}

#[test]
fn f13_us3_as2_return_resets_powers_but_preserves_statistics_and_owned_bombs() {
    let mut state = kart_state();
    state.players[0].bombs_max = 4;
    state.players[0].has_punch = true;
    state.players[0].has_kick = true;
    state.players[0].range = 8;
    state.players[0].move_cooldown = 2;
    state.players[0].kills = 7;
    state.players[0].eliminated_order = Some(1);
    add_bomb(&mut state, 0, 4, 4, 100);
    earn_return(&mut state);
    let player = &state.players[0];
    assert!(player.alive);
    assert_eq!(
        (player.bombs_max, player.range, player.move_cooldown),
        (1, 2, 4)
    );
    assert!(!player.has_punch && !player.has_kick);
    assert_eq!(player.kills, 8);
    assert_eq!(player.bombs_placed, 1);
    assert_eq!(player.eliminated_order, None);
    assert_eq!(player.bombs_active, 1);
    let mut inputs = idle();
    inputs[0].bomb = true;
    step(&mut state, &inputs);
    assert_eq!(state.players[0].bombs_active, 1);
}

#[test]
fn f13_us3_as3_simultaneous_returns_are_distinct_and_slot_ordered() {
    let mut state = state(
        &OPEN,
        [Some((1, 1)), Some((5, 1)), Some((2, 3)), Some((4, 3))],
    );
    for slot in 0..2 {
        state.players[slot].alive = false;
        state.players[slot].kart = Some(eksplode_re_core::KartState {
            index: slot as u16,
            entered_tick: 0,
            cooldown_left: 0,
            travel: None,
            pending_return: None,
        });
        add_bomb(&mut state, slot as u8, 2 + slot as u8 * 2, 2, 1);
        state.bombs.last_mut().unwrap().range = 1;
        state.bombs.last_mut().unwrap().revenge_stint = Some(0);
    }
    step(&mut state, &idle());
    assert!(state.players[0].alive && state.players[1].alive);
    assert_ne!(state.players[0].pos, state.players[1].pos);
    assert!(state.players[0].kart.is_none() && state.players[1].kart.is_none());
}

#[test]
fn f13_us3_as4_old_wrong_owner_and_teammate_sources_never_revive() {
    for (source, owner, teammate) in [
        (None, 0, false),
        (Some(1), 0, false),
        (Some(0), 2, false),
        (Some(0), 0, true),
    ] {
        let mut state = kart_state();
        if teammate {
            state.config.mode = eksplode_re_core::Mode::Team;
            state.started_teams = 3;
            state.players[0].team = Some(0);
            state.players[1].team = Some(0);
            state.players[2].team = Some(1);
        }
        add_bomb(&mut state, owner, 5, 1, 1);
        state.bombs[0].revenge_stint = source;
        state.bombs[0].range = 0;
        step(&mut state, &idle());
        assert!(!state.players[0].alive);
    }
}

#[test]
fn f13_us3_as5_no_safe_square_waits_for_the_first_available_square() {
    let mut state = kart_state();
    state.players[3] = state.players[2].clone();
    state.players[3].id = pid(3);
    state.players[3].pos = pos(5, 1);
    state.players[1].pos = pos(1, 2);
    for row in 1..6 {
        for column in 1..6 {
            let at = pos(column, row);
            if ![pos(1, 2), pos(1, 3), pos(5, 1), pos(5, 5)].contains(&at) {
                state.arena.set_tile(at, eksplode_re_core::Tile::Wall);
            }
        }
    }
    add_bomb(&mut state, 0, 1, 3, 1);
    state.bombs[0].revenge_stint = Some(0);
    step(&mut state, &idle());
    assert!(!state.players[0].alive);
    assert_eq!(
        state.players[0].kart.unwrap().pending_return,
        Some(pos(1, 2))
    );
    state
        .arena
        .set_tile(pos(2, 2), eksplode_re_core::Tile::Floor);
    step(&mut state, &idle());
    assert!(state.players[0].alive);
    assert_eq!(state.players[0].pos, pos(2, 2));
}

#[test]
fn f13_us3_as6_a_second_death_and_matching_stint_can_return_again() {
    let mut state = kart_state();
    state.players[3] = state.players[2].clone();
    state.players[3].id = pid(3);
    state.players[3].pos = pos(5, 1);
    earn_return(&mut state);
    assert!(state.players[0].alive);
    let at = state.players[0].pos;
    add_bomb(&mut state, 0, at.x, at.y, 1);
    state.bombs.last_mut().unwrap().range = 0;
    step(&mut state, &idle());
    let stint = state.players[0].kart.unwrap().entered_tick;
    assert!(stint > 0);
    add_bomb(&mut state, 0, 5, 5, 1);
    state.bombs.last_mut().unwrap().range = 0;
    state.bombs.last_mut().unwrap().revenge_stint = Some(stint);
    step(&mut state, &idle());
    assert!(state.players[0].alive);
    assert!(state.players[0].kart.is_none());
}

#[test]
fn f13_us3_as7_multikill_return_is_resolved_before_the_last_survivor_result() {
    let mut state = kart_state();
    state.players[1].pos = pos(1, 2);
    state.players[2].pos = pos(1, 4);
    add_bomb(&mut state, 0, 1, 3, 1);
    state.bombs[0].revenge_stint = Some(0);
    step(&mut state, &idle());
    assert!(state.players[0].alive);
    assert_eq!(state.phase, Phase::Ended(Outcome::Winner(pid(0))));
    assert_eq!(state.players[0].kills, 2);
}

fn kart_state() -> eksplode_re_core::GameState {
    let mut state = state(&OPEN, [Some((1, 1)), Some((5, 1)), Some((5, 5)), None]);
    state.players[0].alive = false;
    state.players[0].kart = Some(eksplode_re_core::KartState {
        index: 1,
        entered_tick: 0,
        cooldown_left: 0,
        travel: None,
        pending_return: None,
    });
    state
}

#[test]
fn f13_us3_as4_a_chain_trigger_does_not_inherit_the_trigger_owners_revenge() {
    let mut state = kart_state();
    add_bomb(&mut state, 0, 2, 1, 1);
    state.bombs[0].range = 1;
    state.bombs[0].revenge_stint = Some(0);
    add_bomb(&mut state, 2, 3, 1, 60);
    step(&mut state, &idle());
    assert!(!state.players[0].alive);
    assert_eq!(state.players[0].kills, 0);
    assert_eq!(state.players[2].kills, 1);
    assert!(state
        .flames
        .iter()
        .filter(|flame| flame.pos == pos(5, 1))
        .all(|flame| flame.revenge_stints[0].is_none()));
}

#[test]
fn f13_us3_as4_refreshed_flames_keep_only_current_stint_eligible() {
    for current_stint in [0, 1] {
        let mut state = kart_state();
        state.tick = 1;
        state.players[0].kart.as_mut().unwrap().entered_tick = current_stint;
        add_bomb(&mut state, 0, 3, 3, 1);
        state.bombs[0].range = 1;
        state.bombs[0].revenge_stint = Some(0);
        step(&mut state, &idle());
        state.players[1].pos = pos(3, 3);
        add_bomb(&mut state, 2, 3, 3, 1);
        state.bombs[0].range = 1;
        step(&mut state, &idle());
        assert_eq!(state.players[0].alive, current_stint == 0);
        assert_eq!(state.players[0].kills, 1);
        assert_eq!(state.players[2].kills, 1);
    }
}

#[test]
fn f13_us4_as3_genuinely_returned_player_stays_alive_at_closing() {
    let mut state = kart_state();
    earn_return(&mut state);
    assert!(state.players[0].alive);
    state.config.round_limit_ticks = 100;
    state.config.closing_ticks = 50;
    state.tick = 50;
    step(&mut state, &idle());
    assert!(state.players[0].alive);
    assert!(state.players[0].kart.is_none());
}

#[test]
fn f13_us1_as2_all_sizes_have_a_closed_unique_perimeter() {
    for (width, height) in [(7, 7), (15, 13), (19, 15), (25, 19), (255, 255)] {
        let length = eksplode_re_core::kart::perimeter_len(width, height);
        let mut positions = std::collections::BTreeSet::new();
        for index in 0..length {
            let (here, _) = eksplode_re_core::kart::track_position(width, height, index).unwrap();
            let (next, _) =
                eksplode_re_core::kart::track_position(width, height, (index + 1) % length)
                    .unwrap();
            assert!(positions.insert(here));
            assert_eq!(
                u16::from(here.x.abs_diff(next.x)) + u16::from(here.y.abs_diff(next.y)),
                1
            );
            let (first, direction) =
                eksplode_re_core::kart::launch_lane(width, height, index).unwrap();
            assert!(first.x > 0 && first.y > 0 && first.x < width - 1 && first.y < height - 1);
            assert!(first.step(direction, width, height).is_some());
        }
        assert!(eksplode_re_core::kart::track_position(width, height, length).is_none());
    }
}

#[test]
fn f13_foundation_defaults_and_source_validation() {
    let state = state(&OPEN, two_safe());
    assert!(state
        .players
        .iter()
        .all(|player| player.kart.is_none() && player.immobilized_left == 0));
    let mut observation = state.observe(pid(0));
    observation.flames.push(eksplode_re_core::Flame {
        pos: pos(2, 2),
        ticks_left: 5,
        owners: 1,
        revenge_stints: [Some(1), Some(0), None, None],
    });
    observation.sanitize_participation();
    assert_eq!(observation.flames[0].revenge_stints, [None; 4]);
}

#[test]
fn f13_us1_as1_elimination_creates_a_kart_without_ending_the_round() {
    let mut state = state(&OPEN, [Some((1, 5)), Some((5, 1)), Some((3, 2)), None]);
    state.players[2].has_punch = true;
    add_bomb(&mut state, 0, 3, 3, 1);
    step(&mut state, &idle());
    assert_eq!(state.phase, Phase::Playing);
    assert!(!state.players[2].alive);
    assert!(state.players[2].kart.is_some());
    assert!(!state.players[2].has_punch);
    assert!(state.players[3].kart.is_none());
}

#[test]
fn f13_us1_as2_kart_circuits_both_ways_and_stops_on_release() {
    let mut state = kart_state();
    state.config.base_move_cooldown = 1;
    let mut inputs = idle();
    inputs[0].mv = Some(eksplode_re_core::Dir::Right);
    step(&mut state, &inputs);
    assert_eq!(state.players[0].kart.unwrap().index, 2);
    for _ in 0..23 {
        step(&mut state, &inputs);
    }
    assert_eq!(state.players[0].kart.unwrap().index, 1);
    inputs[0].mv = Some(eksplode_re_core::Dir::Left);
    for _ in 0..24 {
        step(&mut state, &inputs);
    }
    assert_eq!(state.players[0].kart.unwrap().index, 1);
    step(&mut state, &idle());
    assert_eq!(state.players[0].kart.unwrap().travel, None);
}

#[test]
fn f13_us1_as3_kart_ignores_vertical_movement_and_punch() {
    let mut state = kart_state();
    let mut inputs = idle();
    inputs[0].mv = Some(eksplode_re_core::Dir::Down);
    inputs[0].punch = true;
    step(&mut state, &inputs);
    assert_eq!(state.players[0].kart.unwrap().index, 1);
    assert!(state.bombs.is_empty());
}

#[test]
fn f13_us1_as4_kart_throws_inward_once_despite_old_owned_bombs() {
    let mut state = kart_state();
    add_bomb(&mut state, 0, 4, 4, 60);
    let mut inputs = idle();
    inputs[0].bomb = true;
    step(&mut state, &inputs);
    assert_eq!(state.bombs.len(), 2);
    let thrown = state
        .bombs
        .iter()
        .find(|bomb| bomb.revenge_stint == Some(0))
        .unwrap();
    assert_eq!(thrown.pos, pos(1, 1));
    assert_eq!(thrown.flight.unwrap().dir, eksplode_re_core::Dir::Down);
    step(&mut state, &inputs);
    assert_eq!(state.bombs.len(), 2);
}

#[test]
fn f13_us1_as5_chaser_and_practice_never_create_karts() {
    for mode in [
        eksplode_re_core::Mode::Chaser,
        eksplode_re_core::Mode::Normal,
    ] {
        let starts = if mode == eksplode_re_core::Mode::Chaser {
            [Some((1, 1)), Some((5, 1)), Some((5, 5)), None]
        } else {
            only(0, 1, 1)
        };
        let mut state = state_with(
            Config {
                mode,
                ..Config::default()
            },
            &OPEN,
            starts,
        );
        state.players[0].hearts = 1;
        add_bomb(&mut state, 0, 1, 1, 1);
        step(&mut state, &idle());
        assert!(state.players.iter().all(|player| player.kart.is_none()));
    }
}

#[test]
fn f13_us1_as6_karts_never_count_as_living_or_collide_with_board() {
    let mut state = kart_state();
    add_bomb(&mut state, 1, 1, 1, 1);
    step(&mut state, &idle());
    assert_eq!(state.alive_count(), 2);
    assert!(state.players[0].kart.is_some());
    assert_eq!(state.players[0].pos, pos(1, 1));
}

fn throw_from_top(state: &mut eksplode_re_core::GameState) {
    state.config.punch_hop_ticks = 1;
    let mut inputs = idle();
    inputs[0].bomb = true;
    step(state, &inputs);
}

#[test]
fn f13_us2_as1_free_second_square_is_the_first_landing() {
    let mut state = kart_state();
    throw_from_top(&mut state);
    assert_eq!(state.bombs[0].pos, pos(1, 1));
    step(&mut state, &idle());
    assert_eq!(state.bombs[0].pos, pos(1, 2));
    assert!(state.bombs[0].flight.is_none());
}

#[test]
fn f13_us2_as2_obstacles_and_survivors_are_skipped_on_arrival() {
    let mut state = kart_state();
    state
        .arena
        .set_tile(pos(1, 2), eksplode_re_core::Tile::Wall);
    state.players[1].pos = pos(1, 3);
    throw_from_top(&mut state);
    for _ in 0..3 {
        step(&mut state, &idle());
    }
    assert_eq!(state.bombs[0].pos, pos(1, 4));
    assert!(state.bombs[0].flight.is_none());
    assert!(state.players[1].immobilized_left > 0);
}

#[test]
fn f13_us2_as3_direct_hit_blocks_exactly_one_second_of_moves() {
    for frequency in [7, 20] {
        let mut state = kart_state();
        state.config.tick_hz = frequency;
        state.players[1].pos = pos(1, 2);
        throw_from_top(&mut state);
        step(&mut state, &idle());
        assert_eq!(state.players[1].immobilized_left, frequency);
        assert!(state.players[1].alive);
        let mut inputs = idle();
        inputs[1].mv = Some(eksplode_re_core::Dir::Right);
        for _ in 0..frequency {
            step(&mut state, &inputs);
            assert_eq!(state.players[1].pos, pos(1, 2));
        }
        step(&mut state, &inputs);
        assert_eq!(state.players[1].pos, pos(2, 2));
    }
}

#[test]
fn f13_us2_as4_stun_keeps_bomb_and_punch_actions_available() {
    let mut state = kart_state();
    state.players[1].immobilized_left = 3;
    state.players[1].has_punch = true;
    state.players[1].facing = eksplode_re_core::Dir::Down;
    add_bomb(&mut state, 0, 5, 2, 60);
    let mut inputs = idle();
    inputs[1].bomb = true;
    inputs[1].punch = true;
    step(&mut state, &inputs);
    assert_eq!(state.players[1].immobilized_left, 2);
    assert_eq!(state.players[1].bombs_active, 1);
    assert!(state
        .bombs
        .iter()
        .any(|bomb| bomb.owner == pid(0) && bomb.flight.is_some()));
}

#[test]
fn f13_us2_as5_blocked_line_refuses_and_edge_falls_back_to_first_square() {
    let mut state = kart_state();
    for row in 1..6 {
        state
            .arena
            .set_tile(pos(1, row), eksplode_re_core::Tile::Wall);
    }
    throw_from_top(&mut state);
    assert!(state.bombs.is_empty());
    assert_eq!(state.players[0].bombs_active, 0);
    state
        .arena
        .set_tile(pos(1, 1), eksplode_re_core::Tile::Floor);
    throw_from_top(&mut state);
    for _ in 0..6 {
        step(&mut state, &idle());
    }
    assert_eq!(state.bombs[0].pos, pos(1, 1));
    assert!(state.bombs[0].flight.is_none());
}

#[test]
fn f13_us2_as6_airborne_fuse_expires_under_ordinary_rules() {
    let mut state = kart_state();
    state.config.fuse_ticks = 1;
    throw_from_top(&mut state);
    step(&mut state, &idle());
    assert!(state.bombs.is_empty());
    assert_eq!(state.players[0].bombs_active, 0);
    assert!(state.flames.iter().any(|flame| flame.pos == pos(1, 1)));
}

#[test]
fn f13_us2_as7_team_direct_hits_respect_friendly_fire() {
    for friendly_fire in [false, true] {
        let mut state = kart_state();
        state.config.mode = eksplode_re_core::Mode::Team;
        state.config.friendly_fire = friendly_fire;
        state.started_teams = 3;
        state.players[0].team = Some(0);
        state.players[1].team = Some(0);
        state.players[2].team = Some(1);
        state.players[1].pos = pos(1, 2);
        throw_from_top(&mut state);
        step(&mut state, &idle());
        assert_eq!(state.players[1].immobilized_left > 0, friendly_fire);
    }
}

#[test]
fn f13_us4_as1_first_warning_clears_karts_before_actions() {
    for tick in [50, 51] {
        let mut state = kart_state();
        state.config.round_limit_ticks = 100;
        state.config.closing_ticks = 50;
        state.tick = tick;
        state.players[0].kart.as_mut().unwrap().pending_return = Some(pos(3, 3));
        let mut inputs = idle();
        inputs[0].mv = Some(eksplode_re_core::Dir::Right);
        inputs[0].bomb = true;
        let events = step(&mut state, &inputs);
        assert!(state.players[0].kart.is_none());
        assert!(state.bombs.is_empty());
        assert!(events.contains(&Event::PatternStarted));
    }
}

#[test]
fn f13_us4_as2_old_kart_bombs_finish_but_no_karts_are_created_after_closing() {
    let mut state = kart_state();
    throw_from_top(&mut state);
    state.config.round_limit_ticks = 100;
    state.config.closing_ticks = 50;
    state.tick = 50;
    state.bombs[0].ticks_left = 1;
    let events = step(&mut state, &idle());
    assert!(events
        .iter()
        .any(|event| matches!(event, Event::BombExploded { owner, .. } if *owner == pid(0))));
    assert!(state.players[0].kart.is_none());
}

#[test]
fn f13_us4_as3_returned_living_state_stays_living_when_closing_starts() {
    let mut state = kart_state();
    state.players[0].kart = None;
    state.players[0].alive = true;
    state.config.round_limit_ticks = 100;
    state.config.closing_ticks = 50;
    state.tick = 50;
    step(&mut state, &idle());
    assert!(state.players[0].alive);
    assert!(state.players[0].kart.is_none());
}

#[test]
fn f13_us4_as4_result_and_rematch_clear_temporary_participation() {
    let mut state = kart_state();
    state.players[1].immobilized_left = 20;
    add_bomb(&mut state, 1, 5, 5, 1);
    step(&mut state, &idle());
    assert!(matches!(state.phase, Phase::Ended(_)));
    assert!(state
        .players
        .iter()
        .all(|player| player.kart.is_none() && player.immobilized_left == 0));
    let mut ended = state.clone();
    assert!(step(&mut ended, &idle()).is_empty());
    assert_eq!(ended, state);
    let fresh = eksplode_re_core::GameState::new(Config::default(), 2, [true; 4]).unwrap();
    assert!(fresh
        .players
        .iter()
        .all(|player| player.kart.is_none() && player.immobilized_left == 0));
}

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
        revenge_stints: [None; 4],
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
