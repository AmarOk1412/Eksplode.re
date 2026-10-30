#![allow(clippy::unwrap_used)]

mod common;

use common::wait_for_game as wait_for;
use common::*;
use eksplode_re_core::{Config, Dir, DropTable, Event, PowerUpKind};
use eksplode_re_net::protocol::v2::RoomRef;
use eksplode_re_net::protocol::{ClientMsg, ServerMsg};
use eksplode_re_net::{Client, Host};

fn create(host: &Host, name: &str) -> (Client, RoomRef) {
    let client = Client::connect_unjoined(&host.addr().to_string()).unwrap();
    let room = client
        .create_room(name, &eksplode_re_core::RoomSettings::default(), 4)
        .unwrap();
    (client, room)
}

/// A developer host whose bombs burn so long that nobody dies during a movement test.
fn plain_dev_host() -> Host {
    host_with(|c| {
        c.dev = true;
        c.tick = std::time::Duration::from_millis(20);
        c.game = Config {
            fuse_ticks: 3000,
            ..Config::default()
        };
    })
}

fn dev_host() -> Host {
    host_with(|c| {
        c.dev = true;
        c.tick = std::time::Duration::from_millis(20);
        c.game = Config {
            block_density: 1.0,
            ..Config::default()
        };
    })
}

fn is_error(code: &'static str) -> impl FnMut(&ServerMsg) -> bool {
    move |m| matches!(m, ServerMsg::Error { code: c, .. } if c == code)
}

fn is_dev_state(m: &ServerMsg) -> bool {
    matches!(m, ServerMsg::DevState { .. })
}

fn dev_state(c: &Client) -> (bool, [f32; 8]) {
    match wait_for(c, is_dev_state) {
        Some(ServerMsg::DevState { paused, drops, .. }) => (paused, drops),
        other => panic!("expected DevState, got {other:?}"),
    }
}

fn set_drops(c: &Client, chances: [f32; 8]) {
    c.send(ClientMsg::DevSetDrops {
        chances: chances.to_vec(),
    })
    .unwrap();
}

const DEFAULTS: fn() -> [f32; 8] = || DropTable::defaults(Config::default().powerup_chance).chances;

#[test]
fn f3_us4_as5_every_dev_message_is_refused_when_the_host_is_not_in_developer_mode() {
    let host = host();
    let (a, _room) = create(&host, "Ann");
    match wait_for(&a, |m| matches!(m, ServerMsg::Welcome { .. })) {
        Some(ServerMsg::Welcome { dev, .. }) => assert!(!dev, "Welcome.dev is false"),
        other => panic!("{other:?}"),
    }
    for msg in [
        ClientMsg::DevPauseBots { paused: true },
        ClientMsg::DevSetDrops {
            chances: vec![1.0; 8],
        },
        ClientMsg::DevResetDrops,
    ] {
        a.send(msg).unwrap();
        assert!(wait_for(&a, is_error("Unsupported")).is_some());
    }
    // nothing was applied or broadcast
    assert!(a
        .recv_timeout(std::time::Duration::from_millis(200))
        .map(ServerMsg::into_game_message)
        .map_or(true, |m| !is_dev_state(&m)));
}

#[test]
fn f3_us4_as5_only_the_owner_may_use_the_dev_commands() {
    let host = dev_host();
    let (owner, room) = create(&host, "Ann");
    assert_eq!(welcome_slot(&owner), 0);
    let guest = Client::connect_unjoined(&host.addr().to_string()).unwrap();
    guest.join_room(&room, "Bob").unwrap();
    assert_eq!(welcome_slot(&guest), 1);
    guest
        .send(ClientMsg::DevPauseBots { paused: true })
        .unwrap();
    assert!(wait_for(&guest, is_error("NotOwner")).is_some());
    set_drops(&guest, [1.0; 8]);
    assert!(wait_for(&guest, is_error("NotOwner")).is_some());
    guest.send(ClientMsg::DevResetDrops).unwrap();
    assert!(wait_for(&guest, is_error("NotOwner")).is_some());
    // the owner's view did not change
    owner
        .send(ClientMsg::DevPauseBots { paused: false })
        .unwrap();
    let (paused, drops) = dev_state(&owner);
    assert!(!paused);
    assert_eq!(drops, DEFAULTS());
}

