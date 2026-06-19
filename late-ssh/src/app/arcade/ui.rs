use late_core::models::leaderboard::{DailyCompletionStatus, DailyGame};
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
        GAME_SELECTION_NES_2048, GAME_SELECTION_NES_BRICK_BREAKER,
        GAME_SELECTION_NES_CONCENTRATION_ROOM, GAME_SELECTION_NES_DABG,
        GAME_SELECTION_NES_ESCAPE_FROM_PONG, GAME_SELECTION_NES_FALLING, GAME_SELECTION_NES_RHDE,
        GAME_SELECTION_NES_SQUIRREL_DOMINO, GAME_SELECTION_NES_THWAITE,
        GAME_SELECTION_NES_ZAP_RUDER, GAME_SELECTION_NONOGRAMS, GAME_SELECTION_RUBIKS_CUBE,
        GAME_SELECTION_SNAKE, GAME_SELECTION_SOLITAIRE, GAME_SELECTION_SUDOKU,
        GAME_SELECTION_TETRIS,
    },
};

type DailyRewardTiers = &'static [(&'static str, i64)];
type DailyRow = (
    usize,
    &'static str,
    &'static str,
    bool,
    DailyGame,
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

pub fn draw_game_overlay(
    frame: &mut Frame,
    area: Rect,
    heading: &str,
    subtitle: &str,
    color: Color,
) {
    let overlay_area = centered_rect(area, 28.min(area.width), 4.min(area.height));
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
        spans.push(Span::raw(" "));
        spans.push(Span::styled(desc, Style::default().fg(theme::TEXT_DIM())));
    }
    Line::from(spans)
}

pub fn game_title(selection: usize) -> &'static str {
    if let Some(rom) = super::input::nes_rom_for_selection(selection) {
        return super::nes_cabinet::state::ROMS[rom].title;
    }

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
    pub le_word_state: &'a super::le_word::state::State,
    pub nes_cabinet_state: &'a super::nes_cabinet::state::State,
    pub sudoku_state: &'a super::sudoku::state::State,
    pub nonogram_state: &'a super::nonogram::state::State,
    pub solitaire_state: &'a super::solitaire::state::State,
    pub minesweeper_state: &'a super::minesweeper::state::State,
    pub daily_completion: Option<&'a DailyCompletionStatus>,
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
        } else if view.game_selection == GAME_SELECTION_RUBIKS_CUBE {
            super::rubiks_cube::ui::draw_game(frame, area, view.rubiks_cube_state, show_bottom_bar);
            return;
        } else if view.game_selection == GAME_SELECTION_LE_WORD {
            super::le_word::ui::draw_game(frame, area, view.le_word_state, show_bottom_bar);
            return;
        } else if super::input::is_nes_selection(view.game_selection) {
            super::nes_cabinet::ui::draw_game(frame, area, view.nes_cabinet_state, show_bottom_bar);
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

    if area.height < 10 || area.width < 50 {
        frame.render_widget(
            Paragraph::new("Terminal too small for The Arcade").alignment(Alignment::Center),
            area,
        );
        return;
    }

    let content_area = area;

    let show_header = content_area.height >= 25;
    let layout = if show_header {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(10), // Header (added 1 for top padding)
                Constraint::Length(1),  // Spacer
                Constraint::Min(0),     // Content
            ])
            .split(content_area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0)])
            .split(content_area)
    };

    if show_header {
        draw_header(frame, layout[0], view.game_selection);
        draw_game_list(frame, layout[2], view);
    } else {
        draw_game_list(frame, layout[0], view);
    }
}

