use super::*;
use chrono::TimeZone;

#[test]
fn format_deadline_scales_units() {
    let now = Utc.with_ymd_and_hms(2026, 7, 8, 12, 0, 0).unwrap();
    assert_eq!(
        format_deadline(now + chrono::Duration::hours(50), now),
        "2d 2h"
    );
    assert_eq!(
        format_deadline(now + chrono::Duration::minutes(90), now),
        "1h 30m"
    );
    assert_eq!(
        format_deadline(now + chrono::Duration::minutes(41), now),
        "41m"
    );
    assert_eq!(format_deadline(now - chrono::Duration::hours(1), now), "0m");
}

#[test]
fn fresh_turn_edges_notifies_each_became_my_turn_edge_once() {
    let a = Uuid::from_u128(1);
    let b = Uuid::from_u128(2);
    let mut notified = HashSet::from([a]);

    // Already-notified id stays quiet; a new my-turn match is an edge.
    assert_eq!(fresh_turn_edges(&mut notified, &[a, b]), vec![b]);
    assert_eq!(fresh_turn_edges(&mut notified, &[a, b]), Vec::<Uuid>::new());

    // Turn passes to the opponent and comes back: a fresh edge.
    assert_eq!(fresh_turn_edges(&mut notified, &[b]), Vec::<Uuid>::new());
    assert_eq!(fresh_turn_edges(&mut notified, &[a, b]), vec![a]);

    // Finished matches fall out of the set.
    assert_eq!(fresh_turn_edges(&mut notified, &[]), Vec::<Uuid>::new());
    assert!(notified.is_empty());
}

#[tokio::test]
async fn only_events_this_session_can_see_cost_a_repaint() {
    use crate::app::activity::{event::ActivityEvent, publisher::ActivityPublisher};
    use crate::app::games::chips::svc::ChipService;
    use late_core::test_utils::create_test_user;

    let test_db = crate::test_helpers::new_test_db().await;
    let me = create_test_user(&test_db.db, "daily-repaint-me").await;
    let them = create_test_user(&test_db.db, "daily-repaint-them").await;
    let (activity_tx, _activity_rx) = broadcast::channel::<ActivityEvent>(8);
    let svc = DailyService::new(
        test_db.db.clone(),
        ChipService::new(test_db.db.clone()),
        ActivityPublisher::new(test_db.db.clone(), activity_tx),
    );
    let (notifier, _outbox) = crate::app::notify::channel();
    let mut state = DailyState::new(svc.clone(), me.id, notifier);
    // Settle the construction snapshot so later ticks are quiet.
    let _ = state.tick(false);

    let elsewhere = Uuid::from_u128(42);
    let aim = PoolAimShare {
        azimuth: 0.0,
        tip: [0.0, 0.0],
        pull: 0.0,
        mode: ShotMode::Idle,
        place: None,
        called_pocket: None,
    };

    // The whole point: somebody lining up a shot on a table this session is
    // not at is the commonest event on this feed and the least visible here.
    // Repainting for it rebuilds a frame on every session on the replica,
    // several times a second, for as long as anybody is aiming anywhere.
    svc.publish_aim(elsewhere, them.id, aim);
    let tick = state.tick(false);
    assert!(
        !tick.changed,
        "an aim on a table this session is not at must not repaint it"
    );
    assert!(tick.banner.is_none());

    // Nor does this session's own aim echoing back: the draft it is being
    // given right now is already on the board.
    svc.publish_aim(elsewhere, me.id, aim);
    assert!(
        !state.tick(false).changed,
        "my own aim comes back to me unread"
    );

    // A move in a match this session is not watching is the lobby snapshot's
    // news, and the snapshot raises its own flag.
    let effect = state.apply_event(DailyEvent::MovePlayed {
        match_id: elsewhere,
        by_user_id: them.id,
        label: "e4".to_string(),
    });
    assert!(!effect.changed, "no board of mine is showing that match");

    // What is addressed to this session still lands, banner and all.
    let effect = state.apply_event(DailyEvent::Error {
        user_id: me.id,
        message: "not your turn".to_string(),
    });
    assert!(
        effect.changed && effect.banner.is_some(),
        "my error is mine"
    );

    let effect = state.apply_event(DailyEvent::Error {
        user_id: them.id,
        message: "not your turn".to_string(),
    });
    assert!(
        !effect.changed && effect.banner.is_none(),
        "somebody else's error is not"
    );
}

