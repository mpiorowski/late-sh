//! The header: the month and zone on one row, the controls on the next, an
//! error under them when there is one. Every control is a hit target.
use super::{
    state::{Action, CalendarState},
    ui::{accent, base, button, clipped_title, dim, row, separator, styled_row},
};
use crate::app::common::theme;
use chrono::TimeZone;
use late_core::models::calendar::CalendarView;
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
};

fn context(s: &CalendarState, width: u16) -> Line<'static> {
    let mut line = Line::from(vec![
        Span::styled(s.selected.format("%B %Y").to_string(), accent()),
        separator(),
        Span::styled(s.tz.to_string(), dim()),
    ]);
    if line.width() > width as usize {
        let zone =
            s.tz.from_local_datetime(&s.selected.and_hms_opt(12, 0, 0).unwrap())
                .earliest()
                .map(|date| date.format("%Z").to_string())
                .unwrap_or_else(|| s.tz.to_string());
        line = Line::from(vec![
            Span::styled(s.selected.format("%b %Y").to_string(), accent()),
            Span::raw(" "),
            Span::styled(zone, dim()),
        ]);
    }
    if s.loading {
        line.spans.push(separator());
        line.spans.push(Span::styled("loading", dim()));
    }
    clipped_title(line, width)
}

/// Draws the header into the top of `area` and returns the rows it took.
pub(super) fn draw(frame: &mut Frame, area: Rect, s: &CalendarState) -> u16 {
    styled_row(
        frame,
        Rect::new(area.x + 1, area.y, area.width.saturating_sub(2), 1),
        context(s, area.width.saturating_sub(2)),
    );
    let y = area.y + 1;
    let right = area.right();
    let mut x = area.x;
    let wide = area.width >= 72;
    let view = match (s.view, wide) {
        (CalendarView::Month, _) => "v Month",
        (CalendarView::List, _) => "v List",
    };
    let board = match (s.show_board, wide) {
        (true, true) => "b Board shown",
        (false, true) => "b Board hidden",
        (true, false) => "b Board ✓",
        (false, false) => "b Board ✗",
    };
    let new = if wide { "n New event" } else { "n New" };
    for (label, action) in [
        (new, Action::New),
        ("t Today", Action::Today),
        ("[ Previous", Action::Previous),
        ("] Next", Action::Next),
        (view, Action::View),
        (board, Action::Board),
    ] {
        let label = if !wide && label.starts_with('[') {
            "[ Prev"
        } else {
            label
        };
        button(frame, s, &mut x, y, right, label, action, false);
    }
    let mut height = 2;
    if let Some(error) = &s.error {
        row(
            frame,
            Rect::new(area.x, area.y + height, area.width, 1),
            error,
            base().fg(theme::ERROR()),
        );
        height += 1;
    }
    height
}
