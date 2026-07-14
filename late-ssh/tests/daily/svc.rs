use late_core::{
    models::{
        chat_room::ChatRoom,
        chat_room_member::ChatRoomMember,
        daily_match::DailyMatch,
        voice_channel::{TARGET_CHAT_ROOM, VoiceChannel},
    },
    test_utils::{TestDb, create_test_user},
};
use late_ssh::app::activity::event::{ActivityEvent, ActivityKind};
use late_ssh::app::activity::publisher::ActivityPublisher;
use late_ssh::app::games::chips::svc::ChipService;
use late_ssh::app::lobby::daily::battleship::DailyBattleshipState;
use late_ssh::app::lobby::daily::connect4::DailyConnect4State;
use late_ssh::app::lobby::daily::games::DailyGame;
use late_ssh::app::lobby::daily::svc::{
    DAILY_MAX_ACTIVE_ENTRIES, DailyChessState, DailyOutcome, DailyService,
};
use tokio::sync::broadcast;
use uuid::Uuid;

use super::helpers::new_test_db;

async fn daily_service(test_db: &TestDb) -> DailyService {
    daily_service_with_activity(test_db).await.0
}

/// A service plus a receiver on its activity feed, for asserting the #lounge
/// result line a finished match emits.
async fn daily_service_with_activity(
    test_db: &TestDb,
) -> (DailyService, broadcast::Receiver<ActivityEvent>) {
    let (activity_tx, activity_rx) = broadcast::channel::<ActivityEvent>(64);
    let publisher = ActivityPublisher::new(test_db.db.clone(), activity_tx);
    let svc = DailyService::new(
        test_db.db.clone(),
        ChipService::new(test_db.db.clone()),
        publisher,
    );
    (svc, activity_rx)
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
        .post_challenge(challenger.id, DailyGame::Chess, None)
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
        .post_challenge(challenger.id, DailyGame::Chess, Some(target.id))
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
        .post_challenge(challenger.id, DailyGame::Chess, None)
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
        .post_challenge(challenger.id, DailyGame::Chess, None)
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
async fn finished_match_posts_a_lounge_result_line() {
    let test_db = new_test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-lounge-challenger").await;
    let opponent = create_test_user(&test_db.db, "daily-lounge-opponent").await;
    let (svc, mut activity_rx) = daily_service_with_activity(&test_db).await;

    let challenge = svc
        .post_challenge(challenger.id, DailyGame::Chess, None)
        .await
        .expect("post challenge");
    let claimed = svc
        .claim_challenge(opponent.id, challenge.id)
        .await
        .expect("claim challenge");
    let (white, black) = white_black(&claimed);

    // Fool's mate again: black delivers Qh4#.
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

    // The result line is emitted from a spawned task (username resolution is
    // async), so await it with a timeout.
    let event = tokio::time::timeout(std::time::Duration::from_secs(5), activity_rx.recv())
        .await
        .expect("a lounge result event arrives")
        .expect("activity channel open");

    assert_eq!(event.user_id, Some(black), "attributed to the winner");
    assert!(
        matches!(
            &event.kind,
            ActivityKind::DailyResult { game, match_id }
                if game == "Chess" && *match_id == claimed.id
        ),
        "expected a Chess DailyResult for this match, got {:?}",
        event.kind
    );
    assert!(
        event.action.starts_with("beat ") && event.action.ends_with("at Chess"),
        "unexpected result phrasing: {:?}",
        event.action
    );
}

#[tokio::test]
async fn resign_finishes_match_for_the_other_player() {
    let test_db = new_test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-resign-challenger").await;
    let opponent = create_test_user(&test_db.db, "daily-resign-opponent").await;
    let svc = daily_service(&test_db).await;

    let challenge = svc
        .post_challenge(challenger.id, DailyGame::Chess, None)
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
        .post_challenge(challenger.id, DailyGame::Chess, None)
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
    // Stored revision starts at 0; a write by the turn holder that expects 0
    // applies and advances the row to revision 1 (turn passes to black).
    state["revision"] = serde_json::json!(1);
    let applied = DailyMatch::update_state(&client, claimed.id, &state, white, black, deadline, 0)
        .await
        .expect("update state");
    assert_eq!(applied, 1, "matching expected revision by white applies");

    // A superseded write: a writer that loaded at revision 0 (expects 0) but
    // the row is already at 1. Dropped by the compare-and-swap even though it
    // is now black's turn.
    state["revision"] = serde_json::json!(2);
    let superseded =
        DailyMatch::update_state(&client, claimed.id, &state, black, white, deadline, 0)
            .await
            .expect("update state");
    assert_eq!(
        superseded, 0,
        "expected revision 0 over stored 1 must not apply"
    );

    // A duplicate in-flight write by the off-turn player is dropped even with
    // the matching expected revision.
    let wrong_turn =
        DailyMatch::update_state(&client, claimed.id, &state, white, black, deadline, 1)
            .await
            .expect("update state");
    assert_eq!(wrong_turn, 0, "write by the off-turn player must not apply");

    let fresh = DailyMatch::update_state(&client, claimed.id, &state, black, white, deadline, 1)
        .await
        .expect("update state");
    assert_eq!(
        fresh, 1,
        "matching expected revision by the turn holder applies"
    );
}

/// Regression: a battleship hit keeps `turn_user_id` on the shooter, so the
/// turn guard alone cannot reject a duplicate. Two shots loaded at the same
/// base revision must not both apply — the second is a superseded write, not
/// last-write-wins. (Under the old `stored <= incoming` guard the second write
/// slipped through because the turn never changed.)
#[tokio::test]
async fn same_revision_writes_that_keep_the_turn_are_serialized() {
    let test_db = new_test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-cas-challenger").await;
    let opponent = create_test_user(&test_db.db, "daily-cas-opponent").await;
    let svc = daily_service(&test_db).await;

    let challenge = svc
        .post_challenge(challenger.id, DailyGame::Battleship, None)
        .await
        .expect("post challenge");
    let claimed = svc
        .claim_challenge(opponent.id, challenge.id)
        .await
        .expect("claim challenge");
    let shooter = claimed
        .turn_user_id
        .expect("an active match has a player on the clock");
    let deadline = chrono::Utc::now() + chrono::Duration::hours(24);
    let client = test_db.db.get().await.expect("db client");

    let mut state = claimed.state.clone();
    let base = state["revision"].as_i64().unwrap_or(0);
    // Both writers loaded `base` and both keep the turn on the shooter, as a
    // hit does.
    state["revision"] = serde_json::json!(base + 1);
    let first = DailyMatch::update_state(
        &client, claimed.id, &state, shooter, shooter, deadline, base,
    )
    .await
    .expect("update state");
    assert_eq!(first, 1, "the first hit applies");
    let second = DailyMatch::update_state(
        &client, claimed.id, &state, shooter, shooter, deadline, base,
    )
    .await
    .expect("update state");
    assert_eq!(
        second, 0,
        "a second write from the same base revision must be superseded, not last-write-wins"
    );
}

#[tokio::test]
async fn sweeper_forfeits_matches_past_their_deadline() {
    let test_db = new_test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-sweep-challenger").await;
    let opponent = create_test_user(&test_db.db, "daily-sweep-opponent").await;
    let svc = daily_service(&test_db).await;

    let challenge = svc
        .post_challenge(challenger.id, DailyGame::Chess, None)
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
            svc.post_challenge(poster.id, DailyGame::Chess, None)
                .await
                .expect("post challenge under the cap"),
        );
    }
    let over = svc.post_challenge(poster.id, DailyGame::Chess, None).await;
    assert!(over.is_err(), "posted past the cap");

    // A claim converts one open challenge into an active match: the poster's
    // entry count stays at the cap.
    svc.claim_challenge(claimer.id, challenges[0].id)
        .await
        .expect("claim");
    let still_over = svc.post_challenge(poster.id, DailyGame::Chess, None).await;
    assert!(
        still_over.is_err(),
        "active matches must count toward the cap"
    );

    // Cancelling an open challenge frees a slot.
    svc.cancel_challenge(poster.id, challenges[1].id)
        .await
        .expect("cancel own challenge");
    svc.post_challenge(poster.id, DailyGame::Chess, None)
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

    let result = svc
        .post_challenge(user.id, DailyGame::Chess, Some(user.id))
        .await;
    assert!(result.is_err(), "self-challenge accepted");
}

