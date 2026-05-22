use late_core::models::chips::difficulty_bonus;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::app::common::theme;

pub fn draw(frame: &mut Frame, area: Rect, scroll: u16) {
    let sections = Layout::vertical([
        Constraint::Length(1), // heading
        Constraint::Length(1), // summary
        Constraint::Length(1), // breathing
        Constraint::Min(0),    // body
    ])
    .split(area);

    frame.render_widget(Paragraph::new(section_heading("Guide")), sections[0]);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "Chips, leaderboards, Arcade, and room-game controls. j/k scroll.",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])),
        sections[1],
    );

    let max_scroll = content_line_count().saturating_sub(sections[3].height as usize) as u16;
    let scroll = scroll.min(max_scroll);
    frame.render_widget(
        Paragraph::new(guide_lines())
            .scroll((scroll, 0))
            .wrap(Wrap { trim: false }),
        sections[3],
    );
}

pub(crate) fn content_line_count() -> usize {
    guide_lines().len()
}

pub(crate) fn bot_context_lines() -> Vec<String> {
    let mut lines = Vec::new();
    for section in guide_sections() {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(section.title.to_string());
        lines.extend(section.body.into_iter().map(|line| format!("  {line}")));
    }
    lines
}

struct GuideSection {
    title: &'static str,
    body: Vec<String>,
}

fn guide_lines() -> Vec<Line<'static>> {
    render_sections(guide_sections())
}

fn guide_sections() -> Vec<GuideSection> {
    let mut sections = Vec::new();
    sections.extend(chip_sections());
    sections.extend(leaderboard_sections());
    sections.extend(arcade_sections());
    sections.extend(room_game_sections());
    sections
}

fn chip_sections() -> Vec<GuideSection> {
    vec![
        GuideSection {
            title: "Earn Chips",
            body: vec![
                "New accounts start with 1,000 chips.".to_string(),
                "Daily puzzle wins pay once per daily board:".to_string(),
                format!("easy       {:>4} chips", difficulty_bonus("easy")),
                format!("medium     {:>4} chips", difficulty_bonus("medium")),
                format!("hard       {:>4} chips", difficulty_bonus("hard")),
                "Solitaire draw-1 pays medium; draw-3 pays hard.".to_string(),
                format!(
                    "Bonsai watering pays {} chips once per UTC day.",
                    crate::app::bonsai::svc::WATER_CHIP_BONUS
                ),
            ],
        },
        GuideSection {
            title: "Top Chips",
            body: vec![
                "Monthly Top Chips counts positive earnings only.".to_string(),
                "Spending chips does not lower your monthly rank.".to_string(),
                "Floor restores are excluded from the board.".to_string(),
            ],
        },
    ]
}

fn leaderboard_sections() -> Vec<GuideSection> {
    vec![
        GuideSection {
            title: "Arcade Wins",
            body: vec![
                "Counts daily Sudoku, Nonograms, Solitaire, and Minesweeper.".to_string(),
                "Each completed daily adds monthly points:".to_string(),
                "easy / draw-1  1 pt".to_string(),
                "medium         3 pts".to_string(),
                "hard / draw-3  5 pts".to_string(),
                "More hard dailies across more games wins the board.".to_string(),
            ],
        },
        GuideSection {
            title: "Score Games",
            body: vec![
                "Tetris, 2048, and Snake record run scores.".to_string(),
                "Monthly boards use scores recorded this month.".to_string(),
                "All-time boards use each user's saved best score.".to_string(),
            ],
        },
        GuideSection {
            title: "Timing",
            body: vec![
                "Monthly boards reset on the 1st, UTC.".to_string(),
                "All-time score boards persist.".to_string(),
                "Hub refreshes from the server about every 30 seconds.".to_string(),
            ],
        },
    ]
}

