#![allow(clippy::unwrap_used)]

use eksplode_re_web_render::lobby::{
    commands, BombRole, BombState, Depth, Direction, EffectKind, LobbyTimeline, Pose, SceneFrame,
    VignetteKind, ALL_KINDS, DURATION,
};

#[test]
fn f14_us4_as1_kart_riders_are_seated_in_side_view() {
    let frame = frame_at(VignetteKind::KartRight, 120);
    assert_eq!(frame.actors.len(), 1);
    assert_eq!(frame.actors[0].pose, Pose::Ride);
    assert!(frame.actors[0].position.1 < 174.0);
    let mut background = frame.clone();
    background.actors.clear();
    assert!(
        commands(&frame, (640.0, 200.0)).len() > commands(&background, (640.0, 200.0)).len() + 30
    );
    let mut wheels = frame.clone();
    wheels.phase += 4.0;
    assert_ne!(
        commands(&frame, (640.0, 200.0)),
        commands(&wheels, (640.0, 200.0)),
        "wheels turn even with the chassis position held fixed"
    );
}

#[test]
fn f14_us4_as2_karts_cross_in_both_directions_facing_their_travel() {
    for (kind, direction) in [
        (VignetteKind::KartLeft, Direction::Left),
        (VignetteKind::KartRight, Direction::Right),
    ] {
        let early = frame_at(kind, 60);
        let later = frame_at(kind, 150);
        assert_eq!(early.actors[0].direction, direction);
        assert_eq!(early.actors[0].pose, Pose::Ride);
        assert_eq!(later.actors[0].pose, Pose::Ride);
        assert_eq!(
            later.actors[0].position.0 > early.actors[0].position.0,
            direction == Direction::Right
        );
        assert_ne!(
            commands(&early, (640.0, 200.0)),
            commands(&later, (640.0, 200.0))
        );
    }
}

#[test]
fn f14_us4_as3_kart_passes_leave_no_objects_at_rollover() {
    for kind in [VignetteKind::KartLeft, VignetteKind::KartRight] {
        assert!(!frame_at(kind, 120).actors.is_empty());
        let frame = frame_at(kind, 299);
        assert!(frame.actors.is_empty() && frame.bombs.is_empty() && frame.effects.is_empty());
    }
}

#[test]
fn f14_us1_as5_every_sliding_ninety_second_window_contains_all_visible_motifs() {
    for seed in 0..16 {
        let mut scene = LobbyTimeline::new(seed);
        let mut last_seen = [None; 12];
        let mut empty = 0;
        for tick in 0usize..15000 {
            let frame = scene.sample(0.0).unwrap();
            assert!(frame.actors.len() <= 4 && frame.bombs.len() <= 3 && frame.effects.len() <= 8);
            assert!(frame
                .effects
                .iter()
                .all(|effect| effect.size < 100.0 && (0.0..=1.0).contains(&effect.opacity)));
            for (index, last) in last_seen.iter_mut().enumerate() {
                if frame.coverage & (1 << index) != 0 {
                    *last = Some(tick);
                }
            }
            if frame.actors.is_empty() && frame.bombs.is_empty() && frame.effects.is_empty() {
                empty += 1;
            } else {
                empty = 0;
            }
            assert!(empty <= 120, "scene was empty for more than two seconds");
            if tick >= 5399 {
                for (index, last) in last_seen.iter().enumerate() {
                    assert!(
                        last.is_some_and(|last| tick - last <= 3900),
                        "seed {seed}, tick {tick}, motif {index} missing or older than 65 seconds"
                    );
                }
            }
            scene.advance(1).unwrap();
        }
    }
}

#[test]
fn f14_us3_as1_distinct_characters_run_on_the_same_level() {
    let frame = frame_at(VignetteKind::SameLevel, 90);
    assert_eq!(frame.actors.len(), 2);
    assert_ne!(frame.actors[0].look, frame.actors[1].look);
    assert!(frame
        .actors
        .iter()
        .all(|actor| actor.depth == Depth::Foreground && actor.direction == Direction::Right));
    assert!(
        frame_at(VignetteKind::SameLevel, 110).actors[1].position.0 > frame.actors[1].position.0
    );
}

