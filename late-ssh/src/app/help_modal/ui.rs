use crate::app::common::text_layout;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::common::{scroll_view::ScrollViewport, theme};

use super::{data::HelpTopic, state::HelpModalState};

pub(crate) fn draw(frame: &mut Frame, area: Rect, state: &HelpModalState, listen_url: &str) {
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
        state.view().set_viewport(ScrollViewport::default());
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
        .map(|topic| text_layout::width(topic.title()))
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
    let hit = state.hit_line();
    let lines: Vec<Line> = state
        .current_lines(listen_url)
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let style = match hit == Some(index) {
                true => Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .bg(theme::BG_HIGHLIGHT()),
                false => Style::default().fg(theme::TEXT()),
            };
            Line::from(Span::styled(line, style))
        })
        .collect();
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    let view = state.view();
    view.set_viewport(ScrollViewport {
        body,
        content_height: u16::try_from(paragraph.line_count(body.width)).unwrap_or(u16::MAX),
        ..ScrollViewport::default()
    });
    frame.render_widget(paragraph.scroll((view.offset(), 0)), body);

    draw_footer(frame, layout[3], state.query());
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
        let text = text_layout::fit(&format!("  {}", topic.title()), width);

        frame.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), row);
    }
    state.set_tab_rects(rects);
}

/// The key row, or the `/` prompt while a search is being typed.
fn draw_footer(frame: &mut Frame, area: Rect, query: Option<&str>) {
    let key = Style::default().fg(theme::AMBER_DIM());
    let label = Style::default().fg(theme::TEXT_DIM());
    let footer = match query {
        Some(query) => Line::from(vec![
            Span::raw("  "),
            Span::styled("/", key),
            Span::styled(query.to_string(), Style::default().fg(theme::TEXT_BRIGHT())),
            Span::styled("▏", key),
            Span::styled("   Enter", key),
            Span::styled(" find  ", label),
            Span::styled("Esc", key),
            Span::styled(" cancel", label),
        ]),
        None => Line::from(vec![
            Span::raw("  "),
            Span::styled("Tab/S+Tab", key),
            Span::styled(" section  ", label),
            Span::styled("j/k g/G", key),
            Span::styled(" scroll  ", label),
            Span::styled("/", key),
            Span::styled(" search  ", label),
            Span::styled("n/N", key),
            Span::styled(" next  ", label),
            Span::styled("?/Esc/q", key),
            Span::styled(" close", label),
        ]),
    };
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
