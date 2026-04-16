use std::sync::Arc;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::common::theme;
use late_core::models::leaderboard::{BadgeTier, LeaderboardData};

// ── Shared game frame ──────────────────────────────────────────

pub fn draw_game_frame<'a>(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    info_lines: Vec<Line<'a>>,
) -> Rect {
    let block = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let layout = Layout::horizontal([Constraint::Fill(1), Constraint::Length(28)]).split(inner);

    let info_block = Block::default()
        .title(" Info ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let info_inner = info_block.inner(layout[1]);
    frame.render_widget(info_block, layout[1]);
    frame.render_widget(Paragraph::new(info_lines), info_inner);

    layout[0]
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

pub fn info_label_value<'a>(label: &'a str, value: String, color: Color) -> Line<'a> {
    Line::from(vec![
        Span::styled(
            format!("{:<11}", label),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled(
            value,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ])
}

pub fn key_hint(key: &str, desc: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{:<12}", key),
            Style::default().fg(theme::AMBER_DIM()),
        ),
        Span::styled(desc.to_string(), Style::default().fg(theme::TEXT_DIM())),
    ])
}

pub fn info_tagline(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default()
            .fg(theme::TEXT_MUTED())
            .add_modifier(Modifier::ITALIC),
    ))
}

pub struct GamesHubView<'a> {
    pub game_selection: usize,
    pub is_playing_game: bool,
    pub twenty_forty_eight_state: &'a super::twenty_forty_eight::state::State,
    pub tetris_state: &'a super::tetris::state::State,
    pub sudoku_state: &'a super::sudoku::state::State,
    pub nonogram_state: &'a super::nonogram::state::State,
    pub solitaire_state: &'a super::solitaire::state::State,
    pub minesweeper_state: &'a super::minesweeper::state::State,
    pub blackjack_state: &'a super::blackjack::state::State,
    pub is_admin: bool,
    pub leaderboard: &'a Arc<LeaderboardData>,
}

pub fn draw_games_hub(frame: &mut Frame, area: Rect, view: &GamesHubView<'_>) {
    if view.is_playing_game {
        if view.game_selection == 0 {
            super::twenty_forty_eight::ui::draw_game(frame, area, view.twenty_forty_eight_state);
            return;
        } else if view.game_selection == 1 {
            super::tetris::ui::draw_game(frame, area, view.tetris_state);
            return;
        } else if view.game_selection == 2 {
            super::sudoku::ui::draw_game(frame, area, view.sudoku_state);
            return;
        } else if view.game_selection == 3 {
            super::nonogram::ui::draw_game(frame, area, view.nonogram_state);
            return;
        } else if view.game_selection == 4 {
            super::minesweeper::ui::draw_game(frame, area, view.minesweeper_state);
            return;
        } else if view.game_selection == 5 {
            super::solitaire::ui::draw_game(frame, area, view.solitaire_state);
            return;
        } else if view.game_selection == 6 && view.is_admin {
            super::blackjack::ui::draw_game(frame, area, view.blackjack_state);
            return;
        }
    }

    let block = Block::default()
        .title(" The Arcade ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 10 || inner.width < 50 {
        frame.render_widget(
            Paragraph::new("Terminal too small for The Arcade").alignment(Alignment::Center),
            inner,
        );
        return;
    }

    // Two-column layout: game list (left) + leaderboard (right)
    let cols = Layout::horizontal([Constraint::Fill(1), Constraint::Length(28)]).split(inner);

    let show_header = cols[0].height >= 25;
    let layout = if show_header {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(9), // Header (added 1 for top padding)
                Constraint::Length(1), // Spacer
                Constraint::Min(0),    // Content
            ])
            .split(cols[0])
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0)])
            .split(cols[0])
    };

    if show_header {
        draw_header(frame, layout[0], view.game_selection);
        draw_game_list(frame, layout[2], view);
    } else {
        draw_game_list(frame, layout[0], view);
    }
    draw_leaderboard_sidebar(frame, cols[1], view.leaderboard);
}

