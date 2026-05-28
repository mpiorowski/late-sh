use late_core::models::leaderboard::LeaderboardData;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    hub::state::{HubState, HubTab},
};

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &HubState,
    quest_state: &crate::app::hub::dailies::state::QuestState,
    shop_state: &crate::app::hub::shop::state::ShopState,
    leaderboard: &LeaderboardData,
    user_id: Uuid,
) {
    let popup = centered_percent_rect(80, 85, area);
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
        Constraint::Length(1), // breathing room above footer
        Constraint::Length(1), // footer
    ])
    .split(inner);

    draw_tabs(frame, layout[1], state);
    state.set_body_area(layout[3]);
    match state.selected_tab() {
        HubTab::Leaderboard => {
            crate::app::hub::leaderboard::draw(frame, layout[3], leaderboard, user_id)
        }
        HubTab::Dailies => crate::app::hub::dailies::ui::draw(frame, layout[3], quest_state),
        HubTab::Shop => crate::app::hub::shop::ui::draw(frame, layout[3], shop_state),
        HubTab::Events => crate::app::hub::events::draw(frame, layout[3]),
        HubTab::Guide => crate::app::hub::guide::draw(frame, layout[3], state.guide_scroll()),
    }
    draw_footer(frame, layout[5], state.selected_tab());
}

fn draw_tabs(frame: &mut Frame, area: Rect, state: &HubState) {
    let selected = state.selected_tab();
    let mut spans = vec![Span::raw("  ")];
    let mut rects: [Rect; 5] = [Rect::new(0, 0, 0, 0); 5];
    // The leading "  " is two cells of padding before the first tab cell.
    let mut cursor_x = area.x.saturating_add(2);
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
        let label = format!(" {} {} ", index + 1, tab.label());
        let width = label.chars().count() as u16;
        let cell_end = cursor_x.saturating_add(width).min(area.x + area.width);
        rects[index] = Rect::new(
            cursor_x,
            area.y,
            cell_end.saturating_sub(cursor_x),
            area.height.min(1),
        );
        spans.push(Span::styled(label, style));
        spans.push(Span::raw(" "));
        cursor_x = cell_end.saturating_add(1);
    }
    state.set_tab_rects(rects);
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_footer(frame: &mut Frame, area: Rect, tab: HubTab) {
    let key = Style::default().fg(theme::AMBER_DIM());
    let text = Style::default().fg(theme::TEXT_DIM());
    let mut spans = vec![
        Span::raw("  "),
        Span::styled("Tab/S+Tab", key),
        Span::styled(" switch tabs  ", text),
        Span::styled("1-5", key),
        Span::styled(" jump  ", text),
    ];
    if tab == HubTab::Guide {
        spans.extend([
            Span::styled("j/k PgUp/PgDn", key),
            Span::styled(" scroll  ", text),
            Span::styled("wheel", key),
            Span::styled(" scroll  ", text),
        ]);
    }
    spans.extend([Span::styled("click", key), Span::styled(" tab  ", text)]);
    spans.extend([Span::styled("Esc/q", key), Span::styled(" close", text)]);
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn centered_percent_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let percent_x = percent_x.min(100);
    let percent_y = percent_y.min(100);
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}
