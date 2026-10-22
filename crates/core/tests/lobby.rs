#![allow(clippy::unwrap_used)]

use bomberust_core::lobby::{Controller, Lobby, LobbyError};

#[test]
fn us2_as1_a_slot_can_be_given_to_a_bot() {
    let mut l = Lobby::new();
    l.assign(0, Controller::Local, "Ann").unwrap();
    let slot = l.add_bot().unwrap();
    assert_eq!(slot, 1);
    assert_eq!(l.slots[1].controller, Controller::Bot);
    assert!(l.slots[1].ready, "bots are always ready");
    assert_eq!(l.participants(), 2);
}

#[test]
fn never_more_than_four_participants() {
    let mut l = Lobby::new();
    for _ in 0..4 {
        l.add_bot().unwrap();
    }
    assert_eq!(l.add_bot(), Err(LobbyError::Full));
    assert_eq!(l.participants(), 4);
    assert_eq!(
        l.assign(0, Controller::Remote, "x"),
        Err(LobbyError::SlotTaken(0))
    );
}

#[test]
fn removing_a_bot_frees_the_slot() {
    let mut l = Lobby::from_counts(1, 3).unwrap();
    assert_eq!(l.first_free(), None);
    l.clear(2).unwrap();
    assert_eq!(l.first_free(), Some(2));
    assert_eq!(l.add_bot(), Ok(2));
}

#[test]
fn out_of_range_slots_are_rejected() {
    let mut l = Lobby::new();
    assert_eq!(
        l.assign(4, Controller::Bot, "x"),
        Err(LobbyError::SlotOutOfRange(4))
    );
    assert_eq!(l.clear(9), Err(LobbyError::SlotOutOfRange(9)));
}

#[test]
fn from_counts_puts_humans_first() {
    let l = Lobby::from_counts(2, 1).unwrap();
    assert_eq!(l.slots[0].controller, Controller::Local);
    assert_eq!(l.slots[1].controller, Controller::Local);
    assert_eq!(l.slots[2].controller, Controller::Bot);
    assert_eq!(l.slots[3].controller, Controller::Empty);
    assert_eq!(l.active(), [true, true, true, false]);
    assert!(Lobby::from_counts(3, 2).is_err());
}

#[test]
fn an_empty_lobby_cannot_start() {
    let mut l = Lobby::new();
    assert!(!l.can_start());
    l.add_bot().unwrap();
    assert!(l.can_start());
}
