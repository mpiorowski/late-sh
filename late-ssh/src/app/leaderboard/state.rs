use late_core::models::leaderboard::{DailyPuzzle, LeaderboardData, RankedEntry, ScoreGame};

const EMPTY: &[RankedEntry] = &[];

/// One selectable board on the Leaderboards page. The two bespoke boards
/// lead; the per-game boards come straight off the late-core rosters, so a
/// game added there appears here without a page change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Board {
    TopChips,
    ArcadeWins,
    Daily(DailyPuzzle),
    Score(ScoreGame),
}

impl Board {
    /// Page order: bespoke boards, then daily puzzles, then score games,
    /// each roster in its declaration order.
    pub(crate) fn all() -> Vec<Self> {
        let mut boards = vec![Self::TopChips, Self::ArcadeWins];
        boards.extend(DailyPuzzle::ALL.iter().copied().map(Self::Daily));
        boards.extend(ScoreGame::ALL.iter().copied().map(Self::Score));
        boards
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::TopChips => "Top Chips",
            Self::ArcadeWins => "Arcade Wins",
            Self::Daily(puzzle) => puzzle.title(),
            Self::Score(game) => game.title(),
        }
    }

    /// One line under the board title saying what the numbers are.
    pub(crate) fn hint(self) -> &'static str {
        match self {
            Self::TopChips => "monthly net chip delta, shop spend ignored",
            Self::ArcadeWins => "daily puzzle points: easy 1 · medium 3 · hard 5",
            Self::Daily(_) => "daily wins",
            Self::Score(_) => "best score",
        }
    }

    /// Suffix printed after each value; empty for raw scores.
    pub(crate) fn value_label(self) -> &'static str {
        match self {
            Self::TopChips => "chips",
            Self::ArcadeWins => "pts",
            Self::Daily(_) => "wins",
            Self::Score(_) => "",
        }
    }

    pub(crate) fn monthly(self, data: &LeaderboardData) -> &[RankedEntry] {
        match self {
            Self::TopChips => &data.monthly_chip_earners,
            Self::ArcadeWins => &data.arcade_champions,
            Self::Daily(puzzle) => data
                .daily_board(puzzle)
                .map_or(EMPTY, |board| &board.monthly),
            Self::Score(game) => data.score_board(game).map_or(EMPTY, |board| &board.monthly),
        }
    }

    /// `None` for the two bespoke boards, which are monthly-only.
    pub(crate) fn all_time(self, data: &LeaderboardData) -> Option<&[RankedEntry]> {
        match self {
            Self::TopChips | Self::ArcadeWins => None,
            Self::Daily(puzzle) => Some(
                data.daily_board(puzzle)
                    .map_or(EMPTY, |board| &board.all_time),
            ),
            Self::Score(game) => Some(
                data.score_board(game)
                    .map_or(EMPTY, |board| &board.all_time),
            ),
        }
    }
}

pub(crate) struct LeaderboardPageState {
    boards: Vec<Board>,
    selected: usize,
}

impl LeaderboardPageState {
    pub(crate) fn new() -> Self {
        Self {
            boards: Board::all(),
            selected: 0,
        }
    }

    pub(crate) fn boards(&self) -> &[Board] {
        &self.boards
    }

    pub(crate) fn selected_index(&self) -> usize {
        self.selected
    }

    pub(crate) fn selected_board(&self) -> Board {
        self.boards[self.selected]
    }

    pub(crate) fn select_next(&mut self) {
        self.selected = (self.selected + 1) % self.boards.len();
    }

    pub(crate) fn select_previous(&mut self) {
        self.selected = (self.selected + self.boards.len() - 1) % self.boards.len();
    }
}
