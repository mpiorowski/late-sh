//! Theme resolution and hit geometry happen during this frame, never at load.
use super::{
    navigation::Selection,
    state::{Action, CalendarState, Hit, Modal, Pane, ScrollPane, month_start},
};
use crate::app::common::{primitives::hint_line, theme};
use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, TimeZone, Utc};
use late_core::models::calendar::{CalendarEvent, CalendarView, EventTiming, event_access};
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
/// A board post's title: warm, the colour the Live panel gives events.
pub(super) fn board_title() -> Style {
    base().fg(theme::AMBER_DIM())
}
/// The viewer's own event's title: green, a colour the board never uses.
pub(super) fn own_title() -> Style {
    base().fg(theme::SUCCESS())
}
/// The title style for an event of either kind.
pub(super) fn title_style(e: &CalendarEvent) -> Style {
    if e.is_board() {
        board_title()
    } else {
        own_title()
    }
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
/// Tint the day's canvas after its contents are drawn, preserving the
/// stronger selection fill. Terminal-owned backgrounds remain untouched.
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
/// The page's cursor: an amber dot before the line and no fill, so the
/// grid stays quiet and the eye still finds what Enter opens. Modals keep
/// the filled `selected_line` for their focused control.
fn marked_line(mut line: Line<'static>, marked: bool) -> Line<'static> {
    if marked {
        line.spans.insert(0, Span::styled("● ", accent()));
    }
    line
}
/// A row in a column of rows (the agenda, the list): the dot's two cells
/// are kept on every row, so the text never shifts as the mark moves.
fn marked_row(mut line: Line<'static>, marked: bool) -> Line<'static> {
    if marked {
        line.spans.insert(0, Span::styled("● ", accent()));
    } else {
        line.spans.insert(0, Span::raw("  "));
    }
    line
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
            bright().add_modifier(Modifier::BOLD),
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
pub(super) fn local_time_label(instant: DateTime<Utc>, tz: chrono_tz::Tz, date: bool) -> String {
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
/// `·6 ` before a board event's title, so a narrow cell clips the title
/// and never the count; nothing while nobody said so.
fn going_span(e: &CalendarEvent) -> Option<Span<'static>> {
    (e.is_board() && e.going > 0)
        .then(|| Span::styled(format!("·{} ", e.going), base().fg(theme::AMBER_DIM())))
}
/// Who the event is from: the poster's name on the board, `you` on a
/// personal event.
fn source_span(e: &CalendarEvent) -> Span<'static> {
    if e.is_board() {
        Span::styled(format!("by {}", e.creator_name), dim())
    } else {
        Span::styled("just you", dim())
    }
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
    spans.push(separator());
    spans.push(source_span(e));
    Line::from(spans)
}
/// One row for an event: the time, the title, the count. A personal event
/// reads dimmer than a board post, so the two tell apart in one grid.
fn event_line(e: &CalendarEvent, tz: chrono_tz::Tz) -> Line<'static> {
    let time = match e.timing {
        EventTiming::AllDay { .. } => "all day".into(),
        EventTiming::Timed { start, .. } => local_time_label(start, tz, false),
    };
    let mut spans = vec![Span::styled(time, muted()), Span::raw(" ")];
    spans.extend(going_span(e));
    spans.push(Span::styled(e.title.clone(), title_style(e)));
    Line::from(spans)
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
    // The foot says what the keys do to what is selected; a short terminal
    // keeps the row for the grid.
    let footer = (area.height >= 14).then(|| Rect::new(area.x, area.bottom() - 1, area.width, 1));
    if let Some(footer) = footer {
        styled_row(frame, footer, hint_line(&footer_hints(s)));
    }
    let body = Rect::new(
        area.x,
        area.y + header_height,
        area.width,
        area.height
            .saturating_sub(header_height + u16::from(footer.is_some())),
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
        CalendarView::List => draw_list(frame, body, s),
    }
}
/// The foot's hints for what is selected: moving, opening, and on an
/// event the actions the three rules give the viewer.
fn footer_hints(s: &CalendarState) -> Vec<(&'static str, &'static str)> {
    let moving: (&str, &str) = match s.view {
        CalendarView::Month => ("arrows", "days"),
        CalendarView::List => ("arrows", "events"),
    };
    match s.selected_event() {
        None => vec![moving, ("Enter", "new event here")],
        Some(e) => {
            let access = event_access(&e, s.viewer, s.staff);
            let mut hints = vec![moving, ("j/k", "events"), ("Enter", "open")];
            if access.rsvp {
                hints.push(("i", if s.going(e.id) { "I'm out" } else { "I'm in" }));
            }
            if access.edit {
                hints.push(("e", "edit"));
            }
            if access.delete {
                hints.push(("d", "delete"));
            }
            hints.push(("right-click", "menu"));
            hints
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn grid_rule(
    frame: &mut Frame,
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
}
fn draw_month(frame: &mut Frame, area: Rect, s: &CalendarState) {
    if area.width < 15 || area.height < 9 {
        styled_row(frame, area, hint_line(&[("v", "List")]));
        hit(s, area, Action::View);
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
        grid_rule(frame, area, &widths, area.y + 1, "╭", "┬", "╮");
        for week in 0..6 {
            let mut x = area.x;
            for (col, w) in widths.iter().enumerate() {
                let date = first + Duration::days((week * 7 + col) as i64);
                let count = s.day_events(date).len();
                let rect = Rect::new(x + 1, area.y + 2 + week as u16, w - 1, 1);
                let style = if date == s.selected {
                    bright().add_modifier(Modifier::BOLD)
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
                let marker = if date == s.today() && date != s.selected && spare > 0 {
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
                styled_row(
                    frame,
                    rect,
                    marked_line(line, date == s.selected && spare > 0),
                );
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
        grid_rule(frame, area, &widths, area.y + 8, "╰", "┴", "╯");
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
    grid_rule(frame, area, &widths, y, "╭", "┬", "╮");
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
                    bright().add_modifier(Modifier::BOLD)
                } else if date == today {
                    accent()
                } else if date.month() != s.selected.month() {
                    dim()
                } else {
                    base()
                };
                let capacity = cell.height.saturating_sub(1) as usize;
                let mut date_label = Line::styled(
                    format!(
                        "{}{}",
                        if date == today && date != s.selected {
                            "• "
                        } else {
                            ""
                        },
                        date.day()
                    ),
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
                    marked_line(date_label, date == s.selected),
                );
                hit(s, cell, Action::Date(date));
                let previews = if events.len() > capacity {
                    capacity.saturating_sub(1)
                } else {
                    capacity
                };
                for (n, e) in events.iter().take(previews).enumerate() {
                    let rect = Rect::new(cell.x, cell.y + 1 + n as u16, cell.width, 1);
                    let mut spans: Vec<Span<'static>> = going_span(e).into_iter().collect();
                    spans.push(Span::styled(e.title.clone(), title_style(e)));
                    styled_row(frame, rect, clipped_title(Line::from(spans), rect.width));
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
                    hit(s, rect, Action::Date(date));
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
        let mut line = Line::from(vec![Span::styled("Nothing on", dim()), separator()]);
        line.spans
            .extend(hint_line(&[("n", "to post one")]).spans.into_iter().skip(1));
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
                marked_row(event_line(e, s.tz), selected),
            );
        }
        if y + 1 < inner.height as isize {
            styled_row(
                frame,
                Rect::new(inner.x, inner.y + (y + 1) as u16, inner.width, 1),
                marked_row(timing_line(e, s.tz), false),
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
fn list_lines<'a>(
    s: &CalendarState,
    events: &[&'a CalendarEvent],
) -> Vec<(Line<'static>, Option<&'a CalendarEvent>)> {
    let mut rows = Vec::new();
    let mut last = None;
    for e in events {
        let date = e.timing.dates(s.tz).0.max(month_start(s.selected));
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
            ));
            last = Some(date);
        }
        let mut line = event_line(e, s.tz);
        line.spans.push(separator());
        line.spans.push(source_span(e));
        rows.push((line, Some(*e)));
    }
    rows
}
fn draw_list(frame: &mut Frame, area: Rect, s: &CalendarState) {
    let events = s.ordered_events();
    let rows = list_lines(s, &events);
    s.max_scroll
        .set(rows.len().saturating_sub(area.height as usize));
    s.list_rows.set(area.height as usize);
    pane(s, area, Pane::List);
    if rows.is_empty() {
        row(frame, area, "Nothing on this month", dim());
    }
    for (r, (text, event)) in rows
        .iter()
        .skip(s.scroll.min(s.max_scroll.get()))
        .take(area.height as usize)
        .enumerate()
    {
        let rect = Rect::new(area.x, area.y + r as u16, area.width, 1);
        styled_row(
            frame,
            rect,
            marked_row(
                clipped_title(text.clone(), rect.width.saturating_sub(2)),
                event.is_some_and(|e| s.selection == Selection::Event(e.id)),
            ),
        );
        if let Some(e) = event {
            hit(s, rect, Action::Event(e.id));
        }
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
            Modal::Editor(_) => 30,
            Modal::Details(_) => 24,
            Modal::Delete(_) => 8,
        },
    );
    frame.render_widget(Clear, rect);
    let title = match modal {
        Modal::Editor(e) if e.existing.is_some() => " Edit event ",
        Modal::Editor(_) => " New event ",
        Modal::Details(e) if e.is_board() => " On the board ",
        Modal::Details(_) => " Your event ",
        Modal::Delete(_) => " Delete event? ",
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
            if inner.height > 1 && e.is_board() && e.creator_id != s.viewer {
                row(
                    frame,
                    Rect::new(inner.x, inner.y + 1, inner.width, 1),
                    format!(
                        "Posted by {}. Taking it down is a moderator's call.",
                        e.creator_name
                    ),
                    muted(),
                );
            }
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
            let access = event_access(e, s.viewer, s.staff);
            let going = s.going(e.id);
            let mut who = vec![source_span(e)];
            if e.is_board() {
                who.push(separator());
                who.push(Span::styled(
                    match (e.going, going) {
                        (0, _) => "nobody's in yet".to_string(),
                        (1, true) => "you're in".to_string(),
                        (n, true) => format!("{n} in, you included"),
                        (n, false) => format!("{n} in"),
                    },
                    if going { accent() } else { bright() },
                ));
            }
            let mut lines = vec![
                Line::styled(&e.title, bright().add_modifier(Modifier::BOLD)),
                timing_line(e, s.tz),
                Line::from(who),
                Line::from(""),
            ];
            lines.extend(e.description.lines().map(|s| Line::from(s.to_owned())));
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
            let y = inner.bottom() - 1;
            if access.rsvp {
                button(
                    frame,
                    s,
                    &mut x,
                    y,
                    inner.right(),
                    if going { "i I'm out" } else { "i I'm in" },
                    Action::Rsvp,
                    false,
                );
            }
            if access.edit {
                button(
                    frame,
                    s,
                    &mut x,
                    y,
                    inner.right(),
                    "e Edit",
                    Action::Edit,
                    false,
                );
            }
            if access.delete {
                button(
                    frame,
                    s,
                    &mut x,
                    y,
                    inner.right(),
                    "d Delete",
                    Action::Delete,
                    false,
                );
            }
            button(
                frame,
                s,
                &mut x,
                y,
                inner.right(),
                "Close",
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