#[test]
fn f14_us3_as2_other_characters_can_run_in_the_opposite_direction() {
    let frame = frame_at(VignetteKind::CounterRunBlast, 30);
    assert_eq!(frame.actors.len(), 2);
    assert_ne!(frame.actors[0].direction, frame.actors[1].direction);
    assert!(
        frame_at(VignetteKind::CounterRunBlast, 60).actors[1]
            .position
            .0
            < frame.actors[1].position.0
    );
}

#[test]
fn f14_us3_as3_a_lead_character_can_be_caught_and_the_scene_clears() {
    let frame = frame_at(VignetteKind::CounterRunBlast, 200);
    assert_eq!(frame.actors[0].pose, Pose::Squash);
    assert!(frame.actors[0].opacity < 1.0);
    assert!(frame
        .effects
        .iter()
        .any(|effect| effect.kind == EffectKind::LocalBlast));
    let end = frame_at(VignetteKind::CounterRunBlast, 299);
    assert!(end.actors.is_empty() && end.bombs.is_empty() && end.effects.is_empty());
}

#[test]
fn f14_us3_as4_safe_exits_and_eliminations_are_different_outcomes() {
    assert_ne!(
        frame_at(VignetteKind::RunDrop, 200).actors[0].pose,
        Pose::Squash
    );
    assert_eq!(
        frame_at(VignetteKind::CounterRunBlast, 200).actors[0].pose,
        Pose::Squash
    );
}

#[test]
fn f14_us3_as5_background_runners_are_smaller_higher_and_painted_first() {
    println!(
        "browser fixture deck for seed 4: {:?}",
        LobbyTimeline::new(4).deck()
    );
    let frame = frame_at(VignetteKind::BackgroundRun, 90);
    let foreground = frame
        .actors
        .iter()
        .find(|actor| actor.depth == Depth::Foreground)
        .unwrap();
    let background = frame
        .actors
        .iter()
        .find(|actor| actor.depth == Depth::Background)
        .unwrap();
    assert!(background.scale.0 < foreground.scale.0);
    assert!(background.position.1 < foreground.position.1);
    assert_eq!(frame.actors[0].depth, Depth::Background);
    let mut back_only = frame.clone();
    back_only
        .actors
        .retain(|actor| actor.depth == Depth::Background);
    back_only.bombs.clear();
    back_only.effects.clear();
    assert!(commands(&frame, (640.0, 200.0)).starts_with(&commands(&back_only, (640.0, 200.0))));
}

#[test]
fn f14_us3_as6_a_pushed_bomb_is_followed_by_an_incoming_bomb_and_flattening() {
    let push = frame_at(VignetteKind::PushSquash, 90);
    assert_eq!(push.actors[0].pose, Pose::Push);
    assert_eq!(push.bombs[0].role, BombRole::Pushed);
    let contact = frame_at(VignetteKind::PushSquash, 170);
    let incoming = contact
        .bombs
        .iter()
        .find(|bomb| bomb.role == BombRole::Incoming)
        .unwrap();
    assert!((incoming.position.0 - contact.actors[0].position.0).abs() < 5.0);
    let squash = frame_at(VignetteKind::PushSquash, 185);
    assert_eq!(squash.actors[0].pose, Pose::Squash);
    assert!(squash.actors[0].scale.1 < push.actors[0].scale.1);
    let end = frame_at(VignetteKind::PushSquash, 299);
    assert!(end.actors.is_empty() && end.bombs.is_empty() && end.effects.is_empty());
}

fn frame_at(kind: VignetteKind, tick: u32) -> SceneFrame {
    let mut scene = LobbyTimeline::new(24);
    for _ in 0..140 {
        if scene.kind() == kind {
            break;
        }
        scene.advance(15).unwrap();
    }
    assert_eq!(scene.kind(), kind);
    let mut remaining = tick;
    while remaining > 0 {
        let delta = remaining.min(15);
        scene.advance(delta).unwrap();
        remaining -= delta;
    }
    scene.sample(0.0).unwrap()
}

