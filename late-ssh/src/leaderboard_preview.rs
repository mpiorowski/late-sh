use std::{io, time::Duration};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use late_core::models::leaderboard::{HighScoreEntry, LeaderboardData, RankedEntry};
use ratatui::{Terminal, backend::CrosstermBackend};
use uuid::Uuid;

use crate::app::hub::{
    state::{HubState, HubTab},
    ui,
};

const CURRENT_USER_ID: Uuid = Uuid::from_u128(0x6d65_7661_6e6c_6300_0000_0000_0000_0001);

/// Run the resizable, database-free Hub leaderboard preview.
pub fn run(edge_to_edge: bool) -> io::Result<()> {
    let _session = TerminalSession::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let data = mock_data();
    let mut state = HubState::new();
    state.open(HubTab::Leaderboard);

    let result = loop {
        terminal.draw(|frame| {
            ui::draw_leaderboard_preview(
                frame,
                frame.area(),
                &state,
                &data,
                CURRENT_USER_ID,
                edge_to_edge,
            );
        })?;

        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                let quit = matches!(key.code, KeyCode::Esc | KeyCode::Char('q'))
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL));
                if quit {
                    break Ok(());
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
    };

    terminal.show_cursor()?;
    result
}

struct TerminalSession;

impl TerminalSession {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error);
        }
        Ok(Self)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

fn mock_data() -> LeaderboardData {
    LeaderboardData {
        monthly_chip_earners: ranked_board(
            0x1000,
            &[
                (1, "lemoncmd", 52_575),
                (2, "mars", 45_475),
                (3, "astersystem", 43_500),
                (4, "Bambus", 38_100),
                (5, "mojoro", 36_125),
                (6, "cws", 32_813),
                (7, "coko7", 28_850),
                (8, "flagrantior", 26_765),
                (9, "n0ll", 26_500),
                (10, "lunar", 24_500),
                (47, "mevanlc", 5_075),
            ],
        ),
        arcade_champions: ranked_board(
            0x2000,
            &[
                (1, "lemoncmd", 415),
                (2, "Bambus", 335),
                (3, "mojoro", 282),
                (4, "cws", 257),
                (5, "Perzium", 241),
                (6, "coko7", 230),
                (7, "flagrantior", 209),
                (8, "n0ll", 206),
                (9, "lunar", 173),
                (10, "OvidsMuse", 155),
                (64, "mevanlc", 8),
            ],
        ),
        monthly_tetris_high_scores: score_board(
            "Lateris",
            0x3000,
            &[
                (1, "beforeuall", 2_205_888),
                (2, "laksuall", 882_734),
                (3, "0x53", 336_718),
                (4, "damax", 256_874),
                (5, "Inkk_is_here", 202_959),
                (169, "mevanlc", 17_190),
            ],
        ),
        monthly_2048_high_scores: score_board(
            "2048",
            0x4000,
            &[
                (1, "mevanlc", 123_212),
                (2, "andrewg", 71_688),
                (3, "mojoro", 67_580),
                (4, "choedev", 67_332),
                (5, "Schnouki", 44_668),
            ],
        ),
        monthly_snake_high_scores: score_board(
            "Snake",
            0x5000,
            &[
                (1, "mojoro", 62_600),
                (2, "fellshard", 53_240),
                (3, "flagrantior", 34_770),
                (4, "n0ll", 32_070),
                (5, "bole", 28_210),
            ],
        ),
        monthly_traffic_high_scores: score_board(
            "Traffic",
            0x6000,
            &[
                (1, "odd", 2_722),
                (2, "qmay654", 2_338),
                (3, "beforeuall", 2_281),
                (4, "mnem", 1_097),
                (5, "lazo", 882),
            ],
        ),
        monthly_le_word_wins: ranked_board(
            0xb000,
            &[
                (1, "odd", 15),
                (2, "qmay654", 10),
                (3, "beforeuall", 9),
                (4, "mnem", 7),
                (5, "lazo", 2),
            ],
        ),
        all_time_le_word_wins: ranked_board(
            0xc000,
            &[
                (1, "beforeuall", 3_333),
                (2, "qmay654", 222),
                (3, "odd", 99),
                (4, "mnem", 80),
                (5, "lazo", 61),
            ],
        ),
        le_word_win_streaks: ranked_board(
            0xd000,
            &[
                (1, "odd", 15),
                (2, "qmay654", 12),
                (3, "beforeuall", 10),
                (4, "mnem", 8),
                (5, "lazo", 7),
            ],
        ),
        high_scores: all_time_scores(),
        ..LeaderboardData::default()
    }
}