fn draw_header(frame: &mut Frame, area: Rect, selection: usize) {
    let (art, subtitle, subtitle_indent) = match selection {
        0 => (
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
        1 => (
            vec![
                r#"     ████████╗███████╗████████╗██████╗ ██╗███████╗"#,
                r#"     ╚══██╔══╝██╔════╝╚══██╔══╝██╔══██╗██║██╔════╝"#,
                r#"        ██║   █████╗     ██║   ██████╔╝██║███████╗"#,
                r#"        ██║   ██╔══╝     ██║   ██╔══██╗██║╚════██║"#,
                r#"        ██║   ███████╗   ██║   ██║  ██║██║███████║"#,
                r#"        ╚═╝   ╚══════╝   ╚═╝   ╚═╝  ╚═╝╚═╝╚══════╝"#,
            ],
            "Endless falling blocks. Speed rises as you survive.",
            "     ",
        ),
        2 => (
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
        3 => (
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
        4 => (
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
        5 => (
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
        6 => (
            vec![
                r#"     ██████╗ ██╗      █████╗  ██████╗██╗  ██╗     ██╗ █████╗  ██████╗██╗  ██╗"#,
                r#"     ██╔══██╗██║     ██╔══██╗██╔════╝██║ ██╔╝     ██║██╔══██╗██╔════╝██║ ██╔╝"#,
                r#"     ██████╔╝██║     ███████║██║     █████╔╝      ██║███████║██║     █████╔╝ "#,
                r#"     ██╔══██╗██║     ██╔══██║██║     ██╔═██╗ ██   ██║██╔══██║██║     ██╔═██╗ "#,
                r#"     ██████╔╝███████╗██║  ██║╚██████╗██║  ██╗╚█████╔╝██║  ██║╚██████╗██║  ██╗"#,
                r#"     ╚═════╝ ╚══════╝╚═╝  ╚═╝ ╚═════╝╚═╝  ╚═╝ ╚════╝ ╚═╝  ╚═╝ ╚═════╝╚═╝  ╚═╝"#,
            ],
            "Hit or stand against the house. Chips on the line every hand.",
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

fn draw_game_list(frame: &mut Frame, area: Rect, view: &GamesHubView<'_>) {
    let mut lines = Vec::new();
    let selection = view.game_selection;
    let mut selected_line: usize = 0;

    // ── High Score Games ──
    lines.push(Line::from(Span::styled(
        "─── High Score Games ───",
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    for (idx, name, desc, available, status) in [
        (
            0,
            "2048",
            "Slide, merge, and chase the warmest tile.",
            true,
            Some(format!(
                "Best {}",
                view.twenty_forty_eight_state
                    .best_score
                    .max(view.twenty_forty_eight_state.score)
            )),
        ),
        (
            1,
            "Tetris",
            "Endless falling blocks. Speed rises as you survive.",
            true,
            Some(format!("Best {}", view.tetris_state.best_score)),
        ),
    ] {
        let is_selected = idx == selection;
        if is_selected {
            selected_line = lines.len();
        }

        let marker = if is_selected { "> " } else { "  " };
        let style = if is_selected {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD)
        } else if available {
            Style::default().fg(theme::TEXT())
        } else {
            Style::default().fg(theme::TEXT_MUTED())
        };

        let mut title_line = vec![
            Span::styled(marker, style),
            Span::styled(format!("[ {} ]", name), style),
        ];

        let padding_len = 16_usize.saturating_sub(name.len() + 4);
        title_line.push(Span::raw(" ".repeat(padding_len)));

        if available {
            if let Some(status) = status {
                title_line.push(Span::styled(status, Style::default().fg(theme::SUCCESS())));
            }
        } else {
            title_line.push(Span::styled(
                "Coming Soon",
                Style::default().fg(theme::TEXT_DIM()),
            ));
        }

        lines.push(Line::from(title_line));
        lines.push(Line::from(vec![
            Span::raw("      "),
            Span::styled(desc, Style::default().fg(theme::TEXT_DIM())),
        ]));
        lines.push(Line::from(""));
    }

    // ── Daily Games ──
    lines.push(Line::from(Span::styled(
        "─── Daily Games ───",
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "Daily runs, personal retries, streaks, and leaderboards.",
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]));
    lines.push(Line::from(""));

    for (idx, name, desc, available, status) in [
        (
            2,
            "Sudoku",
            "Classic newspaper puzzle, rebuilt for the terminal.",
            true,
            match view.sudoku_state.mode {
                super::sudoku::state::Mode::Daily => {
                    format!("Daily {}", view.sudoku_state.difficulty_key())
                }
                super::sudoku::state::Mode::Personal => {
                    format!("Personal {}", view.sudoku_state.difficulty_key())
                }
            },
        ),
        (
            3,
            "Nonograms",
            "Pixel puzzles painted by logic, one clue at a time.",
            view.nonogram_state.has_puzzles(),
            {
                let size_key = view
                    .nonogram_state
                    .selected_pack()
                    .map(|p| p.size_key.as_str())
                    .unwrap_or("unknown");
                match view.nonogram_state.mode {
                    super::nonogram::state::Mode::Daily => format!("Daily {}", size_key),
                    super::nonogram::state::Mode::Personal => format!("Personal {}", size_key),
                }
            },
        ),
        (
            4,
            "Minesweeper",
            "Flag mines, clear the field. Three lives.",
            true,
            match view.minesweeper_state.mode {
                super::minesweeper::state::Mode::Daily => {
                    format!("Daily {}", view.minesweeper_state.difficulty_key())
                }
                super::minesweeper::state::Mode::Personal => {
                    format!("Personal {}", view.minesweeper_state.difficulty_key())
                }
            },
        ),
        (
            5,
            "Solitaire",
            "Klondike with daily and personal deals over SSH.",
            true,
            match view.solitaire_state.mode {
                super::solitaire::state::Mode::Daily => {
                    format!("Daily {}", view.solitaire_state.difficulty_key())
                }
                super::solitaire::state::Mode::Personal => {
                    format!("Personal {}", view.solitaire_state.difficulty_key())
                }
            },
        ),
    ] {
        let is_selected = idx == selection;
        if is_selected {
            selected_line = lines.len();
        }

        let marker = if is_selected { "> " } else { "  " };
        let style = if is_selected {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD)
        } else if available {
            Style::default().fg(theme::TEXT())
        } else {
            Style::default().fg(theme::TEXT_MUTED())
        };

        let mut title_line = vec![
            Span::styled(marker, style),
            Span::styled(format!("[ {} ]", name), style),
        ];

        let padding_len = 16_usize.saturating_sub(name.len() + 4);
        title_line.push(Span::raw(" ".repeat(padding_len)));

        if available {
            title_line.push(Span::styled(status, Style::default().fg(theme::SUCCESS())));
        } else {
            title_line.push(Span::styled(
                "Coming Soon",
                Style::default().fg(theme::TEXT_DIM()),
            ));
        }

        lines.push(Line::from(title_line));
        lines.push(Line::from(vec![
            Span::raw("      "),
            Span::styled(desc, Style::default().fg(theme::TEXT_DIM())),
        ]));
        lines.push(Line::from(""));
    }

    // ── Multiplayer ──
    lines.push(Line::from(Span::styled(
        "─── Multiplayer ───",
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    for (idx, name, desc, available, status) in [
        (
            6,
            "Blackjack",
            "Hit or stand against the house. Single-player chips table.",
            view.is_admin,
            if view.is_admin {
                Some(format!("Balance {}", view.blackjack_state.balance))
            } else {
                None
            },
        ),
        (
            7,
            "Texas Hold'em",
            "The ultimate late-night poker table.",
            false,
            None,
        ),
        (
            8,
            "Bridge",
            "Classic trick-taking for four. Deep strategy, cozy pace.",
            false,
            None,
        ),
        (
            9,
            "Thousand",
            "Polish card classic. Bid, meld, and outsmart your rivals.",
            false,
            None,
        ),
        (
            10,
            "Chess",
            "Async correspondence chess. Move at your own pace.",
            false,
            None,
        ),
        (
            11,
            "Battleship",
            "Fire a shot, check back tomorrow.",
            false,
            None,
        ),
        (
            12,
            "Tron",
            "Real-time lightbike arena. Last one standing wins.",
            false,
            None,
        ),
    ] {
        if available {
            let is_selected = idx == selection;
            if is_selected {
                selected_line = lines.len();
            }
            let marker = if is_selected { "> " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT())
            };
            let mut title_line = vec![
                Span::styled(marker, style),
                Span::styled(format!("[ {} ]", name), style),
            ];
            let padding_len = 16_usize.saturating_sub(name.len() + 4);
            title_line.push(Span::raw(" ".repeat(padding_len)));
            if let Some(status) = status {
                title_line.push(Span::styled(status, Style::default().fg(theme::SUCCESS())));
            }
            lines.push(Line::from(title_line));
            lines.push(Line::from(vec![
                Span::raw("      "),
                Span::styled(desc, Style::default().fg(theme::TEXT_DIM())),
            ]));
            lines.push(Line::from(""));
        } else if idx == 6 {
            let is_selected = idx == selection;
            if is_selected {
                selected_line = lines.len();
            }
            let padding_len = 16_usize.saturating_sub(name.len() + 4);
            lines.push(Line::from(vec![
                Span::styled(
                    if is_selected { "> " } else { "  " },
                    Style::default().fg(theme::TEXT_MUTED()),
                ),
                Span::styled(
                    format!("[ {} ]", name),
                    Style::default().fg(theme::TEXT_MUTED()),
                ),
                Span::raw(" ".repeat(padding_len)),
                Span::styled("Admin Only", Style::default().fg(theme::TEXT_DIM())),
            ]));
            lines.push(Line::from(vec![
                Span::raw("      "),
                Span::styled(desc, Style::default().fg(theme::TEXT_MUTED())),
            ]));
            lines.push(Line::from(""));
        } else {
            draw_coming_soon_entry(&mut lines, name, desc);
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

fn draw_coming_soon_entry<'a>(lines: &mut Vec<Line<'a>>, name: &'a str, desc: &'a str) {
    let padding_len = 16_usize.saturating_sub(name.len() + 4);
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("[ {} ]", name),
            Style::default().fg(theme::TEXT_MUTED()),
        ),
        Span::raw(" ".repeat(padding_len)),
        Span::styled("Coming Soon", Style::default().fg(theme::TEXT_DIM())),
    ]));
    lines.push(Line::from(vec![
        Span::raw("      "),
        Span::styled(desc, Style::default().fg(theme::TEXT_MUTED())),
    ]));
    lines.push(Line::from(""));
}

// ── Leaderboard sidebar (right panel in arcade lobby) ──────────

fn draw_leaderboard_sidebar(frame: &mut Frame, area: Rect, data: &Arc<LeaderboardData>) {
    let block = Block::default()
        .title(" Leaderboard (🗘 30s) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let block_inner = block.inner(area);
    frame.render_widget(block, area);

    if block_inner.height < 4 || block_inner.width < 10 {
        return;
    }

    // 1-col padding on each side
    let inner = Rect {
        x: block_inner.x + 1,
        y: block_inner.y,
        width: block_inner.width.saturating_sub(2),
        height: block_inner.height,
    };

    let mut lines: Vec<Line<'static>> = Vec::new();

    // ── Chip Leaders ──
    if !data.chip_leaders.is_empty() {
        lines.push(Line::from(Span::styled(
            "Chip Leaders",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));

        for (i, entry) in data.chip_leaders.iter().take(5).enumerate() {
            let medal = match i {
                0 => "\u{25c6} ", // ◆
                _ => "  ",
            };
            let medal_style = if i == 0 {
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT_DIM())
            };
            let name_style = if i == 0 {
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT())
            };
            let max_name = (inner.width as usize).saturating_sub(10);
            let name: String = entry.username.chars().take(max_name).collect();
            lines.push(Line::from(vec![
                Span::styled(medal, medal_style),
                Span::styled(name, name_style),
                Span::styled(
                    format!(" {}", entry.balance),
                    Style::default().fg(theme::SUCCESS()),
                ),
            ]));
        }

        lines.push(Line::from(""));
    }

    // ── Today's Champions ──
    lines.push(Line::from(Span::styled(
        "Today's Champions",
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    if data.today_champions.is_empty() {
        lines.push(Line::from(Span::styled(
            "No wins yet today",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    } else {
        for (i, entry) in data.today_champions.iter().take(5).enumerate() {
            let medal = match i {
                0 => "\u{25c6} ", // ◆
                _ => "  ",
            };
            let medal_style = if i == 0 {
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT_DIM())
            };
            let name_style = if i == 0 {
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT())
            };
            let max_name = (inner.width as usize).saturating_sub(8);
            let name: String = entry.username.chars().take(max_name).collect();
            lines.push(Line::from(vec![
                Span::styled(medal, medal_style),
                Span::styled(name, name_style),
                Span::styled(
                    format!(" {}", entry.count),
                    Style::default().fg(theme::TEXT_DIM()),
                ),
            ]));
        }
    }

    lines.push(Line::from(""));

    // ── Streak Leaders ──
    lines.push(Line::from(Span::styled(
        "Streak Leaders",
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    if data.streak_leaders.is_empty() {
        lines.push(Line::from(Span::styled(
            "No active streaks",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    } else {
        for (i, entry) in data.streak_leaders.iter().take(5).enumerate() {
            let badge = BadgeTier::from_streak(entry.count);
            let badge_str = badge.map(|b| b.label()).unwrap_or("");
            let badge_color = match badge {
                Some(BadgeTier::Gold) => theme::AMBER_GLOW(),
                Some(BadgeTier::Silver) => theme::TEXT_BRIGHT(),
                Some(BadgeTier::Bronze) => theme::AMBER_DIM(),
                None => theme::TEXT_DIM(),
            };
            let medal = if i == 0 {
                "\u{25c6} " // ◆
            } else {
                ""
            };
            let medal_style = Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD);
            let name_style = if i == 0 {
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT())
            };
            let max_name = (inner.width as usize).saturating_sub(10);
            let name: String = entry.username.chars().take(max_name).collect();
            lines.push(Line::from(vec![
                Span::styled(format!("{badge_str} "), Style::default().fg(badge_color)),
                Span::styled(medal, medal_style),
                Span::styled(name, name_style),
                Span::styled(
                    format!(" {}d", entry.count),
                    Style::default().fg(theme::TEXT_DIM()),
                ),
            ]));
        }
    }

    // ── All-Time High Scores ──
    if !data.high_scores.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "All-Time High Scores",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));

        let mut current_game: &str = "";
        let mut game_first = true;
        for entry in &data.high_scores {
            if entry.game != current_game {
                current_game = entry.game;
                game_first = true;
                lines.push(Line::from(Span::styled(
                    current_game.to_string(),
                    Style::default().fg(theme::TEXT_DIM()),
                )));
            }
            let medal = if game_first {
                "\u{25c6} " // ◆
            } else {
                "  "
            };
            let medal_style = if game_first {
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT_DIM())
            };
            let name_style = if game_first {
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT())
            };
            game_first = false;
            let max_name = (inner.width as usize).saturating_sub(10);
            let name: String = entry.username.chars().take(max_name).collect();
            lines.push(Line::from(vec![
                Span::styled(medal, medal_style),
                Span::styled(name, name_style),
                Span::styled(
                    format!(" {}", entry.score),
                    Style::default().fg(theme::SUCCESS()),
                ),
            ]));
        }
    }

    // ── Info ──
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Info",
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    let muted = Style::default().fg(theme::TEXT_MUTED());

    // Streak tiers
    lines.push(Line::from(Span::styled("Streak tiers:", muted)));
    lines.push(Line::from(vec![
        Span::styled("  ", muted),
        Span::styled("\u{2605}", Style::default().fg(theme::AMBER_DIM())),
        Span::styled("   Bronze   3+d", muted),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  ", muted),
        Span::styled(
            "\u{2605}\u{2605}",
            Style::default().fg(theme::TEXT_BRIGHT()),
        ),
        Span::styled("  Silver   7+d", muted),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  ", muted),
        Span::styled(
            "\u{2605}\u{2605}\u{2605}",
            Style::default().fg(theme::AMBER_GLOW()),
        ),
        Span::styled(" Gold    14+d", muted),
    ]));

    lines.push(Line::from(""));

    // Chip economy
    lines.push(Line::from(Span::styled("Late Chips:", muted)));
    for hint in [
        "  Bonsai water +200/day",
        "  Easy win      +50",
        "  Medium win   +100",
        "  Hard win     +150",
        "  Floor         100",
    ] {
        lines.push(Line::from(Span::styled(hint, muted)));
    }
    lines.push(Line::from(""));
    for hint in ["Sudoku, Nonograms,", "Minesweeper, Solitaire"] {
        lines.push(Line::from(Span::styled(hint, muted)));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}