#[test]
fn f3_us4_as1_welcome_says_dev_and_a_joiner_gets_the_dev_state() {
    let host = dev_host();
    let (c, _room) = create(&host, "Ann");
    match wait_for(&c, |m| matches!(m, ServerMsg::Welcome { .. })) {
        Some(ServerMsg::Welcome { dev, .. }) => assert!(dev),
        other => panic!("{other:?}"),
    }
    let (paused, drops) = dev_state(&c);
    assert!(!paused);
    assert_eq!(drops, DEFAULTS());
}

#[test]
fn dev_state_is_broadcast_to_everybody_when_it_changes() {
    let host = dev_host();
    let (owner, room) = create(&host, "Ann");
    welcome_slot(&owner);
    let guest = Client::connect_unjoined(&host.addr().to_string()).unwrap();
    guest.join_room(&room, "Bob").unwrap();
    welcome_slot(&guest);
    let _ = dev_state(&owner);
    let _ = dev_state(&guest);
    owner
        .send(ClientMsg::DevPauseBots { paused: true })
        .unwrap();
    assert!(dev_state(&owner).0);
    assert!(dev_state(&guest).0);
    set_drops(&owner, [0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0]);
    assert_eq!(
        dev_state(&guest).1,
        [0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0]
    );
}

/// Every block destroyed in the match, as `(destroyed blocks, spawned kinds)`, until the first
/// explosion is over.
fn bomb_and_watch(c: &Client) -> (usize, Vec<PowerUpKind>) {
    let mut sent = false;
    let (mut destroyed, mut spawned) = (0, Vec::new());
    let mut exploded = false;
    let mut after = 0;
    while let Some(m) = wait_for(c, |_| true) {
        match m {
            ServerMsg::Snapshot {
                tick: t, events, ..
            } => {
                for e in events {
                    match e {
                        Event::BombExploded { .. } => exploded = true,
                        Event::BlockDestroyed { .. } => destroyed += 1,
                        Event::PowerUpSpawned { kind, .. } => spawned.push(kind),
                        _ => {}
                    }
                }
                if exploded {
                    after += 1;
                    if after > 3 {
                        break;
                    }
                }
                let _ = c.send(ClientMsg::Input {
                    tick: t,
                    mv: None,
                    bomb: !sent,
                    punch: false,
                });
                sent = true;
            }
            ServerMsg::Ended { .. } => break,
            _ => {}
        }
    }
    (destroyed, spawned)
}

fn start_and_bomb(c: &Client, chances: [f32; 8], before_start: bool) -> (usize, Vec<PowerUpKind>) {
    welcome_slot(c);
    let _ = dev_state(c);
    c.send(ClientMsg::AddBot).unwrap();
    if before_start {
        set_drops(c, chances);
        let _ = dev_state(c);
    }
    c.send(ClientMsg::Start).unwrap();
    assert!(wait_for(c, |m| matches!(m, ServerMsg::Start { .. })).is_some());
    if !before_start {
        // the table changes while the match runs
        set_drops(c, chances);
    }
    bomb_and_watch(c)
}

#[test]
fn f3_us4_as3_a_table_with_punch_at_100_percent_drops_punch_from_every_block() {
    let host = dev_host();
    let (c, _room) = create(&host, "Ann");
    let (destroyed, spawned) = start_and_bomb(&c, [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0], true);
    assert!(destroyed >= 1, "the bomb must destroy a block");
    assert_eq!(spawned.len(), destroyed, "every block dropped something");
    assert!(
        spawned.iter().all(|k| *k == PowerUpKind::Punch),
        "{spawned:?}"
    );
}

#[test]
fn f3_us4_as3_a_table_of_zeroes_drops_nothing_and_punch_at_zero_never_drops_punch() {
    let host = dev_host();
    let (c, _room) = create(&host, "Ann");
    let (destroyed, spawned) = start_and_bomb(&c, [0.0; 8], true);
    assert!(destroyed >= 1);
    assert!(spawned.is_empty(), "{spawned:?}");

    let host = dev_host();
    let (c, _room) = create(&host, "Ann");
    // everything but punch at 100 %: sums above 1.0 are scaled and punch never shows up
    let (destroyed, spawned) = start_and_bomb(&c, [1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0], true);
    assert!(destroyed >= 1);
    assert!(spawned.len() <= destroyed, "at most one item per block");
    assert!(!spawned.is_empty());
    assert!(!spawned.contains(&PowerUpKind::Punch));
}

