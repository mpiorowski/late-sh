use crate::app::common::theme;
use std::ops::Range;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

pub(crate) fn filtered_list_areas(area: Rect, mine_only: bool) -> (Option<Rect>, Rect) {
    if !mine_only || area.height < 2 {
        return (None, area);
    }
    let [status_area, list_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);
    (Some(status_area), list_area)
}

pub(crate) fn draw_mine_only_status(frame: &mut Frame, area: Rect, label: &str) {
    if area.width == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "mine only",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" · showing your {label}"),
                Style::default().fg(theme::TEXT_FAINT()),
            ),
        ])),
        area,
    );
}

/// The first row drawn, given the one the last draw started at. The window
/// stays put while the selection moves inside it and scrolls only when the
/// selected story, or the neighbour story past it (`context`), would leave
/// it, so there is always a story's worth of list ahead of the selection.
/// When the selected story and both neighbours do not fit, each neighbour
/// gets an even share of the rows the selected story leaves, and a story
/// taller than the window shows its first rows. A row is the caller's
/// unit: a terminal row in News, a whole entry in RSS.
pub(crate) fn scroll_offset(
    previous: usize,
    total_rows: usize,
    selected: &Range<usize>,
    context: &Range<usize>,
    height: usize,
) -> usize {
    let max_offset = total_rows.saturating_sub(height);
    let spare = height.saturating_sub(selected.len());
    let above = selected.start.saturating_sub(context.start);
    let below = context.end.saturating_sub(selected.end);
    let (above, below) = if above + below <= spare {
        (above, below)
    } else {
        let above = above.min(spare / 2);
        (above, below.min(spare - above))
    };
    let top = selected.start - above;
    let bottom = selected.end + below;
    let mut offset = previous.min(max_offset);
    if bottom > offset + height {
        offset = bottom.saturating_sub(height);
    }
    if top < offset {
        offset = top;
    }
    offset.min(max_offset)
}

#[cfg(test)]
#[path = "list_ui_test.rs"]
mod list_ui_test;
