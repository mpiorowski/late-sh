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

pub const MODAL_WIDTH: u16 = 124;
pub const MODAL_HEIGHT: u16 = 40;

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
        Constraint::Length(1), // breathing room
        Constraint::Length(1), // tabs
        Constraint::Length(1), // breathing room
        Constraint::Min(14),   // body
        Constraint::Length(1), // footer
    ])
    .split(inner);

    draw_tabs(frame, layout[1], state.selected_tab());
    match state.selected_tab() {
        HubTab::Leaderboard => {
            crate::app::hub::leaderboard::draw(frame, layout[3], leaderboard, user_id)
        }
        HubTab::Dailies => crate::app::hub::dailies::draw(frame, layout[3]),
        HubTab::Shop => crate::app::hub::shop::draw(frame, layout[3]),
        HubTab::Events => crate::app::hub::events::draw(frame, layout[3]),
    }
    draw_footer(frame, layout[4], state.selected_tab());
}

fn draw_tabs(frame: &mut Frame, area: Rect, selected: HubTab) {
    let mut spans = vec![Span::raw("  ")];
    for (index, tab) in HubTab::ALL.iter().copied().enumerate() {
        let active = tab == selected;
        let style = if active {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .bg(theme::BG_HIGHLIGHT())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        spans.push(Span::styled(
            format!(" {} {} ", index + 1, tab.label()),
            style,
        ));
        spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_footer(frame: &mut Frame, area: Rect, _tab: HubTab) {
    let key = Style::default().fg(theme::AMBER_DIM());
    let text = Style::default().fg(theme::TEXT_DIM());
    let line = Line::from(vec![
        Span::raw("  "),
        Span::styled("Tab/S+Tab", key),
        Span::styled(" switch tabs  ", text),
        Span::styled("1-4", key),
        Span::styled(" jump  ", text),
        Span::styled("Esc/q", key),
        Span::styled(" close", text),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modal_dimensions_fit_settings_neighborhood() {
        assert!(MODAL_HEIGHT >= 30);
        assert!(MODAL_WIDTH >= 80);
    }
}
