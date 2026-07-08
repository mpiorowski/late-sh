use late_core::{
    models::daily_match::DailyMatch,
    test_utils::{TestDb, create_test_user},
};
use late_ssh::app::daily::svc::{DAILY_MAX_ACTIVE_ENTRIES, DailyChessState, DailyService};
use late_ssh::app::games::chips::svc::ChipService;
use uuid::Uuid;

use super::helpers::new_test_db;

async fn daily_service(test_db: &TestDb) -> DailyService {
    DailyService::new(test_db.db.clone(), ChipService::new(test_db.db.clone()))
}

fn chess_state(row: &DailyMatch) -> DailyChessState {
    serde_json::from_value(row.state.clone()).expect("parse daily chess state")
}

fn white_black(row: &DailyMatch) -> (Uuid, Uuid) {
    let state = chess_state(row);
    (state.colors.white, state.colors.black)
}

/// a1 = 0 .. h8 = 63, file + 8 * rank.
const fn sq(file: usize, rank: usize) -> usize {
    file + 8 * rank
}

#[tokio::test]
async fn claim_has_exactly_one_winner() {
    let test_db = new_test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-race-challenger").await;
    let first = create_test_user(&test_db.db, "daily-race-first").await;
    let second = create_test_user(&test_db.db, "daily-race-second").await;
    let svc = daily_service(&test_db).await;

    let challenge = svc
        .post_challenge(challenger.id, None)
        .await
        .expect("post challenge");

    // The challenger can never claim their own challenge.
    let own = svc.claim_challenge(challenger.id, challenge.id).await;
    assert!(own.is_err(), "challenger claimed own challenge");

    let (a, b) = tokio::join!(
        svc.claim_challenge(first.id, challenge.id),
        svc.claim_challenge(second.id, challenge.id),
    );
    let winners = usize::from(a.is_ok()) + usize::from(b.is_ok());
    assert_eq!(winners, 1, "exactly one simultaneous claim must win");

    let client = test_db.db.get().await.expect("db client");
    let row = DailyMatch::get(&client, challenge.id)
        .await
        .expect("load match")
        .expect("match exists");
    assert_eq!(row.status, DailyMatch::STATUS_ACTIVE);
    let opponent = row.opponent_id.expect("opponent set");
    assert!(opponent == first.id || opponent == second.id);

    // Colors were assigned and it is white's move with a live deadline.
    let (white, black) = white_black(&row);
    assert_eq!(row.turn_user_id, Some(white));
    assert!([white, black].contains(&challenger.id));
    assert!(row.turn_deadline_at.expect("deadline set") > chrono::Utc::now());

    let snapshot = svc.subscribe_snapshot().borrow().clone();
    assert_eq!(snapshot.open_challenges.len(), 0);
    assert_eq!(snapshot.active_matches.len(), 1);
    assert_eq!(snapshot.active_matches[0].id, challenge.id);
}

#[tokio::test]
async fn directed_challenge_is_claimable_only_by_target() {
    let test_db = new_test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-direct-challenger").await;
    let target = create_test_user(&test_db.db, "daily-direct-target").await;
    let bystander = create_test_user(&test_db.db, "daily-direct-bystander").await;
    let svc = daily_service(&test_db).await;

    let challenge = svc
        .post_challenge(challenger.id, Some(target.id))
        .await
        .expect("post directed challenge");

    let stolen = svc.claim_challenge(bystander.id, challenge.id).await;
    assert!(stolen.is_err(), "non-target claimed a directed challenge");

    let claimed = svc
        .claim_challenge(target.id, challenge.id)
        .await
        .expect("target claims");
    assert_eq!(claimed.opponent_id, Some(target.id));
}

#[tokio::test]
async fn moves_validate_turn_and_legality() {
    let test_db = new_test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-move-challenger").await;
    let opponent = create_test_user(&test_db.db, "daily-move-opponent").await;
    let outsider = create_test_user(&test_db.db, "daily-move-outsider").await;
    let svc = daily_service(&test_db).await;

    let challenge = svc
        .post_challenge(challenger.id, None)
        .await
        .expect("post challenge");
    let claimed = svc
        .claim_challenge(opponent.id, challenge.id)
        .await
        .expect("claim challenge");
    let (white, black) = white_black(&claimed);

    // Black may not move first, outsiders never.
    let out_of_turn = svc.play_move(black, claimed.id, sq(4, 6), sq(4, 4)).await;
    assert!(out_of_turn.is_err(), "black moved out of turn");
    let outsider_move = svc
        .play_move(outsider.id, claimed.id, sq(4, 1), sq(4, 3))
        .await;
    assert!(outsider_move.is_err(), "outsider moved");

    // White cannot play an illegal move (e2 to e5).
    let illegal = svc.play_move(white, claimed.id, sq(4, 1), sq(4, 4)).await;
    assert!(illegal.is_err(), "illegal move accepted");

    // White plays e4; the turn flips to black and the deadline resets.
    svc.play_move(white, claimed.id, sq(4, 1), sq(4, 3))
        .await
        .expect("legal white move");

    let client = test_db.db.get().await.expect("db client");
    let row = DailyMatch::get(&client, claimed.id)
        .await
        .expect("load match")
        .expect("match exists");
    assert_eq!(row.turn_user_id, Some(black));
    assert!(row.turn_deadline_at.expect("deadline") > chrono::Utc::now());
    let state = chess_state(&row);
    assert_eq!(state.revision, 1);
    assert_eq!(state.move_history.len(), 1);
    assert_eq!(state.move_history[0].label, "e4");
    assert_eq!(state.position_history.len(), 2);
    assert_ne!(state.fen, state.position_history[0]);
}

