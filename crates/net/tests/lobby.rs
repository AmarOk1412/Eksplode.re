mod common;

use bomberust_core::Controller;
use bomberust_net::protocol::{ClientMsg, RejectReason, ServerMsg, PROTOCOL_VERSION};
use common::*;

fn join_msg(name: &str) -> ClientMsg {
    ClientMsg::Join {
        version: PROTOCOL_VERSION,
        name: name.into(),
        look: None,
        room: None,
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
        look: None,
        room: None,
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
        |m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.slots[1].controller == Controller::Remote),
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
        |m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.slots[1].ready),
    );
    assert!(seen.is_some());

    b.send(ClientMsg::Leave).unwrap();
    let freed = wait_for(
        &a,
        |m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.slots[1].controller == Controller::Empty),
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
        |m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.slots[0].controller == Controller::Local),
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

fn error_code(raw: &mut Raw) -> Option<String> {
    match raw.recv_until(|m| matches!(m, ServerMsg::Error { .. })) {
        Some(ServerMsg::Error { code, .. }) => Some(code),
        _ => None,
    }
}

fn joined_raw(host: &bomberust_net::Host, name: &str) -> Raw {
    let mut raw = Raw::connect(host.addr());
    raw.send(&join_msg(name));
    assert!(raw
        .recv_until(|m| matches!(m, ServerMsg::Welcome { .. }))
        .is_some());
    raw
}

#[test]
fn the_owner_can_start_and_add_or_remove_bots() {
    let host = host();
    let mut owner = joined_raw(&host, "Ann");
    owner.send(&ClientMsg::AddBot);
    let lobby = owner.recv_until(
        |m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.slots[1].controller == Controller::Bot),
    );
    assert!(lobby.is_some());
    owner.send(&ClientMsg::RemoveBot);
    let lobby = owner.recv_until(
        |m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.slots[1].controller == Controller::Empty),
    );
    assert!(lobby.is_some());
    owner.send(&ClientMsg::Start);
    assert!(owner
        .recv_until(|m| matches!(m, ServerMsg::Start { .. }))
        .is_some());
}

#[test]
fn a_non_owner_gets_notowner_and_nothing_changes() {
    let host = host();
    let mut owner = joined_raw(&host, "Ann");
    let mut other = joined_raw(&host, "Bob");
    for msg in [ClientMsg::Start, ClientMsg::AddBot, ClientMsg::RemoveBot] {
        other.send(&msg);
        assert_eq!(
            error_code(&mut other).as_deref(),
            Some("NotOwner"),
            "{msg:?}"
        );
    }
    // the owner never saw a Start
    owner.send(&ClientMsg::Ping { nonce: 1 });
    let seen = owner.recv_until(|m| matches!(m, ServerMsg::Start { .. } | ServerMsg::Pong { .. }));
    assert!(matches!(seen, Some(ServerMsg::Pong { .. })));
}

#[test]
fn start_during_a_match_is_cannotstart_and_bot_commands_are_wrongphase() {
    let host = host();
    let mut owner = joined_raw(&host, "Ann");
    owner.send(&ClientMsg::Start);
    assert!(owner
        .recv_until(|m| matches!(m, ServerMsg::Start { .. }))
        .is_some());
    owner.send(&ClientMsg::Start);
    assert_eq!(error_code(&mut owner).as_deref(), Some("CannotStart"));
    owner.send(&ClientMsg::AddBot);
    assert_eq!(error_code(&mut owner).as_deref(), Some("WrongPhase"));
    owner.send(&ClientMsg::RemoveBot);
    assert_eq!(error_code(&mut owner).as_deref(), Some("WrongPhase"));
}

#[test]
fn adding_a_bot_to_a_full_lobby_is_noslot() {
    let host = host();
    let mut owner = joined_raw(&host, "Ann");
    for _ in 0..3 {
        owner.send(&ClientMsg::AddBot);
    }
    owner.send(&ClientMsg::AddBot);
    assert_eq!(error_code(&mut owner).as_deref(), Some("NoSlot"));
}

#[test]
fn ownership_moves_when_the_owner_leaves() {
    let host = host();
    let mut first = joined_raw(&host, "Ann");
    let mut second = joined_raw(&host, "Bob");
    second.send(&ClientMsg::Start);
    assert_eq!(error_code(&mut second).as_deref(), Some("NotOwner"));
    first.send(&ClientMsg::Leave);
    // wait until the lobby shows slot 0 as empty
    assert!(second
        .recv_until(|m| matches!(m, ServerMsg::Lobby { lobby, .. } if lobby.slots[0].controller == Controller::Empty))
        .is_some());
    second.send(&ClientMsg::Start);
    assert!(second
        .recv_until(|m| matches!(m, ServerMsg::Start { .. }))
        .is_some());
}

#[test]
fn an_in_process_connection_has_not_joined_until_it_says_so() {
    let host = host();
    let c = host.connect(false).unwrap();
    c.send(ClientMsg::Start).unwrap();
    let got = wait_for(&c, |m| matches!(m, ServerMsg::Error { .. }));
    assert!(matches!(got, Some(ServerMsg::Error { code, .. }) if code == "NotJoined"));

    let c = host.connect(false).unwrap();
    c.send(join_msg("Ann")).unwrap();
    assert_eq!(welcome_slot(&c), 0);
}