#[test]
fn f3_us4_as4_a_table_set_mid_match_applies_to_blocks_destroyed_afterwards() {
    let host = dev_host();
    let (c, _room) = create(&host, "Ann");
    let (destroyed, spawned) = start_and_bomb(&c, [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0], false);
    assert!(destroyed >= 1);
    assert_eq!(spawned.len(), destroyed);
    assert!(spawned.iter().all(|k| *k == PowerUpKind::Punch));
}

#[test]
fn f3_us4_as4_reset_restores_the_default_table() {
    let host = dev_host();
    let (c, _room) = create(&host, "Ann");
    welcome_slot(&c);
    let _ = dev_state(&c);
    set_drops(&c, [0.5; 8]);
    assert_eq!(dev_state(&c).1, [0.5; 8]);
    c.send(ClientMsg::DevResetDrops).unwrap();
    assert_eq!(dev_state(&c).1, DEFAULTS());
}

#[test]
fn bad_values_are_refused_and_change_nothing() {
    let host = dev_host();
    let (c, _room) = create(&host, "Ann");
    welcome_slot(&c);
    let _ = dev_state(&c);
    for chances in [
        vec![1.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -0.1],
        vec![0.1; 7],
        vec![0.1; 9],
    ] {
        c.send(ClientMsg::DevSetDrops {
            chances: chances.clone(),
        })
        .ok();
        assert!(wait_for(&c, is_error("BadRoom")).is_some(), "{chances:?}");
    }
    // the table is still the default one
    c.send(ClientMsg::DevPauseBots { paused: false }).unwrap();
    assert_eq!(dev_state(&c).1, DEFAULTS());
}

/// Positions of every bot in the snapshots until `n` snapshots passed.
fn bot_positions(c: &Client, n: usize) -> Vec<eksplode_re_core::Pos> {
    let mut out = Vec::new();
    let mut tick;
    while let Some(m) = wait_for(c, |m| matches!(m, ServerMsg::Snapshot { .. })) {
        if let ServerMsg::Snapshot { tick: t, state, .. } = m {
            tick = t;
            out.push((state.players[1].pos, state.bombs.len()));
            let _ = c.send(ClientMsg::Input {
                tick,
                mv: None,
                bomb: false,
                punch: false,
            });
            if out.len() >= n {
                break;
            }
        }
    }
    out.into_iter().map(|(p, _)| p).collect()
}

#[test]
fn f3_us4_as2_paused_bots_stand_still_and_resume_on_request() {
    let host = dev_host();
    let (c, _room) = create(&host, "Ann");
    welcome_slot(&c);
    let _ = dev_state(&c);
    c.send(ClientMsg::AddBot).unwrap();
    c.send(ClientMsg::DevPauseBots { paused: true }).unwrap();
    assert!(dev_state(&c).0);
    c.send(ClientMsg::Start).unwrap();
    assert!(wait_for(&c, |m| matches!(m, ServerMsg::Start { .. })).is_some());
    let still = bot_positions(&c, 40);
    assert!(
        still.windows(2).all(|w| w[0] == w[1]),
        "a paused bot moved: {still:?}"
    );
    c.send(ClientMsg::DevPauseBots { paused: false }).unwrap();
    let moving = bot_positions(&c, 200);
    assert!(
        moving.windows(2).any(|w| w[0] != w[1]),
        "the bot did not resume"
    );
}

#[test]
fn f3_us4_as2_pausing_mid_match_stops_the_bots_within_20_ticks_but_not_the_humans() {
    let host = plain_dev_host();
    let (c, _room) = create(&host, "Ann");
    welcome_slot(&c);
    let _ = dev_state(&c);
    c.send(ClientMsg::AddBot).unwrap();
    c.send(ClientMsg::Start).unwrap();
    assert!(wait_for(&c, |m| matches!(m, ServerMsg::Start { .. })).is_some());
    let _ = bot_positions(&c, 10);
    c.send(ClientMsg::DevPauseBots { paused: true }).unwrap();
    let after = bot_positions(&c, 60);
    let settled = &after[20..];
    assert!(
        settled.windows(2).all(|w| w[0] == w[1]),
        "the bot kept moving: {settled:?}"
    );
    // the human is unaffected: moving down changes its position
    let mut positions = Vec::new();
    let mut tick = 0;
    for _ in 0..40 {
        if let Some(ServerMsg::Snapshot { tick: t, state, .. }) =
            wait_for(&c, |m| matches!(m, ServerMsg::Snapshot { .. }))
        {
            tick = t;
            positions.push(state.players[0].pos);
        }
        let _ = c.send(ClientMsg::Input {
            tick,
            mv: Some(Dir::Down),
            bomb: false,
            punch: false,
        });
    }
    assert!(
        positions.first() != positions.last(),
        "the human could not move"
    );
}

