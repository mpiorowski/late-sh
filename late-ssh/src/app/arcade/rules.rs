//! The `!` rules card, one per Arcade game.
//!
//! One exhaustive match over the roster ("One tree" in `CONTEXT.md`), so a
//! new game fails the build here until it has a page. A lobby row carries
//! its game's title and what changes about it and nothing else, so this is
//! where a player reads how a game is played (`DESIGN_GUIDE.md` §4.5).
//!
//! A page states rules, and every number in it is read from the source of
//! truth rather than written out, so a card cannot drift from the rules it
//! describes: the chips come from `DailyPuzzle::tiers`, the way the lobby
//! marks and the Arcade Wins points do.

use late_core::models::leaderboard::{ArcadeGame, DailyPuzzle, ScoreGame, Tiers};
use ratatui::text::Line;

/// What closes a rules card. Uniform across the Arcade: the card is the
/// only thing listening while it is up.
pub const CLOSE_KEYS: &str = "! / q / Esc closes";

/// The width every rules card is drawn at.
pub const WIDTH: u16 = 62;

/// A game's rules card.
pub struct Rules {
    pub title: String,
    pub body: Vec<Line<'static>>,
}

impl Rules {
    /// Rows the card needs: the title, a blank, the body, a blank, the
    /// close line, and the two border rows.
    pub fn height(&self) -> u16 {
        self.body.len() as u16 + 6
    }
}

/// The rules card for `game`.
pub fn rules_for(game: ArcadeGame) -> Rules {
    match game {
        ArcadeGame::Puzzle(puzzle) => daily_rules(puzzle),
        ArcadeGame::Score(score) => score_rules(score),
    }
}

fn daily_rules(puzzle: DailyPuzzle) -> Rules {
    let mut body = match puzzle {
        // The games that already had a page keep their own copy; it is the
        // body that is theirs, not the frame.
        DailyPuzzle::LeWord => super::le_word::ui::rules_lines(),
        DailyPuzzle::WordWheel => super::word_wheel::ui::rules_lines(),
        DailyPuzzle::Quartets => super::quartets::ui::rules_lines(),
        DailyPuzzle::Sudoku => lines(&[
            "Fill the grid so that every row, every column and every 3x3 box holds the digits 1 to 9 exactly once.",
            "",
            "A digit you entered turns red when it repeats inside its row, column or box. Pencil marks hold the candidates you have not committed to.",
        ]),
        DailyPuzzle::Nonogram => lines(&[
            "Fill the cells the clues describe. Each number is a run of filled cells in that row or column, the runs in the order given, with at least one empty cell between them.",
            "",
            "A clue turns green once the line satisfies it and red once no filling of the line can. Every shipped puzzle is solvable by line logic alone and has one answer, so nothing here needs a guess.",
        ]),
        DailyPuzzle::Minesweeper => lines(&[
            "Open every cell that is not a mine. A number counts the mines touching that cell; flag the ones you have worked out.",
            "",
            "On an open number whose flags account for it, one key opens the rest of its neighbours. Opening a mine costs a life, and the board ends on the third.",
        ]),
        DailyPuzzle::Solitaire => lines(&[
            "Klondike. Build the four foundations up from ace to king in a suit, and the tableau down in alternating colours. Only a king fills an empty column.",
            "",
            "Draw one and draw three are separate boards and separate wins.",
        ]),
        DailyPuzzle::RubiksCube => lines(&[
            "Turn the faces until every side is one colour. Everyone gets the same scramble for the day, and there is no personal cube.",
            "",
            "The view rotates without touching the cube. Reset returns the day's scramble, and is refused once the cube is solved.",
        ]),
        DailyPuzzle::SlidingPuzzle => lines(&[
            "Slide tiles into the gap until the board is in order. Easy, medium and hard are 3x3, 4x4 and 5x5 including the gap.",
            "",
            "The day's board carries a piece from the Artboard gallery; numbers overlay it until it is solved. Personal boards are random, save, and pay nothing.",
        ]),
        DailyPuzzle::Netwalk => lines(&[
            "Turn each tile until the whole network is lit from the source in the middle. Easy, medium and hard are 5x5, 7x7 and 9x9.",
            "",
            "Turning never changes how many arms a tile has, so any wiring that lights every tile wins, not only the one it was dealt from. Lock a tile you are sure of.",
        ]),
        DailyPuzzle::ChessPuzzle => lines(&[
            "Find the winning line of a real game. The opponent plays the setup move, then answers each move you find.",
            "",
            "A wrong move is taken back and costs one of three lives; the board ends on the third and names the line you missed. Any checkmate wins, on the line or off it.",
        ]),
    };
    body.push(Line::from(""));
    body.push(payout_line(puzzle));
    Rules {
        title: format!("{} Rules", puzzle.title()),
        body,
    }
}

fn score_rules(game: ScoreGame) -> Rules {
    let body = match game {
        ScoreGame::TwentyFortyEight => lines(&[
            "Every move slides the whole board one way, and two touching tiles of the same number merge into their sum. One new tile appears after each move.",
            "",
            "A merge scores the tile it makes. The run ends when no move is left.",
        ]),
        ScoreGame::Lateris => lines(&[
            "Fit the falling pieces into full rows. A full row clears and scores; the pieces fall faster as you go.",
            "",
            "A piece can be held back for later, one at a time. The run ends when a new piece has no room.",
        ]),
        ScoreGame::Snake => lines(&[
            "Steer the snake into food to grow and score. It never stops moving, and it speeds up as the level rises.",
            "",
            "Hitting a rock, an edge or your own body ends the run. The score a pickup pays rises with the level, and doubles while you are powered up.",
        ]),
        ScoreGame::Traffic => lines(&[
            "Overtake traffic and reach the end of the track. Crashing ends the run, and a track you do not finish scores nothing.",
            "",
            "Each track is graded on how fast you finished it, so every track is worth the same at its best. Your score is the sum of your best on each track.",
        ]),
    };
    Rules {
        title: format!("{} Rules", game.title()),
        body,
    }
}

/// What the daily pays, read from `DailyPuzzle::tiers` so the card, the
/// lobby marks and the payout stay one number.
fn payout_line(puzzle: DailyPuzzle) -> Line<'static> {
    let tiers = puzzle.tiers();
    let text = match tiers {
        Tiers::Single(tier) => format!("Pays {} chips, once a UTC day.", tier.chips()),
        Tiers::Keyed(_) => {
            let boards: Vec<String> = tiers
                .list()
                .iter()
                .map(|(key, tier)| format!("{key} {}", tier.chips()))
                .collect();
            format!(
                "Chips, each once a UTC day: {}. A replay is practice.",
                boards.join(" · ")
            )
        }
    };
    Line::from(text)
}

fn lines(rows: &[&str]) -> Vec<Line<'static>> {
    rows.iter().map(|row| Line::from(row.to_string())).collect()
}

#[cfg(test)]
#[path = "rules_test.rs"]
mod rules_test;
