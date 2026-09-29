//! The live board: what one active match looks like from across the room.
//!
//! A compact, per-game snapshot of a match's position, built once per
//! snapshot publish from the same state JSON the service already parses for
//! move counts, and carried on every `DailyMatchItem`. The #lounge strip
//! (`live_strip.rs`) paints one active match at the top of the Home chat,
//! the viewer's own included: the one somebody is lining up a shot on, else
//! the one that moved last (`pick_featured`), but only while something just
//! happened to it (`strip_is_fresh`), and a match that just ended for a
//! minute after (`finish_headline`). It holds positions only, never a hidden
//! hand or a fleet: a live board is a spectator's view even for the players,
//! so it keeps exactly the secrets the spectate board keeps (`battleship_ui`
//! shows public shots only, `briscola_ui` shows backs).

use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use cozy_chess::Board;
use late_core::models::daily_match::DailyResult;
use serde_json::Value;
use uuid::Uuid;

use crate::app::games::{
    chess_core::{rules, types::ChessPiece},
    pool_core::{rules::PoolRules, table::TableSpec},
};

use super::{
    backgammon::{self, DailyBackgammonState},
    battleship::DailyBattleshipState,
    briscola::DailyBriscolaState,
    checkers::{self, DailyCheckersState},
    connect4::{self, DailyConnect4State},
    games::DailyGame,
    pool::{DailyPoolState, PoolAimShare},
    reversi::{self, DailyReversiState},
    svc::{DailyChessState, DailyFinishedItem, DailyMatchItem, DailyWinPayout},
};

/// An aim older than this is a player who stopped, not one lining up.
pub const LIVE_AIM_WINDOW: Duration = Duration::from_secs(8);
/// A featured match stays up at least this long once it goes up, unless
/// somebody starts aiming elsewhere: a newer move waits its turn, so a busy
/// lobby does not flip the board before anyone has looked at it.
pub const LIVE_HOLD: Duration = Duration::from_secs(60);
/// The #lounge strip stays up this long after the featured match's last
/// write: long enough for a regular glancing back to catch it, short enough
/// that it leaves and can come back. A correspondence match sits active for
/// days between moves, so "active" alone would keep the strip up for good.
pub const LIVE_STRIP_LINGER: Duration = Duration::from_secs(5 * 60);
/// A match that just ended holds the strip this long with its final board
/// and the result, the best advert the lobby has.
pub const LIVE_FINISH_LINGER: Duration = Duration::from_secs(60);

/// The featured match as the strip paints it: its board, and the
/// shooter's aim while one is fresh.
pub struct LiveView<'a> {
    pub item: &'a DailyMatchItem,
    pub board: &'a LiveBoard,
    pub aim: Option<&'a PoolAimShare>,
}

/// What the #lounge strip paints: a match in play, or the final board of one
/// that just ended with the result as the strip announces it.
pub struct LiveStripView<'a> {
    pub view: LiveView<'a>,
    /// `Some` for a finished match (`finish_headline`); the strip then opens
    /// nothing on click, since the board is gone from the lobby.
    pub finish: Option<&'a str>,
}

/// Whether the #lounge strip is up for the featured match: its row was
/// written inside `LIVE_STRIP_LINGER`, or its shooter is lining up a shot.
/// `now_utc` is the clock the row's `updated` was stamped with.
pub fn strip_is_fresh(
    updated: DateTime<Utc>,
    aimed_at: Option<Instant>,
    now_utc: DateTime<Utc>,
    now: Instant,
) -> bool {
    let aiming = aimed_at.is_some_and(|at| now.saturating_duration_since(at) < LIVE_AIM_WINDOW);
    let linger = chrono::Duration::from_std(LIVE_STRIP_LINGER).expect("linger fits chrono");
    aiming || now_utc.signed_duration_since(updated) < linger
}

/// The result line the #lounge strip shows under a finished match's last
/// board, read off the finished row as the snapshot carries it, so it is
/// the same on every replica. Chips are named only once the payout, a
/// second write behind the finish, is on the row and says `paid`.
pub fn finish_headline(item: &DailyFinishedItem) -> String {
    let phrase = super::state::result_phrase(item.result);
    match item.winner_user_id {
        Some(winner_id) => {
            let winner = if winner_id == item.challenger_id {
                &item.challenger_username
            } else {
                &item.opponent_username
            };
            let winner = winner.as_deref().unwrap_or("player");
            match item.win_payout {
                Some(DailyWinPayout::Paid) => {
                    format!(
                        "{winner} won · {phrase} · +{} chips",
                        item.game.win_payout()
                    )
                }
                Some(DailyWinPayout::Unplayed)
                | Some(DailyWinPayout::PairDayCapped)
                | Some(DailyWinPayout::Failed)
                | None => format!("{winner} won · {phrase}"),
            }
        }
        // A plain draw says so once; a draw by some other road names it.
        None => match item.result {
            DailyResult::Draw => "a draw".to_string(),
            DailyResult::Checkmate
            | DailyResult::Resign
            | DailyResult::Timeout
            | DailyResult::FleetSunk
            | DailyResult::FourInARow
            | DailyResult::MostDiscs
            | DailyResult::NoMoves
            | DailyResult::BorneOff
            | DailyResult::MostPoints
            | DailyResult::EightPotted
            | DailyResult::EarlyEight
            | DailyResult::NinePotted
            | DailyResult::FrameWon => format!("a draw · {phrase}"),
        },
    }
}

