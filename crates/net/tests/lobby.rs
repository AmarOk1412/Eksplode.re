mod common;

use bomberust_core::Controller;
use bomberust_net::protocol::{ClientMsg, RejectReason, ServerMsg, PROTOCOL_VERSION};
use common::*;

fn join_msg(name: &str) -> ClientMsg {
    ClientMsg::Join {
        version: PROTOCOL_VERSION,
        name: name.into(),
    }
}

#[test]
fn the_first_message_must_be_join() {
    let host = host();
    let mut raw = Raw::connect(host.addr());
    raw.send(&ClientMsg::SetReady { ready: true });
    match raw.recv() {
        Some(ServerMsg::Error { code, .. }) => assert_eq!(code, "NotJoined"),
        other => panic!("{other:?}"),
    }
    assert!(raw.recv().is_none(), "connection is closed");
}

#[test]
fn a_wrong_protocol_version_is_rejected() {
    let host = host();
    let mut raw = Raw::connect(host.addr());
    raw.send(&ClientMsg::Join {
        version: PROTOCOL_VERSION + 1,
        name: "Ann".into(),
    });
    assert_eq!(
        raw.recv(),
        Some(ServerMsg::Rejected {
            reason: RejectReason::BadVersion
        })
    );
    assert!(raw.recv().is_none());
}

#[test]
fn bad_names_are_rejected() {
    let host = host();
    for name in ["", "   ", "this name is far too long", "bad\u{7}name"] {
        let mut raw = Raw::connect(host.addr());
        raw.send(&join_msg(name));
        assert_eq!(
            raw.recv(),
            Some(ServerMsg::Rejected {
                reason: RejectReason::BadName
            }),
            "{name:?}"
        );
    }
}

#[test]
fn joining_assigns_slots_in_order_and_welcomes() {
    let host = host();
    let a = connect(host.addr(), "Ann");
    assert_eq!(welcome_slot(&a), 0);
    let b = connect(host.addr(), "Bob");
    assert_eq!(welcome_slot(&b), 1);
    // everyone sees the updated lobby
    let seen = wait_for(
        &a,
        |m| matches!(m, ServerMsg::Lobby { lobby } if lobby.slots[1].controller == Controller::Remote),
    );
    assert!(seen.is_some());
}

#[test]
fn a_second_join_from_the_same_connection_is_an_error() {
    let host = host();
    let mut raw = Raw::connect(host.addr());
    raw.send(&join_msg("Ann"));
    assert!(raw
        .recv_until(|m| matches!(m, ServerMsg::Welcome { .. }))
        .is_some());
    raw.send(&join_msg("Ann again"));
    let err = raw.recv_until(|m| matches!(m, ServerMsg::Error { .. }));
    assert!(matches!(err, Some(ServerMsg::Error { code, .. }) if code == "AlreadyJoined"));
}

#[test]
fn the_fifth_join_is_refused_as_full() {
    let host = host();
    let _clients = join_all(&host, 4);
    let mut raw = Raw::connect(host.addr());
    raw.send(&join_msg("late"));
    assert_eq!(
        raw.recv(),
        Some(ServerMsg::Rejected {
            reason: RejectReason::Full
        })
    );
}

#[test]
fn bots_take_slots_and_count_toward_the_limit() {
    let host = host();
    host.add_bot();
    host.add_bot();
    let a = connect(host.addr(), "Ann");
    assert_eq!(welcome_slot(&a), 2);
    let b = connect(host.addr(), "Bob");
    assert_eq!(welcome_slot(&b), 3);
    let mut raw = Raw::connect(host.addr());
    raw.send(&join_msg("late"));
    assert_eq!(
        raw.recv(),
        Some(ServerMsg::Rejected {
            reason: RejectReason::Full
        })
    );
}

#[test]
fn joining_during_a_match_is_refused() {
    let host = host();
    let a = connect(host.addr(), "Ann");
    welcome_slot(&a);
    host.add_bot();
    host.start_match();
    assert!(wait_for(&a, |m| matches!(m, ServerMsg::Start { .. })).is_some());
    let mut raw = Raw::connect(host.addr());
    raw.send(&join_msg("late"));
    assert_eq!(
        raw.recv(),
        Some(ServerMsg::Rejected {
            reason: RejectReason::InMatch
        })
    );
}

#[test]
fn ready_flags_are_shared_and_leaving_frees_the_slot() {
    let host = host();
    let a = connect(host.addr(), "Ann");
    welcome_slot(&a);
    let b = connect(host.addr(), "Bob");
    welcome_slot(&b);
    b.send(ClientMsg::SetReady { ready: true }).unwrap();
    let seen = wait_for(
        &a,
        |m| matches!(m, ServerMsg::Lobby { lobby } if lobby.slots[1].ready),
    );
    assert!(seen.is_some());

    b.send(ClientMsg::Leave).unwrap();
    let freed = wait_for(
        &a,
        |m| matches!(m, ServerMsg::Lobby { lobby } if lobby.slots[1].controller == Controller::Empty),
    );
    assert!(freed.is_some());
}

#[test]
fn the_host_player_takes_a_local_slot() {
    let host = host();
    let me = host.local_client("Host").unwrap();
    assert_eq!(welcome_slot(&me), 0);
    let other = connect(host.addr(), "Ann");
    assert_eq!(welcome_slot(&other), 1);
    let lobby = wait_for(
        &other,
        |m| matches!(m, ServerMsg::Lobby { lobby } if lobby.slots[0].controller == Controller::Local),
    );
    assert!(lobby.is_some());
}

#[test]
fn a_single_participant_can_start_a_match() {
    let host = host();
    let mut raw = Raw::connect(host.addr());
    raw.send(&join_msg("Ann"));
    raw.recv_until(|m| matches!(m, ServerMsg::Welcome { .. }))
        .unwrap();
    host.start_match();
    assert!(raw
        .recv_until(|m| matches!(m, ServerMsg::Start { .. }))
        .is_some());
}

#[test]
fn ping_gets_a_pong_even_before_joining() {
    let host = host();
    let mut raw = Raw::connect(host.addr());
    raw.send(&ClientMsg::Ping { nonce: 77 });
    assert_eq!(raw.recv(), Some(ServerMsg::Pong { nonce: 77 }));
}
