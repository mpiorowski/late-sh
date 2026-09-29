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
    let _ = state.tick();

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
    let tick = state.tick();
    assert!(
        !tick.changed,
        "an aim on a table this session is not at must not repaint it"
    );
    assert!(tick.banner.is_none());

    // Nor does this session's own aim echoing back: the draft it is being
    // given right now is already on the board.
    svc.publish_aim(elsewhere, me.id, aim);
    assert!(!state.tick().changed, "my own aim comes back to me unread");

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
        result: DailyResult::Checkmate,
    };
    let won_by = |user_id| DailyFinishOutcome::Won {
        user_id,
        payout: DailyWinPayout::Paid,
    };

    // Nothing finished: nothing to tell.
    let quiet = state.tick();
    assert!(!quiet.own_win && !quiet.own_loss);

    // My win: pride, reported once and then taken.
    state.apply_event(finished(me.id, them.id, won_by(me.id)));
    let tick = state.tick();
    assert!(tick.own_win, "my win");
    assert!(!tick.own_loss);
    let again = state.tick();
    assert!(!again.own_win, "taken by the tick that reported it");

    // Their win over me: the sulk.
    state.apply_event(finished(them.id, me.id, won_by(them.id)));
    let tick = state.tick();
    assert!(tick.own_loss, "my loss");
    assert!(!tick.own_win);

    // A draw is neither a win nor a loss, for either seat.
    state.apply_event(finished(me.id, them.id, DailyFinishOutcome::Draw));
    let tick = state.tick();
    assert!(
        !tick.own_win && !tick.own_loss,
        "a draw tells the pet nothing"
    );

    // Somebody else's match is not my news.
    let other = Uuid::from_u128(99);
    state.apply_event(finished(them.id, other, won_by(other)));
    let tick = state.tick();
    assert!(!tick.own_win && !tick.own_loss);
}