fn arcade_sections() -> Vec<GuideSection> {
    vec![
        GuideSection {
            title: "Arcade Overview",
            body: vec![
                "The Arcade mixes daily puzzle runs with endless score chases.".to_string(),
                "Open The Arcade with 2.".to_string(),
                "High-score games: 2048, Tetris, Snake.".to_string(),
                "Daily games: Sudoku, Nonograms, Minesweeper, Solitaire.".to_string(),
            ],
        },
        GuideSection {
            title: "Arcade Lobby",
            body: vec![
                "j/k or arrows browse games.".to_string(),
                "Enter plays the selected game.".to_string(),
                "Esc/q leaves the current game.".to_string(),
                "` returns to Dashboard while a run is active.".to_string(),
            ],
        },
        GuideSection {
            title: "2048",
            body: vec![
                "hjkl or arrows slide tiles.".to_string(),
                "r restarts after game over.".to_string(),
            ],
        },
        GuideSection {
            title: "Tetris",
            body: vec![
                "h/j/k/l or arrows move, soft-drop, rotate.".to_string(),
                "WASD also moves, soft-drops, and rotates.".to_string(),
                "Space hard drops.".to_string(),
                "p pauses; r/n restarts.".to_string(),
            ],
        },
        GuideSection {
            title: "Snake",
            body: vec![
                "hjkl, WASD, or arrows steer.".to_string(),
                "p pauses; r/n restarts.".to_string(),
            ],
        },
        GuideSection {
            title: "Daily Puzzle Common Keys",
            body: vec![
                "d selects the daily board.".to_string(),
                "p selects a personal board.".to_string(),
                "n starts a new personal board.".to_string(),
                "[ and ] change difficulty.".to_string(),
                "hjkl or arrows move cursor.".to_string(),
                "r resets the board.".to_string(),
            ],
        },
        GuideSection {
            title: "Sudoku",
            body: vec![
                "1-9 fills a digit.".to_string(),
                "0 or Backspace clears a cell.".to_string(),
            ],
        },
        GuideSection {
            title: "Nonograms",
            body: vec![
                "Space fills or un-fills a cell.".to_string(),
                "x marks or unmarks.".to_string(),
                "c, 0, or Backspace clears a cell.".to_string(),
            ],
        },
        GuideSection {
            title: "Minesweeper",
            body: vec![
                "Space or Enter reveals.".to_string(),
                "f or x flags and unflags.".to_string(),
            ],
        },
        GuideSection {
            title: "Solitaire",
            body: vec![
                "hjkl or arrows move focus.".to_string(),
                "Space or Enter activates, selects, or moves.".to_string(),
                "a auto-moves one card.".to_string(),
                "f auto-foundations all possible cards.".to_string(),
                "u undoes.".to_string(),
                "c clears selection.".to_string(),
                "{ and } scroll the board.".to_string(),
            ],
        },
    ]
}

