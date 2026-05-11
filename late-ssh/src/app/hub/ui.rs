use late_core::models::leaderboard::LeaderboardData;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    hub::state::{HubState, HubTab},
};

pub const MODAL_WIDTH: u16 = 104;
pub const MODAL_HEIGHT: u16 = 34;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &HubState,
    leaderboard: &LeaderboardData,
    user_id: Uuid,
) {
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Hub ")
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
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(12),
        Constraint::Length(1),
    ])
    .split(inner);

    draw_tabs(frame, layout[0], state.selected_tab());
    match state.selected_tab() {
        HubTab::Leaderboard => {
            crate::app::hub::leaderboard::draw(frame, layout[2], leaderboard, user_id)
        }
        HubTab::Dailies => crate::app::hub::dailies::draw(frame, layout[2]),
        HubTab::Shop => crate::app::hub::shop::draw(frame, layout[2]),
        HubTab::Events => crate::app::hub::events::draw(frame, layout[2]),
    }
    draw_footer(frame, layout[3]);
}

fn draw_tabs(frame: &mut Frame, area: Rect, selected: HubTab) {
    let mut spans = vec![Span::raw("  ")];
    for (index, tab) in HubTab::ALL.iter().copied().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ", Style::default().fg(theme::TEXT_DIM())));
        }
        let style = if tab == selected {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        spans.push(Span::styled(
            format!("{} {}", index + 1, tab.label()),
            style,
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::raw("  "),
        Span::styled("Tab/Shift+Tab", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" tab  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("1-4", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" jump  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    area
}
