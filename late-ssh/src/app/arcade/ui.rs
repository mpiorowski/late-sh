use late_core::models::{
    chips::Difficulty,
    leaderboard::{DailyCompletionStatus, DailyPuzzle},
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::{
    common::theme,
    state::{
        GAME_SELECTION_2048, GAME_SELECTION_LE_WORD, GAME_SELECTION_MINESWEEPER,
        GAME_SELECTION_NONOGRAMS, GAME_SELECTION_RUBIKS_CUBE, GAME_SELECTION_SLIDING_PUZZLE,
        GAME_SELECTION_SNAKE, GAME_SELECTION_SOLITAIRE, GAME_SELECTION_SUDOKU,
        GAME_SELECTION_TETRIS, GAME_SELECTION_TRAFFIC,
    },
};

type DailyRewardTiers = &'static [(&'static str, i64)];

/// Smallest area the Arcade lobby (quest strip + game grid) still fits in.
const LOBBY_MIN_WIDTH: u16 = 50;
const LOBBY_MIN_HEIGHT: u16 = 10;

/// The three-tier games pay [`Difficulty::chips`] per tier. Solitaire's draw
/// modes are not difficulties, but pay the medium and hard chip amounts; the
/// mapping lives here per the `Difficulty` doc.
const TIERED_REWARDS: DailyRewardTiers = &[
    (Difficulty::Easy.key(), Difficulty::Easy.chips()),
    (Difficulty::Medium.key(), Difficulty::Medium.chips()),
    (Difficulty::Hard.key(), Difficulty::Hard.chips()),
];
const SOLITAIRE_REWARDS: DailyRewardTiers = &[
    ("draw-1", Difficulty::Medium.chips()),
    ("draw-3", Difficulty::Hard.chips()),
];
type DailyRow = (
    usize,
    &'static str,
    &'static str,
    bool,
    DailyPuzzle,
    DailyRewardTiers,
);

// ── Arcade game frame ─────────────────────────────────────────

pub struct GameBottomBar {
    pub status: Line<'static>,
    pub keys: Line<'static>,
    pub tip: Option<Line<'static>>,
}

pub fn draw_game_frame(
    frame: &mut Frame,
    area: Rect,
    _title: &str,
    bottom: GameBottomBar,
    show_bottom_bar: bool,
) -> Rect {
    let bottom_has_tip = bottom.tip.is_some();
    let content_area = game_content_area(area, bottom_has_tip, show_bottom_bar);
    if content_area == area {
        return area;
    }

    let mut constraints = vec![
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ];
    if bottom_has_tip {
        constraints.push(Constraint::Length(1));
    }
    let rows = Layout::vertical(constraints).split(area);

    frame.render_widget(
        Paragraph::new(bottom.status).alignment(Alignment::Center),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(bottom.keys).alignment(Alignment::Center),
        rows[2],
    );
    if let Some(tip) = bottom.tip {
        frame.render_widget(Paragraph::new(tip).alignment(Alignment::Center), rows[3]);
    }

    rows[0]
}

pub fn game_content_area(area: Rect, bottom_has_tip: bool, show_bottom_bar: bool) -> Rect {
    let bottom_rows: u16 = if bottom_has_tip { 3 } else { 2 };
    if !show_bottom_bar || area.height < bottom_rows + 3 {
        return area;
    }

    let mut constraints = vec![
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ];
    if bottom_has_tip {
        constraints.push(Constraint::Length(1));
    }

    Layout::vertical(constraints).split(area)[0]
}

pub fn tip_line(text: impl Into<String>) -> Line<'static> {
    Line::from(Span::styled(
        text.into(),
        Style::default()
            .fg(theme::TEXT_MUTED())
            .add_modifier(Modifier::ITALIC),
    ))
}

/// Where a game-over/solved overlay sits within the board area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayAnchor {
    /// Dead center of the board (default for most games).
    Center,
    /// Pinned near the top, so the finished board below stays visible (used by
    /// puzzles where seeing the solved grid is the payoff).
    Top,
}

pub fn draw_game_overlay(
    frame: &mut Frame,
    area: Rect,
    heading: &str,
    subtitle: &str,
    color: Color,
) {
    draw_game_overlay_anchored(frame, area, heading, subtitle, color, OverlayAnchor::Center);
}

