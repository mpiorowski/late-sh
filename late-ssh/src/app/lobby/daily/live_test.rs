use chrono::Utc;

use super::*;
use crate::app::games::{
    chess_core::types::{ChessColor, ChessPieceKind},
    pool_core::ball::CUE,
};
use crate::app::lobby::daily::svc::DailyWinPayout;
use late_core::models::daily_match::DailyResult;

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
fn a_state_that_does_not_read_is_an_error_not_an_empty_summary() {
    assert!(MatchSummary::of(DailyGame::Chess, &serde_json::json!({})).is_err());
}

#[test]
fn the_finish_headline_names_the_winner_and_only_paid_chips() {
    let (eggy, weslin) = (Uuid::from_u128(1), Uuid::from_u128(2));
    let finished = |winner: Option<Uuid>, result: DailyResult, payout: Option<DailyWinPayout>| {
        DailyFinishedItem {
            id: Uuid::from_u128(9),
            game: DailyGame::EightBall,
            challenger_id: eggy,
            challenger_username: Some("eggy".to_string()),
            opponent_id: weslin,
            opponent_username: Some("weslin".to_string()),
            winner_user_id: winner,
            result,
            win_payout: payout,
            finished_at: Utc::now(),
            move_count: 0,
            board: MatchSummary::of(
                DailyGame::EightBall,
                &serde_json::to_value(DailyPoolState::new(PoolRules::EightBall, eggy, weslin))
                    .unwrap(),
            )
            .expect("a fresh rack reads")
            .board,
            challenger_seen: false,
            opponent_seen: false,
        }
    };

    assert_eq!(
        finish_headline(&finished(
            Some(weslin),
            DailyResult::EightPotted,
            Some(DailyWinPayout::Paid)
        )),
        "weslin won · eight ball · +400 chips"
    );
    assert_eq!(
        finish_headline(&finished(
            Some(eggy),
            DailyResult::Resign,
            Some(DailyWinPayout::Unplayed)
        )),
        "eggy won · resignation"
    );
    assert_eq!(
        finish_headline(&finished(Some(eggy), DailyResult::Resign, None)),
        "eggy won · resignation",
        "the payout is a second write behind the finish; until it lands no chips are named"
    );
    assert_eq!(
        finish_headline(&finished(None, DailyResult::Draw, None)),
        "a draw"
    );
    assert_eq!(
        finish_headline(&finished(None, DailyResult::NoMoves, None)),
        "a draw · no moves left"
    );
}