/// One match the strip could feature.
#[derive(Clone, Copy, Debug)]
pub struct LiveCandidate {
    pub id: Uuid,
    /// The row's last write: newest is the match that just moved.
    pub updated: DateTime<Utc>,
    /// When the shooter last moved their cue, if they have.
    pub aimed_at: Option<Instant>,
}

/// Pick the match the #lounge strip features, from data every session and
/// every replica shares: the rows' `updated` stamps and the wall clock.
/// Nothing per session goes in (which match a session saw first, when its
/// tick ran), so everyone in the room lands on the same board.
///
/// A fresh aim wins outright, the freshest if several. Otherwise the writes
/// are replayed in order: a match that takes the strip keeps it for
/// `LIVE_HOLD` from the moment it took it, and the next write in line takes
/// over at its own stamp or the end of that hold, whichever is later. So two
/// moves a minute apart each get their minute, in order, and a session that
/// connects mid-hold sees what everyone else sees.
pub fn pick_featured(
    candidates: &[LiveCandidate],
    now_utc: DateTime<Utc>,
    now: Instant,
) -> Option<Uuid> {
    let aiming = |candidate: &&LiveCandidate| {
        candidate
            .aimed_at
            .is_some_and(|at| now.saturating_duration_since(at) < LIVE_AIM_WINDOW)
    };
    if let Some(candidate) = candidates
        .iter()
        .filter(aiming)
        .max_by_key(|candidate| candidate.aimed_at)
    {
        return Some(candidate.id);
    }
    let hold = chrono::Duration::from_std(LIVE_HOLD).expect("hold fits chrono");
    let mut ordered: Vec<&LiveCandidate> = candidates.iter().collect();
    // The id breaks a tie between two rows stamped the same instant, so
    // the order is the same on every session.
    ordered.sort_by_key(|candidate| (candidate.updated, candidate.id));
    let mut ordered = ordered.into_iter();
    let first = ordered.next()?;
    let (mut shown, mut shown_at) = (first.id, first.updated);
    for next in ordered {
        let takeover = next.updated.max(shown_at + hold);
        if takeover > now_utc {
            break;
        }
        shown = next.id;
        shown_at = takeover;
    }
    Some(shown)
}

/// What the snapshot reads off one active match's state JSON: the summary
/// fields the lobby rows show, plus the live board.
#[derive(Clone, Debug)]
pub struct MatchSummary {
    /// Chess only; `None` for games without colours.
    pub white_id: Option<Uuid>,
    pub black_id: Option<Uuid>,
    /// Moves, shots, drops, plays: "how far along is this match".
    pub move_count: usize,
    pub board: LiveBoard,
}

/// One ball on the cloth. Positions are table metres, the same frame the
/// full renderer maps from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiveBall {
    pub id: u8,
    pub pos: [f64; 2],
}

/// The position, per game. Exhaustive over `DailyGame`: a new roster game
/// has to say what it looks like from across the room, even if the answer is
/// two lines of text.
#[derive(Clone, Debug)]
pub enum LiveBoard {
    /// Chess and chess960 share a board; index is `rank * 8 + file`, rank 0
    /// being White's first rank, as `chess_core::board_ui` reads it.
    Chess {
        pieces: Box<[Option<ChessPiece>; 64]>,
        /// Squares of the last move, `(from, to)`.
        last: Option<(usize, usize)>,
    },
    Battleship {
        /// Public shot tallies by side, side 0 being the challenger.
        sides: [ShotTally; 2],
    },
    ConnectFour {
        grid: connect4::Grid,
        last: Option<(usize, usize)>,
    },
    Reversi {
        grid: reversi::Grid,
        last: Option<(usize, usize)>,
    },
    Checkers {
        grid: checkers::Grid,
        /// Where the last move landed.
        last: Option<(usize, usize)>,
    },
    Backgammon {
        white_id: Uuid,
        /// Pips left to bear off, `[white, red]`.
        pips: [u32; 2],
    },
    Briscola {
        /// Seat 0's user id; the seats are the claim-time coin flip.
        seat0_id: Uuid,
        /// Points captured per seat.
        points: [u32; 2],
        stock_remaining: usize,
    },
    Pool {
        rules: PoolRules,
        spec: &'static TableSpec,
        /// Balls still on the table.
        balls: Vec<LiveBall>,
        /// The cue ball's spot, if it is on the table.
        cue: Option<[f64; 2]>,
        /// Seat 0's user id, so the frame score can be named.
        seat0_id: Uuid,
        /// Snooker frame score per seat; zeros in eight- and nine-ball.
        scores: [i32; 2],
        /// The last shot as the commentator called it (`3, 6 down`).
        last: Option<String>,
    },
}

