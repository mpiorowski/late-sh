use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{
    bonsai_v2::{
        ratty_3d::RattyBonsaiFrame,
        render::render_tree_lines,
        state::{BonsaiV2State, branch_label},
    },
    common::theme,
};

const MODAL_WIDTH: u16 = 88;
const MODAL_HEIGHT: u16 = 32;

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &BonsaiV2State,
    _beat: f32,
    ratty_3d: &mut RattyBonsaiFrame,
) {
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Dynamic Bonsai ")
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
        Constraint::Length(2),
        Constraint::Length(1),
    ])
    .split(inner);

    draw_tree(frame, layout[0], state, ratty_3d);
    draw_status(frame, layout[1], state);
    draw_footer(frame, layout[2], state.ratty_3d_enabled);
}

fn draw_tree(
    frame: &mut Frame,
    area: Rect,
    state: &BonsaiV2State,
    ratty_3d: &mut RattyBonsaiFrame,
) {
    if state.ratty_3d_enabled {
        frame.render_widget(Clear, area);
        ratty_3d.place(area);
        return;
    }

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
    let health_color = health_color(state.water_stress);
    let status = status_label(state);
    let selected = state
        .selected_branch()
        .map(|branch| {
            let ramification = if branch.ramification > 0 {
                format!(" p{}/3", branch.ramification)
            } else {
                String::new()
            };
            let split = if branch.last_pruned_day.is_some() {
                " split"
            } else {
                ""
            };
            format!(
                "branch {} {}{}{}",
                branch.id,
                branch_label(branch),
                ramification,
                split
            )
        })
        .unwrap_or_else(|| "no branch selected".to_string());
    let summary = Line::from(vec![
        strong("Branch Graph"),
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
        Span::styled(status.to_string(), Style::default().fg(health_color)),
    ])
    .centered();

    let detail = detail_line(&selected, state.message.as_deref());

    frame.render_widget(Paragraph::new(vec![summary, detail]), area);
}

fn status_label(state: &BonsaiV2State) -> &'static str {
    if !state.is_alive {
        "rip"
    } else if state.water_stress >= 60 {
        "dry"
    } else if state.water_stress >= 25 {
        "watch"
    } else {
        "alive"
    }
}

fn detail_line(selected: &str, message: Option<&str>) -> Line<'static> {
    let normalized_message = message.and_then(|msg| normalize_detail_message(selected, msg));
    let (text, style) = if let Some(message) = normalized_message {
        (message, Style::default().fg(theme::AMBER_GLOW()))
    } else if selected != "no branch selected" {
        (selected, Style::default().fg(theme::TEXT_BRIGHT()))
    } else {
        (
            "select a branch, steer its future, prune its mistakes",
            Style::default().fg(theme::TEXT_DIM()),
        )
    };

    Line::from(Span::styled(text.to_string(), style)).centered()
}

fn normalize_detail_message<'a>(selected: &str, message: &'a str) -> Option<&'a str> {
    let message = message.trim();
    if message.is_empty() || message.eq_ignore_ascii_case(selected.trim()) {
        return None;
    }
    if message.starts_with("Selected branch ") {
        return None;
    }
    Some(message)
}

fn draw_footer(frame: &mut Frame, area: Rect, ratty_3d_enabled: bool) {
    let mut spans = vec![
        key("w"),
        text(" water"),
        gap(),
        key("tab"),
        text(" sel"),
        gap(),
        key("←↓↑→/hjkl"),
        text(" steer"),
        gap(),
        key("x"),
        text(" cut"),
        gap(),
        key("p"),
        text(" pinch"),
        gap(),
        key("s"),
        text(" split"),
        gap(),
        key("3"),
        text(if ratty_3d_enabled { " 2d" } else { " 3d" }),
        gap(),
    ];
    spans.extend([
        key("c"),
        text(" copy"),
        gap(),
        key("?"),
        text(" guide"),
        gap(),
        key("q"),
        text(" close"),
    ]);
    let line = Line::from(spans).centered();
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
    Span::raw(" ")
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
