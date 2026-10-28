use std::collections::HashMap;

use late_core::models::leaderboard::{
    ArcadeGame, DailyCompletionStatus, DailyPuzzle, ScoreGame, Tiers,
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use uuid::Uuid;

use super::solvers::{Roster, TodaySolvers};
use crate::app::{activity::event::ActivityGame, common::theme};

/// Smallest area the Arcade lobby (quest strip + game grid) still fits in.
const LOBBY_MIN_WIDTH: u16 = 50;
const LOBBY_MIN_HEIGHT: u16 = 10;

/// Whether a daily can be opened today. Nonograms ship as an offline pack;
/// without one the card says "Coming Soon" and has no board and no roster.
pub(crate) fn daily_available(
    puzzle: DailyPuzzle,
    nonogram_state: &super::nonogram::state::State,
) -> bool {
    match puzzle {
        DailyPuzzle::Nonogram => nonogram_state.has_puzzles(),
        DailyPuzzle::LeWord
        | DailyPuzzle::RubiksCube
        | DailyPuzzle::SlidingPuzzle
        | DailyPuzzle::Sudoku
        | DailyPuzzle::Minesweeper
        | DailyPuzzle::Solitaire
        | DailyPuzzle::Netwalk => true,
    }
}

/// The chips one daily win pays on a puzzle with one board a day (Le Word,
/// Rubik's Cube), from its tier, for the game's own reward copy.
pub fn single_board_chips(puzzle: DailyPuzzle) -> i64 {
    match puzzle.tiers() {
        Tiers::Single(tier) => tier.chips(),
        Tiers::Keyed(_) => panic!("{puzzle:?} has a board per difficulty, not one"),
    }
}

/// Narrower lobbies drop the `[o who]` hint from the selected card; the
/// count stays, the key is still in the guide.
const SOLVERS_HINT_MIN_WIDTH: u16 = 62;

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

/// The share hint a finished daily adds to its key line: `s` copies the
/// card. Empty while the board is still open.
pub fn share_hints(card_ready: bool) -> Vec<(&'static str, &'static str)> {
    if card_ready {
        vec![("s", "share")]
    } else {
        Vec::new()
    }
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

/// The game behind a lobby selection, for the attention metric.
pub fn game_for_selection(selection: ArcadeGame) -> ActivityGame {
    match selection {
        ArcadeGame::Puzzle(puzzle) => super::daily::activity_game(puzzle),
        ArcadeGame::Score(ScoreGame::TwentyFortyEight) => ActivityGame::TwentyFortyEight,
        ArcadeGame::Score(ScoreGame::Lateris) => ActivityGame::Lateris,
        ArcadeGame::Score(ScoreGame::Snake) => ActivityGame::Snake,
        ArcadeGame::Score(ScoreGame::Traffic) => ActivityGame::Traffic,
    }
}

pub struct ArcadeHubView<'a> {
    pub game_selection: ArcadeGame,
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
    pub netwalk_state: &'a super::netwalk::state::State,
    pub daily_completion: Option<&'a DailyCompletionStatus>,
    /// Dailies this session banked today, ahead of the snapshot above.
    pub session_daily_completion: Option<&'a DailyCompletionStatus>,
    pub quest_state: &'a crate::app::hub::dailies::state::QuestState,
    /// Every user's marks today and their names, for the `N solved` count
    /// and the `o` roster. `session_daily_completion` is OR'd in for the
    /// viewer, as it is for the ✓ marks.
    pub today_statuses: &'a HashMap<Uuid, DailyCompletionStatus>,
    pub today_solver_names: &'a HashMap<Uuid, String>,
    pub user_id: Uuid,
    pub username: &'a str,
    /// The solvers roster is open over the selected daily.
    pub show_solvers: bool,
}

impl ArcadeHubView<'_> {
    fn today_solvers(&self) -> TodaySolvers<'_> {
        TodaySolvers {
            statuses: self.today_statuses,
            usernames: self.today_solver_names,
            own_user_id: self.user_id,
            own_username: self.username,
            own_session: self.session_daily_completion,
        }
    }
}

/// Arcade games always draw their status/keys/tip footer. Mouse hit-testing
/// and the pre-frame raster wipe both run outside the draw path and have to
/// reach the same answer, so it is stated once here instead of being
/// re-decided as a literal at each call site.
pub const SHOW_GAME_BOTTOM_BAR: bool = true;

