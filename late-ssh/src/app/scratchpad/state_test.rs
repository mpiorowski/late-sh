use std::time::Instant;

use uuid::Uuid;

use super::*;
use crate::app::scratchpad::registry::{PairOutcome, PairSide, SharedScratchpadRegistry};

/// Both sides run `/pair @other`, which is the only way a pairing exists.
fn pair_up(
    registry: &SharedScratchpadRegistry,
    alice: Uuid,
    bob: Uuid,
) -> crate::app::scratchpad::registry::SharedScratchpad {
    let side = |user_id, username: &str, session_token: &str| PairSide {
        user_id,
        username: username.to_string(),
        session_token: session_token.to_string(),
    };
    let now = Instant::now();
    registry.try_pair(side(alice, "alice", "sess-a"), bob, now);
    let PairOutcome::Paired { shared, .. } =
        registry.try_pair(side(bob, "bob", "sess-b"), alice, now)
    else {
        panic!("mirroring the intent should pair");
    };
    shared
}

fn paired(alice: Uuid, bob: Uuid) -> (ScratchpadState, ScratchpadState) {
    let registry = SharedScratchpadRegistry::new();
    let shared = pair_up(&registry, alice, bob);

    let alice_state = ScratchpadState::new(
        registry.clone(),
        shared.clone(),
        alice,
        bob,
        "bob".to_string(),
    );
    let bob_state = ScratchpadState::new(registry, shared, bob, alice, "alice".to_string());
    (alice_state, bob_state)
}

#[test]
fn new_seeds_editor_from_the_shared_buffer() {
    let (alice, bob) = (Uuid::from_u128(1), Uuid::from_u128(2));
    let registry = SharedScratchpadRegistry::new();
    let shared = pair_up(&registry, alice, bob);
    shared.lock_recover().content = "fn main() {}".to_string();
    shared.lock_recover().revision = 1;

    let state = ScratchpadState::new(registry, shared, bob, alice, "alice".to_string());
    assert_eq!(state.editor.lines().join("\n"), "fn main() {}");
}

#[test]
fn publish_bumps_revision_and_writes_content_and_cursor() {
    let (mut alice, _bob) = paired(Uuid::from_u128(1), Uuid::from_u128(2));
    alice.editor.insert_str("hello");
    alice.publish();

    let buffer = alice.shared.lock_recover();
    assert_eq!(buffer.content, "hello");
    assert_eq!(buffer.revision, 1);
    assert_eq!(buffer.cursor_for(alice.own_user_id), (0, 5));
}

#[test]
fn sync_from_shared_picks_up_the_partners_publish_but_not_our_own() {
    let (mut alice, mut bob) = paired(Uuid::from_u128(1), Uuid::from_u128(2));

    alice.editor.insert_str("hi");
    alice.publish();
    // Our own publish already advanced last_seen_revision, so re-syncing
    // must be a no-op (it must not clobber the cursor we just set).
    assert!(!alice.sync_from_shared());

    assert!(bob.sync_from_shared(), "bob sees alice's publish");
    assert_eq!(bob.editor.lines().join("\n"), "hi");
}

#[test]
fn typing_after_a_remote_sync_appends_instead_of_scrambling_the_buffer() {
    // Regression test: sync_from_shared used to restore the pre-sync cursor
    // verbatim, so a side that had not typed yet (cursor still at (0,0))
    // would insert its next keystroke at the very start of the partner's
    // freshly-synced text instead of after it.
    let (mut alice, mut bob) = paired(Uuid::from_u128(1), Uuid::from_u128(2));

    alice.editor.insert_str("hello from alice");
    alice.publish();
    assert!(bob.sync_from_shared());

    bob.editor.insert_char('\n');
    bob.editor.insert_str("reply from bob");
    bob.publish();

    assert_eq!(
        bob.editor.lines().join("\n"),
        "hello from alice\nreply from bob"
    );
}

#[test]
fn a_remote_sync_leaves_the_local_yank_register_alone() {
    // Regression test: the sync replaces the buffer with select_all + cut,
    // and cut yanks. Without saving it, every keystroke from the partner
    // silently replaced whatever the user had copied with the whole buffer.
    let (mut alice, mut bob) = paired(Uuid::from_u128(1), Uuid::from_u128(2));
    bob.editor.set_yank_text("copied earlier");

    alice.editor.insert_str("alice types");
    alice.publish();
    assert!(bob.sync_from_shared());

    assert_eq!(bob.editor.yank_text(), "copied earlier");
}

#[test]
fn partner_left_reports_true_once_the_other_side_drops() {
    let (alice, bob) = paired(Uuid::from_u128(1), Uuid::from_u128(2));
    assert!(!alice.partner_left());
    drop(bob);
    assert!(alice.partner_left());
}

#[test]
fn cycle_language_is_visible_to_the_partner_without_a_separate_sync_step() {
    // language() always reads the shared buffer live (see its doc comment),
    // so unlike content there is no local copy to go stale.
    let (mut alice, bob) = paired(Uuid::from_u128(1), Uuid::from_u128(2));
    assert_eq!(
        alice.language(),
        crate::app::scratchpad::highlight::Language::Plain
    );

    alice.cycle_language();

    assert_eq!(
        alice.language(),
        crate::app::scratchpad::highlight::Language::Rust
    );
    assert_eq!(
        bob.language(),
        crate::app::scratchpad::highlight::Language::Rust,
        "both sides share one buffer, so bob sees alice's cycle immediately"
    );
}

#[test]
fn a_click_moves_the_caret_to_that_cell() {
    use ratatui::layout::Rect;
    let (mut alice, _bob) = paired(Uuid::new_v4(), Uuid::new_v4());
    alice.editor.insert_str("abc\ndefgh\nij");
    // Editor drawn at the origin with no scroll.
    alice.record_viewport(Rect::new(0, 0, 20, 10), 0, 0);
    assert!(alice.click_to_cursor(3, 1), "click inside lands");
    assert_eq!(alice.editor.cursor(), (1, 3), "row 1, col 3");
    // A click past the content area is ignored.
    assert!(!alice.click_to_cursor(50, 50));
    // With a vertical scroll, the click row is offset by it.
    alice.record_viewport(Rect::new(0, 0, 20, 10), 1, 0);
    assert!(alice.click_to_cursor(0, 0));
    assert_eq!(alice.editor.cursor().0, 1, "vscroll shifts the clicked row");
}