#[tokio::test]
async fn a_finished_match_tells_the_pet_win_or_loss_and_a_draw_tells_it_nothing() {
    use crate::app::activity::{event::ActivityEvent, publisher::ActivityPublisher};
    use crate::app::games::chips::svc::ChipService;
    use late_core::test_utils::create_test_user;

    let test_db = crate::test_helpers::new_test_db().await;
    let me = create_test_user(&test_db.db, "daily-state-me").await;
    let them = create_test_user(&test_db.db, "daily-state-them").await;
    let (activity_tx, _activity_rx) = broadcast::channel::<ActivityEvent>(8);
    let svc = DailyService::new(
        test_db.db.clone(),
        ChipService::new(test_db.db.clone()),
        ActivityPublisher::new(test_db.db.clone(), activity_tx),
    );
    let (notifier, _outbox) = crate::app::notify::channel();
    let mut state = DailyState::new(svc, me.id, notifier);
    let finished = |challenger: Uuid, opponent: Uuid, outcome| DailyEvent::MatchFinished {
        match_id: Uuid::from_u128(7),
        game: DailyGame::Chess,
        challenger_id: challenger,
        opponent_id: Some(opponent),
        outcome,
        result: "checkmate".to_string(),
    };
    let won_by = |user_id| DailyFinishOutcome::Won {
        user_id,
        payout: DailyWinPayout::Paid,
    };

    // Nothing finished: nothing to tell.
    let quiet = state.tick(false);
    assert!(!quiet.own_win && !quiet.own_loss);

    // My win: pride, reported once and then taken.
    state.apply_event(finished(me.id, them.id, won_by(me.id)));
    let tick = state.tick(false);
    assert!(tick.own_win, "my win");
    assert!(!tick.own_loss);
    let again = state.tick(false);
    assert!(!again.own_win, "taken by the tick that reported it");

    // Their win over me: the sulk.
    state.apply_event(finished(them.id, me.id, won_by(them.id)));
    let tick = state.tick(false);
    assert!(tick.own_loss, "my loss");
    assert!(!tick.own_win);

    // A draw is neither a win nor a loss, for either seat.
    state.apply_event(finished(me.id, them.id, DailyFinishOutcome::Draw));
    let tick = state.tick(false);
    assert!(
        !tick.own_win && !tick.own_loss,
        "a draw tells the pet nothing"
    );

    // Somebody else's match is not my news.
    let other = Uuid::from_u128(99);
    state.apply_event(finished(them.id, other, won_by(other)));
    let tick = state.tick(false);
    assert!(!tick.own_win && !tick.own_loss);
}

/// The #lounge strip goes up when a match is claimed, waits to appear while
/// the viewer is reading, and holds the result once the match ends. The
/// viewer sits on a replica that never wrote any of it: the claim and the
/// resign land on another service over the same database, and reach this
/// one through the `daily_match_changed` notify.
#[tokio::test]
async fn the_lounge_strip_goes_up_on_a_claim_and_holds_the_result_on_every_replica() {
    use crate::app::activity::{event::ActivityEvent, publisher::ActivityPublisher};
    use crate::app::games::chips::svc::ChipService;
    use late_core::test_utils::create_test_user;
    use std::time::Duration;

    let test_db = crate::test_helpers::new_test_db().await;
    let me = create_test_user(&test_db.db, "daily-strip-me").await;
    let them = create_test_user(&test_db.db, "daily-strip-them").await;
    let (activity_tx, _activity_rx) = broadcast::channel::<ActivityEvent>(8);
    let daily_service = || {
        DailyService::new(
            test_db.db.clone(),
            ChipService::new(test_db.db.clone()),
            ActivityPublisher::new(test_db.db.clone(), activity_tx.clone()),
        )
    };
    let writer = daily_service();
    let other_replica = daily_service();
    let mut pg_listener = crate::pg_listener::PgListener::new();
    let _worker = other_replica.start_notify_worker(pg_listener.subscribe(DailyService::CHANNELS));
    let _listener = pg_listener.start(test_db.db.config().clone());
    let mut snapshot_rx = other_replica.subscribe_snapshot();
    let (notifier, _outbox) = crate::app::notify::channel();
    let mut state = DailyState::new(other_replica.clone(), me.id, notifier);
    let _ = state.tick(false);
    assert!(state.live_strip_view().is_none(), "nothing live, no strip");

    let posted = writer
        .post_challenge(them.id, DailyGame::Chess, None)
        .await
        .expect("post");
    writer
        .claim_challenge(me.id, posted.id)
        .await
        .expect("claim");

    // Whether the LISTEN is live before or after the claim, the listening
    // replica's seed read or the notify lands the match in its snapshot.
    let arrived = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let listed = snapshot_rx
                .borrow_and_update()
                .active_matches
                .iter()
                .any(|item| item.id == posted.id);
            if listed {
                return;
            }
            snapshot_rx.changed().await.expect("snapshot watch open");
        }
    })
    .await;
    arrived.expect("the listening replica learns the claim");

    // Appearing would shift the messages under a selection: it waits.
    let _ = state.tick(true);
    assert!(state.live_strip_view().is_none(), "held while reading");
    let _ = state.tick(false);
    let strip = state
        .live_strip_view()
        .expect("a fresh claim puts the match up");
    assert_eq!(strip.view.item.id, posted.id);
    assert!(strip.finish.is_none());

    // The match ends on the other replica: this one's strip keeps the final
    // board with the result, read off the finished row in its snapshot.
    writer.resign(me.id, posted.id).await.expect("resign");
    let finished = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let listed = snapshot_rx
                .borrow_and_update()
                .finished_matches
                .iter()
                .any(|item| item.id == posted.id);
            if listed {
                return;
            }
            snapshot_rx.changed().await.expect("snapshot watch open");
        }
    })
    .await;
    finished.expect("the listening replica learns the finish");
    let _ = state.tick(false);
    let strip = state.live_strip_view().expect("the result holds the strip");
    assert_eq!(strip.view.item.id, posted.id);
    assert_eq!(
        strip.finish,
        Some(format!("{} won · resignation", them.username).as_str()),
        "a resignation before five moves pays nothing, so no chips are named"
    );
}

#[test]
fn draft_picker_wraps_at_both_ends() {
    let mut draft = ChallengeDraft {
        selected: 0,
        directed: false,
        username: None,
    };
    let last = DailyGame::ALL.len() - 1;

    // Up from the first game lands on the last, and down from there comes back.
    draft.move_selection(-1);
    assert_eq!(draft.selected, last);
    draft.move_selection(1);
    assert_eq!(draft.selected, 0);

    // The username prompt owns the keys: j/k type, they don't move the cursor.
    draft.username = Some(String::new());
    draft.move_selection(1);
    assert_eq!(draft.selected, 0);
}