pub fn draw_arcade_hub(frame: &mut Frame, area: Rect, view: &ArcadeHubView<'_>) {
    let show_bottom_bar = SHOW_GAME_BOTTOM_BAR;
    if view.is_playing_game {
        match view.game_selection {
            ArcadeGame::Puzzle(puzzle) => draw_puzzle(frame, area, view, puzzle, show_bottom_bar),
            ArcadeGame::Score(game) => draw_score_game(frame, area, view, game, show_bottom_bar),
        }
        return;
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

    if view.show_solvers
        && let ArcadeGame::Puzzle(game) = view.game_selection
    {
        let roster = view
            .today_solvers()
            .roster(chrono::Utc::now().date_naive(), game);
        draw_solvers_modal(frame, content_area, game, view.username, &roster);
    }
}

fn draw_puzzle(
    frame: &mut Frame,
    area: Rect,
    view: &ArcadeHubView<'_>,
    puzzle: DailyPuzzle,
    show_bottom_bar: bool,
) {
    match puzzle {
        DailyPuzzle::LeWord => {
            super::le_word::ui::draw_game(frame, area, view.le_word_state, show_bottom_bar)
        }
        DailyPuzzle::RubiksCube => {
            super::rubiks_cube::ui::draw_game(frame, area, view.rubiks_cube_state, show_bottom_bar)
        }
        DailyPuzzle::SlidingPuzzle => super::sliding_puzzle::ui::draw_game(
            frame,
            area,
            view.sliding_puzzle_state,
            show_bottom_bar,
        ),
        DailyPuzzle::Sudoku => {
            super::sudoku::ui::draw_game(frame, area, view.sudoku_state, show_bottom_bar)
        }
        DailyPuzzle::Nonogram => {
            super::nonogram::ui::draw_game(frame, area, view.nonogram_state, show_bottom_bar)
        }
        DailyPuzzle::Minesweeper => {
            super::minesweeper::ui::draw_game(frame, area, view.minesweeper_state, show_bottom_bar)
        }
        DailyPuzzle::Solitaire => {
            super::solitaire::ui::draw_game(frame, area, view.solitaire_state, show_bottom_bar)
        }
        DailyPuzzle::Netwalk => {
            super::netwalk::ui::draw_game(frame, area, view.netwalk_state, show_bottom_bar)
        }
    }
}

fn draw_score_game(
    frame: &mut Frame,
    area: Rect,
    view: &ArcadeHubView<'_>,
    game: ScoreGame,
    show_bottom_bar: bool,
) {
    match game {
        ScoreGame::TwentyFortyEight => super::twenty_forty_eight::ui::draw_game(
            frame,
            area,
            view.twenty_forty_eight_state,
            show_bottom_bar,
        ),
        ScoreGame::Lateris => {
            super::tetris::ui::draw_game(frame, area, view.tetris_state, show_bottom_bar)
        }
        ScoreGame::Snake => {
            super::snake::ui::draw_game(frame, area, view.snake_state, show_bottom_bar)
        }
        ScoreGame::Traffic => {
            super::traffic::ui::draw_game(frame, area, view.traffic_state, show_bottom_bar)
        }
    }
}

/// Everyone who solved `game` today, one block per tier, hardest first.
/// Names flow as a wrapped list in the day's shuffled order; the viewer's
/// own name is lit. Sized to its text, clipped to the area.
fn draw_solvers_modal(
    frame: &mut Frame,
    area: Rect,
    game: DailyPuzzle,
    own_username: &str,
    roster: &Roster,
) {
    const MAX_WIDTH: u16 = 64;
    const MIN_WIDTH: u16 = 36;
    let popup_w = MAX_WIDTH
        .min(area.width.saturating_sub(2))
        .max(MIN_WIDTH.min(area.width));
    let text_w = popup_w.saturating_sub(4) as usize;

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (i, tier) in roster.tiers.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(""));
        }
        let solved = match tier.names.len() {
            0 => "nobody yet".to_string(),
            1 => "1 solved".to_string(),
            n => format!("{n} solved"),
        };
        lines.push(Line::from(vec![
            Span::styled(
                tier.difficulty_key.to_string(),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" · {} chips · ", tier.chips),
                Style::default().fg(theme::TEXT_DIM()),
            ),
            Span::styled(solved, Style::default().fg(theme::TEXT())),
        ]));
        for row in wrap_names(&tier.names, text_w) {
            let mut spans: Vec<Span<'static>> = vec![Span::raw("  ")];
            for (j, name) in row.into_iter().enumerate() {
                if j > 0 {
                    spans.push(Span::styled(", ", Style::default().fg(theme::TEXT_DIM())));
                }
                let style = if name == own_username {
                    Style::default()
                        .fg(theme::SUCCESS())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::TEXT())
                };
                spans.push(Span::styled(name, style));
            }
            lines.push(Line::from(spans));
        }
    }
    lines.push(Line::from(""));
    lines.push(crate::app::common::primitives::hint_line(&[
        ("Esc", "close"),
        ("", "order shuffled daily, nobody is first"),
    ]));

    let popup_h = (lines.len() as u16 + 2).min(area.height);
    let popup = centered_rect(area, popup_w, popup_h);
    let title = format!(" {} · {} solved today ", game.title(), roster.total);
    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(theme::TEXT())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()))
        .style(Style::default().bg(theme::BG_CANVAS()));
    let inner = block.inner(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(block, popup);
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        Rect {
            x: inner.x + 1,
            y: inner.y,
            width: inner.width.saturating_sub(1),
            height: inner.height,
        },
    );
}