/// The #lounge strip goes up when a match is claimed, waits to appear while
/// the viewer is reading, and queues the result once the match ends. The
/// viewer sits on a replica that never wrote any of it: the claim and the
/// resign land on another service over the same database, and reach this
/// one through the `daily_match_changed` notify.
#[tokio::test]
async fn a_claim_and_a_result_are_offered_to_the_strip_on_every_replica() {
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
    let _ = state.tick();
    assert!(
        state.live_candidates().is_empty(),
        "nothing live, nothing for the strip"
    );

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

    let _ = state.tick();
    let offered: Vec<LiveSource> = state
        .live_candidates()
        .iter()
        .map(|candidate| candidate.source)
        .collect();
    assert_eq!(offered, vec![LiveSource::DailyMatch(posted.id)]);
    let strip = state
        .live_match_view(posted.id)
        .expect("a fresh claim is a match the strip can paint");
    assert!(strip.finish.is_none());

    // The match ends on the other replica: this one's strip queues the
    // final board with the result, read off the finished row in its
    // snapshot and stamped with the finish.
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
    let _ = state.tick();
    let offered: Vec<LiveSource> = state
        .live_candidates()
        .iter()
        .map(|candidate| candidate.source)
        .collect();
    assert_eq!(
        offered,
        vec![LiveSource::DailyResult(posted.id)],
        "the match left the lobby and its result took its place"
    );
    let strip = state
        .live_result_view(posted.id)
        .expect("the result is kept for the strip");
    assert_eq!(strip.view.item.id, posted.id);
    assert_eq!(
        strip.finish,
        Some(format!("{} won · resignation", them.username).as_str()),
        "a resignation before five moves pays nothing, so no chips are named"
    );

    // Both players see the result, so its row leaves the finished list; the
    // strip keeps it for its linger all the same.
    writer
        .mark_result_seen(me.id, posted.id)
        .await
        .expect("i saw it");
    writer
        .mark_result_seen(them.id, posted.id)
        .await
        .expect("they saw it");
    let gone = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let listed = snapshot_rx
                .borrow_and_update()
                .finished_matches
                .iter()
                .any(|item| item.id == posted.id);
            if !listed {
                return;
            }
            snapshot_rx.changed().await.expect("snapshot watch open");
        }
    })
    .await;
    gone.expect("the listening replica drops the seen result");
    let _ = state.tick();
    assert!(
        state.live_result_view(posted.id).is_some(),
        "the result outlives its row"
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

#[tokio::test]
async fn a_board_reads_how_its_match_stands_and_refuses_an_open_challenge() {
    use crate::app::activity::{event::ActivityEvent, publisher::ActivityPublisher};
    use crate::app::games::chips::svc::ChipService;
    use late_core::test_utils::create_test_user;

    let test_db = crate::test_helpers::new_test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-standing-challenger").await;
    let claimer = create_test_user(&test_db.db, "daily-standing-claimer").await;
    let (activity_tx, _) = tokio::sync::broadcast::channel::<ActivityEvent>(16);
    let svc = DailyService::new(
        test_db.db.clone(),
        ChipService::new(test_db.db.clone()),
        ActivityPublisher::new(test_db.db.clone(), activity_tx),
    );
    let client = test_db.db.get().await.expect("db client");
    let load = |id| {
        let client = &client;
        async move {
            DailyMatch::get(client, id)
                .await
                .expect("load match")
                .expect("match exists")
        }
    };

    let challenge = svc
        .post_challenge(challenger.id, DailyGame::ConnectFour, None)
        .await
        .expect("post challenge");
    let open = DailyMatchDetail::from_row(load(challenge.id).await);
    assert_eq!(
        open.err().as_deref(),
        Some("this challenge has not been claimed")
    );

    svc.claim_challenge(claimer.id, challenge.id)
        .await
        .expect("claim challenge");
    let active = DailyMatchDetail::from_row(load(challenge.id).await).expect("active detail");
    assert_eq!(active.standing, MatchStanding::Active);

    svc.resign(claimer.id, challenge.id)
        .await
        .expect("claimer resigns");
    let finished = DailyMatchDetail::from_row(load(challenge.id).await).expect("finished detail");
    assert_eq!(
        finished.standing,
        MatchStanding::Finished(DailyResult::Resign)
    );
}

/// The held result shows the board the match ended on. The finish writes the
/// final state and the finished status in one update, so the winning move
/// never appears in an active snapshot: the board has to come off the
/// finished row.
#[tokio::test]
async fn the_held_result_shows_the_position_the_match_ended_on() {
    use crate::app::activity::{event::ActivityEvent, publisher::ActivityPublisher};
    use crate::app::games::chips::svc::ChipService;
    use crate::app::lobby::daily::{connect4, live::LiveBoard};
    use late_core::test_utils::create_test_user;

    let test_db = crate::test_helpers::new_test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-final-challenger").await;
    let claimer = create_test_user(&test_db.db, "daily-final-claimer").await;
    let (activity_tx, _activity_rx) = broadcast::channel::<ActivityEvent>(8);
    let svc = DailyService::new(
        test_db.db.clone(),
        ChipService::new(test_db.db.clone()),
        ActivityPublisher::new(test_db.db.clone(), activity_tx),
    );
    let (notifier, _outbox) = crate::app::notify::channel();
    let mut state = DailyState::new(svc.clone(), Uuid::now_v7(), notifier);

    let posted = svc
        .post_challenge(challenger.id, DailyGame::ConnectFour, None)
        .await
        .expect("post");
    let claimed = svc
        .claim_challenge(claimer.id, posted.id)
        .await
        .expect("claim");
    let red = claimed.turn_user_id.expect("red is on the clock");
    let yellow = if red == challenger.id {
        claimer.id
    } else {
        challenger.id
    };
    // Red stacks column b while yellow answers in c.
    for _ in 0..3 {
        svc.play_move(red, claimed.id, 1, 1).await.expect("red");
        svc.play_move(yellow, claimed.id, 2, 2)
            .await
            .expect("yellow");
    }
    let _ = state.tick();
    assert!(
        state.live_match_view(claimed.id).is_some(),
        "the match in play is offered to the strip"
    );

    svc.play_move(red, claimed.id, 1, 1)
        .await
        .expect("red connects four");
    let _ = state.tick();

    let strip = state
        .live_result_view(claimed.id)
        .expect("the result is kept for the strip");
    assert!(strip.finish.is_some());
    let LiveBoard::ConnectFour { grid, last } = strip.view.board else {
        panic!("a connect four match paints a connect four board");
    };
    assert_eq!(*last, Some((3, 1)), "the winning drop is the last one");
    for (row, cells) in grid.iter().enumerate().take(4) {
        assert_eq!(cells[1], Some(connect4::Disc::Red), "row {row} of b");
    }
    assert_eq!(strip.view.item.move_count, 7);
}
