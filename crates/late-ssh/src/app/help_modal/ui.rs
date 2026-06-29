use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::common::theme;

use super::{data::HelpTopic, state::HelpModalState};

pub fn draw(frame: &mut Frame, area: Rect, state: &HelpModalState, pair_url: &str) {
    let popup = centered_percent_rect(80, 85, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Guide ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.height < 5 || inner.width < 10 {
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(1), // breathing room
        Constraint::Min(14),   // nav + body
        Constraint::Length(1), // breathing room
        Constraint::Length(1), // footer
    ])
    .split(inner);

    // Side-nav width sized to the longest topic title, with padding on both
    // sides so the active highlight bar has a little air. Clamp it so a very
    // narrow terminal still leaves room for the body column.
    let nav_text_width = HelpTopic::ALL
        .iter()
        .map(|topic| topic.title().chars().count())
        .max()
        .unwrap_or(0) as u16;
    let main = layout[1];
    let nav_width = (nav_text_width + 4)
        .min(main.width.saturating_sub(8))
        .max(1);

    let columns = Layout::horizontal([
        Constraint::Length(nav_width + 1), // nav + divider border
        Constraint::Min(10),               // body
    ])
    .split(main);

    // A right border on the nav column draws the single divider line between
    // the navbar and the body.
    let nav_block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(theme::BORDER_DIM()));
    let nav_inner = nav_block.inner(columns[0]);
    frame.render_widget(nav_block, columns[0]);
    draw_nav(frame, nav_inner, state);

    let body = columns[1].inner(Margin::new(2, 0));
    state.set_body_area(body);
    let lines: Vec<Line> = state
        .current_lines(pair_url)
        .into_iter()
        .map(|line| Line::from(Span::styled(line, Style::default().fg(theme::TEXT()))))
        .collect();
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((state.current_scroll(), 0)),
        body,
    );

    draw_footer(frame, layout[3]);
}

fn draw_nav(frame: &mut Frame, area: Rect, state: &HelpModalState) {
    let selected = state.selected_topic();
    let mut rects = [Rect::new(0, 0, 0, 0); HelpTopic::ALL.len()];
    let width = area.width as usize;
    let bottom = area.y.saturating_add(area.height);
    for (index, topic) in HelpTopic::ALL.iter().copied().enumerate() {
        let y = area.y.saturating_add(index as u16);
        if y >= bottom {
            break; // ran out of vertical room; remaining rows stay unhittable
        }
        let row = Rect::new(area.x, y, area.width, 1);
        rects[index] = row;

        let active = topic == selected;
        let style = if active {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .bg(theme::BG_HIGHLIGHT())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };

        // Left-pad the label, then fill to the full column width so the active
        // row reads as a solid highlight bar instead of a colored word.
        let mut text = format!("  {}", topic.title());
        let len = text.chars().count();
        if len > width {
            text = text.chars().take(width).collect();
        } else {
            text.push_str(&" ".repeat(width - len));
        }

        frame.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), row);
    }
    state.set_tab_rects(rects);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let footer = Line::from(vec![
        Span::raw("  "),
        Span::styled("Tab/S+Tab", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" switch section  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("↑↓ j/k", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" scroll  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("click/wheel", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" mouse  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("?/Esc/q", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(footer), area);
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