/// Greedy rows of names that fit `width` cells joined by `, `, two cells of
/// indent already taken off. A name longer than the row gets a row alone.
fn wrap_names(names: &[String], width: usize) -> Vec<Vec<String>> {
    let width = width.saturating_sub(2).max(1);
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut used = 0usize;
    for name in names {
        let cells = name.chars().count();
        let needed = if row.is_empty() {
            cells
        } else {
            used + 2 + cells
        };
        if !row.is_empty() && needed > width {
            rows.push(std::mem::take(&mut row));
            used = 0;
        }
        used = if row.is_empty() {
            cells
        } else {
            used + 2 + cells
        };
        row.push(name.clone());
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

fn draw_game_list(frame: &mut Frame, area: Rect, view: &ArcadeHubView<'_>) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let selection = view.game_selection;
    let mut selected_line: usize = 0;

    push_game_section(&mut lines, "─── Daily Games ───");
    lines.push(Line::from(""));

    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "Win once per UTC day for chips. Replay for practice and leaderboard.",
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            " s ",
            Style::default()
                .fg(theme::BG_SELECTION())
                .bg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            "Share your day card",
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            ": copies a spoiler-free score for today's dailies. Every finished puzzle has one too.",
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]));
    lines.push(Line::from(""));

    let today = view.today_solvers();
    let show_hint = area.width >= SOLVERS_HINT_MIN_WIDTH;
    for puzzle in DailyPuzzle::ALL.iter().copied() {
        let card = lobby_card(view, puzzle);
        let game = ArcadeGame::Puzzle(puzzle);
        let title_style = Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD);
        let normal_style = if card.available {
            Style::default().fg(theme::TEXT())
        } else {
            Style::default().fg(theme::TEXT_MUTED())
        };
        let desc_style = if card.available {
            Style::default().fg(theme::TEXT_DIM())
        } else {
            Style::default().fg(theme::TEXT_MUTED())
        };
        let status = if card.available {
            card_status_spans(view, &today, puzzle, show_hint && game == selection)
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
                game,
                name: puzzle.title(),
                descriptions: &[card.description],
                selected_style: title_style,
                normal_style,
                description_style: desc_style,
                status,
                label_width: 16,
            },
        );
    }

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

    for game in ScoreGame::ALL.iter().copied() {
        let card = score_card(view, game);
        draw_game_entry(
            &mut lines,
            &mut selected_line,
            selection,
            GameEntry {
                game: ArcadeGame::Score(game),
                name: game.title(),
                descriptions: &[card.description],
                selected_style: Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
                normal_style: Style::default().fg(theme::TEXT()),
                description_style: Style::default().fg(theme::TEXT_DIM()),
                status: vec![Span::styled(card.best, Style::default().fg(theme::SUCCESS()))],
                label_width: 16,
            },
        );
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

/// A daily's lobby card: its line under the title, and whether it can be
/// played today.
struct DailyRow {
    description: &'static str,
    available: bool,
}

fn lobby_card(view: &ArcadeHubView<'_>, puzzle: DailyPuzzle) -> DailyRow {
    let description = match puzzle {
        DailyPuzzle::LeWord => "Guess the daily five-letter word in six tries.",
        DailyPuzzle::RubiksCube => "Solve today's shared scramble through an angled cube view.",
        DailyPuzzle::SlidingPuzzle => "Slide tiles into order by number or image.",
        DailyPuzzle::Sudoku => "Classic newspaper puzzle, rebuilt for the terminal.",
        DailyPuzzle::Nonogram => "Pixel puzzles painted by logic, one clue at a time.",
        DailyPuzzle::Minesweeper => "Flag mines, clear the field. Three lives.",
        DailyPuzzle::Solitaire => "Klondike with daily and personal deals over SSH.",
        DailyPuzzle::Netwalk => "Turn the tiles until one network lights every one.",
    };
    DailyRow {
        description,
        available: daily_available(puzzle, view.nonogram_state),
    }
}

/// A score game's lobby card: its line under the title and its best.
struct ScoreRow {
    description: &'static str,
    best: String,
}

fn score_card(view: &ArcadeHubView<'_>, game: ScoreGame) -> ScoreRow {
    match game {
        ScoreGame::TwentyFortyEight => ScoreRow {
            description: "Slide, merge, and chase the warmest tile.",
            best: format!(
                "Best {}",
                view.twenty_forty_eight_state
                    .best_score
                    .max(view.twenty_forty_eight_state.score)
            ),
        },
        ScoreGame::Lateris => ScoreRow {
            description: "Endless falling blocks. Speed rises as you survive.",
            best: format!("Best {}", view.tetris_state.best_score),
        },
        ScoreGame::Snake => ScoreRow {
            description: "Eat grow and avoid danger. Speed rises as you survive.",
            best: format!("Best {}", view.snake_state.best_score),
        },
        ScoreGame::Traffic => ScoreRow {
            description: "You're LATE and stuck in TRAFFIC. Overtake or crash.",
            best: if view.traffic_state.best_score > 0 {
                format!("Best {:>11}", view.traffic_state.best_score)
            } else {
                "No runs yet".to_string()
            },
        },
    }
}

fn push_game_section(lines: &mut Vec<Line<'static>>, title: &str) {
    lines.push(Line::from(Span::styled(
        title.to_string(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )));
}

/// A daily card's status: the tier marks, then today's solver count and,
/// on the selected card, the key that opens the roster.
fn card_status_spans(
    view: &ArcadeHubView<'_>,
    today: &TodaySolvers<'_>,
    game: DailyPuzzle,
    with_hint: bool,
) -> Vec<Span<'static>> {
    let mut status =
        daily_reward_status_spans(view.daily_completion, view.session_daily_completion, game);
    status.extend(solved_count_spans(today.solved_count(game), with_hint));
    status
}

/// ` · 14 solved`, dim, plus ` [o who]` when this is the selected card.
fn solved_count_spans(solved: usize, with_hint: bool) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled(
        format!(" · {solved} solved"),
        Style::default().fg(theme::TEXT_DIM()),
    )];
    if with_hint {
        spans.push(Span::styled(" [", Style::default().fg(theme::TEXT_DIM())));
        spans.push(Span::styled("o", Style::default().fg(theme::AMBER())));
        spans.push(Span::styled(
            " who]",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    spans
}

/// One `✓chips`/`✗chips` span per tier of `game.tiers()`, easiest first. A
/// tier is done when either the leaderboard snapshot or this session's own
/// wins (`SessionDailyWins`) say so; the snapshot catches up within a
/// refresh, the session mark is instant.
fn daily_reward_status_spans(
    status: Option<&DailyCompletionStatus>,
    session_status: Option<&DailyCompletionStatus>,
    game: DailyPuzzle,
) -> Vec<Span<'static>> {
    let tiers = game.tiers().list();
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(tiers.len() * 2);
    for (i, (difficulty_key, tier)) in tiers.iter().enumerate() {
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
        spans.push(Span::styled(format!("{glyph}{}", tier.chips()), style));
    }
    spans
}

struct GameEntry<'a> {
    game: ArcadeGame,
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
    selection: ArcadeGame,
    entry: GameEntry<'_>,
) {
    let is_selected = entry.game == selection;
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