#[tokio::test]
async fn checkmate_finishes_match_and_pays_the_winner() {
    let test_db = new_test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-mate-challenger").await;
    let opponent = create_test_user(&test_db.db, "daily-mate-opponent").await;
    let svc = daily_service(&test_db).await;

    let challenge = svc
        .post_challenge(challenger.id, None)
        .await
        .expect("post challenge");
    let claimed = svc
        .claim_challenge(opponent.id, challenge.id)
        .await
        .expect("claim challenge");
    let (white, black) = white_black(&claimed);

    // Fool's mate: 1. f3 e5 2. g4 Qh4#
    svc.play_move(white, claimed.id, sq(5, 1), sq(5, 2))
        .await
        .expect("f3");
    svc.play_move(black, claimed.id, sq(4, 6), sq(4, 4))
        .await
        .expect("e5");
    svc.play_move(white, claimed.id, sq(6, 1), sq(6, 3))
        .await
        .expect("g4");
    svc.play_move(black, claimed.id, sq(3, 7), sq(7, 3))
        .await
        .expect("Qh4#");

    let client = test_db.db.get().await.expect("db client");
    let row = DailyMatch::get(&client, claimed.id)
        .await
        .expect("load match")
        .expect("match exists");
    assert_eq!(row.status, DailyMatch::STATUS_FINISHED);
    assert_eq!(row.result, DailyMatch::RESULT_CHECKMATE);
    assert_eq!(row.winner_user_id, Some(black));
    assert_eq!(row.turn_user_id, None);
    assert_eq!(row.turn_deadline_at, None);

    // No further moves once finished.
    let after = svc.play_move(white, claimed.id, sq(4, 1), sq(4, 3)).await;
    assert!(after.is_err(), "moved in a finished match");

    // The win payout lands through the seeded daily_chess_win_payout
    // template; the credit is spawned, so poll briefly.
    let mut credited = None;
    for _ in 0..100 {
        let rows = client
            .query(
                "SELECT delta FROM chip_ledger WHERE user_id = $1 AND reason = 'daily_chess_win'",
                &[&black],
            )
            .await
            .expect("ledger rows");
        if let Some(row) = rows.first() {
            credited = Some(row.get::<_, i64>("delta"));
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(credited, Some(500), "winner never received the win payout");
}

#[tokio::test]
async fn resign_finishes_match_for_the_other_player() {
    let test_db = new_test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-resign-challenger").await;
    let opponent = create_test_user(&test_db.db, "daily-resign-opponent").await;
    let svc = daily_service(&test_db).await;

    let challenge = svc
        .post_challenge(challenger.id, None)
        .await
        .expect("post challenge");
    let claimed = svc
        .claim_challenge(opponent.id, challenge.id)
        .await
        .expect("claim challenge");
    let (white, black) = white_black(&claimed);

    // Resigning is allowed even when it is not your turn.
    svc.resign(black, claimed.id).await.expect("black resigns");

    let client = test_db.db.get().await.expect("db client");
    let row = DailyMatch::get(&client, claimed.id)
        .await
        .expect("load match")
        .expect("match exists");
    assert_eq!(row.status, DailyMatch::STATUS_FINISHED);
    assert_eq!(row.result, DailyMatch::RESULT_RESIGN);
    assert_eq!(row.winner_user_id, Some(white));
}

#[tokio::test]
async fn stale_revision_writes_are_rejected() {
    let test_db = new_test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-rev-challenger").await;
    let opponent = create_test_user(&test_db.db, "daily-rev-opponent").await;
    let svc = daily_service(&test_db).await;

    let challenge = svc
        .post_challenge(challenger.id, None)
        .await
        .expect("post challenge");
    let claimed = svc
        .claim_challenge(opponent.id, challenge.id)
        .await
        .expect("claim challenge");
    let (white, black) = white_black(&claimed);
    let deadline = chrono::Utc::now() + chrono::Duration::hours(24);

    let client = test_db.db.get().await.expect("db client");
    let mut state = claimed.state.clone();
    state["revision"] = serde_json::json!(5);
    let applied = DailyMatch::update_state(&client, claimed.id, &state, white, black, deadline)
        .await
        .expect("update state");
    assert_eq!(applied, 1, "revision 5 over 0 must apply");

    // A stale write (revision 4 over stored 5) is dropped.
    state["revision"] = serde_json::json!(4);
    let stale = DailyMatch::update_state(&client, claimed.id, &state, black, white, deadline)
        .await
        .expect("update state");
    assert_eq!(stale, 0, "stale revision must not apply");

    // It is no longer white's turn, so a duplicate in-flight write by white
    // is dropped even with a fresh revision.
    state["revision"] = serde_json::json!(6);
    let wrong_turn = DailyMatch::update_state(&client, claimed.id, &state, white, black, deadline)
        .await
        .expect("update state");
    assert_eq!(wrong_turn, 0, "write by the off-turn player must not apply");

    let monotonic = DailyMatch::update_state(&client, claimed.id, &state, black, white, deadline)
        .await
        .expect("update state");
    assert_eq!(
        monotonic, 1,
        "monotonic revision by the turn holder applies"
    );
}

#[tokio::test]
async fn sweeper_forfeits_matches_past_their_deadline() {
    let test_db = new_test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-sweep-challenger").await;
    let opponent = create_test_user(&test_db.db, "daily-sweep-opponent").await;
    let svc = daily_service(&test_db).await;

    let challenge = svc
        .post_challenge(challenger.id, None)
        .await
        .expect("post challenge");
    let claimed = svc
        .claim_challenge(opponent.id, challenge.id)
        .await
        .expect("claim challenge");
    let (white, black) = white_black(&claimed);

    // Not yet expired: nothing to forfeit.
    let untouched = svc.sweep_expired().await.expect("sweep");
    assert!(untouched.is_empty());

    let client = test_db.db.get().await.expect("db client");
    client
        .execute(
            "UPDATE daily_matches
             SET turn_deadline_at = current_timestamp - interval '1 minute'
             WHERE id = $1",
            &[&claimed.id],
        )
        .await
        .expect("age the deadline");

    let forfeited = svc.sweep_expired().await.expect("sweep");
    assert_eq!(forfeited.len(), 1);
    let row = &forfeited[0];
    assert_eq!(row.id, claimed.id);
    assert_eq!(row.status, DailyMatch::STATUS_FINISHED);
    assert_eq!(row.result, DailyMatch::RESULT_TIMEOUT);
    // White was on the clock, so black wins on time.
    assert_eq!(row.winner_user_id, Some(black));
    assert_ne!(row.winner_user_id, Some(white));

    let snapshot = svc.subscribe_snapshot().borrow().clone();
    assert!(snapshot.active_matches.is_empty());
}

#[tokio::test]
async fn active_entry_cap_counts_challenges_and_matches() {
    let test_db = new_test_db().await;
    let poster = create_test_user(&test_db.db, "daily-cap-poster").await;
    let claimer = create_test_user(&test_db.db, "daily-cap-claimer").await;
    let svc = daily_service(&test_db).await;

    let mut challenges = Vec::new();
    for _ in 0..DAILY_MAX_ACTIVE_ENTRIES {
        challenges.push(
            svc.post_challenge(poster.id, None)
                .await
                .expect("post challenge under the cap"),
        );
    }
    let over = svc.post_challenge(poster.id, None).await;
    assert!(over.is_err(), "posted past the cap");

    // A claim converts one open challenge into an active match: the poster's
    // entry count stays at the cap.
    svc.claim_challenge(claimer.id, challenges[0].id)
        .await
        .expect("claim");
    let still_over = svc.post_challenge(poster.id, None).await;
    assert!(
        still_over.is_err(),
        "active matches must count toward the cap"
    );

    // Cancelling an open challenge frees a slot.
    svc.cancel_challenge(poster.id, challenges[1].id)
        .await
        .expect("cancel own challenge");
    svc.post_challenge(poster.id, None)
        .await
        .expect("slot freed by cancel");

    // Cancelled challenges cannot be claimed or re-cancelled by others.
    let claim_cancelled = svc.claim_challenge(claimer.id, challenges[1].id).await;
    assert!(claim_cancelled.is_err(), "claimed a cancelled challenge");
    let foreign_cancel = svc.cancel_challenge(claimer.id, challenges[2].id).await;
    assert!(
        foreign_cancel.is_err(),
        "cancelled someone else's challenge"
    );
}

#[tokio::test]
async fn self_challenge_is_rejected() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "daily-self").await;
    let svc = daily_service(&test_db).await;

    let result = svc.post_challenge(user.id, Some(user.id)).await;
    assert!(result.is_err(), "self-challenge accepted");
}
