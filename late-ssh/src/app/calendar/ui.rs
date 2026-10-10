//! Theme resolution and hit geometry happen during this frame, never at load.
use super::{
    date_entry,
    navigation::Selection,
    state::{Action, CalendarState, Hit, Modal, Pane, ScrollPane, month_start},
};
use crate::app::common::{primitives::hint_line, theme};
use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, TimeZone, Timelike, Utc};
use late_core::models::calendar::{
    CalendarEvent, CalendarSource, CalendarView, EventTiming, event_access,
};
use ratatui::{
    Frame,
    layout::{Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};
pub(super) fn base() -> Style {
    Style::default().fg(theme::TEXT()).bg(theme::BG_CANVAS())
}
pub(super) fn dim() -> Style {
    base().fg(theme::TEXT_DIM())
}
pub(super) fn muted() -> Style {
    base().fg(theme::TEXT_MUTED())
}
pub(super) fn bright() -> Style {
    base().fg(theme::TEXT_BRIGHT())
}
fn rule() -> Style {
    base().fg(theme::BORDER_DIM())
}
fn key_style() -> Style {
    base().fg(theme::AMBER_DIM()).add_modifier(Modifier::BOLD)
}
pub(super) fn separator() -> Span<'static> {
    Span::styled(" · ", base().fg(theme::TEXT_FAINT()))
}
pub(super) fn accent() -> Style {
    base().fg(theme::AMBER()).add_modifier(Modifier::BOLD)
}
fn border(title: impl Into<Line<'static>>) -> Block<'static> {
    Block::default()
        .title(title)
        .title_style(bright().add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(rule())
        .style(base())
}
pub(super) fn hit(s: &CalendarState, area: Rect, action: Action) {
    if area.width > 0 && area.height > 0 {
        s.hits.borrow_mut().push(Hit { area, action });
    }
}
fn pane(s: &CalendarState, area: Rect, pane: Pane) {
    s.panes.borrow_mut().push(ScrollPane { area, pane });
}
pub(super) fn row(frame: &mut Frame, area: Rect, text: impl Into<String>, style: Style) {
    frame.render_widget(Paragraph::new(text.into()).style(style), area);
}
pub(super) fn styled_row(frame: &mut Frame, area: Rect, line: Line<'_>) {
    frame.render_widget(Paragraph::new(line).style(base()), area);
}
fn patch_line(mut line: Line<'static>, style: Style) -> Line<'static> {
    line.style = line.style.patch(style);
    // Explicit span colors must also yield to terminal-owned selection colors.
    for span in &mut line.spans {
        span.style = span.style.patch(style);
    }
    line
}
fn today_background() -> Color {
    theme::blend_toward(theme::BG_CANVAS(), theme::BORDER_ACTIVE(), 0.16)
}
/// Tint the day's canvas after its contents are drawn, preserving event cards
/// and the stronger selection fill. Terminal-owned backgrounds remain untouched.
fn shade_today(frame: &mut Frame, area: Rect, date: NaiveDate, s: &CalendarState) {
    let canvas = theme::BG_CANVAS();
    if date != s.today() || canvas == Color::Reset {
        return;
    }
    let fill = today_background();
    let buffer = frame.buffer_mut();
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let cell = &mut buffer[(x, y)];
            if cell.bg == canvas {
                cell.set_bg(fill);
            }
        }
    }
}
/// Selection is a visible marker as well as a fill. Keep text readable even
/// when a palette uses its accent itself as the selection background.
pub(super) fn selection_style() -> Style {
    let style = theme::selection_style().add_modifier(Modifier::BOLD);
    if theme::BG_CANVAS() == Color::Reset {
        return style;
    }
    let mut fill = theme::BG_SELECTION();
    if theme::current_kind() == theme::ThemeKind::Contrast
        && theme::contrast_ratio(fill, theme::BG_CANVAS()).is_some_and(|ratio| ratio < 3.0)
    {
        // High Contrast promises a visible selection surface as well as text.
        // Keep this calendar treatment local; other screens retain their palette.
        for step in 1..=32 {
            let candidate = theme::blend_toward(fill, theme::BORDER_ACTIVE(), step as f32 / 32.0);
            if theme::contrast_ratio(candidate, theme::BG_CANVAS())
                .is_some_and(|ratio| ratio >= 3.0)
            {
                fill = candidate;
                break;
            }
        }
    }
    let style = style.bg(fill);
    let foreground = theme::TEXT_BRIGHT();
    if theme::contrast_ratio(foreground, fill).is_some_and(|ratio| ratio >= 4.5) {
        return style.fg(foreground);
    }
    let target =
        if theme::contrast_ratio(Color::Black, fill) >= theme::contrast_ratio(Color::White, fill) {
            Color::Black
        } else {
            Color::White
        };
    for step in 1..=32 {
        let candidate = theme::blend_toward(foreground, target, step as f32 / 32.0);
        if theme::contrast_ratio(candidate, fill).is_some_and(|ratio| ratio >= 4.5) {
            return style.fg(candidate);
        }
    }
    style.fg(target)
}
fn selected_line(mut line: Line<'static>, selected: bool) -> Line<'static> {
    if selected {
        line.spans.insert(0, Span::raw("▸"));
        patch_line(line, selection_style())
    } else {
        line
    }
}

/// Keep the selection marker and as much title as fits; indicate omitted text
/// without slicing a wide or combining grapheme in half.
pub(super) fn clipped_title(line: Line<'static>, width: u16) -> Line<'static> {
    if width < 3 || line.width() <= width as usize {
        return line;
    }
    let limit = width as usize - 1;
    let mut used = 0;
    let mut spans = Vec::new();
    'title: for span in &line.spans {
        for grapheme in span.styled_graphemes(line.style) {
            let cells = Span::raw(grapheme.symbol).width();
            if used + cells > limit {
                break 'title;
            }
            spans.push(Span::styled(grapheme.symbol.to_owned(), grapheme.style));
            used += cells;
        }
    }
    spans.push(Span::styled("…", line.style));
    Line::from(spans).style(line.style)
}
fn button_line(label: &str) -> Line<'static> {
    let mut line = if let Some((key, description)) = label.split_once(' ')
        && key.chars().count() == 1
    {
        hint_line(&[(key, description)])
    } else if let Some((description, key)) =
        label.strip_suffix(')').and_then(|s| s.rsplit_once(" ("))
    {
        Line::from(vec![
            Span::styled(format!(" {description}"), base()),
            Span::styled(" (", base().fg(theme::TEXT_FAINT())),
            Span::styled(key.to_owned(), key_style()),
            Span::styled(")", base().fg(theme::TEXT_FAINT())),
        ])
    } else {
        Line::from(vec![Span::styled(
            format!(" {label}"),
            if label == "Delete" {
                key_style()
            } else {
                bright().add_modifier(Modifier::BOLD)
            },
        )])
    };
    line.spans.push(Span::raw(" "));
    line
}
#[allow(clippy::too_many_arguments)] // Rendered geometry and action belong together.
pub(super) fn button(
    frame: &mut Frame,
    s: &CalendarState,
    x: &mut u16,
    y: u16,
    right: u16,
    label: &str,
    action: Action,
    selected: bool,
) {
    let mut line = button_line(label);
    if selected {
        for span in &mut line.spans {
            if span.style.fg == Some(theme::TEXT_DIM()) {
                span.style = bright().add_modifier(Modifier::BOLD);
            }
        }
    }
    let line = selected_line(line, selected);
    let w = line.width() as u16;
    if *x + w > right {
        return;
    }
    let rect = Rect::new(*x, y, w, 1);
    styled_row(frame, rect, line);
    hit(s, rect, action);
    *x += w;
}
pub fn source_label(s: &CalendarState) -> String {
    match s.source {
        CalendarSource::Server => "Server".into(),
        CalendarSource::Personal(id) if id == s.viewer => "Personal".into(),
        CalendarSource::Personal(id) => s
            .public
            .iter()
            .find(|p| p.owner_id == id)
            .map(|p| format!("@{}", p.username))
            .unwrap_or_else(|| "Shared calendar".into()),
    }
}
pub fn timing_label(e: &CalendarEvent, tz: chrono_tz::Tz) -> String {
    match e.timing {
        EventTiming::AllDay {
            start,
            end_exclusive,
        } => {
            let last = end_exclusive - Duration::days(1);
            if start == last {
                format!("{start} · all day")
            } else {
                format!("{start} – {last} · all day")
            }
        }
        EventTiming::Timed { start, end } => format!(
            "{}{}",
            local_time_label(start, tz, true),
            end.map(|e| format!(" – {}", local_time_label(e, tz, true)))
                .unwrap_or_default()
        ),
    }
}
fn local_time_label(instant: DateTime<Utc>, tz: chrono_tz::Tz, date: bool) -> String {
    let local = instant.with_timezone(&tz);
    let repeated = matches!(
        tz.from_local_datetime(&local.naive_local()),
        LocalResult::Ambiguous(..)
    );
    let format = match (date, repeated) {
        (true, true) => "%Y-%m-%d %H:%M %Z",
        (true, false) => "%Y-%m-%d %H:%M",
        (false, true) => "%H:%M %Z",
        (false, false) => "%H:%M",
    };
    local.format(format).to_string()
}
fn source_span(e: &CalendarEvent) -> Span<'static> {
    Span::styled(
        if e.owner_id.is_none() {
            "[Server]"
        } else {
            "[Personal]"
        },
        if e.owner_id.is_none() {
            base().fg(theme::AMBER_DIM())
        } else {
            dim()
        },
    )
}
fn timing_line(e: &CalendarEvent, tz: chrono_tz::Tz) -> Line<'static> {
    let mut spans = Vec::new();
    match e.timing {
        EventTiming::AllDay {
            start,
            end_exclusive,
        } => {
            spans.push(Span::styled(start.to_string(), muted()));
            let last = end_exclusive - Duration::days(1);
            if start != last {
                spans.push(Span::styled(" – ", base().fg(theme::TEXT_FAINT())));
                spans.push(Span::styled(last.to_string(), muted()));
            }
            spans.push(separator());
            spans.push(Span::styled("all day", dim()));
        }
        EventTiming::Timed { start, end } => {
            spans.push(Span::styled(local_time_label(start, tz, true), muted()));
            if let Some(end) = end {
                spans.push(Span::styled(" – ", base().fg(theme::TEXT_FAINT())));
                spans.push(Span::styled(local_time_label(end, tz, true), muted()));
            }
        }
    }
    Line::from(spans)
}
fn event_line(e: &CalendarEvent, tz: chrono_tz::Tz) -> Line<'static> {
    let time = match e.timing {
        EventTiming::AllDay { .. } => "all day".into(),
        EventTiming::Timed { start, .. } => local_time_label(start, tz, false),
    };
    let mut spans = Vec::new();
    if e.owner_id.is_none() {
        spans.push(source_span(e));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(time, muted()));
    spans.push(Span::raw(" "));
    spans.push(Span::styled(e.title.clone(), bright()));
    Line::from(spans)
}
fn labeled(label: &str, value: impl Into<String>, style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(label.to_owned(), dim()),
        Span::styled(": ", base().fg(theme::TEXT_FAINT())),
        Span::styled(value.into(), style),
    ])
}
pub fn draw(frame: &mut Frame, area: Rect, s: &CalendarState) {
    s.hits.borrow_mut().clear();
    s.panes.borrow_mut().clear();
    s.geometry.set(area);
    if area.width < 12 || area.height < 5 {
        row(frame, area, "Calendars · enlarge terminal", dim());
        return;
    }
    frame.render_widget(Paragraph::new("").style(base()), area);
    let header_height = super::toolbar::draw(frame, area, s);
    let panel_height = if area.height < 20 || area.width < 50 {
        1
    } else {
        5
    };
    let panel = Rect::new(
        area.x,
        area.bottom() - panel_height,
        area.width,
        panel_height,
    );
    draw_upcoming_panel(frame, panel, s);
    let body = Rect::new(
        area.x,
        area.y + header_height,
        area.width,
        area.height.saturating_sub(header_height + panel_height),
    );
    match s.view {
        CalendarView::Month => {
            if area.width >= 95 {
                let width = (body.width * 2 / 3).max(56);
                let grid = Rect::new(body.x, body.y, width, body.height);
                let agenda = Rect::new(
                    grid.right() + 1,
                    body.y,
                    body.width.saturating_sub(width + 1),
                    body.height,
                );
                draw_month(frame, grid, s);
                draw_agenda(frame, agenda, s);
            } else {
                draw_month(frame, body, s);
            }
        }
        CalendarView::List => draw_list(frame, body, s, false),
        _ => draw_hours(frame, body, s),
    }
}
#[allow(clippy::too_many_arguments)]
fn grid_rule(
    frame: &mut Frame,
    s: &CalendarState,
    area: Rect,
    widths: &[u16],
    y: u16,
    left: &str,
    middle: &str,
    right: &str,
) {
    let mut text = left.to_string();
    for (i, w) in widths.iter().enumerate() {
        text.push_str(&"─".repeat(w.saturating_sub(1) as usize));
        text.push_str(if i == 6 { right } else { middle });
    }
    row(frame, Rect::new(area.x, y, area.width, 1), text, rule());
    let _ = s;
}
fn draw_month(frame: &mut Frame, area: Rect, s: &CalendarState) {
    if area.width < 15 || area.height < 9 {
        styled_row(frame, area, hint_line(&[("Enter:", "selected-day agenda")]));
        hit(s, area, Action::Agenda(s.selected));
        return;
    }
    let available = area.width - 1;
    let mut widths = vec![available / 7; 7];
    for w in widths.iter_mut().take((available % 7) as usize) {
        *w += 1;
    }
    if area.height < 14 {
        let first = s.range().0;
        let mut x = area.x + 1;
        for (col, w) in widths.iter().enumerate() {
            row(
                frame,
                Rect::new(x, area.y, w - 1, 1),
                (first + Duration::days(col as i64))
                    .format("%a")
                    .to_string(),
                dim(),
            );
            x += w;
        }
        grid_rule(frame, s, area, &widths, area.y + 1, "╭", "┬", "╮");
        for week in 0..6 {
            let mut x = area.x;
            for (col, w) in widths.iter().enumerate() {
                let date = first + Duration::days((week * 7 + col) as i64);
                let count = s.day_events(date).len();
                let rect = Rect::new(x + 1, area.y + 2 + week as u16, w - 1, 1);
                let style = if date == s.selected {
                    selection_style()
                } else if date == s.today() {
                    accent()
                } else if date.month() != s.selected.month() {
                    dim()
                } else {
                    base()
                };
                let label = if count > 0 {
                    format!("{}+{count}", date.day())
                } else {
                    date.day().to_string()
                };
                // Keep counts intact before spending the remaining cells on markers.
                let spare =
                    usize::from(rect.width).saturating_sub(Line::from(label.as_str()).width());
                let marker = if date == s.today() && spare > usize::from(date == s.selected) {
                    "•"
                } else {
                    ""
                };
                row(frame, Rect::new(x, rect.y, 1, 1), "│", rule());
                let mut line =
                    Line::from(vec![Span::styled(format!("{marker}{}", date.day()), style)]);
                if count > 0 {
                    line.spans.push(Span::styled(
                        format!("+{count}"),
                        muted().add_modifier(Modifier::BOLD),
                    ));
                }
                let line = if date == s.selected && spare == 0 {
                    patch_line(line, selection_style())
                } else {
                    selected_line(line, date == s.selected)
                };
                styled_row(frame, rect, line);
                shade_today(frame, rect, date, s);
                hit(s, rect, Action::Date(date));
                x += w;
            }
            row(
                frame,
                Rect::new(area.right() - 1, area.y + 2 + week as u16, 1, 1),
                "│",
                rule(),
            );
        }
        grid_rule(frame, s, area, &widths, area.y + 8, "╰", "┴", "╯");
        return;
    }
    let first = s.range().0;
    let today = s.today();
    let mut x = area.x + 1;
    for (col, width) in widths.iter().enumerate() {
        row(
            frame,
            Rect::new(x, area.y, width.saturating_sub(1), 1),
            (first + Duration::days(col as i64))
                .format("%a")
                .to_string(),
            dim(),
        );
        x += width;
    }
    // A short month is still six complete weeks. Cells have at least a date
    // line; crowded short layouts place explicit counts on that line.
    let total = area.height - 2;
    let mut heights = [total / 6; 6];
    for h in heights.iter_mut().take((total % 6) as usize) {
        *h += 1;
    }
    let mut y = area.y + 1;
    grid_rule(frame, s, area, &widths, y, "╭", "┬", "╮");
    for (week, height) in heights.iter().enumerate() {
        let mut x = area.x;
        for (col, width) in widths.iter().enumerate() {
            let date = first + Duration::days((week * 7 + col) as i64);
            let events = s.day_events(date);
            let cell = Rect::new(
                x + 1,
                y + 1,
                width.saturating_sub(1),
                height.saturating_sub(1),
            );
            if cell.height > 0 {
                let style = if date == s.selected {
                    selection_style()
                } else if date == today {
                    accent()
                } else if date.month() != s.selected.month() {
                    dim()
                } else {
                    base()
                };
                let capacity = cell.height.saturating_sub(1) as usize;
                let mut date_label = Line::styled(
                    format!("{}{}", if date == today { "•" } else { "" }, date.day()),
                    style,
                );
                if capacity == 0 && !events.is_empty() {
                    date_label.spans.push(Span::styled(
                        format!(" +{}", events.len()),
                        muted().add_modifier(Modifier::BOLD),
                    ));
                }
                styled_row(
                    frame,
                    Rect::new(cell.x, cell.y, cell.width, 1),
                    selected_line(date_label, date == s.selected),
                );
                hit(s, cell, Action::Date(date));
                let previews = if events.len() > capacity {
                    capacity.saturating_sub(1)
                } else {
                    capacity
                };
                for (n, e) in events.iter().take(previews).enumerate() {
                    let rect = Rect::new(cell.x, cell.y + 1 + n as u16, cell.width, 1);
                    let mut spans = Vec::new();
                    if s.source.owner().is_some() && e.owner_id.is_none() {
                        spans.push(source_span(e));
                        spans.push(Span::raw(" "));
                    }
                    spans.push(Span::styled(e.title.clone(), bright()));
                    styled_row(frame, rect, Line::from(spans));
                    hit(s, rect, Action::EventAt(e.id, date));
                }
                if events.len() > previews && capacity > 0 {
                    let rect = Rect::new(cell.x, cell.y + cell.height - 1, cell.width, 1);
                    row(
                        frame,
                        rect,
                        format!("+{} more", events.len() - previews),
                        dim(),
                    );
                    hit(s, rect, Action::Agenda(date));
                }
                shade_today(frame, cell, date, s);
            }
            for line in 1..*height {
                row(frame, Rect::new(x, y + line, 1, 1), "│", rule());
            }
            x += width;
        }
        for line in 1..*height {
            row(
                frame,
                Rect::new(area.right() - 1, y + line, 1, 1),
                "│",
                rule(),
            );
        }
        y += height;
        grid_rule(
            frame,
            s,
            area,
            &widths,
            y,
            if week == 5 { "╰" } else { "├" },
            if week == 5 { "┴" } else { "┼" },
            if week == 5 { "╯" } else { "┤" },
        );
    }
}
fn draw_agenda(frame: &mut Frame, area: Rect, s: &CalendarState) {
    let mut title = vec![
        Span::styled(
            format!(" {}", s.selected),
            bright()
                .add_modifier(Modifier::BOLD)
                .bg(if s.selected == s.today() {
                    today_background()
                } else {
                    theme::BG_CANVAS()
                }),
        ),
        separator(),
    ];
    title.extend(hint_line(&[("Enter", "details")]).spans.into_iter().skip(1));
    title.push(Span::raw(" "));
    let b = border(Line::from(title));
    let inner = b.inner(area);
    frame.render_widget(b, area);
    pane(s, inner, Pane::Agenda);
    s.agenda_rows.set(inner.height as usize);
    let events = s.day_events(s.selected);
    s.max_agenda.set(
        events
            .len()
            .saturating_mul(2)
            .saturating_sub(inner.height as usize),
    );
    if events.is_empty() {
        let mut line = Line::from(vec![Span::styled("No events", dim()), separator()]);
        line.spans
            .extend(hint_line(&[("n", "to add")]).spans.into_iter().skip(1));
        styled_row(frame, inner, line);
        return;
    }
    for (n, e) in events.iter().enumerate() {
        let y = (n * 2) as isize - s.agenda_scroll as isize;
        if y + 1 < 0 || y >= inner.height as isize {
            continue;
        }
        let selected = s.selection == Selection::Event(e.id);
        if y >= 0 {
            styled_row(
                frame,
                Rect::new(inner.x, inner.y + y as u16, inner.width, 1),
                selected_line(event_line(e, s.tz), selected),
            );
        }
        if y + 1 < inner.height as isize {
            styled_row(
                frame,
                Rect::new(inner.x, inner.y + (y + 1) as u16, inner.width, 1),
                if selected {
                    patch_line(timing_line(e, s.tz), selection_style())
                } else {
                    timing_line(e, s.tz)
                },
            );
        }
        let top = y.max(0) as u16;
        let bottom = (y + 2).min(inner.height as isize) as u16;
        hit(
            s,
            Rect::new(inner.x, inner.y + top, inner.width, bottom - top),
            Action::Event(e.id),
        );
    }
}
/// Greedy interval partitioning: each overlapping segment receives a distinct
/// lane; intervals meeting exactly at an end boundary may reuse a lane.
#[derive(Clone, Debug)]
pub struct Segment {
    pub event: usize,
    pub start: u32,
    pub end: u32,
    pub lane: usize,
    pub before: bool,
    pub after: bool,
    instant_start: DateTime<Utc>,
    instant_end: DateTime<Utc>,
}
pub fn segments(events: &[CalendarEvent], date: NaiveDate, tz: chrono_tz::Tz) -> Vec<Segment> {
    let mut segments = Vec::new();
    for (i, e) in events.iter().enumerate() {
        if let EventTiming::Timed { start, end } = e.timing {
            let end = end.unwrap_or(start + Duration::hours(1));
            let instant_start = start;
            let instant_end = end;
            let start = start.with_timezone(&tz);
            let end = end.with_timezone(&tz);
            if start.date_naive() > date
                || end.date_naive() < date
                || (end.date_naive() == date && end.time() == chrono::NaiveTime::MIN)
            {
                continue;
            }
            let before = start.date_naive() < date;
            let after = end.date_naive() > date;
            let a = if before {
                0
            } else {
                start.hour() * 60 + start.minute()
            };
            let b = if after {
                1440
            } else {
                end.hour() * 60 + end.minute()
            };
            segments.push(Segment {
                event: i,
                start: a,
                end: b.max(a + 1),
                lane: 0,
                before,
                after,
                instant_start,
                instant_end,
            });
        }
    }
    segments.sort_by_key(|s| (s.start, s.end, s.event));
    let mut lanes: Vec<Vec<usize>> = Vec::new();
    for i in 0..segments.len() {
        let s = &segments[i];
        // A repeated DST hour can reverse wall-clock endpoints. Both actual
        // overlaps and collisions on the civil-hour grid need distinct lanes.
        let lane = lanes
            .iter()
            .position(|previous| {
                previous.iter().all(|&j| {
                    let p = &segments[j];
                    p.end.div_ceil(30) <= s.start / 30
                        && (p.instant_end <= s.instant_start || s.instant_end <= p.instant_start)
                })
            })
            .unwrap_or(lanes.len());
        if lane == lanes.len() {
            lanes.push(Vec::new());
        }
        lanes[lane].push(i);
        segments[i].lane = lane;
    }
    segments
}
fn draw_hours(frame: &mut Frame, area: Rect, s: &CalendarState) {
    if area.width < 12 || area.height < 5 {
        styled_row(
            frame,
            area,
            hint_line(&[("Enter:", "agenda"), ("PgUp/PgDn", "scroll hours")]),
        );
        return;
    }
    let (from, to) = s.range();
    let n = (to - from).num_days();
    let timeline_y = area.y + 5;
    let visible = area.height - 5;
    s.hour_rows.set(visible as usize);
    let max_scroll = 48usize.saturating_sub(visible.min(48) as usize);
    let scroll = s.hour_scroll.min(max_scroll) as u32 * 30;
    pane(s, area, Pane::Grid);
    styled_row(
        frame,
        Rect::new(area.x, area.y, area.width, 1),
        hint_line(&[("PgUp/PgDn", "hours"), ("Ctrl+←/→", "columns")]),
    );
    for r in 0..visible {
        let minute = scroll + r as u32 * 30;
        if minute >= 1440 {
            break;
        }
        row(
            frame,
            Rect::new(area.x, timeline_y + r, 5, 1),
            if minute.is_multiple_of(60) {
                format!("{:02}:00", minute / 60)
            } else {
                "    ·".into()
            },
            if minute.is_multiple_of(60) {
                muted()
            } else {
                rule()
            },
        );
    }
    let viewport = Rect::new(area.x + 6, area.y + 1, area.width - 6, area.height - 1);
    let mut columns: Vec<_> = (0..n)
        .map(|day| {
            let date = from + Duration::days(day);
            let pieces = segments(&s.events, date, s.tz);
            let lanes = pieces.iter().map(|p| p.lane + 1).max().unwrap_or(1);
            (
                date,
                pieces,
                lanes.saturating_mul(12).min(u16::MAX as usize) as u16,
                lanes,
            )
        })
        .collect();
    let minimum: usize = columns.iter().map(|(_, _, width, _)| *width as usize).sum();
    let extra = (viewport.width as usize).saturating_sub(minimum);
    for (index, (_, _, width, _)) in columns.iter_mut().enumerate() {
        *width += (extra / n as usize + usize::from(index < extra % n as usize)) as u16;
    }
    if s.reveal_selected.replace(false) || s.hours_geometry.get() != viewport {
        let mut offset = 0;
        for (date, _, width, _) in &columns {
            if *date == s.selected {
                let visible_width = (*width as usize).min(viewport.width as usize);
                let current = s.day_scroll.get();
                if offset < current || offset + visible_width > current + viewport.width as usize {
                    s.day_scroll
                        .set(if *width as usize >= viewport.width as usize {
                            offset
                        } else {
                            (offset + visible_width).saturating_sub(viewport.width as usize)
                        });
                }
                break;
            }
            offset += *width as usize;
        }
    }
    s.hours_geometry.set(viewport);
    let selected_event = s.selected_event().map(|event| event.id);
    let total_width: usize = columns.iter().map(|(_, _, width, _)| *width as usize).sum();
    s.max_days
        .set(total_width.saturating_sub(viewport.width as usize));
    if !s.loading && s.reveal_event.replace(false) {
        let mut offset = 0;
        for (date, pieces, width, lanes) in &columns {
            if *date == s.selected
                && let Some(piece) = pieces
                    .iter()
                    .find(|piece| selected_event == Some(s.events[piece.event].id))
            {
                let left = offset + piece.lane * *width as usize / lanes;
                let right = offset + (piece.lane + 1) * *width as usize / lanes;
                let current = s.day_scroll.get();
                if left < current {
                    s.day_scroll.set(left);
                } else if right > current + viewport.width as usize {
                    s.day_scroll
                        .set(right.saturating_sub(viewport.width as usize));
                }
                break;
            }
            offset += *width as usize;
        }
    }
    s.day_scroll.set(s.day_scroll.get().min(s.max_days.get()));
    let mut virtual_x = 0i32;
    for (date, pieces, width, lanes) in columns {
        let origin = viewport.x as i32 + virtual_x - s.day_scroll.get() as i32;
        let clip = |x: i32, y: u16, w: u16, h: u16| -> Rect {
            let left = x.max(viewport.x as i32);
            let right = (x + w as i32).min(viewport.right() as i32);
            Rect::new(
                left.max(0) as u16,
                y,
                right.saturating_sub(left).max(0) as u16,
                h,
            )
        };
        let head = clip(origin, area.y + 1, width - 1, 1);
        let style = if date == s.today() || date == s.selected {
            accent()
        } else {
            muted()
        };
        styled_row(
            frame,
            head,
            selected_line(
                Line::styled(date.format("%a %b %d").to_string(), style),
                date == s.selected,
            ),
        );
        hit(s, head, Action::Date(date));
        let all: Vec<_> = s
            .day_events(date)
            .into_iter()
            .filter(|e| matches!(e.timing, EventTiming::AllDay { .. }))
            .collect();
        for (i, e) in all.iter().take(2).enumerate() {
            let rect = clip(origin, area.y + 2 + i as u16, width - 1, 1);
            styled_row(
                frame,
                rect,
                selected_line(event_line(e, s.tz), selected_event == Some(e.id)),
            );
            hit(s, rect, Action::EventAt(e.id, date));
        }
        if all.len() > 2 {
            let rect = clip(origin, area.y + 4, width - 1, 1);
            row(frame, rect, format!("+{} all-day", all.len() - 2), dim());
            hit(s, rect, Action::Agenda(date));
        }
        for r in 0..visible {
            let minute = scroll + r as u32 * 30;
            if minute >= 1440 {
                break;
            }
            let rect = clip(origin, timeline_y + r, width - 1, 1);
            let selected = date == s.selected && s.selection == Selection::Slot(minute as u16);
            let text = if selected {
                format!("▸ {:02}:{:02}", minute / 60, minute % 60)
            } else if minute.is_multiple_of(60) {
                "─".repeat(width as usize - 1)
            } else {
                " ".repeat(width as usize - 1)
            };
            row(
                frame,
                rect,
                text,
                if selected { selection_style() } else { rule() },
            );
            hit(s, rect, Action::Slot(date, minute as u16));
        }
        for piece in pieces {
            let bottom = scroll + visible as u32 * 30;
            if piece.end <= scroll || piece.start >= bottom {
                continue;
            }
            let y = ((piece.start.max(scroll) - scroll) / 30) as u16;
            let end = ((piece.end.min(bottom) - scroll).div_ceil(30)) as u16;
            let lane_start = piece.lane * width as usize / lanes;
            let lane_end = (piece.lane + 1) * width as usize / lanes;
            let rect = clip(
                origin + lane_start as i32,
                timeline_y + y,
                (lane_end - lane_start).saturating_sub(1) as u16,
                (end - y).max(1).min(visible - y),
            );
            let e = &s.events[piece.event];
            let selected = selected_event == Some(e.id);
            let mut spans = Vec::new();
            if selected {
                spans.push(Span::raw("▸"));
            }
            if piece.before || piece.start < scroll {
                spans.push(Span::styled("↑", accent()));
            }
            if piece.after || piece.end > bottom {
                spans.push(Span::styled("↓", accent()));
            }
            spans.push(Span::styled(e.title.clone(), bright()));
            let mut metadata = Vec::new();
            if let EventTiming::Timed { start, .. } = e.timing {
                metadata.push(Span::styled(local_time_label(start, s.tz, false), muted()));
            }
            if e.owner_id.is_none() {
                metadata.push(separator());
                metadata.push(source_span(e));
            }
            let fill = if selected {
                selection_style()
            } else if theme::BG_CANVAS() == ratatui::style::Color::Reset {
                Style::default()
            } else {
                Style::default().bg(theme::BG_HIGHLIGHT())
            };
            // Clear the hour rules before painting a card, including its padding.
            frame.render_widget(Clear, rect);
            frame.render_widget(
                Paragraph::new(vec![
                    clipped_title(patch_line(Line::from(spans), fill), rect.width),
                    patch_line(Line::from(metadata), fill),
                ])
                .style(base().patch(fill)),
                rect,
            );
            hit(s, rect, Action::EventAt(e.id, date));
        }
        shade_today(
            frame,
            clip(origin, viewport.y, width - 1, viewport.height),
            date,
            s,
        );
        virtual_x += width as i32;
    }
    s.max_days
        .set((virtual_x - viewport.width as i32).max(0) as usize);
}
fn list_lines<'a>(
    s: &CalendarState,
    events: &[&'a CalendarEvent],
    upcoming: bool,
) -> Vec<(Line<'static>, Option<&'a CalendarEvent>, usize)> {
    let mut rows = Vec::new();
    let mut last = None;
    for (n, e) in events.iter().enumerate() {
        let date = e.timing.dates(s.tz).0.max(if upcoming {
            NaiveDate::MIN
        } else {
            month_start(s.selected)
        });
        if last != Some(date) {
            rows.push((
                Line::styled(
                    date.format("%A, %B %d, %Y").to_string(),
                    accent().bg(if date == s.today() {
                        today_background()
                    } else {
                        theme::BG_CANVAS()
                    }),
                ),
                None,
                n,
            ));
            last = Some(date);
        }
        rows.push((event_line(e, s.tz), Some(*e), n));
    }
    rows
}
fn draw_list(frame: &mut Frame, area: Rect, s: &CalendarState, upcoming: bool) {
    let owned = s.upcoming();
    let events = if upcoming {
        owned.iter().collect()
    } else {
        s.ordered_events()
    };
    let rows = list_lines(s, &events, upcoming);
    s.max_scroll
        .set(rows.len().saturating_sub(area.height as usize));
    s.list_rows.set(area.height as usize);
    pane(s, area, if upcoming { Pane::Upcoming } else { Pane::List });
    if rows.is_empty() {
        row(
            frame,
            area,
            if upcoming {
                "No upcoming notices"
            } else {
                "No events this month"
            },
            dim(),
        );
    }
    for (r, (text, event, _)) in rows
        .iter()
        .skip(s.scroll.min(s.max_scroll.get()))
        .take(area.height as usize)
        .enumerate()
    {
        let rect = Rect::new(area.x, area.y + r as u16, area.width, 1);
        styled_row(
            frame,
            rect,
            selected_line(
                text.clone(),
                event.is_some_and(|e| s.selection == Selection::Event(e.id)),
            ),
        );
        if let Some(e) = event {
            hit(s, rect, Action::Event(e.id));
        }
    }
}
pub fn draw_upcoming_panel(frame: &mut Frame, area: Rect, s: &CalendarState) {
    let notices = s.upcoming();
    let heading = Line::from(vec![
        Span::raw(" "),
        Span::styled("u", key_style()),
        Span::styled(" Upcoming events", bright().add_modifier(Modifier::BOLD)),
        Span::styled(format!(" ({}) ", notices.len()), dim()),
    ]);
    if area.height == 1 {
        let mut heading = heading;
        heading.spans.remove(0);
        styled_row(frame, area, heading);
        hit(s, area, Action::Upcoming);
        return;
    }
    let b = border(heading);
    let inner = b.inner(area);
    frame.render_widget(b, area);
    hit(s, area, Action::Upcoming);
    if notices.is_empty() {
        row(frame, inner, "No upcoming notices", dim());
    }
    for (i, e) in notices
        .iter()
        .take(3)
        .take(inner.height as usize)
        .enumerate()
    {
        let rect = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        let mut spans = vec![
            source_span(e),
            Span::raw(" "),
            Span::styled(e.title.clone(), bright()),
            separator(),
        ];
        spans.extend(timing_line(e, s.tz).spans);
        styled_row(frame, rect, Line::from(spans));
        hit(s, rect, Action::Event(e.id));
    }
    if notices.len() > 3 && inner.height > 0 {
        let label = Line::from(vec![
            Span::styled(format!("+{} more", notices.len() - 3), muted()),
            separator(),
            Span::styled("u", key_style()),
        ]);
        let w = (label.width() as u16).min(inner.width);
        let rect = Rect::new(inner.right() - w, area.y, w, 1);
        styled_row(frame, rect, label);
        hit(s, rect, Action::Upcoming);
    }
}
fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let width = w.min(area.width);
    let height = h.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}
