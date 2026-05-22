use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{
    bonsai_v2::{
        render::render_tree_lines,
        state::{BonsaiV2Mode, BonsaiV2State, branch_label},
    },
    common::theme,
};

const MODAL_WIDTH: u16 = 88;
const MODAL_HEIGHT: u16 = 32;

pub(crate) fn draw(frame: &mut Frame, area: Rect, state: &BonsaiV2State, _beat: f32) {
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Bonsai V2 ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let layout = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .split(inner);

    draw_tree(frame, layout[0], state);
    draw_status(frame, layout[1], state);
    draw_footer(frame, layout[2]);
}

fn draw_tree(frame: &mut Frame, area: Rect, state: &BonsaiV2State) {
    let mut tree_lines = render_tree_lines(state, area.width as usize, area.height as usize, true);
    let top_pad = area.height.saturating_sub(tree_lines.len() as u16) as usize;
    let mut lines = Vec::with_capacity(top_pad + tree_lines.len());
    for _ in 0..top_pad {
        lines.push(Line::from(""));
    }
    lines.append(&mut tree_lines);
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_status(frame: &mut Frame, area: Rect, state: &BonsaiV2State) {
    let mode = match state.mode {
        BonsaiV2Mode::Inspect => "inspect",
        BonsaiV2Mode::Wire => "wire",
    };
    let health_color = health_color(state.water_stress);
    let selected = state
        .selected_branch()
        .map(|branch| format!("branch {} {}", branch.id, branch_label(branch)))
        .unwrap_or_else(|| "no branch selected".to_string());
    let summary = Line::from(vec![
        strong("Living Graph"),
        dot(),
        Span::styled(
            format!("Day {}", state.age_days),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        dot(),
        Span::styled(
            format!("vigor {}", state.vigor),
            Style::default().fg(theme::SUCCESS()),
        ),
        dot(),
        Span::styled(
            format!("stress {}", state.water_stress),
            Style::default().fg(health_color),
        ),
        dot(),
        Span::styled(mode.to_string(), Style::default().fg(theme::AMBER_DIM())),
    ])
    .centered();

    let selected_line = Line::from(Span::styled(
        selected,
        Style::default().fg(theme::TEXT_BRIGHT()),
    ))
    .centered();
    let action = state
        .message
        .as_deref()
        .map(|msg| Span::styled(msg.to_string(), Style::default().fg(theme::AMBER_GLOW())))
        .unwrap_or_else(|| {
            Span::styled(
                "select a branch, wire its future, prune its mistakes",
                Style::default().fg(theme::TEXT_DIM()),
            )
        });
    let action = Line::from(action).centered();

    frame.render_widget(Paragraph::new(vec![summary, selected_line, action]), area);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        key("w"),
        text(" water"),
        gap(),
        key("tab/n"),
        text(" branch"),
        gap(),
        key("h/l"),
        text(" wire"),
        gap(),
        key("j/k"),
        text(" lift"),
        gap(),
        key("x"),
        text(" prune"),
        gap(),
        key("p"),
        text(" pinch"),
        gap(),
        key("s"),
        text(" copy"),
        gap(),
        key("q"),
        text(" close"),
    ])
    .centered();
    frame.render_widget(Paragraph::new(line), area);
}

fn health_color(stress: i32) -> Color {
    if stress >= 60 {
        theme::ERROR()
    } else if stress >= 25 {
        theme::AMBER()
    } else {
        theme::SUCCESS()
    }
}

fn strong(label: &str) -> Span<'static> {
    Span::styled(
        label.to_string(),
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    )
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
