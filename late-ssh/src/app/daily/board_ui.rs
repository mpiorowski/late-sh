//! Full-screen daily-match board (`Screen::DailyMatch`). A minimal frame
//! around the shared `chess_core` renderer: status, players, board, move
//! list, result banner. Entered only from the Daily Games modal.

use chrono::Utc;
use late_core::models::daily_match::DailyMatch;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    daily::state::{DailyBoardState, DailyMatchDetail, DailyState, format_deadline},
    files::terminal_image::{TerminalImageFrame, TerminalImageProtocol},
    games::chess_core::{
        board_ui::{self, BoardCtx, pick_tier},
        types::{ChessColor, ChessPieceRenderMode},
    },
};

const INFO_SIDEBAR_WIDTH: u16 = 30;
const INFO_SIDEBAR_MIN_WIDTH: u16 = 96;

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    daily: &DailyState,
    image_protocol: Option<TerminalImageProtocol>,
    terminal_images: &mut TerminalImageFrame,
) {
    let Some(board) = &daily.board else {
        frame.render_widget(
            Paragraph::new("No daily match open — press Esc.").alignment(Alignment::Center),
            area,
        );
        return;
    };
    board.board_geometry.set(None);

    if let Some(error) = &board.load_error {
        draw_center_message(frame, area, &format!("Failed to load match: {error}"));
        return;
    }
    let Some(detail) = &board.detail else {
        draw_center_message(frame, area, "Loading match…");
        return;
    };
    if area.height < 10 || area.width < 30 {
        frame.render_widget(Paragraph::new("The board needs more room."), area);
        return;
    }

    let show_sidebar = area.width >= INFO_SIDEBAR_MIN_WIDTH;
    let content = if show_sidebar {
        let cols =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(INFO_SIDEBAR_WIDTH)])
                .split(area);
        draw_info_rail(frame, cols[1], daily, board, detail);
        cols[0]
    } else {
        area
    };

    let rows = Layout::vertical([
        Constraint::Length(1), // status
        Constraint::Length(1), // top player bar
        Constraint::Min(6),    // board
        Constraint::Length(1), // bottom player bar
        Constraint::Length(1), // key hints
    ])
    .split(content);

    let orientation = daily.board_orientation();
    let my_turn = detail.is_active() && detail.row.turn_user_id == Some(daily.user_id());
    let legal = daily.board_legal_targets();
    let tier = pick_tier(rows[2].width as usize, rows[2].height as usize);
    let bar_width = (tier.board_w() as u16).min(content.width);

    frame.render_widget(
        Paragraph::new(status_line(daily, board, detail)).alignment(Alignment::Center),
        rows[0],
    );
    draw_player_bar(
        frame,
        centered_x(rows[1], bar_width),
        board,
        detail,
        orientation.other(),
    );

    let finished = !detail.is_active();
    let board_ctx = BoardCtx {
        orientation,
        cursor: my_turn.then_some(board.cursor),
        selected: board.selected,
        last: detail.state.last_move().map(|mv| (mv.from, mv.to)),
        check_sq: detail
            .in_check
            .then(|| board_ui::king_square(&detail.pieces, detail.turn))
            .flatten(),
    };
    let board_area = board_ui::draw_board(
        frame,
        rows[2],
        tier,
        &detail.pieces,
        &board_ctx,
        &legal,
        board.match_id,
        image_protocol,
        terminal_images,
        board.piece_render_mode,
        finished,
    );
    if let Some(board_area) = board_area {
        board.board_geometry.set(Some((board_area, tier)));
        if finished {
            let (heading, subtitle, color) = result_banner(daily, board, detail);
            draw_overlay(frame, board_area, heading, &subtitle, color);
        }
    }

    draw_player_bar(
        frame,
        centered_x(rows[3], bar_width),
        board,
        detail,
        orientation,
    );
    frame.render_widget(
        Paragraph::new(key_line(board, detail)).alignment(Alignment::Center),
        rows[4],
    );
}

fn draw_center_message(frame: &mut Frame, area: Rect, message: &str) {
    let rows = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            message.to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        )))
        .alignment(Alignment::Center),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "esc back",
            Style::default().fg(theme::TEXT_FAINT()),
        )))
        .alignment(Alignment::Center),
        rows[2],
    );
}