#[test]
fn f14_us1_as2_side_view_runner_moves_and_drops_a_bomb() {
    let early = frame_at(VignetteKind::RunDrop, 30);
    let later = frame_at(VignetteKind::RunDrop, 120);
    assert_eq!(early.actors.len(), 1);
    assert_eq!(early.actors[0].direction, Direction::Right);
    assert!(matches!(early.actors[0].pose, Pose::StepA | Pose::StepB));
    assert!(later.actors[0].position.0 > early.actors[0].position.0);
    assert_eq!(later.bombs.len(), 1);
    assert!(later.bombs[0].position.0 < later.actors[0].position.0);
    let mut background = early.clone();
    background.actors.clear();
    assert!(
        commands(&early, (640.0, 200.0)).len() > commands(&background, (640.0, 200.0)).len() + 20
    );
}

#[test]
fn f14_us1_as3_bomb_has_a_readable_fuse_then_local_blast() {
    assert_eq!(
        frame_at(VignetteKind::RunDrop, 120).bombs[0].state,
        BombState::Fuse
    );
    let blast = frame_at(VignetteKind::RunDrop, 200);
    assert_eq!(blast.bombs[0].state, BombState::Blast);
    assert!(!blast.effects.is_empty());
    assert!(blast.effects.iter().all(|effect| effect.size < 100.0));
}

#[test]
fn f14_us1_as4_rollover_clears_objects_and_starts_the_next_scene() {
    let end = frame_at(VignetteKind::RunDrop, 299);
    assert!(end.bombs.is_empty());
    assert!(end.effects.is_empty());
    let mut scene = LobbyTimeline::new(42);
    let first = scene.kind();
    for _ in 0..20 {
        scene.advance(15).unwrap();
    }
    assert_eq!(scene.local_tick(), 0);
    assert_ne!(scene.kind(), first);
}

#[test]
fn f14_us1_as5_seeded_decks_are_varied_complete_and_bounded() {
    assert_eq!(LobbyTimeline::new(99).deck(), LobbyTimeline::new(99).deck());
    assert_ne!(LobbyTimeline::new(1).deck(), LobbyTimeline::new(2).deck());
    for previous in 0..7 {
        for next in 0..7 {
            assert!((7 + next - previous) * 300 <= 3900);
        }
    }
    for seed in 0..32 {
        let mut scene = LobbyTimeline::new(seed);
        let mut previous = None;
        for _ in 0..8 {
            let deck = scene.deck();
            for kind in ALL_KINDS {
                assert_eq!(deck.iter().filter(|item| **item == kind).count(), 1);
            }
            assert_ne!(previous, Some(scene.kind()));
            for _ in 0..7 {
                previous = Some(scene.kind());
                for _ in 0..20 {
                    scene.advance(15).unwrap();
                }
            }
        }
    }
}

#[test]
fn f14_foundation_seeded_state_and_bounds() {
    let mut scene = LobbyTimeline::new(27);
    assert_eq!(scene.deck().len(), 7);
    assert_eq!(DURATION, 300);
    assert!(ALL_KINDS.contains(&scene.kind()));
    assert!(scene.cursor() < 7);
    let before = scene.sample(0.0).unwrap();
    assert!(scene.advance(16).is_err());
    assert_eq!(scene.sample(0.0).unwrap(), before);
    assert_eq!(scene.advance(0), Ok(()));
    assert_eq!(scene.sample(0.0).unwrap(), before);
    assert_eq!(scene.sample(0.5), LobbyTimeline::new(27).sample(0.5));
    for _ in 0..400 {
        scene.advance(15).unwrap();
        assert!(scene.local_tick() < 300);
        assert!(scene.cursor() < 7);
        let frame = scene.sample(0.5).unwrap();
        assert!(frame.actors.len() <= 4);
        assert!(frame.bombs.len() <= 3);
        assert!(frame.effects.len() <= 8);
    }
}

#[test]
fn f14_foundation_samples_and_commands_are_finite() {
    let scene = LobbyTimeline::new(u32::MAX);
    for alpha in [f32::NAN, f32::INFINITY, -0.01, 1.0] {
        assert!(scene.sample(alpha).is_err());
    }
    let frame = scene.sample(0.5).unwrap();
    for size in [(640.0, 200.0), (320.0, 100.0), (8192.0, 8192.0)] {
        let list = commands(&frame, size);
        assert!(!list.is_empty());
        assert!(list
            .iter()
            .flat_map(|command| command.to_array())
            .all(f32::is_finite));
    }
    for size in [(0.0, 200.0), (640.0, f32::NAN), (8193.0, 200.0)] {
        assert!(commands(&frame, size).is_empty());
    }
}
