use uuid::Uuid;

use super::*;

fn uid(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

#[test]
fn invite_round_trips_through_take_invite_for() {
    let registry = SharedScratchpadRegistry::new();
    let (alice, bob) = (uid(1), uid(2));
    registry.invite(alice, "alice".to_string(), bob);

    let invite = registry.take_invite_for(bob).expect("invite present");
    assert_eq!(invite.from_user_id, alice);
    assert_eq!(invite.from_username, "alice");
    assert!(registry.take_invite_for(bob).is_none(), "invite is drained");
}

#[test]
fn re_inviting_overwrites_the_pending_invite() {
    let registry = SharedScratchpadRegistry::new();
    let (alice, carol, bob) = (uid(1), uid(3), uid(2));
    registry.invite(alice, "alice".to_string(), bob);
    registry.invite(carol, "carol".to_string(), bob);

    let invite = registry.take_invite_for(bob).expect("invite present");
    assert_eq!(invite.from_user_id, carol, "last invite wins");
}

#[test]
fn accept_indexes_the_pairing_under_both_user_ids() {
    let registry = SharedScratchpadRegistry::new();
    let (alice, bob) = (uid(1), uid(2));
    registry.invite(alice, "alice".to_string(), bob);
    let invite = registry.take_invite_for(bob).unwrap();
    let shared = registry.accept(bob, "bob".to_string(), invite);

    assert!(registry.is_paired(alice));
    assert!(registry.is_paired(bob));
    assert!(Arc::ptr_eq(&shared, &registry.lookup(alice).unwrap()));
    assert!(Arc::ptr_eq(&shared, &registry.lookup(bob).unwrap()));
}

#[test]
fn leave_from_one_side_keeps_the_pairing_alive_for_the_other() {
    let registry = SharedScratchpadRegistry::new();
    let (alice, bob) = (uid(1), uid(2));
    registry.invite(alice, "alice".to_string(), bob);
    let invite = registry.take_invite_for(bob).unwrap();
    registry.accept(bob, "bob".to_string(), invite);

    registry.leave(alice);
    assert!(!registry.is_paired(alice), "the leaver's own entry is gone");
    let shared = registry
        .lookup(bob)
        .expect("the other side keeps their entry");
    assert_eq!(shared.lock_recover().left, Some(alice));
}

#[test]
fn leave_bumps_revision_so_the_survivor_syncs_promptly() {
    // Regression test: `left` alone is invisible to `ScratchpadState::
    // sync_from_shared`'s revision-gated check, so the survivor's screen
    // would only notice a departed partner on their next unrelated
    // keystroke instead of on their very next tick.
    let registry = SharedScratchpadRegistry::new();
    let (alice, bob) = (uid(1), uid(2));
    registry.invite(alice, "alice".to_string(), bob);
    let invite = registry.take_invite_for(bob).unwrap();
    let shared = registry.accept(bob, "bob".to_string(), invite);
    let revision_before = shared.lock_recover().revision;

    registry.leave(alice);

    assert_eq!(shared.lock_recover().revision, revision_before + 1);
}

#[test]
fn leave_from_both_sides_tears_down_the_pairing() {
    let registry = SharedScratchpadRegistry::new();
    let (alice, bob) = (uid(1), uid(2));
    registry.invite(alice, "alice".to_string(), bob);
    let invite = registry.take_invite_for(bob).unwrap();
    registry.accept(bob, "bob".to_string(), invite);

    registry.leave(alice);
    registry.leave(bob);
    assert!(!registry.is_paired(alice));
    assert!(!registry.is_paired(bob));
}

#[test]
fn cursor_and_content_round_trip_per_side() {
    let registry = SharedScratchpadRegistry::new();
    let (alice, bob) = (uid(1), uid(2));
    registry.invite(alice, "alice".to_string(), bob);
    let invite = registry.take_invite_for(bob).unwrap();
    let shared = registry.accept(bob, "bob".to_string(), invite);

    {
        let mut buffer = shared.lock_recover();
        buffer.content = "fn main() {}".to_string();
        buffer.revision += 1;
        buffer.set_cursor_for(alice, (0, 3));
        buffer.set_cursor_for(bob, (0, 7));
    }
    let buffer = shared.lock_recover();
    assert_eq!(buffer.content, "fn main() {}");
    assert_eq!(buffer.cursor_for(alice), (0, 3));
    assert_eq!(buffer.cursor_for(bob), (0, 7));
    assert_eq!(buffer.partner_of(alice).map(|(id, _)| id), Some(bob));
}