fn overlay_size(heading: &str, subtitle: &str, area: Rect) -> (u16, u16) {
    const BORDER_LINES: u16 = 2;
    const MIN_WIDTH: u16 = 28;
    const MAX_WIDTH: u16 = 44;

    let heading_cols = (heading.chars().count() as u16).saturating_add(2);
    let subtitle_cols = subtitle.chars().count() as u16;
    let overlay_w = heading_cols
        .max(subtitle_cols)
        .saturating_add(BORDER_LINES)
        .clamp(MIN_WIDTH, MAX_WIDTH)
        .min(area.width);

    let text_cols = overlay_w.saturating_sub(BORDER_LINES).max(1);
    let subtitle_lines = subtitle_cols.div_ceil(text_cols).max(1);
    let overlay_h = (BORDER_LINES + 1 + subtitle_lines).min(area.height);

    (overlay_w, overlay_h)
}

pub fn draw_game_overlay_anchored(
    frame: &mut Frame,
    area: Rect,
    heading: &str,
    subtitle: &str,
    color: Color,
    anchor: OverlayAnchor,
) {
    let (overlay_w, overlay_h) = overlay_size(heading, subtitle, area);
    let overlay_area = match anchor {
        OverlayAnchor::Center => centered_rect(area, overlay_w, overlay_h),
        OverlayAnchor::Top => {
            let x = area.x + area.width.saturating_sub(overlay_w) / 2;
            let y = area.y + 1.min(area.height.saturating_sub(overlay_h));
            Rect::new(x, y, overlay_w, overlay_h)
        }
    };
    let overlay = Paragraph::new(vec![
        Line::from(Span::styled(
            format!(" {heading} "),
            Style::default()
                .bg(color)
                .fg(ratatui::style::Color::Reset)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            subtitle.to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        )),
    ])
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: false })
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color)),
    );
    frame.render_widget(Clear, overlay_area);
    frame.render_widget(overlay, overlay_area);
}

pub fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

pub fn status_line(segments: Vec<(&'static str, String, Color)>) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (i, (label, value, color)) in segments.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(theme::AMBER_DIM())));
        }
        spans.push(Span::styled(
            format!("{label} "),
            Style::default().fg(theme::TEXT_DIM()),
        ));
        spans.push(Span::styled(
            value,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}

pub fn keys_line(hints: Vec<(&'static str, &'static str)>) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (i, (key, desc)) in hints.into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(theme::AMBER_DIM())));
        }
        spans.push(Span::styled(key, Style::default().fg(theme::AMBER())));
        if !desc.is_empty() {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(desc, Style::default().fg(theme::TEXT_DIM())));
        }
    }
    Line::from(spans)
}

pub fn game_title(selection: usize) -> &'static str {
    match selection {
        GAME_SELECTION_2048 => "2048",
        GAME_SELECTION_TETRIS => "Lateris",
        GAME_SELECTION_LE_WORD => "Le Word",
        GAME_SELECTION_SUDOKU => "Sudoku",
        GAME_SELECTION_NONOGRAMS => "Nonograms",
        GAME_SELECTION_MINESWEEPER => "Minesweeper",
        GAME_SELECTION_SOLITAIRE => "Solitaire",
        GAME_SELECTION_SNAKE => "Snake",
        GAME_SELECTION_RUBIKS_CUBE => "Rubik's Cube",
        GAME_SELECTION_SLIDING_PUZZLE => "Sliding Puzzle",
        GAME_SELECTION_TRAFFIC => "Traffic",
        _ => "The Arcade",
    }
}

pub struct ArcadeHubView<'a> {
    pub game_selection: usize,
    pub is_playing_game: bool,
    pub twenty_forty_eight_state: &'a super::twenty_forty_eight::state::State,
    pub tetris_state: &'a super::tetris::state::State,
    pub snake_state: &'a super::snake::state::State,
    pub rubiks_cube_state: &'a super::rubiks_cube::state::State,
    pub sliding_puzzle_state: &'a super::sliding_puzzle::state::State,
    pub le_word_state: &'a super::le_word::state::State,
    pub traffic_state: &'a super::traffic::state::State,
    pub sudoku_state: &'a super::sudoku::state::State,
    pub nonogram_state: &'a super::nonogram::state::State,
    pub solitaire_state: &'a super::solitaire::state::State,
    pub minesweeper_state: &'a super::minesweeper::state::State,
    pub daily_completion: Option<&'a DailyCompletionStatus>,
    /// Dailies this session banked today, ahead of the snapshot above.
    pub session_daily_completion: Option<&'a DailyCompletionStatus>,
    pub quest_state: &'a crate::app::hub::dailies::state::QuestState,
}

