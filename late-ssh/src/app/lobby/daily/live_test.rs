use std::time::Instant;

use chrono::{TimeZone, Utc};

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
    let pick = |now_secs: i64| {
        pick_featured(None, &candidates(None), at(now_secs), start).map(|featured| featured.id)
    };

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
        pick_featured(None, &aimed, at(130), start + LIVE_AIM_WINDOW / 2).map(|f| f.id),
        Some(pool)
    );
    assert_eq!(
        pick_featured(None, &aimed, at(130), start + LIVE_AIM_WINDOW).map(|f| f.id),
        Some(reversi)
    );

    assert_eq!(pick_featured(None, &[], at(130), start), None);
}

/// A match that moves again loses its old place in the replay, so the
/// replay alone would flip the board early. A session holds the match it is
/// showing for its minute, then rejoins the replay.
#[test]
fn a_match_on_the_strip_keeps_its_minute_when_another_moves_again() {
    let start = Instant::now();
    let t0 = Utc.with_ymd_and_hms(2026, 9, 28, 14, 0, 0).unwrap();
    let at = |secs: i64| t0 + chrono::Duration::seconds(secs);
    let (a, b, c) = (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
    let candidate = |id, secs| LiveCandidate {
        id,
        updated: at(secs),
        aimed_at: None,
    };

    // B moved first, then A, then C: A takes the strip when B's minute ends.
    let before = [candidate(b, 0), candidate(a, 10), candidate(c, 20)];
    let showing = pick_featured(None, &before, at(60), start);
    assert_eq!(
        showing,
        Some(Featured {
            id: a,
            since: at(60)
        })
    );

    // B moves again twenty seconds into A's minute. The replay alone now
    // reads A, C, B, and would hand the strip to C.
    let after = [candidate(b, 80), candidate(a, 10), candidate(c, 20)];
    let held = pick_featured(showing, &after, at(80), start);
    assert_eq!(held, showing, "A keeps its minute");
    assert_eq!(pick_featured(held, &after, at(119), start), showing);

    // Then the queue moves on, each for a minute from when this session
    // put it up.
    let next = pick_featured(held, &after, at(120), start);
    assert_eq!(
        next,
        Some(Featured {
            id: c,
            since: at(120)
        })
    );
    assert_eq!(pick_featured(next, &after, at(179), start), next);
    assert_eq!(
        pick_featured(next, &after, at(180), start),
        Some(Featured {
            id: b,
            since: at(180)
        })
    );

    // The hold is for a match still in the lobby: one that left it gives
    // the strip up at once.
    let gone = [candidate(b, 80), candidate(c, 20)];
    assert_eq!(
        pick_featured(showing, &gone, at(79), start).map(|f| f.id),
        Some(c)
    );
}

/// A burst queues more matches than the linger has minutes for. One whose
/// turn comes after its own linger ran out is skipped, so it does not hold
/// an empty strip in front of a move that just landed.
#[test]
fn a_queued_match_that_went_stale_waiting_does_not_take_a_turn() {
    let start = Instant::now();
    let t0 = Utc.with_ymd_and_hms(2026, 9, 28, 14, 0, 0).unwrap();
    let at = |secs: i64| t0 + chrono::Duration::seconds(secs);
    let id = |n: u128| Uuid::from_u128(n);
    // Seven moves ten seconds apart, then an eighth as the queue runs dry.
    let mut candidates: Vec<LiveCandidate> = (0..7)
        .map(|n| LiveCandidate {
            id: id(n + 1),
            updated: at(n as i64 * 10),
            aimed_at: None,
        })
        .collect();
    candidates.push(LiveCandidate {
        id: id(8),
        updated: at(350),
        aimed_at: None,
    });

    // The seventh would go up at 360, five minutes after its move at 60.
    let pick = pick_featured(None, &candidates, at(365), start);
    assert_eq!(
        pick,
        Some(Featured {
            id: id(8),
            since: at(360)
        }),
        "the fresh move follows the sixth, not the stale seventh"
    );
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