#[test]
fn f3_us4_as2_placed_bombs_keep_burning_while_the_bots_are_paused() {
    let host = dev_host();
    let (c, _room) = create(&host, "Ann");
    welcome_slot(&c);
    let _ = dev_state(&c);
    c.send(ClientMsg::AddBot).unwrap();
    c.send(ClientMsg::DevPauseBots { paused: true }).unwrap();
    assert!(dev_state(&c).0);
    c.send(ClientMsg::Start).unwrap();
    let mut sent = false;
    let mut exploded = false;
    while let Some(m) = wait_for(&c, |m| matches!(m, ServerMsg::Snapshot { .. })) {
        if let ServerMsg::Snapshot { tick, events, .. } = m {
            exploded |= events
                .iter()
                .any(|e| matches!(e, Event::BombExploded { .. }));
            if exploded {
                break;
            }
            let _ = c.send(ClientMsg::Input {
                tick,
                mv: None,
                bomb: !sent,
                punch: false,
            });
            sent = true;
        }
    }
    assert!(exploded, "the human's bomb never exploded");
}

#[test]
fn f3_us2_as10_a_table_with_kick_at_100_percent_drops_only_kicks() {
    let host = dev_host();
    let (c, _room) = create(&host, "Ann");
    let (destroyed, spawned) = start_and_bomb(&c, [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0], true);
    assert!(destroyed >= 1);
    assert_eq!(spawned.len(), destroyed);
    assert!(
        spawned.iter().all(|k| *k == PowerUpKind::Kick),
        "{spawned:?}"
    );
}

fn behaviour_of(c: &Client) -> Option<String> {
    match wait_for(c, is_dev_state) {
        Some(ServerMsg::DevState { behaviour, .. }) => behaviour,
        other => panic!("expected DevState, got {other:?}"),
    }
}

fn force(c: &Client, behaviour: serde_json::Value) {
    c.send(ClientMsg::DevSetBotBehaviour { behaviour }).unwrap();
}

#[test]
fn f12_us1_as7_dev_behaviour_is_broadcast_in_the_lobby_and_during_a_match() {
    let host = dev_host();
    let (owner, room) = create(&host, "Ann");
    welcome_slot(&owner);
    let guest = Client::connect_unjoined(&host.addr().to_string()).unwrap();
    guest.join_room(&room, "Bob").unwrap();
    welcome_slot(&guest);
    assert_eq!(behaviour_of(&owner), None, "random by default");
    assert_eq!(behaviour_of(&guest), None);
    force(&owner, serde_json::json!("protective"));
    assert_eq!(behaviour_of(&owner).as_deref(), Some("protective"));
    assert_eq!(behaviour_of(&guest).as_deref(), Some("protective"));

    owner.send(ClientMsg::Start).unwrap();
    assert!(wait_for(&owner, |m| matches!(m, ServerMsg::Start { .. })).is_some());
    force(&owner, serde_json::json!("aggressive"));
    assert_eq!(behaviour_of(&owner).as_deref(), Some("aggressive"));
    force(&owner, serde_json::Value::Null);
    assert_eq!(behaviour_of(&owner), None);
}

#[test]
fn f12_us1_as8_dev_behaviour_is_refused_and_changes_nothing() {
    let host = host();
    let (a, _room) = create(&host, "Ann");
    welcome_slot(&a);
    force(&a, serde_json::json!("aggressive"));
    assert!(wait_for(&a, is_error("Unsupported")).is_some());

    let host = dev_host();
    let (owner, room) = create(&host, "Ann");
    welcome_slot(&owner);
    let guest = Client::connect_unjoined(&host.addr().to_string()).unwrap();
    guest.join_room(&room, "Bob").unwrap();
    welcome_slot(&guest);
    force(&guest, serde_json::json!("aggressive"));
    assert!(wait_for(&guest, is_error("NotOwner")).is_some());
    for bad in [
        serde_json::json!("random"),
        serde_json::json!("AGGRESSIVE"),
        serde_json::json!(3),
    ] {
        force(&owner, bad.clone());
        assert!(wait_for(&owner, is_error("BadRoom")).is_some(), "{bad}");
    }
    owner
        .send(ClientMsg::DevPauseBots { paused: false })
        .unwrap();
    assert_eq!(behaviour_of(&owner), None);
}