pub fn draw_arcade_hub(frame: &mut Frame, area: Rect, view: &ArcadeHubView<'_>) {
    let show_bottom_bar = true;
    if view.is_playing_game {
        if view.game_selection == GAME_SELECTION_2048 {
            super::twenty_forty_eight::ui::draw_game(
                frame,
                area,
                view.twenty_forty_eight_state,
                show_bottom_bar,
            );
            return;
        } else if view.game_selection == GAME_SELECTION_TETRIS {
            super::tetris::ui::draw_game(frame, area, view.tetris_state, show_bottom_bar);
            return;
        } else if view.game_selection == GAME_SELECTION_SNAKE {
            super::snake::ui::draw_game(frame, area, view.snake_state, show_bottom_bar);
            return;
        } else if view.game_selection == GAME_SELECTION_TRAFFIC {
            super::traffic::ui::draw_game(frame, area, view.traffic_state, show_bottom_bar);
            return;
        } else if view.game_selection == GAME_SELECTION_RUBIKS_CUBE {
            super::rubiks_cube::ui::draw_game(frame, area, view.rubiks_cube_state, show_bottom_bar);
            return;
        } else if view.game_selection == GAME_SELECTION_SLIDING_PUZZLE {
            super::sliding_puzzle::ui::draw_game(
                frame,
                area,
                view.sliding_puzzle_state,
                show_bottom_bar,
            );
            return;
        } else if view.game_selection == GAME_SELECTION_LE_WORD {
            super::le_word::ui::draw_game(frame, area, view.le_word_state, show_bottom_bar);
            return;
        } else if view.game_selection == GAME_SELECTION_SUDOKU {
            super::sudoku::ui::draw_game(frame, area, view.sudoku_state, show_bottom_bar);
            return;
        } else if view.game_selection == GAME_SELECTION_NONOGRAMS {
            super::nonogram::ui::draw_game(frame, area, view.nonogram_state, show_bottom_bar);
            return;
        } else if view.game_selection == GAME_SELECTION_MINESWEEPER {
            super::minesweeper::ui::draw_game(frame, area, view.minesweeper_state, show_bottom_bar);
            return;
        } else if view.game_selection == GAME_SELECTION_SOLITAIRE {
            super::solitaire::ui::draw_game(frame, area, view.solitaire_state, show_bottom_bar);
            return;
        }
    }

    if area.height < LOBBY_MIN_HEIGHT || area.width < LOBBY_MIN_WIDTH {
        crate::app::common::primitives::draw_too_small(
            frame,
            area,
            "The Arcade",
            LOBBY_MIN_WIDTH,
            LOBBY_MIN_HEIGHT,
        );
        return;
    }

    // Quests live at the top of the lobby, not in a modal: they are all
    // arcade quests, so they belong where the games are launched. Short
    // terminals drop the strip before the game list loses room.
    let strip_height = crate::app::hub::dailies::ui::arcade_strip_height(view.quest_state);
    let content_area = if area.height >= strip_height + 13 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(strip_height), Constraint::Min(0)])
            .split(area);
        crate::app::hub::dailies::ui::draw_arcade_strip(frame, rows[0], view.quest_state);
        rows[1]
    } else {
        area
    };

    // No banner art above the list: with the quest strip on top, every row
    // belongs to the games, and small terminals never had the headroom.
    draw_game_list(frame, content_area, view);
}

