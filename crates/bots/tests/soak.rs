#![allow(clippy::unwrap_used)]

mod common;

use common::*;
use eksplode_re_bots::{Bot, DefaultBot};
use eksplode_re_core::{blast, step, Config, GameState, Outcome, Phase, PlayerInput, Pos};
use std::collections::{BTreeSet, VecDeque};

fn play(seed: u64) -> GameState {
    let mut s = GameState::new(Config::default(), seed, [true; 4]).unwrap();
    let mut bots: Vec<DefaultBot> = (0..4).map(|i| DefaultBot::new(seed * 4 + i)).collect();
    let limit = s.config.round_limit_ticks + s.config.closing_lead_ticks + 5;
    for _ in 0..limit {
        if s.phase != Phase::Playing {
            break;
        }
        let mut inputs = [PlayerInput::default(); 4];
        for (i, bot) in bots.iter_mut().enumerate() {
            inputs[i] = bot.decide(&s.observe(pid(i as u8)));
        }
        step(&mut s, &inputs);
    }
    s
}

#[test]
fn us2_as4_four_bot_matches_always_finish() {
    let mut winners = 0;
    for seed in 0..100 {
        let s = play(seed);
        match s.phase {
            Phase::Ended(Outcome::Winner(_)) => winners += 1,
            Phase::Ended(_) => {}
            Phase::Playing => panic!("seed {seed} did not finish within the round limit"),
        }
        assert!(
            s.tick <= s.config.round_limit_ticks + s.config.closing_lead_ticks + 1,
            "seed {seed}"
        );
    }
    println!("100/100 matches finished, {winners} with a winner");
}

/// Is there any tile outside the blast within `max_moves` plain moves?
fn escape_exists(s: &GameState, from: Pos, bomb_range: u8, max_moves: u32) -> bool {
    let danger = blast(&s.arena, from, bomb_range).tiles;
    let mut seen = BTreeSet::from([from]);
    let mut queue = VecDeque::from([(from, 0u32)]);
    while let Some((p, d)) = queue.pop_front() {
        if !danger.contains(&p) {
            return true;
        }
        if d == max_moves {
            continue;
        }
        for dir in eksplode_re_core::Dir::ALL {
            let Some(n) = p.step(dir, s.arena.width(), s.arena.height()) else {
                continue;
            };
            if s.arena.tile(n) == eksplode_re_core::Tile::Floor && seen.insert(n) {
                queue.push_back((n, d + 1));
            }
        }
    }
    false
}

/// A bomb sits under the bot at the start; if any escape exists the bot has to survive.
#[test]
fn us2_as2_bots_escape_in_at_least_95_percent_of_scripted_scenarios() {
    let (mut applicable, mut survived) = (0, 0);
    for seed in 0..200u64 {
        let active = [true, true, false, false];
        let open = Config {
            block_density: 0.35,
            ..Config::default()
        };
        let mut s = GameState::new(open, seed, active).unwrap();
        let me = s.players[0].pos;
        let range = s.players[1].range;
        s.bombs.push(eksplode_re_core::Bomb {
            owner: pid(1),
            pos: me,
            ticks_left: 30,
            range,
            flight: None,
            slide: None,
        });
        s.players[1].bombs_active = 1;
        if !escape_exists(&s, me, range, 5) {
            continue;
        }
        applicable += 1;
        let mut bot = DefaultBot::new(seed);
        for _ in 0..40 {
            let mut inputs = [PlayerInput::default(); 4];
            inputs[0] = bot.decide(&s.observe(pid(0)));
            step(&mut s, &inputs);
        }
        if s.players[0].alive {
            survived += 1;
        }
    }
    assert!(
        applicable >= 100,
        "only {applicable} scenarios had an escape"
    );
    let rate = f64::from(survived) / f64::from(applicable);
    println!("escape rate {survived}/{applicable} = {rate:.3}");
    assert!(rate >= 0.95, "escape rate {rate}");
}

#[test]
fn f4_us2_bots_outlive_a_stationary_player_in_the_end_pattern() {
    let config = Config {
        round_limit_ticks: 600,
        closing_ticks: 300,
        ..Config::default()
    };
    // slot 0 is a bot or stands still; the others stand still, so only the pattern can kill
    let survival = |bot: bool, seed: u64| -> u32 {
        let mut s = GameState::new(config.clone(), seed, [true; 4]).unwrap();
        let mut b = DefaultBot::new(seed);
        while s.phase == Phase::Playing && s.players[0].alive {
            let mut inputs = [PlayerInput::default(); 4];
            if bot {
                inputs[0] = b.decide(&s.observe(pid(0)));
            }
            step(&mut s, &inputs);
        }
        s.tick
    };
    let (mut with_bot, mut idle) = (0u32, 0u32);
    for seed in 0..30 {
        with_bot += survival(true, seed);
        idle += survival(false, seed);
    }
    println!("total survival ticks: bot {with_bot}, idle {idle}");
    assert!(with_bot > idle, "bot {with_bot} must outlive idle {idle}");
}

