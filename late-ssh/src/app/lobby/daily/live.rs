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
    battleship::{self, DailyBattleshipState},
    briscola::{self, DailyBriscolaState},
    checkers::{self, DailyCheckersState},
    connect4::{self, DailyConnect4State},
    cribbage::DailyCribbageState,
    games::DailyGame,
    gin::{DailyGinState, GinMove},
    pool::{DailyPoolState, PoolAimShare},
    reversi::{self, DailyReversiState},
    std_deck,
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
/// has to say what it looks like from across the room. The card games carry
/// what lies face up and how many cards each seat holds, never the faces.
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
        /// Side 0's user id.
        side0_id: Uuid,
        /// Every shot each side fired: the public salvo record, never a
        /// fleet. Indexed by shooter.
        shots: [Vec<LiveShot>; 2],
        /// Ships each side has sunk, indexed by shooter.
        sunk: [usize; 2],
        /// The newest shot and who fired it.
        last: Option<(usize, LiveShot)>,
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
        /// The whole position: every point is public.
        board: backgammon::Board,
        /// Who rolls next.
        turn: backgammon::Color,
        /// The roll waiting on the mover; `None` once the match is over.
        roll: Option<[u8; 2]>,
        /// Points the last turn's hops landed on.
        landed: Vec<u8>,
    },
    Briscola {
        /// Seat 0's user id; the seats are the claim-time coin flip.
        seat0_id: Uuid,
        /// Points captured per seat.
        points: [u32; 2],
        /// Cards held per seat: how many, never which.
        held: [usize; 2],
        /// Cards left to draw, the trump included.
        stock_remaining: usize,
        trump: briscola::Card,
        trick: LiveTrick,
        /// The last card played and its seat.
        last: Option<(usize, briscola::Card)>,
    },
    Cribbage {
        /// Seat 0's user id; the seats are the claim-time coin flip.
        seat0_id: Uuid,
        /// The front pegs.
        scores: [u32; 2],
        /// The back pegs: each seat's score before its latest peg.
        back: [u32; 2],
        /// The hand in play, from one.
        hand: usize,
    },
    GinRummy {
        /// Seat 0's user id; the seats are the claim-time coin flip.
        seat0_id: Uuid,
        scores: [u32; 2],
        /// The hand in play, from one.
        hand: usize,
        /// Cards held per seat: how many, never which.
        held: [usize; 2],
        /// The face-up top of the discard pile, and whether cards lie under it.
        discard: Option<(std_deck::Card, bool)>,
        stock: usize,
        /// The last move and its seat. A stock draw never names its card.
        last: Option<(usize, GinMove)>,
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

/// One battleship shot as the room saw it land.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveShot {
    pub cell: u8,
    pub hit: bool,
}

/// The briscola trick on the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveTrick {
    /// Nothing played yet this match.
    Empty,
    /// One card down, the follower on the clock.
    Led { card: briscola::Card },
    /// The last trick taken, both cards still showing.
    Taken {
        lead: briscola::Card,
        answer: briscola::Card,
        /// Whether the lead took it.
        lead_won: bool,
    },
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
                let shots = |shooter: usize| -> Vec<LiveShot> {
                    state
                        .side(shooter)
                        .shots
                        .iter()
                        .map(|shot| LiveShot {
                            cell: shot.cell,
                            hit: shot.hit,
                        })
                        .collect()
                };
                let sunk = |shooter: usize| {
                    battleship::FLEET_LENGTHS.len() - state.ships_afloat_against(shooter)
                };
                let last = (0..2)
                    .flat_map(|shooter| {
                        state
                            .side(shooter)
                            .shots
                            .iter()
                            .map(move |shot| (shooter, shot))
                    })
                    .max_by_key(|(_, shot)| shot.at)
                    .map(|(shooter, shot)| {
                        (
                            shooter,
                            LiveShot {
                                cell: shot.cell,
                                hit: shot.hit,
                            },
                        )
                    });
                Ok(Self {
                    white_id: None,
                    black_id: None,
                    move_count: state.shot_count(),
                    board: LiveBoard::Battleship {
                        side0_id: state.side(0).user_id,
                        shots: [shots(0), shots(1)],
                        sunk: [sunk(0), sunk(1)],
                        last,
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
                let landed = state
                    .last_turn()
                    .map(|turn| {
                        turn.hops
                            .iter()
                            .map(|&(_, to)| to)
                            .filter(|&to| (to as usize) < backgammon::POINTS)
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(Self {
                    white_id: None,
                    black_id: None,
                    move_count: state.move_count(),
                    board: LiveBoard::Backgammon {
                        white_id: state.white,
                        board: state.board(),
                        turn: state.turn(),
                        roll: state.next_roll,
                        landed,
                    },
                })
            }
            DailyGame::Briscola => {
                let state = DailyBriscolaState::parse(state)?;
                let table = state.table();
                let trick = match (table.led, table.history.last()) {
                    (Some((_, card)), _) => LiveTrick::Led { card },
                    (None, Some(taken)) => LiveTrick::Taken {
                        lead: taken.lead,
                        answer: taken.answer,
                        lead_won: taken.winner == taken.leader,
                    },
                    (None, None) => LiveTrick::Empty,
                };
                Ok(Self {
                    white_id: None,
                    black_id: None,
                    move_count: state.move_count(),
                    board: LiveBoard::Briscola {
                        seat0_id: state.seats[0],
                        points: table.points,
                        held: [table.hands[0].len(), table.hands[1].len()],
                        stock_remaining: state.stock_remaining(),
                        trump: state.trump(),
                        trick,
                        last: table.last,
                    },
                })
            }
            DailyGame::Cribbage => {
                let state = DailyCribbageState::parse(state)?;
                let table = state.table();
                let back = |seat: usize| match table.log.iter().rev().find(|peg| peg.seat == seat) {
                    Some(peg) => table.scores[seat] - peg.points,
                    None => 0,
                };
                Ok(Self {
                    white_id: None,
                    black_id: None,
                    move_count: state.move_count(),
                    board: LiveBoard::Cribbage {
                        seat0_id: state.seats[0],
                        scores: table.scores,
                        back: [back(0), back(1)],
                        hand: table.hand + 1,
                    },
                })
            }
            DailyGame::GinRummy => {
                let state = DailyGinState::parse(state)?;
                let table = state.table();
                let discard = table
                    .discards
                    .last()
                    .map(|&top| (top, table.discards.len() > 1));
                Ok(Self {
                    white_id: None,
                    black_id: None,
                    move_count: state.move_count(),
                    board: LiveBoard::GinRummy {
                        seat0_id: state.seats[0],
                        scores: table.scores,
                        hand: table.hand + 1,
                        held: [table.hands[0].len(), table.hands[1].len()],
                        discard,
                        stock: table.stock_remaining(),
                        last: table.last,
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