fn draw_game_list(frame: &mut Frame, area: Rect, view: &ArcadeHubView<'_>) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let selection = view.game_selection;
    let mut selected_line: usize = 0;

    push_game_section(&mut lines, "─── Score Games ───");
    lines.push(Line::from(""));

    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "Chase personal bests and monthly leaderboard spots.",
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]));
    lines.push(Line::from(""));

    for (idx, name, desc, status) in [
        (
            GAME_SELECTION_2048,
            "2048",
            "Slide, merge, and chase the warmest tile.",
            format!(
                "Best {}",
                view.twenty_forty_eight_state
                    .best_score
                    .max(view.twenty_forty_eight_state.score)
            ),
        ),
        (
            GAME_SELECTION_TETRIS,
            "Lateris",
            "Endless falling blocks. Speed rises as you survive.",
            format!("Best {}", view.tetris_state.best_score),
        ),
        (
            GAME_SELECTION_SNAKE,
            "Snake",
            "Eat grow and avoid danger. Speed rises as you survive.",
            format!("Best {}", view.snake_state.best_score),
        ),
        (
            GAME_SELECTION_TRAFFIC,
            "Traffic",
            "You're LATE and stuck in TRAFFIC. Overtake or crash.",
            if view.traffic_state.best_score > 0 {
                format!("Best {:>11}", view.traffic_state.best_score)
            } else {
                "No runs yet".to_string()
            },
        ),
    ] {
        draw_game_entry(
            &mut lines,
            &mut selected_line,
            selection,
            GameEntry {
                idx,
                name,
                descriptions: &[desc],
                selected_style: Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
                normal_style: Style::default().fg(theme::TEXT()),
                description_style: Style::default().fg(theme::TEXT_DIM()),
                status: vec![Span::styled(status, Style::default().fg(theme::SUCCESS()))],
                label_width: 16,
            },
        );
    }

    push_game_section(&mut lines, "─── Daily Games ───");
    lines.push(Line::from(""));

    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "Win once per UTC day for chips. Replay for practice and leaderboard.",
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]));
    lines.push(Line::from(""));

    let daily_rows: [DailyRow; 5] = [
        (
            GAME_SELECTION_LE_WORD,
            "Le Word",
            "Guess the daily five-letter word in six tries.",
            true,
            DailyPuzzle::LeWord,
            &[("daily", super::le_word::state::DAILY_WIN_REWARD_CHIPS)],
        ),
        (
            GAME_SELECTION_SUDOKU,
            "Sudoku",
            "Classic newspaper puzzle, rebuilt for the terminal.",
            true,
            DailyPuzzle::Sudoku,
            TIERED_REWARDS,
        ),
        (
            GAME_SELECTION_NONOGRAMS,
            "Nonograms",
            "Pixel puzzles painted by logic, one clue at a time.",
            view.nonogram_state.has_puzzles(),
            DailyPuzzle::Nonogram,
            TIERED_REWARDS,
        ),
        (
            GAME_SELECTION_MINESWEEPER,
            "Minesweeper",
            "Flag mines, clear the field. Three lives.",
            true,
            DailyPuzzle::Minesweeper,
            TIERED_REWARDS,
        ),
        (
            GAME_SELECTION_SOLITAIRE,
            "Solitaire",
            "Klondike with daily and personal deals over SSH.",
            true,
            DailyPuzzle::Solitaire,
            SOLITAIRE_REWARDS,
        ),
    ];

    for (idx, name, desc, available, game, tiers) in daily_rows {
        let title_style = Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD);
        let normal_style = if available {
            Style::default().fg(theme::TEXT())
        } else {
            Style::default().fg(theme::TEXT_MUTED())
        };
        let desc_style = if available {
            Style::default().fg(theme::TEXT_DIM())
        } else {
            Style::default().fg(theme::TEXT_MUTED())
        };
        let status = if available {
            daily_reward_status_spans(
                view.daily_completion,
                view.session_daily_completion,
                game,
                tiers,
            )
        } else {
            vec![Span::styled(
                "Coming Soon",
                Style::default().fg(theme::TEXT_DIM()),
            )]
        };

        draw_game_entry(
            &mut lines,
            &mut selected_line,
            selection,
            GameEntry {
                idx,
                name,
                descriptions: &[desc],
                selected_style: title_style,
                normal_style,
                description_style: desc_style,
                status,
                label_width: 16,
            },
        );

        if idx == GAME_SELECTION_LE_WORD {
            draw_game_entry(
                &mut lines,
                &mut selected_line,
                selection,
                GameEntry {
                    idx: GAME_SELECTION_RUBIKS_CUBE,
                    name: "Rubik's Cube",
                    descriptions: &["Solve today's shared scramble through an angled cube view."],
                    selected_style: Style::default()
                        .fg(theme::TEXT_BRIGHT())
                        .add_modifier(Modifier::BOLD),
                    normal_style: Style::default().fg(theme::TEXT()),
                    description_style: Style::default().fg(theme::TEXT_DIM()),
                    status: daily_reward_status_spans(
                        view.daily_completion,
                        view.session_daily_completion,
                        DailyPuzzle::RubiksCube,
                        &[("daily", super::rubiks_cube::state::DAILY_WIN_REWARD_CHIPS)],
                    ),
                    label_width: 16,
                },
            );
            draw_game_entry(
                &mut lines,
                &mut selected_line,
                selection,
                GameEntry {
                    idx: GAME_SELECTION_SLIDING_PUZZLE,
                    name: "Sliding Puzzle",
                    descriptions: &["Slide numbered tiles into order."],
                    selected_style: Style::default()
                        .fg(theme::TEXT_BRIGHT())
                        .add_modifier(Modifier::BOLD),
                    normal_style: Style::default().fg(theme::TEXT()),
                    description_style: Style::default().fg(theme::TEXT_DIM()),
                    status: daily_reward_status_spans(
                        view.daily_completion,
                        view.session_daily_completion,
                        DailyPuzzle::SlidingPuzzle,
                        TIERED_REWARDS,
                    ),
                    label_width: 16,
                },
            );
        }
    }

    // Scroll so the selected game stays at the vertical center of the viewport.
    // No scrolling until the selection passes the midpoint.
    let visible = area.height as usize;
    let third = visible / 3;
    let scroll_y = if visible >= lines.len() {
        0
    } else {
        selected_line
            .saturating_sub(third)
            .min(lines.len().saturating_sub(visible))
    };

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll_y as u16, 0));

    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(4), // Left padding
            Constraint::Min(0),
        ])
        .split(area);

    frame.render_widget(paragraph, layout[1]);
}

