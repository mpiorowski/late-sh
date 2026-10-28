//! One small ASCII picture per arcade game, drawn on the paper's front
//! page over the day's most played game (`front_page`). Plain ASCII, at
//! most five rows and sixteen columns (`arcade_art_test` holds every
//! picture to it), so a picture is a glance and never crowds the numbers
//! under it. The match is exhaustive: a new arcade game breaks the build
//! here until it has a picture.

use late_core::models::leaderboard::{ArcadeGame, DailyPuzzle, ScoreGame};

/// The game's picture, top row first.
pub(crate) fn art(game: ArcadeGame) -> &'static [&'static str] {
    match game {
        ArcadeGame::Puzzle(DailyPuzzle::Sudoku) => &[
            "+---+---+---+",
            "| 5 |   | 3 |",
            "+---+---+---+",
            "|   | 7 |   |",
            "+---+---+---+",
        ],
        ArcadeGame::Puzzle(DailyPuzzle::Nonogram) => &[
            "    3 1 1", //
            "  2 # # .",
            "  1 # . .",
            "1 1 # . #",
        ],
        ArcadeGame::Puzzle(DailyPuzzle::Minesweeper) => &[
            "+--+--+--+",
            "| 1| 1| 1|",
            "+--+--+--+",
            "| 1| *| 1|",
            "+--+--+--+",
        ],
        ArcadeGame::Puzzle(DailyPuzzle::Solitaire) => &[
            ".----. .----.",
            "|A   | |K   |",
            "|  ^ | |  v |",
            "|   A| |   K|",
            "'----' '----'",
        ],
        ArcadeGame::Puzzle(DailyPuzzle::LeWord) => {
            &["[L][A][T][E][R]", "[W][O][R][D][S]", "[_][_][_][_][_]"]
        }
        ArcadeGame::Puzzle(DailyPuzzle::RubiksCube) => &[
            "   +--+--+", //
            "  /  /  /|",
            " +--+--+ +",
            " |  |  |/",
            " +--+--+",
        ],
        ArcadeGame::Puzzle(DailyPuzzle::SlidingPuzzle) => &[
            "+--+--+--+",
            "| 1| 2| 3|",
            "+--+--+--+",
            "| 4|  | 5|",
            "+--+--+--+",
        ],
        ArcadeGame::Puzzle(DailyPuzzle::Netwalk) => &[
            "[]--+--[]", //
            "    |",
            "[]--@--+--[]",
            "    |  |",
            "   []  []",
        ],
        ArcadeGame::Puzzle(DailyPuzzle::ChessPuzzle) => &[
            ". . . r k . . .", //
            ". . . . . . . .",
            ". . . . N . . .",
            ". . . . . . . .",
            ". . . . K . . .",
        ],
        ArcadeGame::Puzzle(DailyPuzzle::WordWheel) => &[
            "      W", //
            "  L \\ | / O",
            "     [H]",
            "  E / | \\ R",
            "      D",
        ],
        ArcadeGame::Puzzle(DailyPuzzle::Quartets) => &[
            "[  ?  ?  ?  ?  ]", //
            "[OAK][EGG][DOG]",
            "[ELM][MAP][YEW]",
            "[ASH][FIG][PUG]",
        ],
        ArcadeGame::Score(ScoreGame::Lateris) => &[
            "      [][]", //
            "  [][][][]",
            "  [][]  [][]",
            "[][][][][][]",
        ],
        ArcadeGame::Score(ScoreGame::TwentyFortyEight) => &[
            "+----+----+",
            "|  2 |  4 |",
            "+----+----+",
            "| 16 |2048|",
            "+----+----+",
        ],
        ArcadeGame::Score(ScoreGame::Snake) => &[
            " @ooooooo", //
            "        o",
            "  ooooooo",
            "  o",
            "  ooooooooo *",
        ],
        ArcadeGame::Score(ScoreGame::Traffic) => &[
            "---------------",
            "  __       __",
            "_/  \\_   _/  \\_",
            "o----o   o----o",
            "---------------",
        ],
    }
}
