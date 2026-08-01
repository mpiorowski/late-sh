use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{common::theme, hub::shop::state::ShopState};

pub(crate) struct HubDrawProps<'a> {
    pub shop_state: &'a ShopState,
    pub pet_species: &'a str,
}

struct HubLayout {
    body: Rect,
    footer: Rect,
}

pub(crate) fn draw(frame: &mut Frame, area: Rect, props: HubDrawProps<'_>) {
    let HubDrawProps {
        shop_state,
        pet_species,
    } = props;

    let layout = draw_hub_shell(frame, area);
    crate::app::hub::shop::ui::draw(frame, layout.body, shop_state, pet_species);
    draw_footer(frame, layout.footer);
}

fn draw_hub_shell(frame: &mut Frame, area: Rect) -> HubLayout {
    let popup = centered_percent_rect(80, 85, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Shop ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let rows = Layout::vertical([
        Constraint::Length(1), // breathing room
        Constraint::Min(14),   // body
        Constraint::Length(1), // breathing room above footer
        Constraint::Length(1), // footer
    ])
    .split(inner);

    HubLayout {
        body: rows[1],
        footer: rows[3],
    }
}

pub(super) fn draw_footer(frame: &mut Frame, area: Rect) {
    let key = Style::default().fg(theme::AMBER_DIM());
    let text = Style::default().fg(theme::TEXT_DIM());
    let spans = vec![
        Span::raw("  "),
        Span::styled("Esc/q", key),
        Span::styled(" close", text),
    ];
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