fn push_game_section(lines: &mut Vec<Line<'static>>, title: &str) {
    lines.push(Line::from(Span::styled(
        title.to_string(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )));
}

/// One `✓chips`/`✗chips` span per tier. A tier is done when either the
/// leaderboard snapshot or this session's own wins (`SessionDailyWins`) say
/// so; the snapshot catches up within a refresh, the session mark is instant.
fn daily_reward_status_spans(
    status: Option<&DailyCompletionStatus>,
    session_status: Option<&DailyCompletionStatus>,
    game: DailyPuzzle,
    tiers: &[(&str, i64)],
) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(tiers.len() * 2);
    for (i, (difficulty_key, chips)) in tiers.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }
        let done = [status, session_status]
            .into_iter()
            .flatten()
            .any(|s| s.completed_difficulty(game, difficulty_key));
        let (glyph, style) = if done {
            ("✓", Style::default().fg(theme::SUCCESS()))
        } else {
            ("✗", Style::default().fg(theme::TEXT_DIM()))
        };
        spans.push(Span::styled(format!("{glyph}{chips}"), style));
    }
    spans
}

struct GameEntry<'a> {
    idx: usize,
    name: &'a str,
    descriptions: &'a [&'a str],
    selected_style: Style,
    normal_style: Style,
    description_style: Style,
    status: Vec<Span<'static>>,
    label_width: usize,
}

fn draw_game_entry(
    lines: &mut Vec<Line<'static>>,
    selected_line: &mut usize,
    selection: usize,
    entry: GameEntry<'_>,
) {
    let is_selected = entry.idx == selection;
    if is_selected {
        *selected_line = lines.len();
    }

    let title_style = if is_selected {
        entry.selected_style
    } else {
        entry.normal_style
    };
    let mut title_line = vec![
        Span::styled(if is_selected { "> " } else { "  " }, title_style),
        Span::styled(format!("[ {} ]", entry.name), title_style),
    ];
    let padding_len = entry.label_width.saturating_sub(entry.name.len() + 4);
    title_line.push(Span::raw(" ".repeat(padding_len.max(1))));
    title_line.extend(entry.status);
    lines.push(Line::from(title_line));

    for description in entry.descriptions {
        lines.push(Line::from(vec![
            Span::raw("      "),
            Span::styled((*description).to_string(), entry.description_style),
        ]));
    }
    lines.push(Line::from(""));
}
