mod common;

use bomberust_bots::{Bot, DefaultBot};
use bomberust_core::{Config, GameState, PowerUpKind, Rng};
use common::*;

/// The bots crate must never reach for the full game state.
#[test]
fn us2_as5_bot_sources_only_know_the_observation() {
    for (name, src) in [
        ("lib.rs", include_str!("../src/lib.rs")),
        ("danger.rs", include_str!("../src/danger.rs")),
        ("pathing.rs", include_str!("../src/pathing.rs")),
        ("default_bot.rs", include_str!("../src/default_bot.rs")),
    ] {
        assert!(!src.contains("GameState"), "{name} mentions GameState");
        assert!(
            !src.contains("hidden_powerups"),
            "{name} reads hidden power-ups"
        );
    }
}

#[test]
fn us2_as5_decisions_do_not_depend_on_hidden_information() {
    for seed in 0..20u64 {
        let a = GameState::new(Config::default(), seed, [true; 4]).unwrap();
        let mut b = a.clone();
        b.rng = Rng::new(seed ^ 0xFFFF);
        for kind in b.hidden_powerups.values_mut() {
            *kind = PowerUpKind::Speed;
        }
        for me in 0..4 {
            let ia = DefaultBot::new(9).decide(&a.observe(pid(me)));
            let ib = DefaultBot::new(9).decide(&b.observe(pid(me)));
            assert_eq!(ia, ib, "seed {seed} slot {me}");
        }
    }
}

#[test]
fn a_bot_only_acts_through_player_input() {
    let s = GameState::new(Config::default(), 1, [true; 4]).unwrap();
    let input = DefaultBot::new(1).decide(&s.observe(pid(0)));
    // the same type a human produces
    let _: bomberust_core::PlayerInput = input;
}