#[test]
fn f4_us2_four_bot_normal_rounds_end_in_the_pattern_with_one_survivor_or_a_draw() {
    let config = Config {
        round_limit_ticks: 600,
        closing_ticks: 300,
        ..Config::default()
    };
    for seed in 0..50u64 {
        let mut s = GameState::new(config.clone(), seed, [true; 4]).unwrap();
        let mut bots: Vec<DefaultBot> = (0..4).map(|i| DefaultBot::new(seed * 4 + i)).collect();
        while s.phase == Phase::Playing {
            let mut inputs = [PlayerInput::default(); 4];
            for (i, bot) in bots.iter_mut().enumerate() {
                inputs[i] = bot.decide(&s.observe(pid(i as u8)));
            }
            step(&mut s, &inputs);
            assert!(s.tick <= 600 + 21, "seed {seed} never ended");
        }
        assert!(s.alive_count() <= 1, "seed {seed}");
    }
}

#[test]
fn f4_sc010_the_large_board_keeps_the_tick_rate_with_four_bots() {
    use eksplode_re_core::{BoardSize, RoomSettings};
    let config = RoomSettings {
        size: BoardSize::Large,
        ..RoomSettings::default()
    }
    .to_config(&Config::default());
    let started = std::time::Instant::now();
    let mut ticks = 0u32;
    // five minutes of play at 20 Hz: 6000 ticks, over as many rounds as it takes
    for seed in 0u64.. {
        let mut s = GameState::new(config.clone(), seed, [true; 4]).unwrap();
        let mut bots: Vec<DefaultBot> = (0..4).map(|i| DefaultBot::new(seed * 4 + i)).collect();
        while s.phase == Phase::Playing {
            let mut inputs = [PlayerInput::default(); 4];
            for (i, bot) in bots.iter_mut().enumerate() {
                inputs[i] = bot.decide(&s.observe(pid(i as u8)));
            }
            step(&mut s, &inputs);
            ticks += 1;
        }
        if ticks >= 6000 {
            break;
        }
    }
    let per_tick = started.elapsed() / ticks;
    println!("{ticks} ticks, {per_tick:?} per tick (four bots, large board)");
    assert!(
        per_tick < std::time::Duration::from_millis(25),
        "{per_tick:?} per tick"
    );
}

#[test]
fn f4_us3_four_bot_team_rounds_always_end_with_a_team_result() {
    for friendly_fire in [true, false] {
        let config = Config {
            mode: eksplode_re_core::Mode::Team,
            friendly_fire,
            round_limit_ticks: 600,
            closing_ticks: 300,
            ..Config::default()
        };
        let mut team_wins = 0;
        for seed in 0..50u64 {
            let mut s = GameState::new(config.clone(), seed, [true; 4]).unwrap();
            let mut bots: Vec<DefaultBot> = (0..4).map(|i| DefaultBot::new(seed * 4 + i)).collect();
            while s.phase == Phase::Playing {
                let mut inputs = [PlayerInput::default(); 4];
                for (i, bot) in bots.iter_mut().enumerate() {
                    inputs[i] = bot.decide(&s.observe(pid(i as u8)));
                }
                step(&mut s, &inputs);
                assert!(s.tick <= 600 + 21, "seed {seed} never ended");
            }
            match s.phase {
                Phase::Ended(Outcome::TeamWin(_)) => team_wins += 1,
                Phase::Ended(Outcome::Draw) => {}
                other => panic!("seed {seed}: {other:?}"),
            }
        }
        println!("friendly fire {friendly_fire}: {team_wins}/50 rounds with a winning team");
        assert!(team_wins > 0);
    }
}