fn room_game_sections() -> Vec<GuideSection> {
    vec![
        GuideSection {
            title: "Room Games",
            body: vec![
                "Open Rooms with 3.".to_string(),
                "Directory filters: All, Blackjack, Chess, Poker, Tic-Tac-Toe, Tron.".to_string(),
                "j/k or arrows navigate rooms.".to_string(),
                "h/l or left/right cycles filters.".to_string(),
                "/ searches by room name.".to_string(),
                "Enter enters the selected room.".to_string(),
                "n creates a new room when the selected game supports creation.".to_string(),
                "Esc clears create/search/query/filter before leaving room state.".to_string(),
            ],
        },
        GuideSection {
            title: "Create Room Forms",
            body: vec![
                "Room name maxes at 48 chars; search query maxes at 32 chars.".to_string(),
                "A user can have up to 3 open tables per game kind.".to_string(),
                "Blackjack form: name, pace, stake.".to_string(),
                "Poker form: name, pace, blinds, starting stack.".to_string(),
                "Tic-Tac-Toe form: name.".to_string(),
            ],
        },
        GuideSection {
            title: "Active Room",
            body: vec![
                "Game is on top; embedded game chat is below.".to_string(),
                "` returns to Dashboard; backtick on Dashboard returns to last game.".to_string(),
                "i composes in embedded chat.".to_string(),
                "Esc clears selected embedded-chat message first.".to_string(),
                "j/k selects embedded-chat messages unless the game claims the key.".to_string(),
                "PageUp/PageDown scroll embedded chat.".to_string(),
                "r/e/d/p/c/f reply, edit, delete, profile, copy, react selected chat message.".to_string(),
                "Ctrl+P pins or unpins selected embedded-chat message.".to_string(),
                "Arrows go to the game first; otherwise embedded chat handles them.".to_string(),
            ],
        },
        GuideSection {
            title: "Blackjack",
            body: vec![
                "Four seats, chips, 6-deck shoe, dealer stands soft 17, blackjack pays 3:2.".to_string(),
                "Paces: Quick 2m, Standard 5m, Chill 10m.".to_string(),
                "Stakes: 10, 50, 100, or 500 chips; max bet is 10x stake.".to_string(),
                "s or Enter sits in first open seat.".to_string(),
                "l leaves seat when safe.".to_string(),
                "[/a previous chip; ]/d next chip.".to_string(),
                "Space throws selected chip.".to_string(),
                "Backspace pulls one chip.".to_string(),
                "c or Ctrl+W clears pending bet.".to_string(),
                "Enter or s locks bet.".to_string(),
                "h or Space hits; s stands; d/D doubles down when eligible.".to_string(),
                "First locked bet starts a fixed 30s betting cap.".to_string(),
            ],
        },
        GuideSection {
            title: "Poker",
            body: vec![
                "Four-seat fixed-stack Texas Hold'em with private hole cards, shared board, side pots, showdown ranking, and chip settlement.".to_string(),
                "Room stacks: 100, 500, 1000, 2000, or 5000 chips.".to_string(),
                "Blinds: 10/20, 25/50, 50/100, or 100/200.".to_string(),
                "s or Enter sits in first open seat.".to_string(),
                "n deals next hand.".to_string(),
                "c, Space, or Enter checks or calls.".to_string(),
                "b or r bets or raises.".to_string(),
                "[/] or -/+ adjusts selected bet/raise amount.".to_string(),
                "a goes all-in.".to_string(),
                "x toggles auto check/fold.".to_string(),
                "f folds; l leaves seat.".to_string(),
            ],
        },
        GuideSection {
            title: "Chess",
            body: vec![
                "Two seats, White and Black. Decisive wins pay 500 chips.".to_string(),
                "Clock presets: blitz, rapid, and 1d/move daily.".to_string(),
                "s sits when not seated.".to_string(),
                "n starts when both players are seated.".to_string(),
                "w/a/s/d or arrows move cursor while seated.".to_string(),
                "Space or Enter selects a piece, then destination.".to_string(),
                "r resigns active game.".to_string(),
                "l leaves seat before or after a game.".to_string(),
            ],
        },
        GuideSection {
            title: "Tron",
            body: vec![
                "Two to four riders. Wins pay 50/75/100 chips by rider count.".to_string(),
                "Speeds: chill, standard, quick.".to_string(),
                "s, Space, or Enter sits when not seated.".to_string(),
                "n starts when at least two riders are seated.".to_string(),
                "w/a/s/d or arrows steer while seated.".to_string(),
                "l leaves seat.".to_string(),
            ],
        },
        GuideSection {
            title: "Tic-Tac-Toe",
            body: vec![
                "Two seats, X and O, no chips.".to_string(),
                "s, Space, or Enter sits when not seated.".to_string(),
                "1-9 places directly.".to_string(),
                "w/a/s/d or arrows move cursor while seated.".to_string(),
                "Space or Enter places on cursor.".to_string(),
                "n starts a new round.".to_string(),
                "l leaves seat and resets board.".to_string(),
            ],
        },
    ]
}

fn render_sections(sections: Vec<GuideSection>) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for section in sections {
        if !lines.is_empty() {
            lines.push(spacer());
        }
        lines.push(section_heading(section.title));
        lines.extend(
            section
                .body
                .into_iter()
                .map(|line| text(&format!("  {line}"))),
        );
    }
    lines
}

fn text(value: &str) -> Line<'static> {
    Line::from(Span::styled(
        value.to_string(),
        Style::default().fg(theme::TEXT_DIM()),
    ))
}

fn spacer() -> Line<'static> {
    Line::from("")
}

fn section_heading(title: &str) -> Line<'static> {
    let dim = Style::default().fg(theme::BORDER());
    let accent = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled("  ── ", dim),
        Span::styled(title.to_string(), accent),
        Span::styled(" ──", dim),
    ])
}
