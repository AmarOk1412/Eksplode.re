mod common;

use common::*;
use eksplode_re_bots::{Bot, DefaultBot, Profile};
use eksplode_re_core::{step, Config, DropTable, GameState, Phase, PlayerInput, Rng};

/// The bots crate must never reach for the full game state.
#[test]
fn us2_as5_bot_sources_only_know_the_observation() {
    for (name, src) in [
        ("lib.rs", include_str!("../src/lib.rs")),
        ("danger.rs", include_str!("../src/danger.rs")),
        ("pathing.rs", include_str!("../src/pathing.rs")),
        ("default_bot.rs", include_str!("../src/default_bot.rs")),
        ("profile.rs", include_str!("../src/profile.rs")),
    ] {
        assert!(!src.contains("GameState"), "{name} mentions GameState");
        assert!(!src.contains("drops"), "{name} reads the drop table");
    }
}

#[test]
fn us2_as5_decisions_do_not_depend_on_hidden_information() {
    for seed in 0..20u64 {
        let a = GameState::new(Config::default(), seed, [true; 4]).unwrap();
        let mut b = a.clone();
        b.rng = Rng::new(seed ^ 0xFFFF);
        b.set_drops(DropTable::defaults(0.9)).unwrap();
        for me in 0..4 {
            let ia = DefaultBot::new(9).decide(&a.observe(pid(me)));
            let ib = DefaultBot::new(9).decide(&b.observe(pid(me)));
            assert_eq!(ia, ib, "seed {seed} slot {me}");
        }
    }
}

#[test]
fn f12_the_default_profile_plays_exactly_like_the_bot_before_profiles() {
    for seed in 0..20u64 {
        let mut s = GameState::new(Config::default(), seed, [true; 4]).unwrap();
        let mut old: Vec<DefaultBot> = (0..4).map(|i| DefaultBot::new(seed * 4 + i)).collect();
        let mut new: Vec<DefaultBot> = (0..4)
            .map(|i| DefaultBot::with_profile(seed * 4 + i, Profile::default()))
            .collect();
        for _ in 0..s.config.round_limit_ticks {
            if s.phase != Phase::Playing {
                break;
            }
            let mut inputs = [PlayerInput::default(); 4];
            for i in 0..4 {
                let obs = s.observe(pid(i as u8));
                inputs[i] = old[i].decide(&obs);
                assert_eq!(
                    inputs[i],
                    new[i].decide(&obs),
                    "seed {seed} tick {}",
                    s.tick
                );
            }
            step(&mut s, &inputs);
        }
    }
}

#[test]
fn a_bot_only_acts_through_player_input() {
    let s = GameState::new(Config::default(), 1, [true; 4]).unwrap();
    let input = DefaultBot::new(1).decide(&s.observe(pid(0)));
    // the same type a human produces
    let _: eksplode_re_core::PlayerInput = input;
}