#[test]
fn f4_us4_soak_chaser_two_hundred_rounds_end_in_time_and_nobody_gets_stuck() {
    let config = Config {
        mode: eksplode_re_core::Mode::Chaser,
        round_limit_ticks: 900,
        ..Config::default()
    };
    let (mut chaser_wins, mut runner_wins, mut spawned, mut ticks_total) = (0, 0, 0u32, 0u32);
    let mut chaser_bombs = 0u32;
    for seed in 0..200u64 {
        let mut s = GameState::new(config.clone(), seed, [true; 4]).unwrap();
        let mut bots: Vec<DefaultBot> = (0..4).map(|i| DefaultBot::new(seed * 4 + i)).collect();
        // the last tick at which each participant moved or placed a bomb
        let mut active_at = [0u32; 4];
        let mut last_pos: Vec<_> = s.players.iter().map(|p| p.pos).collect();
        while s.phase == Phase::Playing {
            let mut inputs = [PlayerInput::default(); 4];
            for (i, bot) in bots.iter_mut().enumerate() {
                inputs[i] = bot.decide(&s.observe(pid(i as u8)));
            }
            let ev = step(&mut s, &inputs);
            spawned += ev
                .iter()
                .filter(|e| matches!(e, eksplode_re_core::Event::PowerUpSpawned { .. }))
                .count() as u32;
            chaser_bombs += ev
                .iter()
                .filter(|e| matches!(e, eksplode_re_core::Event::BombPlaced { player, .. } if s.players[player.index()].role == eksplode_re_core::Role::Chaser))
                .count() as u32;
            for i in 0..4 {
                let moved = s.players[i].pos != last_pos[i];
                let bombed = ev.iter().any(|e| matches!(e, eksplode_re_core::Event::BombPlaced { player, .. } if player.index() == i));
                // a runner closed in by blocks and a bomb cannot go anywhere: that is not stuck
                let exit = eksplode_re_core::Dir::ALL.iter().any(|d| {
                    s.players[i]
                        .pos
                        .step(*d, s.arena.width(), s.arena.height())
                        .is_some_and(|n| {
                            s.arena.tile(n) == eksplode_re_core::Tile::Floor
                                && !s.bombs.iter().any(|b| b.pos == n && b.flight.is_none())
                                && !s.players.iter().any(|o| o.alive && o.pos == n)
                        })
                });
                if moved || bombed || !exit || !s.players[i].alive {
                    active_at[i] = s.tick;
                    last_pos[i] = s.players[i].pos;
                }
                assert!(
                    s.tick - active_at[i] <= 100,
                    "seed {seed}: player {i} stood still for more than 5 seconds at tick {}",
                    s.tick
                );
            }
            assert!(s.tick <= 900 + 20, "seed {seed} did not end");
        }
        ticks_total += s.tick;
        match s.phase {
            Phase::Ended(Outcome::ChaserWins) => chaser_wins += 1,
            Phase::Ended(Outcome::RunnersWin) => runner_wins += 1,
            Phase::Ended(Outcome::Draw) => {}
            other => panic!("seed {seed}: {other:?}"),
        }
    }
    println!("chaser wins {chaser_wins}, runner wins {runner_wins}, bonuses {spawned} in {ticks_total} ticks");
    // the runners flee well, so the chaser rarely finishes a round, but it must hunt
    assert!(runner_wins > 0);
    assert!(
        chaser_bombs >= 100,
        "the chaser only placed {chaser_bombs} bombs in 200 rounds"
    );
    // the schedule: one bonus per 100 ticks while the board is below its cap and a square is free
    let expected = ticks_total / 100;
    assert!(
        spawned * 10 >= expected * 5,
        "{spawned} bonuses for {ticks_total} ticks"
    );
}

// Feature 012: statistics per behaviour and difficulty.

use eksplode_re_bots::{Behaviour, Profile};
use eksplode_re_core::{BotDifficulty, Event};

/// What happened to each slot during one match.
#[derive(Default)]
struct Stats {
    /// Bombs placed while an opponent stood in their blast.
    hits: [u32; 4],
    /// Eliminated by a flame of its own bomb.
    self_kills: [u32; 4],
    /// Ticks alive.
    survived: [u32; 4],
}

/// Plays one match with one bot per slot (`None` for an empty slot).
fn play_profiles(seed: u64, profiles: [Option<Profile>; 4]) -> (GameState, Stats) {
    let active = profiles.map(|p| p.is_some());
    let mut s = GameState::new(Config::default(), seed, active).unwrap();
    let mut bots = profiles.map(|p| p.map(|p| DefaultBot::with_profile(seed * 4 + 7, p)));
    let mut stats = Stats::default();
    let limit = s.config.round_limit_ticks + s.config.closing_lead_ticks + 5;
    for _ in 0..limit {
        if s.phase != Phase::Playing {
            break;
        }
        let mut inputs = [PlayerInput::default(); 4];
        for (i, bot) in bots.iter_mut().enumerate() {
            if let Some(bot) = bot {
                inputs[i] = bot.decide(&s.observe(pid(i as u8)));
            }
        }
        for event in step(&mut s, &inputs) {
            match event {
                Event::BombPlaced { player, pos } => {
                    let me = &s.players[player.index()];
                    let tiles = blast(&s.arena, pos, me.placed_range()).tiles;
                    if s.players
                        .iter()
                        .any(|p| p.alive && p.id != player && tiles.contains(&p.pos))
                    {
                        stats.hits[player.index()] += 1;
                    }
                }
                Event::PlayerEliminated { player } => {
                    let at = s.players[player.index()].pos;
                    if s.flames.iter().any(|f| f.pos == at && f.owned_by(player)) {
                        stats.self_kills[player.index()] += 1;
                    }
                }
                _ => {}
            }
        }
        for (i, p) in s.players.iter().enumerate() {
            if p.alive && active[i] {
                stats.survived[i] += 1;
            }
        }
    }
    (s, stats)
}