pub fn draw_modal(frame: &mut Frame, area: Rect, s: &CalendarState) {
    draw_modal_content(frame, area, s);
    if let Some(menu) = &s.context_menu {
        s.hits.borrow_mut().clear();
        s.panes.borrow_mut().clear();
        let width = menu
            .items
            .iter()
            .map(|item| Line::from(item.label()).width())
            .max()
            .unwrap_or(0) as u16
            + 5;
        let width = width.min(area.width);
        let height = (menu.items.len() as u16 + 2).min(area.height);
        let rect = Rect::new(
            menu.anchor
                .0
                .clamp(area.x, area.right().saturating_sub(width)),
            menu.anchor
                .1
                .clamp(area.y, area.bottom().saturating_sub(height)),
            width,
            height,
        );
        menu.area.set(rect);
        frame.render_widget(Clear, rect);
        let block = border(" Actions ").border_style(accent());
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        for (i, item) in menu.items.iter().enumerate().take(inner.height as usize) {
            let row = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
            styled_row(
                frame,
                row,
                selected_line(Line::styled(item.label(), bright()), i == menu.selected),
            );
            hit(s, row, Action::MenuChoice(i));
        }
    }
}

pub(super) fn picker(
    frame: &mut Frame,
    inner: Rect,
    s: &CalendarState,
    labels: &[Line<'static>],
    selected: usize,
) {
    let height = inner.height.saturating_sub(2).max(1);
    let content = Rect::new(inner.x, inner.y, inner.width, height);
    picker_rows(frame, content, s, labels, selected);
    if inner.height >= 2 {
        let mut x = inner.x;
        button(
            frame,
            s,
            &mut x,
            inner.bottom() - 1,
            inner.right(),
            "Close (Esc)",
            Action::Cancel,
            false,
        );
    }
}

pub(super) fn picker_rows(
    frame: &mut Frame,
    content: Rect,
    s: &CalendarState,
    labels: &[Line<'static>],
    selected: usize,
) {
    let height = content.height;
    if height == 0 {
        return;
    }
    let maximum = labels.len().saturating_sub(height as usize);
    s.picker_rows.set(height as usize);
    s.max_picker.set(maximum);
    let mut offset = s.picker_scroll.get().min(maximum);
    if s.picker_reveal.replace(false) {
        if selected < offset {
            offset = selected;
        } else if selected >= offset + height as usize {
            offset = selected + 1 - height as usize;
        }
    }
    s.picker_scroll.set(offset);
    pane(s, content, Pane::Picker);
    for (i, label) in labels.iter().enumerate().skip(offset).take(height as usize) {
        let row = Rect::new(content.x, content.y + (i - offset) as u16, content.width, 1);
        styled_row(frame, row, selected_line(label.clone(), i == selected));
        hit(s, row, Action::Choice(i));
    }
}

fn draw_modal_content(frame: &mut Frame, area: Rect, s: &CalendarState) {
    let Some(modal) = &s.modal else {
        return;
    };
    s.hits.borrow_mut().clear();
    s.panes.borrow_mut().clear();
    let rect = centered(
        area,
        76,
        match modal {
            Modal::Editor(_) => 33,
            Modal::Details(_) => 24,
            Modal::Settings { .. } => 12,
            Modal::Go(_) => 12,
            Modal::Source(_) => ((s.public.len() + 6).min(24)) as u16,
            Modal::View(_) => 9,
            _ => 20,
        },
    );
    frame.render_widget(Clear, rect);
    let title = match modal {
        Modal::Editor(_) => " Event editor ",
        Modal::Details(_) => " Event details ",
        Modal::Settings { .. } => " Calendar Settings ",
        Modal::Source(_) => " Calendar source ",
        Modal::View(_) => " Calendar view ",
        Modal::Go(_) => " Go to date ",
        Modal::Import(_) => " Import iCalendar event ",
        Modal::Delete(_) => " Delete event? ",
        Modal::Upcoming => " Upcoming events ",
        Modal::Agenda => " Selected-day agenda ",
    };
    let block = border(title)
        .title_style(accent())
        .border_style(base().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(rect).inner(Margin::new(1, 0));
    frame.render_widget(block, rect);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    match modal {
        Modal::Import(import) => super::import::draw(frame, inner, s, import),
        Modal::Source(selected) => {
            let labels: Vec<_> = std::iter::once(Line::styled("Server", bright()))
                .chain(std::iter::once(Line::styled(
                    "My personal calendar",
                    bright(),
                )))
                .chain(s.public.iter().map(|p| {
                    Line::from(vec![
                        Span::styled(format!("@{}", p.username), bright()),
                        separator(),
                        Span::styled("public, read-only", dim()),
                    ])
                }))
                .collect();
            picker(frame, inner, s, &labels, *selected);
        }
        Modal::View(selected) => {
            let labels: Vec<_> = CalendarView::ALL
                .iter()
                .map(|v| Line::styled(v.label(), bright()))
                .collect();
            picker(frame, inner, s, &labels, *selected);
        }
        Modal::Go(input) => {
            let mut ta = (**input).clone();
            ta.set_style(base());
            ta.set_cursor_line_style(base());
            ta.set_cursor_style(selection_style());
            frame.render_widget(&ta, Rect::new(inner.x, inner.y, inner.width, 1));
            if inner.height >= 3 {
                let text = input.lines().join("");
                let result = date_entry::parse(&text, s.selected, s.today());
                let preview = if let Some(error) = &s.error {
                    Line::styled(error, base().fg(theme::ERROR()))
                } else if let Ok(date) = result {
                    Line::from(vec![
                        Span::styled("Go to ", dim()),
                        Span::styled(date.to_string(), accent()),
                        separator(),
                        Span::styled(date.format("%A").to_string(), muted()),
                    ])
                } else {
                    Line::styled("Enter a date or calendar offset", dim())
                };
                let lines = vec![
                    preview,
                    Line::from(vec![
                        Span::styled("Offsets from ", dim()),
                        Span::styled(s.selected.to_string(), bright()),
                    ]),
                    Line::from(vec![
                        Span::styled("Today ", dim()),
                        Span::styled(s.today().to_string(), bright()),
                        separator(),
                        Span::styled(s.tz.to_string(), dim()),
                    ]),
                    Line::from(vec![
                        Span::styled("Oct 2, 2026", muted()),
                        separator(),
                        Span::styled("2 Oct", muted()),
                        separator(),
                        Span::styled("2026/10/2", muted()),
                    ]),
                    Line::from(vec![
                        Span::styled("2 months ago", muted()),
                        separator(),
                        Span::styled("in 3 weeks", muted()),
                        separator(),
                        Span::styled("+2w", muted()),
                    ]),
                ];
                frame.render_widget(
                    Paragraph::new(lines)
                        .wrap(Wrap { trim: false })
                        .style(base()),
                    Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 2),
                );
                let mut x = inner.x;
                button(
                    frame,
                    s,
                    &mut x,
                    inner.bottom() - 1,
                    inner.right(),
                    "Go (Enter)",
                    Action::Save,
                    false,
                );
                button(
                    frame,
                    s,
                    &mut x,
                    inner.bottom() - 1,
                    inner.right(),
                    "Cancel (Esc)",
                    Action::Cancel,
                    false,
                );
            }
        }
        Modal::Settings { draft, focus } => {
            let labels = [
                labeled(
                    "Week starts",
                    if draft.week_start == 0 {
                        "Monday"
                    } else {
                        "Sunday"
                    },
                    bright(),
                ),
                labeled("Default view", draft.default_view.label(), bright()),
                labeled(
                    "Server overlay",
                    if draft.server_overlay {
                        "Enabled"
                    } else {
                        "Disabled"
                    },
                    bright(),
                ),
                labeled(
                    "Personal calendar",
                    if draft.public {
                        "Public to signed-in users"
                    } else {
                        "Private"
                    },
                    bright(),
                ),
                Line::styled("Save", bright().add_modifier(Modifier::BOLD)),
                Line::styled("Cancel", dim()),
            ];
            for (i, label) in labels.iter().enumerate().take(inner.height as usize) {
                let r = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
                styled_row(frame, r, selected_line(label.clone(), i == *focus));
                hit(
                    s,
                    r,
                    if i == 4 {
                        Action::Save
                    } else if i == 5 {
                        Action::Cancel
                    } else {
                        Action::ToggleField(i)
                    },
                );
            }
            if inner.height > 7 {
                let line = s.error.as_ref().map_or_else(
                    || {
                        hint_line(&[
                            ("Tab", "focus"),
                            ("Enter/Space", "change"),
                            ("Ctrl+S", "save"),
                        ])
                    },
                    |error| Line::styled(error.clone(), base().fg(theme::ERROR())),
                );
                styled_row(frame, Rect::new(inner.x, inner.y + 7, inner.width, 1), line);
            }
        }
        Modal::Editor(e) => super::editor::draw(frame, inner, s, e),
        Modal::Delete(e) => {
            styled_row(
                frame,
                inner,
                Line::from(vec![
                    Span::styled("Delete “", dim()),
                    Span::styled(e.title.clone(), bright().add_modifier(Modifier::BOLD)),
                    Span::styled("”?", dim()),
                ]),
            );
            let mut x = inner.x;
            button(
                frame,
                s,
                &mut x,
                (inner.y + 2).min(inner.bottom() - 1),
                inner.right(),
                "y Delete",
                Action::Save,
                false,
            );
            button(
                frame,
                s,
                &mut x,
                (inner.y + 2).min(inner.bottom() - 1),
                inner.right(),
                "n Cancel",
                Action::Cancel,
                false,
            );
            if let Some(error) = &s.error
                && inner.height > 4
            {
                row(
                    frame,
                    Rect::new(inner.x, inner.y + 4, inner.width, 1),
                    error,
                    base().fg(theme::ERROR()),
                );
            }
        }
        Modal::Details(e) => {
            let access = event_access(e, s.viewer, s.role);
            let mut lines = vec![
                Line::styled(&e.title, bright().add_modifier(Modifier::BOLD)),
                timing_line(e, s.tz),
                Line::from(vec![
                    source_span(e),
                    separator(),
                    Span::styled(s.tz.to_string(), dim()),
                    separator(),
                    Span::styled(format!("revision {}", e.revision), dim()),
                ]),
                Line::from(""),
            ];
            lines.extend(e.description.lines().map(|s| Line::from(s.to_owned())));
            if access.notifications {
                lines.push(labeled(
                    "Notifications",
                    e.notice_lead_seconds
                        .map(|n| {
                            format!(
                                "{} before",
                                humantime::format_duration(std::time::Duration::from_secs(
                                    n as u64
                                ))
                            )
                        })
                        .unwrap_or_else(|| "disabled".into()),
                    if e.notice_lead_seconds.is_some() {
                        bright()
                    } else {
                        dim()
                    },
                ));
            }
            let content = Rect::new(
                inner.x,
                inner.y,
                inner.width,
                inner.height.saturating_sub(2),
            );
            pane(s, content, Pane::List);
            let p = Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .style(base());
            s.max_scroll.set(
                p.line_count(content.width)
                    .saturating_sub(content.height as usize),
            );
            frame.render_widget(
                p.scroll((s.scroll.min(s.max_scroll.get()) as u16, 0)),
                content,
            );
            let mut x = inner.x;
            if access.edit {
                button(
                    frame,
                    s,
                    &mut x,
                    inner.bottom() - 1,
                    inner.right(),
                    "e Edit",
                    Action::Edit,
                    false,
                );
                button(
                    frame,
                    s,
                    &mut x,
                    inner.bottom() - 1,
                    inner.right(),
                    "Delete",
                    Action::Delete,
                    false,
                );
            }
            button(
                frame,
                s,
                &mut x,
                inner.bottom() - 1,
                inner.right(),
                "y Copy iCal",
                Action::Copy,
                false,
            );
            button(
                frame,
                s,
                &mut x,
                inner.bottom() - 1,
                inner.right(),
                "Close",
                Action::Cancel,
                false,
            );
        }
        Modal::Upcoming | Modal::Agenda => {
            let content = Rect::new(
                inner.x,
                inner.y,
                inner.width,
                inner.height.saturating_sub(2),
            );
            if matches!(modal, Modal::Upcoming) {
                draw_list(frame, content, s, true);
            } else {
                draw_agenda(frame, content, s);
            }
            let mut x = inner.x;
            if matches!(modal, Modal::Agenda)
                && (s.source == CalendarSource::Personal(s.viewer)
                    || (s.source == CalendarSource::Server
                        && s.role != late_core::models::calendar::CreationTier::User))
            {
                button(
                    frame,
                    s,
                    &mut x,
                    inner.bottom() - 1,
                    inner.right(),
                    "n New",
                    Action::New,
                    false,
                );
            }
            button(
                frame,
                s,
                &mut x,
                inner.bottom() - 1,
                inner.right(),
                "Close (Esc)",
                Action::Cancel,
                false,
            );
        }
    }
    if s.pending {
        row(
            frame,
            Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
            "Saving…",
            accent(),
        );
    }
}
