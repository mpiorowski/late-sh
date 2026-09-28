use std::time::{Duration, Instant};

use chrono::{TimeZone, Utc};

use super::*;
use crate::app::games::{
    chess_core::types::{ChessColor, ChessPieceKind},
    pool_core::ball::CUE,
};
use crate::app::lobby::daily::svc::DailyWinPayout;
use late_core::models::daily_match::DailyMatch;

#[test]
fn chess_summary_reads_the_opening_position() {
    let (white, black) = (Uuid::from_u128(1), Uuid::from_u128(2));
    let state =
        serde_json::to_value(DailyChessState::new(white, black, &Board::default())).unwrap();

    let summary = MatchSummary::of(DailyGame::Chess, &state).expect("a fresh chess state reads");

    assert_eq!(summary.white_id, Some(white));
    assert_eq!(summary.black_id, Some(black));
    assert_eq!(summary.move_count, 0);
    let LiveBoard::Chess { pieces, last } = summary.board else {
        panic!("a chess match paints a chess board");
    };
    assert_eq!(last, None, "nobody has moved");
    // Index is rank * 8 + file: e1 holds the white king, e8 the black one.
    assert_eq!(
        pieces[4],
        Some(ChessPiece {
            color: ChessColor::White,
            kind: ChessPieceKind::King,
        })
    );
    assert_eq!(
        pieces[60],
        Some(ChessPiece {
            color: ChessColor::Black,
            kind: ChessPieceKind::King,
        })
    );
    assert_eq!(pieces.iter().flatten().count(), 32);
}

#[test]
fn pool_summary_keeps_the_cue_apart_from_the_object_balls() {
    let state = serde_json::to_value(DailyPoolState::new(
        PoolRules::EightBall,
        Uuid::from_u128(1),
        Uuid::from_u128(2),
    ))
    .unwrap();

    let summary = MatchSummary::of(DailyGame::EightBall, &state).expect("a fresh rack reads");

    let LiveBoard::Pool {
        balls, cue, last, ..
    } = summary.board
    else {
        panic!("a pool match paints a table");
    };
    assert_eq!(balls.len(), 15, "a full rack of object balls");
    assert!(balls.iter().all(|ball| ball.id != CUE));
    assert!(
        cue.is_some(),
        "the cue ball is on the table before the break"
    );
    assert_eq!(last, None);
}

#[test]
fn a_state_that_does_not_read_has_no_summary() {
    assert!(MatchSummary::of(DailyGame::Chess, &serde_json::json!({})).is_none());
}

/// Drives the picker through one lobby's afternoon: a move, an aim elsewhere,
/// the aim going quiet, and the hold running out.
#[test]
fn activity_takes_the_strip_and_recency_takes_it_back() {
    let start = Instant::now();
    let at = |secs: u64| start + Duration::from_secs(secs);
    let (chess, pool) = (Uuid::from_u128(10), Uuid::from_u128(20));
    let written = |minute: u32| Utc.with_ymd_and_hms(2026, 9, 28, 14, minute, 0).unwrap();
    let candidates = |pool_aimed_at: Option<Instant>| {
        vec![
            LiveCandidate {
                id: chess,
                updated: written(30),
                aimed_at: None,
            },
            LiveCandidate {
                id: pool,
                updated: written(10),
                aimed_at: pool_aimed_at,
            },
        ]
    };

    // Nobody aiming: the match that moved last goes up.
    let first = pick_featured(None, &candidates(None), at(0));
    assert_eq!(
        first,
        Some(Featured {
            id: chess,
            since: at(0)
        })
    );

    // Somebody picks up a cue: they take the strip even inside the hold.
    let aiming = pick_featured(first, &candidates(Some(at(5))), at(5));
    assert_eq!(
        aiming,
        Some(Featured {
            id: pool,
            since: at(5)
        })
    );

    // Still aiming a minute later: the table stays up, its clock unchanged.
    let still = pick_featured(aiming, &candidates(Some(at(64))), at(65));
    assert_eq!(still, aiming);

    // They put the cue down: the hold is long spent, so recency wins again.
    let after = pick_featured(still, &candidates(Some(at(65))), at(65) + LIVE_AIM_WINDOW);
    assert_eq!(
        after,
        Some(Featured {
            id: chess,
            since: at(65) + LIVE_AIM_WINDOW,
        })
    );

    // The featured match leaving the lobby frees the strip for what is left.
    let alone = [candidates(None)[1]];
    assert_eq!(
        pick_featured(after, &alone, at(200)).map(|f| f.id),
        Some(pool)
    );
    assert_eq!(pick_featured(after, &[], at(200)), None);
}

/// The strip goes up on a write and comes down `LIVE_STRIP_LINGER` later,
/// unless the shooter is still lining up.
#[test]
fn the_strip_is_fresh_after_a_write_or_under_an_aim() {
    let start = Instant::now();
    let written = Utc.with_ymd_and_hms(2026, 9, 28, 14, 0, 0).unwrap();
    let just_after = written + chrono::Duration::seconds(30);
    let long_after = written + chrono::Duration::from_std(LIVE_STRIP_LINGER).unwrap();

    assert!(strip_is_fresh(written, None, just_after, start));
    assert!(
        !strip_is_fresh(written, None, long_after, start),
        "the linger ran out"
    );
    assert!(
        strip_is_fresh(
            written,
            Some(start),
            long_after,
            start + LIVE_AIM_WINDOW / 2
        ),
        "a fresh aim keeps a stale match up"
    );
    assert!(
        !strip_is_fresh(written, Some(start), long_after, start + LIVE_AIM_WINDOW),
        "an aim past its window is a player who stopped"
    );
}

#[test]
fn the_finish_headline_names_the_winner_and_only_paid_chips() {
    let (eggy, weslin) = (Uuid::from_u128(1), Uuid::from_u128(2));
    let item = DailyMatchItem {
        id: Uuid::from_u128(9),
        game: DailyGame::EightBall,
        challenger_id: eggy,
        challenger_username: Some("eggy".to_string()),
        opponent_id: weslin,
        opponent_username: Some("weslin".to_string()),
        white_id: None,
        black_id: None,
        turn_user_id: Some(eggy),
        turn_deadline_at: None,
        move_count: 12,
        updated: Utc::now(),
        board: None,
    };
    let won = |user_id, payout| DailyFinishOutcome::Won { user_id, payout };

    assert_eq!(
        finish_headline(
            &item,
            won(weslin, DailyWinPayout::Paid),
            DailyMatch::RESULT_EIGHT_POTTED
        ),
        "weslin won · eight ball · +400 chips"
    );
    assert_eq!(
        finish_headline(
            &item,
            won(eggy, DailyWinPayout::Unplayed),
            DailyMatch::RESULT_RESIGN
        ),
        "eggy won · resignation"
    );
    assert_eq!(
        finish_headline(&item, DailyFinishOutcome::Draw, DailyMatch::RESULT_DRAW),
        "a draw"
    );
    assert_eq!(
        finish_headline(&item, DailyFinishOutcome::Draw, DailyMatch::RESULT_NO_MOVES),
        "a draw · no moves left"
    );
}