#[test]
fn f12_sc001_behaviour_distribution() {
    let mut counts = [0u32; 3];
    for seed in 0..75u64 {
        for slot in 0..4 {
            let b = Behaviour::pick(seed.wrapping_mul(4).wrapping_add(slot));
            counts[Behaviour::ALL.iter().position(|x| *x == b).unwrap()] += 1;
        }
    }
    println!("behaviours over 300 seats: {counts:?}");
    for c in counts {
        let share = f64::from(c) / 300.0;
        assert!((0.25..=0.42).contains(&share), "{counts:?}");
    }
}

#[test]
fn f12_sc004_aggressive_hits_more_protective_survives_longer() {
    // the same 100 seeds, once with four Aggressive bots and once with four Protective ones
    let mut totals = Vec::new();
    for behaviour in [Behaviour::Aggressive, Behaviour::Protective] {
        let profile = Some(Profile {
            behaviour,
            difficulty: BotDifficulty::Hard,
        });
        let (mut hits, mut survived) = (0u32, 0u64);
        for seed in 0..100u64 {
            let (_, stats) = play_profiles(seed, [profile; 4]);
            hits += stats.hits.iter().sum::<u32>();
            survived += stats.survived.iter().map(|t| u64::from(*t)).sum::<u64>();
        }
        println!(
            "{behaviour:?}: hits {hits}, mean survival {} ticks",
            survived / 400
        );
        totals.push((hits, survived));
    }
    let (aggressive, protective) = (totals[0], totals[1]);
    assert!(
        f64::from(aggressive.0) >= 1.3 * f64::from(protective.0),
        "hits {} vs {}",
        aggressive.0,
        protective.0
    );
    assert!(protective.1 > aggressive.1, "survival {totals:?}");
}

/// Share of decided 1-v-1 rounds won by `strong` against `weak`, both Balanced, over 100 seeds;
/// the sides swap every other seed.
fn win_rate(strong: BotDifficulty, weak: BotDifficulty) -> f64 {
    let profile = |difficulty| {
        Some(Profile {
            behaviour: Behaviour::Balanced,
            difficulty,
        })
    };
    let (mut won, mut decided) = (0u32, 0u32);
    for seed in 0..100u64 {
        let strong_slot = (seed % 2) as usize;
        let mut profiles = [None; 4];
        profiles[strong_slot] = profile(strong);
        profiles[1 - strong_slot] = profile(weak);
        let (s, _) = play_profiles(seed, profiles);
        if let Phase::Ended(Outcome::Winner(w)) = s.phase {
            decided += 1;
            won += u32::from(w.index() == strong_slot);
        }
    }
    let rate = f64::from(won) / f64::from(decided.max(1));
    println!("{strong:?} vs {weak:?}: {won}/{decided} = {rate:.2}");
    rate
}

#[test]
fn f12_sc002_win_rates_by_difficulty() {
    use BotDifficulty::{Easy, Hard, Medium};
    assert!(win_rate(Hard, Easy) >= 0.65);
    assert!(win_rate(Medium, Easy) >= 0.55);
    assert!(win_rate(Hard, Medium) >= 0.55);
}

#[test]
fn f12_sc003_easy_self_eliminates_more() {
    let mut self_kills = Vec::new();
    for difficulty in [BotDifficulty::Easy, BotDifficulty::Hard] {
        let mut total = 0u32;
        for seed in 0..100u64 {
            let profiles = [0, 1, 2, 3].map(|i| Some(Profile::pick(seed * 4 + i, difficulty)));
            let (_, stats) = play_profiles(seed, profiles);
            total += stats.self_kills.iter().sum::<u32>();
        }
        println!("{difficulty:?}: {total} self-eliminations");
        self_kills.push(total);
    }
    assert!(self_kills[0] > 0, "{self_kills:?}");
    assert!(self_kills[0] >= 2 * self_kills[1], "{self_kills:?}");
}