/// One battleship side's public record: what it fired and what it found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShotTally {
    pub shots: usize,
    pub hits: usize,
    pub sunk: usize,
}

impl MatchSummary {
    /// Read one active row's state. An error when the JSON does not read as
    /// this game's state; the snapshot leaves the match out and says why
    /// (`svc::SnapshotRowError`).
    pub fn of(game: DailyGame, state: &Value) -> Result<Self> {
        match game {
            DailyGame::Chess | DailyGame::Chess960 => {
                let state = DailyChessState::parse(state)?;
                let board: Board = match state.fen.parse() {
                    Ok(board) => board,
                    Err(error) => bail!("parsing daily chess fen: {error:?}"),
                };
                let last = state.move_history.last().map(|m| (m.from, m.to));
                Ok(Self {
                    white_id: Some(state.colors.white),
                    black_id: Some(state.colors.black),
                    move_count: state.move_history.len(),
                    board: LiveBoard::Chess {
                        pieces: Box::new(rules::board_pieces(&board)),
                        last,
                    },
                })
            }
            DailyGame::Battleship => {
                let state = DailyBattleshipState::parse(state)?;
                let tally = |shooter: usize| {
                    let side = state.side(shooter);
                    let target = state.side(DailyBattleshipState::opponent_index(shooter));
                    ShotTally {
                        shots: side.shots.len(),
                        hits: side.shots.iter().filter(|shot| shot.hit).count(),
                        sunk: target
                            .ships
                            .iter()
                            .filter(|ship| state.ship_sunk(shooter, ship))
                            .count(),
                    }
                };
                Ok(Self {
                    white_id: None,
                    black_id: None,
                    move_count: state.shot_count(),
                    board: LiveBoard::Battleship {
                        sides: [tally(0), tally(1)],
                    },
                })
            }
            DailyGame::ConnectFour => {
                let state = DailyConnect4State::parse(state)?;
                Ok(Self {
                    white_id: None,
                    black_id: None,
                    move_count: state.move_count(),
                    board: LiveBoard::ConnectFour {
                        grid: state.grid(),
                        last: state.last_drop(),
                    },
                })
            }
            DailyGame::Reversi => {
                let state = DailyReversiState::parse(state)?;
                Ok(Self {
                    white_id: None,
                    black_id: None,
                    move_count: state.move_count(),
                    board: LiveBoard::Reversi {
                        grid: state.grid(),
                        last: state.last_move(),
                    },
                })
            }
            DailyGame::Checkers => {
                let state = DailyCheckersState::parse(state)?;
                Ok(Self {
                    white_id: None,
                    black_id: None,
                    move_count: state.move_count(),
                    board: LiveBoard::Checkers {
                        grid: state.grid(),
                        last: state.last_move().and_then(|path| path.last().copied()),
                    },
                })
            }
            DailyGame::Backgammon => {
                let state = DailyBackgammonState::parse(state)?;
                let board = state.board();
                Ok(Self {
                    white_id: None,
                    black_id: None,
                    move_count: state.move_count(),
                    board: LiveBoard::Backgammon {
                        white_id: state.white,
                        pips: [
                            board.pip_count(backgammon::Color::White),
                            board.pip_count(backgammon::Color::Red),
                        ],
                    },
                })
            }
            DailyGame::Briscola => {
                let state = DailyBriscolaState::parse(state)?;
                let table = state.table();
                Ok(Self {
                    white_id: None,
                    black_id: None,
                    move_count: state.move_count(),
                    board: LiveBoard::Briscola {
                        seat0_id: state.seats[0],
                        points: table.points,
                        stock_remaining: state.stock_remaining(),
                    },
                })
            }
            DailyGame::EightBall | DailyGame::NineBall | DailyGame::Snooker => {
                let state = DailyPoolState::parse(state)?;
                let spec = state.spec()?;
                let cue = state
                    .rack
                    .on_table()
                    .find(|ball| ball.id == crate::app::games::pool_core::ball::CUE)
                    .map(|ball| ball.pos);
                let balls = state
                    .rack
                    .on_table()
                    .filter(|ball| ball.id != crate::app::games::pool_core::ball::CUE)
                    .map(|ball| LiveBall {
                        id: ball.id,
                        pos: ball.pos,
                    })
                    .collect();
                Ok(Self {
                    white_id: None,
                    black_id: None,
                    move_count: state.move_count(),
                    board: LiveBoard::Pool {
                        rules: state.rules,
                        spec,
                        balls,
                        cue,
                        seat0_id: state.seats[0],
                        scores: state.scores,
                        last: state.shots.last().map(|shot| shot.label.clone()),
                    },
                })
            }
        }
    }
}

#[cfg(test)]
#[path = "live_test.rs"]
mod live_test;
