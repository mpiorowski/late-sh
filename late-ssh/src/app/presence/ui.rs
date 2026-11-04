//! What the pages draw of presence: the view Home's tiles read, the pinned
//! presence block (Home's presence tile), and the pulse tile.

use late_core::models::presence_sample::HeadcountPoint;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::pulse::{columns, glyphs, height, peak};
use crate::app::{
    chat::state::ActiveFriend,
    common::{away::AWAY_GLYPH, marquee::marquee_text, theme},
};

/// Who is online now and the day behind it, as Home's tiles read it: the
/// presence block, the pulse, the friends list, and the clock, which
/// shows the same clock text big.
pub(crate) struct PresenceView<'a> {
    pub clock: &'a str,
    pub online: usize,
    pub friends: &'a [ActiveFriend],
    /// The last day of headcount samples (`PresenceSession::history`).
    pub history: &'a [HeadcountPoint],
}

/// The pinned two-row presence block, Home's presence tile: row one is the
/// online count (left) and the clock (right); row two is connected friends, an
/// away one carrying the away glyph. Both rows always render so the panel list below never shifts.
pub(crate) fn draw_core_block(
    frame: &mut Frame,
    area: Rect,
    clock_text: &str,
    online_count: usize,
    active_friends: &[ActiveFriend],
    tick: usize,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let row = |offset: u16| Rect {
        x: area.x,
        y: area.y + offset,
        width: area.width,
        height: 1,
    };

    // Row 0: presence left, clock right.
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("● ", Style::default().fg(theme::SUCCESS())),
            Span::styled(
                online_count.to_string(),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" here", Style::default().fg(theme::TEXT_DIM())),
        ])),
        row(0),
    );
    let mut parts = clock_text.rsplitn(2, ' ');
    let time = parts.next().unwrap_or(clock_text);
    let label = parts.next();
    // Native `⊙` (U+2299 circled dot operator). Reliably mono across
    // terminals, reads as a small clock face without competing with digits.
    let mut clock_spans: Vec<Span<'static>> =
        vec![Span::styled("⊙ ", Style::default().fg(theme::AMBER_DIM()))];
    clock_spans.push(Span::styled(
        time.to_string(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ));
    if let Some(label) = label {
        clock_spans.push(Span::raw(" "));
        clock_spans.push(Span::styled(
            label.to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(clock_spans)).right_aligned(),
        row(0),
    );

    if area.height < 2 {
        return;
    }

    // Row 1: connected friends. Blank when there are none: the reserved
    // row is what keeps chrome stable.
    if !active_friends.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                friend_names_text(active_friends, area.width as usize, tick),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            )])),
            row(1),
        );
    }
}

/// Every connected friend on the one reserved row, in `active_friends`
/// order (here before away, then most recent login). The list scrolls
/// (marquee) when it overruns the rail instead of stopping at the few names
/// that happen to fit, so the whole crowd can be read.
fn friend_names_text(friends: &[ActiveFriend], width: usize, tick: usize) -> String {
    marquee_text(&friend_names_joined(friends), width, tick)
}

/// The friends row before the marquee: Home's render gate reads it
/// too (`home_marquee_scrolling`), to know whether the row scrolls.
pub(crate) fn friend_names_joined(friends: &[ActiveFriend]) -> String {
    friends
        .iter()
        .map(|friend| match friend.away {
            true => format!("@{} {}", friend.username, AWAY_GLYPH),
            false => format!("@{}", friend.username),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The pulse tile: the people online now over a day of them as a
/// sparkline, the peak named, a time axis under it. One row is the
/// sparkline alone, two add the header, three or more the axis too. Reads
/// the session's tick copy of the history, nothing else.
pub(crate) fn draw_pulse_tile(
    frame: &mut Frame,
    area: Rect,
    online: usize,
    history: &[HeadcountPoint],
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let (header, spark_rows, axis) = match area.height {
        1 => (false, 1, false),
        2 => (true, 1, false),
        rows => (true, rows - 2, true),
    };
    let now = chrono::Utc::now();
    let mut y = area.y;
    if header {
        draw_pulse_header(
            frame,
            Rect::new(area.x, y, area.width, 1),
            online,
            peak(history),
        );
        y += 1;
    }
    let mut cols = columns(history, now, usize::from(area.width));
    // The last column is this minute: the live count, which may sit above
    // the bucket's sample.
    if let Some(last) = cols.last_mut() {
        *last = (*last).max(Some(online as i64));
    }
    let max = cols.iter().flatten().copied().max().unwrap_or(0).max(1);
    draw_pulse_spark(
        frame,
        Rect::new(area.x, y, area.width, spark_rows),
        &cols,
        max,
    );
    y += spark_rows;
    if axis {
        draw_pulse_axis(frame, Rect::new(area.x, y, area.width, 1));
    }
}

/// `online 12` on the left, `peak 31 · 21:05` on the right while there is
/// a day to speak of; the right side goes first on a narrow tile.
fn draw_pulse_header(frame: &mut Frame, row: Rect, online: usize, peak: Option<HeadcountPoint>) {
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
fn draw_pulse_spark(frame: &mut Frame, area: Rect, cols: &[Option<i64>], max: i64) {
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
            (Some(_), false) => Style::default().fg(pulse_shade(height(*column, max))),
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
fn pulse_shade(height: f32) -> Color {
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
fn draw_pulse_axis(frame: &mut Frame, row: Rect) {
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

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