fn battleship_state(row: &DailyMatch) -> DailyBattleshipState {
    DailyBattleshipState::parse(&row.state).expect("parse daily battleship state")
}

#[tokio::test]
async fn battleship_hits_fire_again_and_sinking_the_fleet_pays() {
    let test_db = new_test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-bs-challenger").await;
    let opponent = create_test_user(&test_db.db, "daily-bs-opponent").await;
    let svc = daily_service(&test_db).await;

    let challenge = svc
        .post_challenge(challenger.id, DailyGame::Battleship, None)
        .await
        .expect("post battleship challenge");
    assert_eq!(challenge.game_kind, DailyMatch::GAME_KIND_BATTLESHIP);
    let claimed = svc
        .claim_challenge(opponent.id, challenge.id)
        .await
        .expect("claim battleship challenge");

    // Both fleets were placed at claim time and someone is on the clock.
    let state = battleship_state(&claimed);
    assert_eq!(state.sides[0].user_id, challenger.id);
    assert_eq!(state.sides[1].user_id, opponent.id);
    let shooter = claimed.turn_user_id.expect("first shooter");
    let shooter_side = state.side_index_of(shooter).expect("shooter plays");
    let target_side = DailyBattleshipState::opponent_index(shooter_side);
    let other = state.side(target_side).user_id;

    let enemy_cells: Vec<usize> = state.sides[target_side]
        .ships
        .iter()
        .flat_map(|ship| ship.cells.iter().map(|cell| *cell as usize))
        .collect();
    assert_eq!(enemy_cells.len(), 17, "classic fleet is 17 cells");
    let water = (0..100)
        .find(|cell| !enemy_cells.contains(cell))
        .expect("some water");

    // Out of turn and off the grid are rejected.
    let out_of_turn = svc.play_move(other, claimed.id, water, water).await;
    assert!(out_of_turn.is_err(), "opponent fired out of turn");
    let off_grid = svc.play_move(shooter, claimed.id, 100, 100).await;
    assert!(off_grid.is_err(), "fired off the grid");

    // A hit keeps the turn.
    svc.play_move(shooter, claimed.id, enemy_cells[0], enemy_cells[0])
        .await
        .expect("first hit");
    let client = test_db.db.get().await.expect("db client");
    let row = DailyMatch::get(&client, claimed.id)
        .await
        .expect("load match")
        .expect("match exists");
    assert_eq!(row.turn_user_id, Some(shooter), "a hit must fire again");

    // The same cell cannot be shot twice.
    let repeat = svc
        .play_move(shooter, claimed.id, enemy_cells[0], enemy_cells[0])
        .await;
    assert!(repeat.is_err(), "fired twice at the same square");

    // A miss passes the turn; the opponent misses right back.
    svc.play_move(shooter, claimed.id, water, water)
        .await
        .expect("miss");
    let row = DailyMatch::get(&client, claimed.id)
        .await
        .expect("load match")
        .expect("match exists");
    assert_eq!(row.turn_user_id, Some(other), "a miss must pass the turn");
    let state = battleship_state(&row);
    let their_water = (0..100)
        .find(|cell| {
            !state.sides[shooter_side]
                .ships
                .iter()
                .any(|ship| ship.cells.contains(&(*cell as u8)))
        })
        .expect("some water");
    svc.play_move(other, claimed.id, their_water, their_water)
        .await
        .expect("opponent misses back");

    // Hits keep firing, so the shooter can run the whole fleet down.
    for cell in &enemy_cells[1..] {
        svc.play_move(shooter, claimed.id, *cell, *cell)
            .await
            .expect("sink the fleet");
    }

    let row = DailyMatch::get(&client, claimed.id)
        .await
        .expect("load match")
        .expect("match exists");
    assert_eq!(row.status, DailyMatch::STATUS_FINISHED);
    assert_eq!(row.result, DailyMatch::RESULT_FLEET_SUNK);
    assert_eq!(row.winner_user_id, Some(shooter));
    assert_eq!(row.turn_user_id, None);
    assert_eq!(row.turn_deadline_at, None);

    // The 300-chip battleship payout lands through its own seeded template;
    // the credit is spawned, so poll briefly.
    let mut credited = None;
    for _ in 0..100 {
        let rows = client
            .query(
                "SELECT delta FROM chip_ledger
                 WHERE user_id = $1 AND reason = 'daily_battleship_win'",
                &[&shooter],
            )
            .await
            .expect("ledger rows");
        if let Some(row) = rows.first() {
            credited = Some(row.get::<_, i64>("delta"));
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(credited, Some(300), "winner never received the win payout");
}

#[tokio::test]
async fn battleship_resign_finishes_for_the_other_player() {
    let test_db = new_test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-bs-resigner").await;
    let opponent = create_test_user(&test_db.db, "daily-bs-survivor").await;
    let svc = daily_service(&test_db).await;

    let challenge = svc
        .post_challenge(challenger.id, DailyGame::Battleship, None)
        .await
        .expect("post battleship challenge");
    let claimed = svc
        .claim_challenge(opponent.id, challenge.id)
        .await
        .expect("claim battleship challenge");

    svc.resign(challenger.id, claimed.id)
        .await
        .expect("challenger resigns");

    let client = test_db.db.get().await.expect("db client");
    let row = DailyMatch::get(&client, claimed.id)
        .await
        .expect("load match")
        .expect("match exists");
    assert_eq!(row.status, DailyMatch::STATUS_FINISHED);
    assert_eq!(row.result, DailyMatch::RESULT_RESIGN);
    assert_eq!(row.winner_user_id, Some(opponent.id));
}

fn connect4_state(row: &DailyMatch) -> DailyConnect4State {
    DailyConnect4State::parse(&row.state).expect("parse daily connect4 state")
}

#[tokio::test]
async fn connect4_turns_alternate_and_connecting_four_pays() {
    let test_db = new_test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-c4-challenger").await;
    let opponent = create_test_user(&test_db.db, "daily-c4-opponent").await;
    let svc = daily_service(&test_db).await;

    let challenge = svc
        .post_challenge(challenger.id, DailyGame::ConnectFour, None)
        .await
        .expect("post connect4 challenge");
    assert_eq!(challenge.game_kind, DailyMatch::GAME_KIND_CONNECTFOUR);
    let claimed = svc
        .claim_challenge(opponent.id, challenge.id)
        .await
        .expect("claim connect4 challenge");

    // The claim-time coin flip picked who's red, and red is on the clock.
    let state = connect4_state(&claimed);
    let (red, yellow) = (state.red, state.yellow);
    assert!([challenger.id, opponent.id].contains(&red));
    assert_ne!(red, yellow);
    assert_eq!(claimed.turn_user_id, Some(red));

    // Out of turn and off the board are rejected.
    let out_of_turn = svc.play_move(yellow, claimed.id, 0, 0).await;
    assert!(out_of_turn.is_err(), "yellow dropped out of turn");
    let off_board = svc.play_move(red, claimed.id, 7, 7).await;
    assert!(off_board.is_err(), "dropped off the board");

    // Unlike battleship, the turn always passes.
    svc.play_move(red, claimed.id, 0, 0)
        .await
        .expect("red opens");
    let client = test_db.db.get().await.expect("db client");
    let row = DailyMatch::get(&client, claimed.id)
        .await
        .expect("load match")
        .expect("match exists");
    assert_eq!(row.turn_user_id, Some(yellow), "a drop must pass the turn");

    // Fill column a (alternating discs, so no line), then one more bounces.
    for _ in 0..5 {
        let row = DailyMatch::get(&client, claimed.id)
            .await
            .expect("load match")
            .expect("match exists");
        let mover = row.turn_user_id.expect("someone on the clock");
        svc.play_move(mover, claimed.id, 0, 0)
            .await
            .expect("fill column a");
    }
    let full = svc.play_move(red, claimed.id, 0, 0).await;
    assert!(full.is_err(), "dropped into a full column");

    // Red stacks column b while yellow answers in c: a vertical four.
    for _ in 0..3 {
        svc.play_move(red, claimed.id, 1, 1)
            .await
            .expect("red stacks b");
        svc.play_move(yellow, claimed.id, 2, 2)
            .await
            .expect("yellow answers in c");
    }
    svc.play_move(red, claimed.id, 1, 1)
        .await
        .expect("red connects four");

    let row = DailyMatch::get(&client, claimed.id)
        .await
        .expect("load match")
        .expect("match exists");
    assert_eq!(row.status, DailyMatch::STATUS_FINISHED);
    assert_eq!(row.result, DailyMatch::RESULT_FOUR_IN_A_ROW);
    assert_eq!(row.winner_user_id, Some(red));
    assert_eq!(row.turn_user_id, None);
    assert_eq!(row.turn_deadline_at, None);
    let state = connect4_state(&row);
    assert_eq!(state.winning_line().expect("a line ended it").len(), 4);

    // The 400-chip connect4 payout lands through its own seeded template;
    // the credit is spawned, so poll briefly.
    let mut credited = None;
    for _ in 0..100 {
        let rows = client
            .query(
                "SELECT delta FROM chip_ledger
                 WHERE user_id = $1 AND reason = 'daily_connect4_win'",
                &[&red],
            )
            .await
            .expect("ledger rows");
        if let Some(row) = rows.first() {
            credited = Some(row.get::<_, i64>("delta"));
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(credited, Some(400), "winner never received the win payout");
}

#[tokio::test]
async fn connect4_full_board_draws_and_pays_nobody() {
    let test_db = new_test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-c4-drawer").await;
    let opponent = create_test_user(&test_db.db, "daily-c4-drawee").await;
    let svc = daily_service(&test_db).await;

    let challenge = svc
        .post_challenge(challenger.id, DailyGame::ConnectFour, None)
        .await
        .expect("post connect4 challenge");
    let claimed = svc
        .claim_challenge(opponent.id, challenge.id)
        .await
        .expect("claim connect4 challenge");
    let client = test_db.db.get().await.expect("db client");

    // A concrete drop order that fills all 42 cells without ever connecting
    // four. Column-cycling can't: with 7 columns the disc colors form a
    // checkerboard whose `\` diagonals are monochrome, so Red connects on the
    // main diagonal long before the board fills.
    let draw_order = [
        4, 5, 4, 2, 3, 1, 3, 0, 2, 3, 3, 4, 2, 2, 2, 3, 0, 3, 2, 1, 4, 5, 1, 4, 5, 6, 0, 6, 4, 5,
        5, 0, 0, 1, 0, 1, 5, 1, 6, 6, 6, 6,
    ];
    for column in draw_order {
        let row = DailyMatch::get(&client, claimed.id)
            .await
            .expect("load match")
            .expect("match exists");
        let mover = row.turn_user_id.expect("still someone's turn");
        svc.play_move(mover, claimed.id, column, column)
            .await
            .expect("drop");
    }

    let row = DailyMatch::get(&client, claimed.id)
        .await
        .expect("load match")
        .expect("match exists");
    assert_eq!(row.status, DailyMatch::STATUS_FINISHED);
    assert_eq!(row.result, DailyMatch::RESULT_DRAW);
    assert_eq!(row.winner_user_id, None);
    assert_eq!(connect4_state(&row).move_count(), 42);

    // A draw pays nobody: give the (absent) credit task a moment, then look.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let rows = client
        .query(
            "SELECT 1 FROM chip_ledger
             WHERE reason = 'daily_connect4_win' AND (user_id = $1 OR user_id = $2)",
            &[&challenger.id, &opponent.id],
        )
        .await
        .expect("ledger rows");
    assert!(rows.is_empty(), "a drawn match paid a winner");
}

#[tokio::test]
async fn finished_results_linger_until_each_player_acks() {
    let test_db = new_test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-seen-challenger").await;
    let opponent = create_test_user(&test_db.db, "daily-seen-opponent").await;
    let stranger = create_test_user(&test_db.db, "daily-seen-stranger").await;
    let svc = daily_service(&test_db).await;

    let challenge = svc
        .post_challenge(challenger.id, DailyGame::Chess, None)
        .await
        .expect("post challenge");
    let claimed = svc
        .claim_challenge(opponent.id, challenge.id)
        .await
        .expect("claim challenge");
    let (white, black) = white_black(&claimed);
    svc.resign(black, claimed.id).await.expect("black resigns");

    // The finished match enters the snapshot unseen by both players.
    let snapshot = svc.subscribe_snapshot().borrow().clone();
    assert_eq!(snapshot.active_matches.len(), 0);
    assert_eq!(snapshot.finished_matches.len(), 1);
    let item = &snapshot.finished_matches[0];
    assert_eq!(item.id, claimed.id);
    assert!(!item.challenger_seen && !item.opponent_seen);
    assert_eq!(item.outcome_for(white), DailyOutcome::Won);
    assert_eq!(item.outcome_for(black), DailyOutcome::Lost);

    // A non-player ack touches nothing.
    let client = test_db.db.get().await.expect("db client");
    let touched = DailyMatch::mark_result_seen(&client, claimed.id, stranger.id)
        .await
        .expect("stranger ack");
    assert_eq!(touched, 0, "a non-player must not ack a result");

    // One player's ack keeps the row for the other player.
    svc.mark_result_seen(black, claimed.id)
        .await
        .expect("loser acks");
    let snapshot = svc.subscribe_snapshot().borrow().clone();
    assert_eq!(snapshot.finished_matches.len(), 1);
    let item = &snapshot.finished_matches[0];
    let black_is_challenger = claimed.challenger_id == black;
    assert_eq!(item.challenger_seen, black_is_challenger);
    assert_eq!(item.opponent_seen, !black_is_challenger);

    // A repeat ack is a no-op at the row level.
    let touched = DailyMatch::mark_result_seen(&client, claimed.id, black)
        .await
        .expect("repeat ack");
    assert_eq!(touched, 0, "a repeat ack must touch 0 rows");

    // The second player's ack clears the row from the snapshot.
    svc.mark_result_seen(white, claimed.id)
        .await
        .expect("winner acks");
    let snapshot = svc.subscribe_snapshot().borrow().clone();
    assert!(snapshot.finished_matches.is_empty());
}

#[tokio::test]
async fn claim_creates_private_match_chat_with_voice() {
    let test_db = new_test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-chat-challenger").await;
    let opponent = create_test_user(&test_db.db, "daily-chat-opponent").await;
    let svc = daily_service(&test_db).await;

    let challenge = svc
        .post_challenge(challenger.id, DailyGame::Chess, None)
        .await
        .expect("post challenge");
    // An open challenge has nobody to talk to yet.
    assert!(challenge.chat_room_id.is_none());

    let claimed = svc
        .claim_challenge(opponent.id, challenge.id)
        .await
        .expect("claim challenge");
    let chat_room_id = claimed.chat_room_id.expect("claim created a chat room");

    let client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::get(&client, chat_room_id)
        .await
        .expect("load chat room")
        .expect("chat room exists");
    assert_eq!(room.kind, "game");
    assert_eq!(room.visibility, "private");
    assert!(!room.auto_join);
    assert_eq!(
        room.slug.as_deref(),
        Some(format!("daily-{}", claimed.id).as_str())
    );

    // Exactly the two players are members.
    assert!(
        ChatRoomMember::is_member(&client, chat_room_id, challenger.id)
            .await
            .expect("challenger membership")
    );
    assert!(
        ChatRoomMember::is_member(&client, chat_room_id, opponent.id)
            .await
            .expect("opponent membership")
    );

    // The claim also wired an enabled voice channel onto the chat room.
    let voice = VoiceChannel::find_for_target(&client, TARGET_CHAT_ROOM, chat_room_id)
        .await
        .expect("voice lookup")
        .expect("voice channel exists");
    assert!(voice.enabled);
    assert!(
        voice.display_name.contains("chess"),
        "voice label names the game: {}",
        voice.display_name
    );
}

#[tokio::test]
async fn stale_match_chat_rooms_are_reaped_after_30_days() {
    let test_db = new_test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-reap-challenger").await;
    let opponent = create_test_user(&test_db.db, "daily-reap-opponent").await;
    let svc = daily_service(&test_db).await;

    let mut match_ids = Vec::new();
    for _ in 0..3 {
        let challenge = svc
            .post_challenge(challenger.id, DailyGame::Chess, None)
            .await
            .expect("post challenge");
        let claimed = svc
            .claim_challenge(opponent.id, challenge.id)
            .await
            .expect("claim challenge");
        match_ids.push(claimed.id);
    }
    let (stale_finished, fresh_finished, stale_active) = (match_ids[0], match_ids[1], match_ids[2]);
    svc.resign(challenger.id, stale_finished)
        .await
        .expect("resign stale match");
    svc.resign(challenger.id, fresh_finished)
        .await
        .expect("resign fresh match");

    let client = test_db.db.get().await.expect("db client");
    // Backdate one finished match and the still-active one past the window.
    for id in [stale_finished, stale_active] {
        client
            .execute(
                "UPDATE daily_matches
                 SET updated = current_timestamp - INTERVAL '31 days'
                 WHERE id = $1",
                &[&id],
            )
            .await
            .expect("backdate match");
    }

    let deleted = DailyMatch::delete_stale_chat_rooms(&client)
        .await
        .expect("reap stale chat rooms");
    assert_eq!(deleted, 1, "only the old finished match's chat is reaped");

    // The stale finished match: chat room and voice channel gone, match row
    // kept with chat_room_id cleared by the FK.
    let row = DailyMatch::get(&client, stale_finished)
        .await
        .expect("load stale match")
        .expect("stale match row survives");
    assert!(row.chat_room_id.is_none());

    // The fresh finished match and the old-but-active match keep their chat.
    for id in [fresh_finished, stale_active] {
        let row = DailyMatch::get(&client, id)
            .await
            .expect("load match")
            .expect("match exists");
        let chat_room_id = row.chat_room_id.expect("chat room still attached");
        assert!(
            ChatRoom::get(&client, chat_room_id)
                .await
                .expect("load chat room")
                .is_some()
        );
        assert!(
            VoiceChannel::find_for_target(&client, TARGET_CHAT_ROOM, chat_room_id)
                .await
                .expect("voice lookup")
                .is_some()
        );
    }
}