fn draw_header(frame: &mut Frame, area: Rect, selection: usize) {
    let (art, subtitle, subtitle_indent) = match selection {
        GAME_SELECTION_2048 => (
            vec![
                r#"     ██████╗  ██████╗ ██╗  ██╗ █████╗ "#,
                r#"     ╚════██╗██╔═████╗██║  ██║██╔══██╗"#,
                r#"      █████╔╝██║██╔██║███████║╚█████╔╝"#,
                r#"     ██╔═══╝ ████╔╝██║╚════██║██╔══██╗"#,
                r#"     ███████╗╚██████╔╝     ██║╚█████╔╝"#,
                r#"     ╚══════╝ ╚═════╝      ╚═╝ ╚════╝ "#,
            ],
            "Slide, merge, and chase the warmest tile on the board.",
            "     ",
        ),
        GAME_SELECTION_TETRIS => (
            vec![
                r#"     ██╗      █████╗ ████████╗███████╗██████╗ ██╗███████╗"#,
                r#"     ██║     ██╔══██╗╚══██╔══╝██╔════╝██╔══██╗██║██╔════╝"#,
                r#"     ██║     ███████║   ██║   █████╗  ██████╔╝██║███████╗"#,
                r#"     ██║     ██╔══██║   ██║   ██╔══╝  ██╔══██╗██║╚════██║"#,
                r#"     ███████╗██║  ██║   ██║   ███████╗██║  ██║██║███████║"#,
                r#"     ╚══════╝╚═╝  ╚═╝   ╚═╝   ╚══════╝╚═╝  ╚═╝╚═╝╚══════╝"#,
            ],
            "Endless falling blocks. Speed rises as you survive.",
            "     ",
        ),
        GAME_SELECTION_SUDOKU => (
            vec![
                r#"     ███████╗██╗   ██╗██████╗  ██████╗ ██╗  ██╗██╗   ██╗"#,
                r#"     ██╔════╝██║   ██║██╔══██╗██╔═══██╗██║ ██╔╝██║   ██║"#,
                r#"     ███████╗██║   ██║██║  ██║██║   ██║█████╔╝ ██║   ██║"#,
                r#"     ╚════██║██║   ██║██║  ██║██║   ██║██╔═██╗ ██║   ██║"#,
                r#"     ███████║╚██████╔╝██████╔╝╚██████╔╝██║  ██╗╚██████╔╝"#,
                r#"     ╚══════╝ ╚═════╝ ╚═════╝  ╚═════╝ ╚═╝  ╚═╝ ╚═════╝ "#,
            ],
            "Classic newspaper puzzle, rebuilt for the terminal.",
            "     ",
        ),
        GAME_SELECTION_LE_WORD => (
            vec![
                r#"     ██╗     ███████╗    ██╗    ██╗ ██████╗ ██████╗ ██████╗ "#,
                r#"     ██║     ██╔════╝    ██║    ██║██╔═══██╗██╔══██╗██╔══██╗"#,
                r#"     ██║     █████╗      ██║ █╗ ██║██║   ██║██████╔╝██║  ██║"#,
                r#"     ██║     ██╔══╝      ██║███╗██║██║   ██║██╔══██╗██║  ██║"#,
                r#"     ███████╗███████╗    ╚███╔███╔╝╚██████╔╝██║  ██║██████╔╝"#,
                r#"     ╚══════╝╚══════╝     ╚══╝╚══╝  ╚═════╝ ╚═╝  ╚═╝╚═════╝ "#,
            ],
            "Six guesses, one daily word, classic green-yellow-gray clues.",
            "     ",
        ),
        GAME_SELECTION_RUBIKS_CUBE => (
            vec![
                r#"     ██████╗ ██╗   ██╗██████╗ ██╗██╗  ██╗"#,
                r#"     ██╔══██╗██║   ██║██╔══██╗██║██║ ██╔╝"#,
                r#"     ██████╔╝██║   ██║██████╔╝██║█████╔╝ "#,
                r#"     ██╔══██╗██║   ██║██╔══██╗██║██╔═██╗ "#,
                r#"     ██║  ██║╚██████╔╝██████╔╝██║██║  ██╗"#,
                r#"     ╚═╝  ╚═╝ ╚═════╝ ╚═════╝ ╚═╝╚═╝  ╚═╝"#,
            ],
            "Turn a real cube model through three visible sides and a mini net.",
            "     ",
        ),
        GAME_SELECTION_NONOGRAMS => (
            vec![
                r#"     ███╗   ██╗ ██████╗ ███╗   ██╗ ██████╗  ██████╗ ██████╗  █████╗ ███╗   ███╗███████╗"#,
                r#"     ████╗  ██║██╔═══██╗████╗  ██║██╔═══██╗██╔════╝ ██╔══██╗██╔══██╗████╗ ████║██╔════╝"#,
                r#"     ██╔██╗ ██║██║   ██║██╔██╗ ██║██║   ██║██║  ███╗██████╔╝███████║██╔████╔██║███████╗"#,
                r#"     ██║╚██╗██║██║   ██║██║╚██╗██║██║   ██║██║   ██║██╔══██╗██╔══██║██║╚██╔╝██║╚════██║"#,
                r#"     ██║ ╚████║╚██████╔╝██║ ╚████║╚██████╔╝╚██████╔╝██║  ██║██║  ██║██║ ╚═╝ ██║███████║"#,
                r#"     ╚═╝  ╚═══╝ ╚═════╝ ╚═╝  ╚═══╝ ╚═════╝  ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═╝╚═╝     ╚═╝╚══════╝"#,
            ],
            "Pixel puzzles painted by logic, one clue at a time.",
            "     ",
        ),
        GAME_SELECTION_MINESWEEPER => (
            vec![
                r#"     ███╗   ███╗██╗███╗   ██╗███████╗███████╗"#,
                r#"     ████╗ ████║██║████╗  ██║██╔════╝██╔════╝"#,
                r#"     ██╔████╔██║██║██╔██╗ ██║█████╗  ███████╗"#,
                r#"     ██║╚██╔╝██║██║██║╚██╗██║██╔══╝  ╚════██║"#,
                r#"     ██║ ╚═╝ ██║██║██║ ╚████║███████╗███████║"#,
                r#"     ╚═╝     ╚═╝╚═╝╚═╝  ╚═══╝╚══════╝╚══════╝"#,
            ],
            "Flag mines, clear the field. Three lives, no guessing around.",
            "     ",
        ),
        GAME_SELECTION_SOLITAIRE => (
            vec![
                r#"     ███████╗ ██████╗ ██╗     ██╗████████╗ █████╗ ██╗██████╗ ███████╗"#,
                r#"     ██╔════╝██╔═══██╗██║     ██║╚══██╔══╝██╔══██╗██║██╔══██╗██╔════╝"#,
                r#"     ███████╗██║   ██║██║     ██║   ██║   ███████║██║██████╔╝█████╗  "#,
                r#"     ╚════██║██║   ██║██║     ██║   ██║   ██╔══██║██║██╔══██╗██╔══╝  "#,
                r#"     ███████║╚██████╔╝███████╗██║   ██║   ██║  ██║██║██║  ██║███████╗"#,
                r#"     ╚══════╝ ╚═════╝ ╚══════╝╚═╝   ╚═╝   ╚═╝  ╚═╝╚═╝╚═╝  ╚═╝╚══════╝"#,
            ],
            "Classic Klondike, dealt fresh every day.",
            "     ",
        ),
        GAME_SELECTION_SNAKE => (
            vec![
                r#"     ███████╗███╗   ██╗ █████╗ ██╗  ██╗███████╗"#,
                r#"     ██╔════╝████╗  ██║██╔══██╗██║ ██╔╝██╔════╝"#,
                r#"     ███████╗██╔██╗ ██║███████║█████╔╝ █████╗  "#,
                r#"     ╚════██║██║╚██╗██║██╔══██║██╔═██╗ ██╔══╝  "#,
                r#"     ███████║██║ ╚████║██║  ██║██║  ██╗███████╗"#,
                r#"     ╚══════╝╚═╝  ╚═══╝╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝"#,
            ],
            "Classic Snake game, eat, grow and survive!",
            "     ",
        ),
        selection if super::input::is_nes_selection(selection) => (
            vec![
                r#"     ███╗   ██╗███████╗███████╗"#,
                r#"     ████╗  ██║██╔════╝██╔════╝"#,
                r#"     ██╔██╗ ██║█████╗  ███████╗"#,
                r#"     ██║╚██╗██║██╔══╝  ╚════██║"#,
                r#"     ██║ ╚████║███████╗███████║"#,
                r#"     ╚═╝  ╚═══╝╚══════╝╚══════╝"#,
            ],
            "Select a homebrew ROM. Potatis renders the NES frame into the terminal.",
            "     ",
        ),

        _ => (
            vec![
                r#"     ██████╗ ██████╗  ██████╗ █████╗ ██████╗ ███████╗"#,
                r#"    ██╔══██╗██╔══██╗██╔════╝██╔══██╗██╔══██╗██╔════╝"#,
                r#"    ███████║██████╔╝██║     ███████║██║  ██║█████╗  "#,
                r#"    ██╔══██║██╔══██╗██║     ██╔══██║██║  ██║██╔══╝  "#,
                r#"    ██║  ██║██║  ██║╚██████╗██║  ██║██████╔╝███████╗"#,
                r#"    ╚═╝  ╚═╝╚═╝  ╚═╝ ╚═════╝╚═╝  ╚═╝╚═════╝ ╚══════╝"#,
            ],
            "Welcome to the Clubhouse Arcade. Browse with j/k, open with Enter.",
            "     ",
        ),
    };

    let mut header_text = vec![Line::from("")];
    header_text.extend(art.into_iter().map(|line| {
        Line::from(Span::styled(
            line,
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ))
    }));
    header_text.push(Line::from(""));
    header_text.push(Line::from(Span::styled(
        format!("{subtitle_indent}{subtitle}"),
        Style::default().fg(theme::TEXT_DIM()),
    )));

    let paragraph = Paragraph::new(header_text).alignment(Alignment::Left);
    frame.render_widget(paragraph, area);
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
            DailyGame::LeWord,
            &[("daily", 100)],
        ),
        (
            GAME_SELECTION_SUDOKU,
            "Sudoku",
            "Classic newspaper puzzle, rebuilt for the terminal.",
            true,
            DailyGame::Sudoku,
            &[("easy", 100), ("medium", 250), ("hard", 500)],
        ),
        (
            GAME_SELECTION_NONOGRAMS,
            "Nonograms",
            "Pixel puzzles painted by logic, one clue at a time.",
            view.nonogram_state.has_puzzles(),
            DailyGame::Nonogram,
            &[("easy", 100), ("medium", 250), ("hard", 500)],
        ),
        (
            GAME_SELECTION_MINESWEEPER,
            "Minesweeper",
            "Flag mines, clear the field. Three lives.",
            true,
            DailyGame::Minesweeper,
            &[("easy", 100), ("medium", 250), ("hard", 500)],
        ),
        (
            GAME_SELECTION_SOLITAIRE,
            "Solitaire",
            "Klondike with daily and personal deals over SSH.",
            true,
            DailyGame::Solitaire,
            &[("draw-1", 250), ("draw-3", 500)],
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
            daily_reward_status_spans(view.daily_completion, game, tiers)
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
                        DailyGame::RubiksCube,
                        &[("daily", super::rubiks_cube::state::DAILY_WIN_REWARD_CHIPS)],
                    ),
                    label_width: 16,
                },
            );
        }
    }

    push_game_section(&mut lines, "─── NES Cabinet ───");
    lines.push(Line::from(""));

    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "Homebrew ROMs running through Potatis by github.com/henrikpersson/potatis.",
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "For the best experience, press Z and zoom out your terminal font.",
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]));
    lines.push(Line::from(""));

    for (idx, rom, desc) in [
        (
            GAME_SELECTION_NES_SQUIRREL_DOMINO,
            super::nes_cabinet::state::ROM_SQUIRREL_DOMINO,
            "Domino-clearing puzzle duel.",
        ),
        (
            GAME_SELECTION_NES_THWAITE,
            super::nes_cabinet::state::ROM_THWAITE,
            "Missile-defense arcade shooter.",
        ),
        (
            GAME_SELECTION_NES_DABG,
            super::nes_cabinet::state::ROM_DABG,
            "Platform shooter with co-op.",
        ),
        (
            GAME_SELECTION_NES_FALLING,
            super::nes_cabinet::state::ROM_FALLING,
            "Dodge-and-collect score chase.",
        ),
        (
            GAME_SELECTION_NES_BRICK_BREAKER,
            super::nes_cabinet::state::ROM_BRICK_BREAKER,
            "Breakout-style brick smashing.",
        ),
        (
            GAME_SELECTION_NES_ESCAPE_FROM_PONG,
            super::nes_cabinet::state::ROM_ESCAPE_FROM_PONG,
            "Pong-from-the-ball puzzle.",
        ),
        (
            GAME_SELECTION_NES_RHDE,
            super::nes_cabinet::state::ROM_RHDE,
            "Furniture-fight strategy oddity.",
        ),
        (
            GAME_SELECTION_NES_CONCENTRATION_ROOM,
            super::nes_cabinet::state::ROM_CONCENTRATION_ROOM,
            "Memory card game for one or two.",
        ),
        (
            GAME_SELECTION_NES_ZAP_RUDER,
            super::nes_cabinet::state::ROM_ZAP_RUDER,
            "Air-hockey toy with controller fallback.",
        ),
        (
            GAME_SELECTION_NES_2048,
            super::nes_cabinet::state::ROM_2048,
            "Tile-merging puzzle ROM.",
        ),
    ] {
        draw_game_entry(
            &mut lines,
            &mut selected_line,
            selection,
            GameEntry {
                idx,
                name: super::nes_cabinet::state::ROMS[rom].title,
                descriptions: &[desc],
                selected_style: Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
                normal_style: Style::default().fg(theme::TEXT()),
                description_style: Style::default().fg(theme::TEXT_DIM()),
                status: vec![Span::styled("ROM", Style::default().fg(theme::SUCCESS()))],
                label_width: 24,
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

fn push_game_section(lines: &mut Vec<Line<'static>>, title: &str) {
    lines.push(Line::from(Span::styled(
        title.to_string(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )));
}

fn daily_reward_status_spans(
    status: Option<&DailyCompletionStatus>,
    game: DailyGame,
    tiers: &[(&str, i64)],
) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(tiers.len() * 2);
    for (i, (difficulty_key, chips)) in tiers.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }
        let done = status
            .map(|s| s.completed_difficulty(game, difficulty_key))
            .unwrap_or(false);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_rect_centers_inside_larger_area() {
        let area = Rect::new(2, 3, 80, 24);
        let centered = centered_rect(area, 30, 10);

        assert_eq!(centered, Rect::new(27, 10, 30, 10));
    }

    #[test]
    fn centered_rect_clamps_to_available_area() {
        let area = Rect::new(2, 3, 80, 24);
        let centered = centered_rect(area, 100, 40);

        assert_eq!(centered, area);
    }
}