fn status_line(
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
) -> Line<'static> {
    if board.resign_confirm {
        return Line::from(Span::styled(
            "Resign this match? Press r again to confirm.",
            Style::default()
                .fg(theme::ERROR())
                .add_modifier(Modifier::BOLD),
        ));
    }
    let deadline = detail
        .row
        .turn_deadline_at
        .map(|at| format_deadline(at, Utc::now()));
    let mut spans = Vec::new();
    if detail.is_active() {
        let my_turn = detail.row.turn_user_id == Some(daily.user_id());
        let text = if my_turn {
            "Your move".to_string()
        } else {
            format!(
                "Waiting for {}",
                name_for(board, detail.row.turn_user_id.unwrap_or(Uuid::nil()))
            )
        };
        spans.push(Span::styled(
            text,
            Style::default()
                .fg(if my_turn {
                    theme::AMBER()
                } else {
                    theme::TEXT_DIM()
                })
                .add_modifier(Modifier::BOLD),
        ));
        if let Some(deadline) = deadline {
            spans.push(Span::styled(
                format!("   {deadline} on the clock"),
                Style::default().fg(theme::TEXT_DIM()),
            ));
        }
        if detail.in_check {
            spans.push(Span::styled(
                "   check",
                Style::default()
                    .fg(theme::ERROR())
                    .add_modifier(Modifier::BOLD),
            ));
        }
    } else {
        let (heading, subtitle, color) = result_banner(daily, board, detail);
        spans.push(Span::styled(
            format!("{heading} — {subtitle}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(mv) = detail.state.move_history.last() {
        spans.push(Span::styled(
            format!("   last {}", mv.label),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    Line::from(spans)
}

fn result_banner(
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
) -> (&'static str, String, Color) {
    let winner_text = |winner: Option<Uuid>| -> String {
        match winner {
            Some(id) if id == daily.user_id() => "you win".to_string(),
            Some(id) => format!("{} wins", name_for(board, id)),
            None => "no winner".to_string(),
        }
    };
    let won = detail.row.winner_user_id == Some(daily.user_id());
    let color = if won {
        theme::SUCCESS()
    } else if detail.row.winner_user_id.is_some() {
        theme::AMBER()
    } else {
        theme::TEXT_MUTED()
    };
    match detail.row.result.as_str() {
        DailyMatch::RESULT_CHECKMATE => {
            ("Checkmate", winner_text(detail.row.winner_user_id), color)
        }
        DailyMatch::RESULT_DRAW => ("Draw", "game drawn".to_string(), theme::TEXT_MUTED()),
        DailyMatch::RESULT_RESIGN => {
            ("Resignation", winner_text(detail.row.winner_user_id), color)
        }
        DailyMatch::RESULT_TIMEOUT => (
            "Timeout",
            format!("{} on the 24h clock", winner_text(detail.row.winner_user_id)),
            color,
        ),
        _ if detail.row.status == DailyMatch::STATUS_CANCELLED => {
            ("Cancelled", "challenge withdrawn".to_string(), theme::TEXT_MUTED())
        }
        _ => ("Finished", winner_text(detail.row.winner_user_id), color),
    }
}

fn draw_player_bar(
    frame: &mut Frame,
    rect: Rect,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    color: ChessColor,
) {
    if rect.height == 0 {
        return;
    }
    let user_id = detail.state.user_for_color(color);
    let on_turn = detail.is_active() && detail.turn == color;
    let dot_color = if on_turn {
        theme::AMBER_GLOW()
    } else {
        theme::TEXT_FAINT()
    };
    let mut left = vec![
        Span::raw("  "),
        Span::styled("\u{25CF} ", Style::default().fg(dot_color)),
        Span::styled(
            format!("{} ", color.label()),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            name_for(board, user_id),
            Style::default().fg(theme::TEXT()),
        ),
    ];
    if on_turn && let Some(deadline) = detail.row.turn_deadline_at {
        left.push(Span::styled(
            format!("   {}", format_deadline(deadline, Utc::now())),
            Style::default().fg(theme::AMBER()).add_modifier(Modifier::BOLD),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(left)), rect);
}

fn key_line(board: &DailyBoardState, detail: &DailyMatchDetail) -> Line<'static> {
    let mut spans = Vec::new();
    let hint = |spans: &mut Vec<Span<'static>>, key: &str, desc: &str| {
        spans.push(Span::styled(
            key.to_string(),
            Style::default().fg(theme::AMBER()),
        ));
        spans.push(Span::styled(
            format!(" {desc}   "),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    };
    if detail.is_active() {
        hint(&mut spans, "arrows/wasd", "move cursor");
        hint(&mut spans, "Space/Enter", "pick / play");
        hint(&mut spans, "r", "resign");
    }
    hint(
        &mut spans,
        "p",
        if board.piece_render_mode == ChessPieceRenderMode::Graphics {
            "pieces png"
        } else {
            "pieces ascii"
        },
    );
    hint(&mut spans, "Esc", "back to daily games");
    if let Some(last) = spans.last_mut() {
        let trimmed = last.content.trim_end().to_string();
        *last = Span::styled(trimmed, Style::default().fg(theme::TEXT_DIM()));
    }
    Line::from(spans)
}

fn draw_info_rail(
    frame: &mut Frame,
    area: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
) {
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(theme::BORDER_DIM()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let inner = Rect {
        x: inner.x + 1,
        y: inner.y,
        width: inner.width.saturating_sub(1),
        height: inner.height,
    };

    let label_value = |label: &str, value: String, color: Color| -> Line<'static> {
        Line::from(vec![
            Span::styled(
                format!("{label:<9}"),
                Style::default().fg(theme::TEXT_DIM()),
            ),
            Span::styled(value, Style::default().fg(color)),
        ])
    };
    let state_text = if detail.is_active() {
        format!("{} to move", detail.turn.label())
    } else {
        detail.row.result.clone()
    };
    let mut lines = vec![
        Line::from(Span::styled(
            "Correspondence chess — one move per day.".to_string(),
            Style::default()
                .fg(theme::TEXT_DIM())
                .add_modifier(Modifier::ITALIC),
        )),
        Line::raw(""),
        label_value(
            "White",
            name_for(board, detail.state.colors.white),
            theme::TEXT_BRIGHT(),
        ),
        label_value(
            "Black",
            name_for(board, detail.state.colors.black),
            theme::TEXT_BRIGHT(),
        ),
        label_value(
            "Clock",
            "24h per move".to_string(),
            theme::AMBER(),
        ),
        label_value(
            "Deadline",
            detail
                .row
                .turn_deadline_at
                .map(|at| format_deadline(at, Utc::now()))
                .unwrap_or_else(|| "—".to_string()),
            theme::AMBER(),
        ),
        label_value("State", state_text, theme::SUCCESS()),
        Line::raw(""),
        Line::from(Span::styled(
            "Move list".to_string(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )),
    ];
    let _ = daily;

    let budget = (inner.height as usize).saturating_sub(lines.len());
    append_moves(&mut lines, detail, budget);
    frame.render_widget(Paragraph::new(lines), inner);
}

fn append_moves(lines: &mut Vec<Line<'static>>, detail: &DailyMatchDetail, budget: usize) {
    if budget == 0 {
        return;
    }
    let history = &detail.state.move_history;
    if history.is_empty() {
        lines.push(Line::from(Span::styled(
            "no moves yet",
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )));
        return;
    }
    let mut pairs: Vec<Line<'static>> = Vec::new();
    let mut idx = 0;
    let mut number = 1;
    while idx < history.len() {
        let white = history[idx].label.clone();
        let black = history.get(idx + 1).map(|mv| mv.label.clone());
        let mut spans = vec![
            Span::styled(
                format!("{number:>3}. "),
                Style::default().fg(theme::TEXT_FAINT()),
            ),
            Span::styled(format!("{white:<9}"), Style::default().fg(theme::TEXT())),
        ];
        if let Some(black) = black {
            spans.push(Span::styled(black, Style::default().fg(theme::TEXT_DIM())));
        }
        pairs.push(Line::from(spans));
        idx += 2;
        number += 1;
    }
    if pairs.len() <= budget {
        lines.extend(pairs);
    } else {
        lines.push(Line::from(Span::styled(
            "  \u{22EE}",
            Style::default().fg(theme::TEXT_FAINT()),
        )));
        let skip = pairs.len() - (budget - 1);
        lines.extend(pairs.into_iter().skip(skip));
    }
}

fn draw_overlay(frame: &mut Frame, board_area: Rect, heading: &str, subtitle: &str, color: Color) {
    let width = (heading.chars().count().max(subtitle.chars().count()) as u16 + 8)
        .min(board_area.width);
    let height = 5.min(board_area.height);
    let overlay = Rect {
        x: board_area.x + board_area.width.saturating_sub(width) / 2,
        y: board_area.y + board_area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, overlay);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color));
    let inner = block.inner(overlay);
    frame.render_widget(block, overlay);
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(inner);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            heading.to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            subtitle.to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        )))
        .alignment(Alignment::Center),
        rows[1],
    );
}

fn name_for(board: &DailyBoardState, user_id: Uuid) -> String {
    board
        .names
        .get(&user_id)
        .cloned()
        .unwrap_or_else(|| "player".to_string())
}

fn centered_x(rect: Rect, width: u16) -> Rect {
    let width = width.min(rect.width);
    Rect {
        x: rect.x + (rect.width - width) / 2,
        y: rect.y,
        width,
        height: rect.height,
    }
}
