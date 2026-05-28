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
    hub::{dailies::state::QuestState, shop::state::ShopState},
};

pub struct HubDrawProps<'a> {
    pub state: &'a HubState,
    pub quest_state: &'a QuestState,
    pub shop_state: &'a ShopState,
    pub leaderboard: &'a LeaderboardData,
    pub user_id: Uuid,
    pub pet_species: &'a str,
}

pub fn draw(frame: &mut Frame, area: Rect, props: HubDrawProps<'_>) {
    let HubDrawProps {
        state,
        quest_state,
        shop_state,
        leaderboard,
        user_id,
        pet_species,
    } = props;

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

    draw_tabs(frame, layout[1], state.selected_tab());
    match state.selected_tab() {
        HubTab::Leaderboard => {
            crate::app::hub::leaderboard::draw(frame, layout[3], leaderboard, user_id)
        }
        HubTab::Dailies => crate::app::hub::dailies::ui::draw(frame, layout[3], quest_state),
        HubTab::Shop => crate::app::hub::shop::ui::draw(frame, layout[3], shop_state, pet_species),
        HubTab::Events => crate::app::hub::events::draw(frame, layout[3]),
        HubTab::Guide => crate::app::hub::guide::draw(frame, layout[3], state.guide_scroll()),
    }
    draw_footer(frame, layout[5], state.selected_tab());
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
        ]);
    }
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
