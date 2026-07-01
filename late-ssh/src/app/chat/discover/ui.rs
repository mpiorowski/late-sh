use crate::app::chat::svc::DiscoverRoomItem;
use crate::app::common::{primitives::format_relative_time, theme};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
};

pub struct DiscoverListView<'a> {
    pub items: Vec<&'a DiscoverRoomItem>,
    pub selected_index: usize,
    pub loading: bool,
    pub filtering: bool,
    pub query: &'a str,
}

/// One dense line per room so the list shows many rooms at once.
const ITEM_HEIGHT: u16 = 1;
/// Fixed width for the `#slug` column so the stats align into a tidy second
/// column regardless of room-name length.
const NAME_WIDTH: usize = 24;

pub fn draw_discover_list(frame: &mut Frame, area: Rect, view: &DiscoverListView<'_>) {
    let inner_area = area;

    if view.loading {
        let text = Text::from("Loading rooms...");
        let loading_p = Paragraph::new(text).style(Style::default().fg(theme::TEXT_DIM()));
        frame.render_widget(loading_p, inner_area);
        return;
    }

    if view.items.is_empty() {
        let msg = if view.query.trim().is_empty() {
            "No public rooms to discover right now.".to_string()
        } else {
            format!("No rooms match \"{}\".", view.query.trim())
        };
        let empty_p =
            Paragraph::new(Text::from(msg)).style(Style::default().fg(theme::TEXT_DIM()));
        frame.render_widget(empty_p, inner_area);
        return;
    }

    let visible_rows = (inner_area.height / ITEM_HEIGHT).max(1) as usize;
    let selected_index = view.selected_index.min(view.items.len().saturating_sub(1));
    let start_index = selected_index.saturating_sub(visible_rows.saturating_sub(1));
    let end_index = (start_index + visible_rows).min(view.items.len());
    let visible_len = end_index.saturating_sub(start_index);

    let constraints =
        std::iter::repeat_n(Constraint::Length(ITEM_HEIGHT), visible_len).collect::<Vec<_>>();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner_area);

    for (row, row_area) in layout.iter().copied().enumerate() {
        let idx = start_index + row;
        let item = view.items[idx];
        let selected = idx == selected_index;

        let bg_color = if selected {
            theme::BG_SELECTION()
        } else {
            Color::Reset
        };

        let line = room_line(item, selected);
        let p = Paragraph::new(line).style(Style::default().bg(bg_color));
        frame.render_widget(p, row_area);
    }
}

fn room_line(item: &DiscoverRoomItem, selected: bool) -> Line<'static> {
    let activity = item
        .last_message_at
        .map(format_relative_time)
        .unwrap_or_else(|| "no messages yet".to_string());
    let member_noun = if item.member_count == 1 {
        "member"
    } else {
        "members"
    };
    let message_noun = if item.message_count == 1 {
        "message"
    } else {
        "messages"
    };

    let marker = if selected { "› " } else { "  " };
    let name = pad_name(&format!("#{}", item.slug), NAME_WIDTH);

    Line::from(vec![
        Span::styled(marker, Style::default().fg(theme::AMBER())),
        Span::styled(
            name,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{:>5} ", item.member_count),
            Style::default().fg(theme::AMBER()),
        ),
        Span::styled(member_noun, Style::default().fg(theme::TEXT_DIM())),
        Span::styled("  ·  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            format!("{} ", item.message_count),
            Style::default().fg(theme::TEXT()),
        ),
        Span::styled(message_noun, Style::default().fg(theme::TEXT_DIM())),
        Span::styled("  ·  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(activity, Style::default().fg(theme::TEXT_DIM())),
    ])
}

/// Left-pad `name` to `width`, truncating overly long slugs with an ellipsis so
/// the stats column stays aligned.
fn pad_name(name: &str, width: usize) -> String {
    let count = name.chars().count();
    if count > width {
        let truncated: String = name.chars().take(width.saturating_sub(1)).collect();
        format!("{truncated}…")
    } else {
        format!("{name:<width$}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ratatui::{Terminal, backend::TestBackend};
    use uuid::Uuid;

    fn discover_item(slug: &str, members: i64, messages: i64) -> DiscoverRoomItem {
        DiscoverRoomItem {
            room_id: Uuid::from_u128(1),
            slug: slug.to_string(),
            member_count: members,
            message_count: messages,
            last_message_at: Some(Utc::now()),
        }
    }

    fn render_discover(view: DiscoverListView<'_>) -> String {
        let width = 80;
        let height = 10;
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");

        terminal
            .draw(|frame| draw_discover_list(frame, Rect::new(0, 0, width, height), &view))
            .expect("draw");

        let buffer = terminal.backend().buffer();
        let mut rendered = String::new();
        for y in 0..height {
            for x in 0..width {
                rendered.push_str(buffer[(x, y)].symbol());
            }
            rendered.push('\n');
        }
        rendered
    }

    #[test]
    fn loading_state_does_not_claim_there_are_no_rooms() {
        let rendered = render_discover(DiscoverListView {
            items: Vec::new(),
            selected_index: 0,
            loading: true,
            filtering: false,
            query: "",
        });

        assert!(rendered.contains("Loading rooms..."));
        assert!(!rendered.contains("No public rooms"));
    }

    #[test]
    fn loaded_empty_state_explains_no_discoverable_rooms() {
        let rendered = render_discover(DiscoverListView {
            items: Vec::new(),
            selected_index: 0,
            loading: false,
            filtering: false,
            query: "",
        });

        assert!(rendered.contains("No public rooms to discover right now."));
    }

    #[test]
    fn empty_filter_result_names_the_query() {
        let rendered = render_discover(DiscoverListView {
            items: Vec::new(),
            selected_index: 0,
            loading: false,
            filtering: true,
            query: "zzz",
        });

        assert!(rendered.contains("No rooms match \"zzz\"."));
    }

    #[test]
    fn each_room_renders_on_a_single_line() {
        let a = discover_item("rust", 12, 3);
        let b = discover_item("python", 6, 1);
        let rendered = render_discover(DiscoverListView {
            items: vec![&a, &b],
            selected_index: 0,
            loading: false,
            filtering: false,
            query: "",
        });

        let lines: Vec<&str> = rendered.lines().collect();
        assert!(lines[0].contains("#rust"));
        assert!(lines[0].contains("12 members"));
        assert!(lines[0].contains("3 messages"));
        // Second room is on the very next row — no multi-line blocks.
        assert!(lines[1].contains("#python"));
    }
}
