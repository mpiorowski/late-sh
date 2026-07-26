use uuid::Uuid;

use super::*;

fn uid(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

fn side(user_id: Uuid, username: &str, session_token: &str) -> PairSide {
    PairSide {
        user_id,
        username: username.to_string(),
        session_token: session_token.to_string(),
    }
}

/// Both sides run `/pair @other` and land in the same buffer. `joined`
/// mirrors what `ScratchpadState::new` does when each session opens it.
fn pair_up(registry: &SharedScratchpadRegistry, now: Instant) -> SharedScratchpad {
    let outcome = registry.try_pair(side(uid(1), "alice", "sess-a"), uid(2), now);
    assert!(matches!(outcome, PairOutcome::Waiting), "{outcome:?}");
    let PairOutcome::Paired { shared, .. } =
        registry.try_pair(side(uid(2), "bob", "sess-b"), uid(1), now)
    else {
        panic!("mirroring the intent should pair");
    };
    let mut buffer = shared.lock_recover();
    buffer.mark_joined(uid(1));
    buffer.mark_joined(uid(2));
    drop(buffer);
    shared
}

#[test]
fn one_sided_pair_waits_and_does_not_create_a_pairing() {
    let registry = SharedScratchpadRegistry::new();
    let now = Instant::now();

    let outcome = registry.try_pair(side(uid(1), "alice", "sess-a"), uid(2), now);

    assert!(matches!(outcome, PairOutcome::Waiting), "{outcome:?}");
    assert!(
        registry.poll(uid(2), "sess-b").pairing.is_none(),
        "the target is not pulled into anything by a one-sided ask"
    );
    assert!(
        registry.poll(uid(1), "sess-a").pairing.is_none(),
        "nor is the asker"
    );
}

#[test]
fn the_target_gets_a_notice_once() {
    let registry = SharedScratchpadRegistry::new();
    let now = Instant::now();
    registry.try_pair(side(uid(1), "alice", "sess-a"), uid(2), now);

    assert_eq!(
        registry.poll(uid(2), "sess-b").notice.as_deref(),
        Some("alice")
    );
    assert_eq!(
        registry.poll(uid(2), "sess-b").notice,
        None,
        "the notice is drained by the first session that reads it"
    );
}

#[test]
fn re_asking_does_not_ping_the_target_again() {
    let registry = SharedScratchpadRegistry::new();
    let now = Instant::now();
    registry.try_pair(side(uid(1), "alice", "sess-a"), uid(2), now);
    assert!(registry.poll(uid(2), "sess-b").notice.is_some());

    let outcome = registry.try_pair(side(uid(1), "alice", "sess-a"), uid(2), now);

    assert!(matches!(outcome, PairOutcome::AlreadyAsked), "{outcome:?}");
    assert!(
        registry.poll(uid(2), "sess-b").notice.is_none(),
        "a second ask must not put another banner in front of the target"
    );
}

#[test]
fn a_suppressed_ask_still_records_the_intent() {
    // The cooldown rate limits the ping, never the handshake: bob mirroring
    // after alice's quiet re-ask still has to pair them.
    let registry = SharedScratchpadRegistry::new();
    let now = Instant::now();
    registry.try_pair(side(uid(1), "alice", "sess-a"), uid(2), now);
    registry.try_pair(side(uid(1), "alice", "sess-a"), uid(2), now);

    let outcome = registry.try_pair(side(uid(2), "bob", "sess-b"), uid(1), now);

    assert!(matches!(outcome, PairOutcome::Paired { .. }), "{outcome:?}");
}

#[test]
fn alternating_targets_does_not_reopen_the_ping() {
    // `intents` holds one entry per asker, so asking someone else clears
    // alice's own live intent. Without a per-pair record she could bounce
    // between two names and ping bob on every other command.
    let registry = SharedScratchpadRegistry::new();
    let now = Instant::now();
    registry.try_pair(side(uid(1), "alice", "sess-a"), uid(2), now);
    assert!(registry.poll(uid(2), "sess-b").notice.is_some());

    registry.try_pair(side(uid(1), "alice", "sess-a"), uid(3), now);
    let outcome = registry.try_pair(side(uid(1), "alice", "sess-a"), uid(2), now);

    assert!(matches!(outcome, PairOutcome::AlreadyAsked), "{outcome:?}");
    assert!(registry.poll(uid(2), "sess-b").notice.is_none());
}

#[test]
fn the_ping_comes_back_once_the_cooldown_lapses() {
    let registry = SharedScratchpadRegistry::new();
    let now = Instant::now();
    registry.try_pair(side(uid(1), "alice", "sess-a"), uid(2), now);
    registry.poll(uid(2), "sess-b");

    let later = now + PAIR_NOTICE_COOLDOWN;
    let outcome = registry.try_pair(side(uid(1), "alice", "sess-a"), uid(2), later);

    assert!(matches!(outcome, PairOutcome::Waiting), "{outcome:?}");
    assert_eq!(
        registry.poll(uid(2), "sess-b").notice.as_deref(),
        Some("alice"),
        "a genuine retry after the ask lapsed is not refused"
    );
}

#[test]
fn one_asker_cannot_mute_someone_elses_ask() {
    let registry = SharedScratchpadRegistry::new();
    let now = Instant::now();
    registry.try_pair(side(uid(1), "alice", "sess-a"), uid(2), now);
    registry.poll(uid(2), "sess-b");

    let outcome = registry.try_pair(side(uid(3), "carol", "sess-c"), uid(2), now);

    assert!(matches!(outcome, PairOutcome::Waiting), "{outcome:?}");
    assert_eq!(
        registry.poll(uid(2), "sess-b").notice.as_deref(),
        Some("carol")
    );
}

#[test]
fn pairing_clears_the_cooldown_between_those_two() {
    // They paired, so the ask was answered. Once both leave, either of them
    // must be able to ping the other again without waiting it out.
    let registry = SharedScratchpadRegistry::new();
    let now = Instant::now();
    pair_up(&registry, now);
    registry.leave(uid(1));
    registry.leave(uid(2));

    let outcome = registry.try_pair(side(uid(1), "alice", "sess-a"), uid(2), now);

    assert!(matches!(outcome, PairOutcome::Waiting), "{outcome:?}");
    assert_eq!(
        registry.poll(uid(2), "sess-b").notice.as_deref(),
        Some("alice")
    );
}

#[test]
fn mirroring_the_ask_pairs_both_sides_into_one_buffer() {
    let registry = SharedScratchpadRegistry::new();
    let shared = pair_up(&registry, Instant::now());

    let alice = registry
        .poll(uid(1), "sess-a")
        .pairing
        .expect("alice paired");
    let bob = registry.poll(uid(2), "sess-b").pairing.expect("bob paired");
    assert!(Arc::ptr_eq(&shared, &alice));
    assert!(Arc::ptr_eq(&shared, &bob));
}

#[test]
fn an_expired_intent_does_not_pair() {
    let registry = SharedScratchpadRegistry::new();
    let now = Instant::now();
    registry.try_pair(side(uid(1), "alice", "sess-a"), uid(2), now);

    let late = now + PAIR_INTENT_TTL;
    let outcome = registry.try_pair(side(uid(2), "bob", "sess-b"), uid(1), late);

    assert!(
        matches!(outcome, PairOutcome::Waiting),
        "a stale ask is not an agreement; bob's own ask starts fresh: {outcome:?}"
    );
    assert!(registry.poll(uid(2), "sess-b").pairing.is_none());
}

#[test]
fn an_intent_aimed_at_someone_else_is_left_alone() {
    let registry = SharedScratchpadRegistry::new();
    let now = Instant::now();
    // Alice wants Carol, not Bob.
    registry.try_pair(side(uid(1), "alice", "sess-a"), uid(3), now);

    let outcome = registry.try_pair(side(uid(2), "bob", "sess-b"), uid(1), now);
    assert!(matches!(outcome, PairOutcome::Waiting), "{outcome:?}");

    let outcome = registry.try_pair(side(uid(3), "carol", "sess-c"), uid(1), now);
    assert!(
        matches!(outcome, PairOutcome::Paired { .. }),
        "alice's ask for carol survived bob's unrelated ask: {outcome:?}"
    );
}

#[test]
fn a_pairing_is_bound_to_the_session_that_asked() {
    // A user's other SSH sessions must not be dragged into the editor.
    let registry = SharedScratchpadRegistry::new();
    pair_up(&registry, Instant::now());

    assert!(registry.poll(uid(1), "sess-a").pairing.is_some());
    assert!(
        registry.poll(uid(1), "another-tty").pairing.is_none(),
        "a second session of the same user stays where it is"
    );
}

#[test]
fn pairing_with_a_busy_user_is_refused_on_both_sides() {
    let registry = SharedScratchpadRegistry::new();
    let now = Instant::now();
    pair_up(&registry, now);

    let outcome = registry.try_pair(side(uid(3), "carol", "sess-c"), uid(1), now);
    assert!(matches!(outcome, PairOutcome::TargetBusy), "{outcome:?}");

    let outcome = registry.try_pair(side(uid(1), "alice", "sess-a"), uid(3), now);
    assert!(matches!(outcome, PairOutcome::AlreadyPaired), "{outcome:?}");
}

#[test]
fn leave_from_one_side_keeps_the_pairing_alive_for_the_other() {
    let registry = SharedScratchpadRegistry::new();
    let shared = pair_up(&registry, Instant::now());

    registry.leave(uid(1));

    assert!(
        registry.poll(uid(1), "sess-a").pairing.is_none(),
        "the leaver's own entry is gone"
    );
    assert!(
        registry.poll(uid(2), "sess-b").pairing.is_some(),
        "the other side keeps their entry"
    );
    assert_eq!(shared.lock_recover().left, Some(uid(1)));
}

#[test]
fn leave_bumps_revision_so_the_survivor_syncs_promptly() {
    // Regression test: `left` alone is invisible to `ScratchpadState::
    // sync_from_shared`'s revision-gated check, so the survivor's screen
    // would only notice a departed partner on their next unrelated
    // keystroke instead of on their very next tick.
    let registry = SharedScratchpadRegistry::new();
    let shared = pair_up(&registry, Instant::now());
    let revision_before = shared.lock_recover().revision;

    registry.leave(uid(1));

    assert_eq!(shared.lock_recover().revision, revision_before + 1);
}

#[test]
fn leave_from_both_sides_tears_down_the_pairing() {
    let registry = SharedScratchpadRegistry::new();
    pair_up(&registry, Instant::now());

    registry.leave(uid(1));
    registry.leave(uid(2));

    assert!(registry.poll(uid(1), "sess-a").pairing.is_none());
    assert!(registry.poll(uid(2), "sess-b").pairing.is_none());
}

#[test]
fn a_partner_who_never_opened_the_editor_is_not_left_paired_forever() {
    // Alice asks, then her session dies before bob mirrors. Bob's session
    // pairs against a token nobody will ever poll with, so when bob leaves
    // the whole pairing has to go: otherwise alice can never `/pair` again.
    let registry = SharedScratchpadRegistry::new();
    let now = Instant::now();
    registry.try_pair(side(uid(1), "alice", "dead-session"), uid(2), now);
    let PairOutcome::Paired { shared, .. } =
        registry.try_pair(side(uid(2), "bob", "sess-b"), uid(1), now)
    else {
        panic!("mirroring the intent should pair");
    };
    shared.lock_recover().mark_joined(uid(2));

    registry.leave(uid(2));

    let outcome = registry.try_pair(side(uid(1), "alice", "sess-a2"), uid(3), now);
    assert!(
        matches!(outcome, PairOutcome::Waiting),
        "alice is free to pair again: {outcome:?}"
    );
}

#[test]
fn cursor_and_content_round_trip_per_side() {
    let registry = SharedScratchpadRegistry::new();
    let shared = pair_up(&registry, Instant::now());

    {
        let mut buffer = shared.lock_recover();
        buffer.content = "fn main() {}".to_string();
        buffer.revision += 1;
        buffer.set_cursor_for(uid(1), (0, 3));
        buffer.set_cursor_for(uid(2), (0, 7));
    }
    let buffer = shared.lock_recover();
    assert_eq!(buffer.content, "fn main() {}");
    assert_eq!(buffer.cursor_for(uid(1)), (0, 3));
    assert_eq!(buffer.cursor_for(uid(2)), (0, 7));
    assert_eq!(buffer.partner_of(uid(1)).map(|(id, _)| id), Some(uid(2)));
}
