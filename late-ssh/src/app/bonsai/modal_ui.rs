use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{
    bonsai::{
        care::{BonsaiCareState, CareMode, branch_targets_for},
        state::BonsaiState,
        ui::{TreeOverlay, render_tree_art_lines, tree_ascii},
    },
    common::theme,
};

const MODAL_WIDTH: u16 = 72;
const MODAL_HEIGHT: u16 = 26;

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    bonsai: &BonsaiState,
    care: &BonsaiCareState,
    beat: f32,
) {
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Bonsai Care ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    draw_help_hint(frame, popup);

    let layout = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);

    draw_tree(frame, layout[0], bonsai, care, beat);
    draw_status(frame, layout[1], bonsai, care);
    draw_footer(frame, layout[3]);
}

fn draw_tree(
    frame: &mut Frame,
    area: Rect,
    bonsai: &BonsaiState,
    care: &BonsaiCareState,
    beat: f32,
) {
    let stage = bonsai.stage();
    let art = tree_ascii(stage, bonsai.seed, bonsai.is_wilting());
    let targets = branch_targets_for(stage, bonsai.seed, care.date, &art, care.branch_goal);

    let mut tree_lines = render_tree_art_lines(
        stage,
        bonsai.seed,
        bonsai.is_wilting(),
        area.width as usize,
        beat,
        Some(TreeOverlay {
            targets: &targets,
            cut_branch_ids: &care.cut_branch_ids,
            cursor_x: care.cursor_x,
            cursor_y: care.cursor_y,
            show_selection: care.mode == CareMode::Prune,
        }),
    );

    let mut lines = Vec::new();
    let top_pad = area.height.saturating_sub(tree_lines.len() as u16) as usize;
    for _ in 0..top_pad {
        lines.push(Line::from(""));
    }
    lines.append(&mut tree_lines);

    if care.water_animation_ticks > 0 {
        if let Some(line) = lines.last_mut() {
            line.spans.push(Span::styled(
                "  drip",
                Style::default()
                    .fg(theme::SUCCESS())
                    .add_modifier(Modifier::BOLD),
            ));
        }
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_status(frame: &mut Frame, area: Rect, bonsai: &BonsaiState, care: &BonsaiCareState) {
    let summary = Line::from(vec![
        Span::styled(
            bonsai.stage().label().to_string(),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        dot(),
        Span::styled(
            format!("Day {}", bonsai.age_days),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ])
    .centered();

    let action = if let Some(msg) = care.message.as_deref() {
        Line::from(Span::styled(
            msg.to_string(),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ))
    } else {
        let (text, color) = action_hint(bonsai, care);
        Line::from(Span::styled(text, Style::default().fg(color)))
    }
    .centered();

    frame.render_widget(Paragraph::new(vec![summary, action]), area);
}

fn action_hint(bonsai: &BonsaiState, care: &BonsaiCareState) -> (String, Color) {
    if !bonsai.is_alive {
        return ("plant anew with w".to_string(), theme::AMBER());
    }
    let remaining = care.branch_goal.saturating_sub(care.branches_done());
    let branch_word = if remaining == 1 { "branch" } else { "branches" };
    match (care.watered, remaining, bonsai.is_admin) {
        (false, 0, _) => ("water today before midnight".to_string(), theme::AMBER()),
        (false, n, _) => (
            format!("water today, cut {n} overgrown {branch_word}"),
            theme::AMBER(),
        ),
        (true, 0, false) => (
            "daily care done, next watering tomorrow".to_string(),
            theme::SUCCESS(),
        ),
        (true, 0, true) => (
            "daily care done, water again anytime".to_string(),
            theme::SUCCESS(),
        ),
        (true, n, false) => (
            format!("cut {n} overgrown {branch_word} before midnight"),
            theme::AMBER(),
        ),
        (true, n, true) => (
            format!("water anytime, cut {n} overgrown {branch_word}"),
            theme::AMBER(),
        ),
    }
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        key("w"),
        text(" water"),
        gap(),
        key("x"),
        text(" cut"),
        gap(),
        key("p"),
        text(" reshape"),
        gap(),
        key("s"),
        text(" copy"),
        gap(),
        key("hjkl/←↑↓→"),
        text(" move"),
        gap(),
        key("q"),
        text(" close"),
    ])
    .centered();
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_help_hint(frame: &mut Frame, popup: Rect) {
    let width = 9;
    let area = Rect {
        x: popup.x + popup.width.saturating_sub(width + 2),
        y: popup.y,
        width,
        height: 1,
    };
    let line = Line::from(vec![Span::raw(" "), key("?"), text(" help ")]);
    frame.render_widget(Paragraph::new(line), area);
}

fn key(label: &str) -> Span<'static> {
    Span::styled(
        label.to_string(),
        Style::default()
            .fg(theme::AMBER_DIM())
            .add_modifier(Modifier::BOLD),
    )
}

fn text(label: &str) -> Span<'static> {
    Span::styled(label.to_string(), Style::default().fg(theme::TEXT_DIM()))
}

fn dot() -> Span<'static> {
    Span::styled("  ·  ", Style::default().fg(theme::BORDER_DIM()))
}

fn gap() -> Span<'static> {
    Span::raw("   ")
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}
