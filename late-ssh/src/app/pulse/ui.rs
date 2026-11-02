//! The pulse tile: the people online now over a day of them as a
//! sparkline, the peak named, a time axis under it. Reads the owned
//! history the session copies on its tick, nothing else.

use chrono::Utc;
use late_core::models::presence_sample::HeadcountPoint;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::state::{columns, glyphs, height, peak};
use crate::app::common::theme;

pub struct PulseView<'a> {
    /// People online right now, the presence row's count.
    pub online: usize,
    /// The last day of samples, oldest first.
    pub history: &'a [HeadcountPoint],
}

/// Rows the header and the axis take when the tile has them: one row is
/// the sparkline alone, two add the header, three or more the axis too.
pub fn draw(frame: &mut Frame, area: Rect, view: &PulseView<'_>) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let (header, spark_rows, axis) = match area.height {
        1 => (false, 1, false),
        2 => (true, 1, false),
        rows => (true, rows - 2, true),
    };
    let now = Utc::now();
    let mut y = area.y;
    if header {
        draw_header(
            frame,
            Rect::new(area.x, y, area.width, 1),
            view.online,
            peak(view.history),
        );
        y += 1;
    }
    let mut cols = columns(view.history, now, usize::from(area.width));
    // The last column is this minute: the live count, which may sit above
    // the bucket's sample.
    if let Some(last) = cols.last_mut() {
        let live = view.online as i64;
        *last = Some(match *last {
            Some(sampled) => sampled.max(live),
            None => live,
        });
    }
    let max = cols.iter().flatten().copied().max().unwrap_or(0).max(1);
    draw_spark(
        frame,
        Rect::new(area.x, y, area.width, spark_rows),
        &cols,
        max,
    );
    y += spark_rows;
    if axis {
        draw_axis(frame, Rect::new(area.x, y, area.width, 1));
    }
}

/// `online 12` on the left, `peak 31 · 21:05` on the right while there is
/// a day to speak of; the right side goes first on a narrow tile.
fn draw_header(frame: &mut Frame, row: Rect, online: usize, peak: Option<HeadcountPoint>) {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let left = Line::from(vec![
        Span::styled("online ", dim),
        Span::styled(
            online.to_string(),
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    let right = match peak {
        Some(point) => Line::from(vec![
            Span::styled("peak ", dim),
            Span::styled(
                point.online.to_string(),
                Style::default().fg(theme::AMBER()),
            ),
            Span::styled(
                format!(" · {}", point.bucket.format("%H:%M")),
                Style::default().fg(theme::TEXT_FAINT()),
            ),
        ]),
        None => Line::from(Span::styled(
            "sampling",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
    };
    let fits = left.width() + 1 + right.width() <= usize::from(row.width);
    frame.render_widget(Paragraph::new(left), row);
    if fits {
        frame.render_widget(Paragraph::new(right).right_aligned(), row);
    }
}

/// The columns over `area.height` rows, bottom row last: each column's
/// glyphs in a colour that climbs with its height against the peak, the
/// last column (now) bright, a gap faint.
fn draw_spark(frame: &mut Frame, area: Rect, cols: &[Option<i64>], max: i64) {
    let rows = usize::from(area.height);
    if rows == 0 {
        return;
    }
    let last = cols.len().saturating_sub(1);
    let mut lines: Vec<Vec<Span<'static>>> = vec![Vec::with_capacity(cols.len()); rows];
    for (x, column) in cols.iter().enumerate() {
        let style = match (column, x == last) {
            (_, true) => Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
            (None, false) => Style::default().fg(theme::TEXT_FAINT()),
            (Some(_), false) => Style::default().fg(shade(height(*column, max))),
        };
        for (row, glyph) in glyphs(*column, max, rows).into_iter().enumerate() {
            // Row 0 is the bottom: the last line drawn.
            lines[rows - 1 - row].push(Span::styled(glyph.to_string(), style));
        }
    }
    let lines: Vec<Line<'static>> = lines.into_iter().map(Line::from).collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// Dim amber low, amber in the middle, glowing near the peak.
fn shade(height: f32) -> ratatui::style::Color {
    if height < 0.34 {
        theme::AMBER_DIM()
    } else if height < 0.67 {
        theme::AMBER()
    } else {
        theme::AMBER_GLOW()
    }
}

/// `24h` left, `12h` in the middle, `now` right, faint; a tile too narrow
/// for the three keeps `now`.
fn draw_axis(frame: &mut Frame, row: Rect) {
    let width = usize::from(row.width);
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let mut cells: Vec<char> = vec![' '; width];
    let mut put = |at: usize, text: &str| {
        for (i, ch) in text.chars().enumerate() {
            if let Some(cell) = cells.get_mut(at + i) {
                *cell = ch;
            }
        }
    };
    if width >= 12 {
        put(0, "24h");
        put(width / 2 - 1, "12h");
    }
    if width >= 3 {
        put(width - 3, "now");
    }
    let text: String = cells.into_iter().collect();
    frame.render_widget(Paragraph::new(Line::from(Span::styled(text, faint))), row);
}
