//! The live board: what one active match looks like from across the room.
//!
//! A compact, per-game snapshot of a match's position, built once per
//! snapshot publish from the same state JSON the service already parses for
//! move counts, and carried on every `DailyMatchItem`. The live strip
//! (`app/live/`) paints one at the top of the Home chat, the viewer's own
//! included, while something just happened to it, and a match that just
//! ended for a minute after (`finish_headline`); `live_strip.rs` is what a
//! match looks like there. It holds positions only, never a hidden
//! hand or a fleet: a live board is a spectator's view even for the players,
//! so it keeps exactly the secrets the spectate board keeps (`battleship_ui`
//! shows public shots only, `briscola_ui` shows backs).

use anyhow::{Result, bail};
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
    cribbage::{self, DailyCribbageState},
    games::DailyGame,
    gin::{self, DailyGinState},
    pool::{DailyPoolState, PoolAimShare},
    reversi::{self, DailyReversiState},
    svc::{DailyChessState, DailyFinishedItem, DailyMatchItem, DailyWinPayout},
};

/// The featured match as the strip paints it: its board, and the
/// shooter's aim while one is fresh.
pub struct LiveView<'a> {
    pub item: &'a DailyMatchItem,
    pub board: &'a LiveBoard,
    pub aim: Option<&'a PoolAimShare>,
}

/// A match as the live strip paints it: one in play, or the final board of
/// one that just ended with the result as the strip announces it.
pub struct MatchStripView<'a> {
    pub view: LiveView<'a>,
    /// `Some` for a finished match (`finish_headline`); the strip then opens
    /// nothing on click, since the board is gone from the lobby.
    pub finish: Option<&'a str>,
}

/// The result line the live strip shows under a finished match's last
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
            | DailyResult::FrameWon
            | DailyResult::PeggedOut
            | DailyResult::ReachedHundred => format!("a draw · {phrase}"),
        },
    }
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
    /// A race to a target score over many hands: cribbage and gin rummy.
    /// Scores only; the hands stay hidden.
    ScoreRace {
        /// Seat 0's user id; the seats are the claim-time coin flip.
        seat0_id: Uuid,
        scores: [u32; 2],
        target: u32,
        /// The hand in play, from one.
        hand: usize,
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
            DailyGame::Cribbage => {
                let state = DailyCribbageState::parse(state)?;
                let table = state.table();
                Ok(Self {
                    white_id: None,
                    black_id: None,
                    move_count: state.move_count(),
                    board: LiveBoard::ScoreRace {
                        seat0_id: state.seats[0],
                        scores: table.scores,
                        target: cribbage::WINNING_SCORE,
                        hand: table.hand + 1,
                    },
                })
            }
            DailyGame::GinRummy => {
                let state = DailyGinState::parse(state)?;
                let table = state.table();
                Ok(Self {
                    white_id: None,
                    black_id: None,
                    move_count: state.move_count(),
                    board: LiveBoard::ScoreRace {
                        seat0_id: state.seats[0],
                        scores: table.scores,
                        target: gin::TARGET_SCORE,
                        hand: table.hand + 1,
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
