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

/// Drives the picker through one lobby's afternoon from the writes alone:
/// three moves a few seconds apart each get their minute in order, an aim
/// jumps the queue, and the clock is the only thing a session brings.
#[test]
fn every_session_picks_the_same_match_from_the_writes_alone() {
    let start = Instant::now();
    let t0 = Utc.with_ymd_and_hms(2026, 9, 28, 14, 0, 0).unwrap();
    let at = |secs: i64| t0 + chrono::Duration::seconds(secs);
    let (chess, pool, reversi) = (
        Uuid::from_u128(10),
        Uuid::from_u128(20),
        Uuid::from_u128(30),
    );
    let candidates = |pool_aimed_at: Option<Instant>| {
        vec![
            LiveCandidate {
                id: reversi,
                updated: at(20),
                aimed_at: None,
            },
            LiveCandidate {
                id: chess,
                updated: at(0),
                aimed_at: None,
            },
            LiveCandidate {
                id: pool,
                updated: at(10),
                aimed_at: pool_aimed_at,
            },
        ]
    };
    let pick = |now_secs: i64| pick_featured(&candidates(None), at(now_secs), start);

    // The first write goes up; the two behind it wait their turn.
    assert_eq!(pick(5), Some(chess));
    assert_eq!(
        pick(30),
        Some(chess),
        "the pool move landed, chess keeps its minute"
    );
    assert_eq!(pick(70), Some(pool), "pool took over when the hold ran out");
    assert_eq!(pick(130), Some(reversi), "then reversi, a minute later");
    assert_eq!(pick(10_000), Some(reversi), "and stays, nothing newer");

    // A cue being lined up takes the strip whatever the clock says, and
    // the replay resumes when it is put down.
    let aimed = candidates(Some(start));
    assert_eq!(
        pick_featured(&aimed, at(130), start + LIVE_AIM_WINDOW / 2),
        Some(pool)
    );
    assert_eq!(
        pick_featured(&aimed, at(130), start + LIVE_AIM_WINDOW),
        Some(reversi)
    );

    assert_eq!(pick_featured(&[], at(130), start), None);
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
