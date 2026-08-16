use late_core::models::{
    chips::Difficulty,
    leaderboard::{DailyPuzzle, DoorGame, LeaderboardData, RankedEntry, ScoreGame},
};

use crate::app::common::primitives::thousands;

const EMPTY: &[RankedEntry] = &[];

/// One selectable board on the Leaderboards page. The two bespoke boards
/// lead, then every game board; the per-game boards come straight off the
/// late-core rosters, so a game added there appears here without a page
/// change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Board {
    LateaniaAdventurers,
    LateaniaFrontier,
    DoorWins(DoorGame),
    DoorDepth(DoorGame),
    DoorScore(DoorGame),
    TopChips,
    ArcadeWins,
    Daily(DailyPuzzle),
    Score(ScoreGame),
}

/// What the detail pane shows for one board. Every board answers with
/// exactly one of these; the renderer matches all three, so a new shape
/// cannot fall through to a wrong heading.
pub(crate) enum Standings<'a> {
    /// One window of month-scoped standings, headed "this month".
    MonthlyOnly(&'a [RankedEntry]),
    /// One window over all recorded history, headed "all time" (the door
    /// wins boards: a win is forever).
    AllTimeOnly(&'a [RankedEntry]),
    /// One window ranking a current state of the world, headed "right now"
    /// (the Lateania boards: living characters, not history).
    Snapshot(&'a [RankedEntry]),
    /// The default paired monthly and all-time windows.
    Paired {
        monthly: &'a [RankedEntry],
        all_time: &'a [RankedEntry],
    },
}

impl Board {
    /// Page order: the bespoke boards, then the Games group (the Lateania
    /// snapshot boards, then each door's board triple), then daily puzzles,
    /// then score games, each roster in its declaration order.
    pub(crate) fn all() -> Vec<Self> {
        let mut boards = vec![
            Self::TopChips,
            Self::ArcadeWins,
            Self::LateaniaAdventurers,
            Self::LateaniaFrontier,
        ];
        for &game in DoorGame::ALL {
            boards.push(Self::DoorWins(game));
            boards.push(Self::DoorDepth(game));
            boards.push(Self::DoorScore(game));
        }
        boards.extend(DailyPuzzle::ALL.iter().copied().map(Self::Daily));
        boards.extend(ScoreGame::ALL.iter().copied().map(Self::Score));
        boards
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::LateaniaAdventurers => "Lateania Adventurers",
            Self::LateaniaFrontier => "Lateania Frontier",
            Self::DoorWins(DoorGame::Dcss) => "DCSS Wins",
            Self::DoorDepth(DoorGame::Dcss) => "DCSS Deepest Dive",
            Self::DoorScore(DoorGame::Dcss) => "DCSS Top Score",
            Self::DoorWins(DoorGame::Nethack) => "NetHack Wins",
            Self::DoorDepth(DoorGame::Nethack) => "NetHack Deepest Dive",
            Self::DoorScore(DoorGame::Nethack) => "NetHack Top Score",
            Self::DoorWins(DoorGame::Brogue) => "Brogue Wins",
            Self::DoorDepth(DoorGame::Brogue) => "Brogue Deepest Dive",
            Self::DoorScore(DoorGame::Brogue) => "Brogue Top Score",
            Self::TopChips => "Top Chips",
            Self::ArcadeWins => "Arcade Wins",
            Self::Daily(puzzle) => puzzle.title(),
            Self::Score(game) => game.title(),
        }
    }

    /// One line under the board title saying what the numbers are. The
    /// ArcadeWins arm is formatted from [`Difficulty::points`] so the copy
    /// cannot drift from the SQL the points come from.
    pub(crate) fn hint(self) -> String {
        match self {
            Self::LateaniaAdventurers => {
                "living characters by level, ties broken by experience".to_string()
            }
            Self::LateaniaFrontier => "deepest Frontier zone walked, of 20".to_string(),
            Self::DoorWins(DoorGame::Dcss) => "games escaped with the Orb of Zot".to_string(),
            Self::DoorDepth(DoorGame::Dcss) => "deepest dungeon depth ever reached".to_string(),
            Self::DoorScore(DoorGame::Dcss) => "best final score".to_string(),
            Self::DoorWins(DoorGame::Nethack) => "games ascended to demigodhood".to_string(),
            Self::DoorDepth(DoorGame::Nethack) => "deepest dungeon level ever reached".to_string(),
            Self::DoorScore(DoorGame::Nethack) => "best final score".to_string(),
            Self::DoorWins(DoorGame::Brogue) => "games escaped or mastered".to_string(),
            Self::DoorDepth(DoorGame::Brogue) => "deepest dungeon depth ever reached".to_string(),
            Self::DoorScore(DoorGame::Brogue) => "best final score".to_string(),
            Self::TopChips => "monthly net chip delta, shop spend ignored".to_string(),
            Self::ArcadeWins => format!(
                "daily puzzle points: easy {} · medium {} · hard {}",
                Difficulty::Easy.points(),
                Difficulty::Medium.points(),
                Difficulty::Hard.points(),
            ),
            Self::Daily(_) => "daily wins".to_string(),
            Self::Score(_) => "best score".to_string(),
        }
    }

    /// A board value formatted for a standings row. Kept next to `hint` so a
    /// board's copy and its number format live in one place.
    pub(crate) fn format_value(self, value: i64) -> String {
        match self {
            Self::LateaniaAdventurers => format!("lvl {value}"),
            Self::LateaniaFrontier => format!("zone {value}"),
            Self::DoorWins(_) => format!("{} wins", thousands(value)),
            Self::DoorDepth(_) => format!("depth {value}"),
            Self::DoorScore(_) => thousands(value),
            Self::TopChips => format!("{} chips", thousands(value)),
            Self::ArcadeWins => format!("{} pts", thousands(value)),
            Self::Daily(_) => format!("{} wins", thousands(value)),
            Self::Score(_) => thousands(value),
        }
    }

    pub(crate) fn standings(self, data: &LeaderboardData) -> Standings<'_> {
        match self {
            Self::LateaniaAdventurers => Standings::Snapshot(&data.lateania_adventurers),
            Self::LateaniaFrontier => Standings::Snapshot(&data.lateania_frontier),
            Self::DoorWins(game) => {
                Standings::AllTimeOnly(data.door_board(game).map_or(EMPTY, |board| &board.wins))
            }
            Self::DoorDepth(game) => {
                let windows = data.door_board(game).map(|board| &board.depth);
                Standings::Paired {
                    monthly: windows.map_or(EMPTY, |board| &board.monthly),
                    all_time: windows.map_or(EMPTY, |board| &board.all_time),
                }
            }
            Self::DoorScore(game) => {
                let windows = data.door_board(game).map(|board| &board.score);
                Standings::Paired {
                    monthly: windows.map_or(EMPTY, |board| &board.monthly),
                    all_time: windows.map_or(EMPTY, |board| &board.all_time),
                }
            }
            Self::TopChips => Standings::MonthlyOnly(&data.monthly_chip_earners),
            Self::ArcadeWins => Standings::MonthlyOnly(&data.arcade_champions),
            Self::Daily(puzzle) => {
                let windows = data.daily_board(puzzle);
                Standings::Paired {
                    monthly: windows.map_or(EMPTY, |board| &board.monthly),
                    all_time: windows.map_or(EMPTY, |board| &board.all_time),
                }
            }
            Self::Score(game) => {
                let windows = data.score_board(game);
                Standings::Paired {
                    monthly: windows.map_or(EMPTY, |board| &board.monthly),
                    all_time: windows.map_or(EMPTY, |board| &board.all_time),
                }
            }
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