fn all_time_scores() -> Vec<HighScoreEntry> {
    let mut scores = Vec::new();
    scores.extend(score_board(
        "Lateris",
        0x7000,
        &[
            (1, "beforeuall", 2_205_888),
            (2, "laksuall", 882_734),
            (3, "Shattered", 453_913),
            (4, "0x53", 336_718),
            (5, "damax", 256_874),
            (169, "mevanlc", 17_190),
        ],
    ));
    scores.extend(score_board(
        "2048",
        0x8000,
        &[
            (1, "mevanlc", 123_212),
            (2, "imerin", 111_988),
            (3, "mjswenxx", 101_424),
            (4, "crs", 73_892),
            (5, "andrewg", 71_688),
        ],
    ));
    scores.extend(score_board(
        "Snake",
        0x9000,
        &[
            (1, "mojoro", 62_600),
            (2, "HeadedBambus", 56_230),
            (3, "janmar6", 55_700),
            (4, "fellshard", 53_240),
            (5, "flagrantior", 34_770),
            (230, "mevanlc", 940),
        ],
    ));
    scores.extend(score_board(
        "Traffic",
        0xa000,
        &[
            (1, "odd", 2_722),
            (2, "qmay654", 2_338),
            (3, "beforeuall", 2_281),
            (4, "mnem", 1_097),
            (5, "lazo", 882),
        ],
    ));
    scores
}

fn ranked_board(namespace: u128, rows: &[(i64, &str, i64)]) -> Vec<RankedEntry> {
    rows.iter()
        .enumerate()
        .map(|(index, &(rank, username, value))| RankedEntry {
            username: username.to_string(),
            user_id: mock_user_id(namespace, index, username),
            rank,
            value,
        })
        .collect()
}

fn score_board(
    game: &'static str,
    namespace: u128,
    rows: &[(i64, &str, i32)],
) -> Vec<HighScoreEntry> {
    rows.iter()
        .enumerate()
        .map(|(index, &(rank, username, score))| HighScoreEntry {
            game,
            username: username.to_string(),
            user_id: mock_user_id(namespace, index, username),
            rank,
            score,
        })
        .collect()
}

fn mock_user_id(namespace: u128, index: usize, username: &str) -> Uuid {
    if username == "mevanlc" {
        CURRENT_USER_ID
    } else {
        Uuid::from_u128(namespace + index as u128 + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_data_marks_current_user_consistently() {
        let data = mock_data();
        assert_eq!(data.monthly_chip_earners.last().unwrap().rank, 47);
        assert_eq!(data.arcade_champions.last().unwrap().rank, 64);
        assert_eq!(data.le_word_win_streaks.len(), 5);
        assert_eq!(data.monthly_tetris_high_scores.last().unwrap().rank, 169);
        assert_eq!(
            data.monthly_tetris_high_scores.last().unwrap().user_id,
            CURRENT_USER_ID
        );
        assert_eq!(
            data.arcade_champions.last().unwrap().user_id,
            CURRENT_USER_ID
        );
        assert!(
            data.high_scores
                .iter()
                .filter(|entry| entry.user_id == CURRENT_USER_ID)
                .all(|entry| entry.username == "mevanlc")
        );
    }
}
