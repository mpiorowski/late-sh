//! Calendar actions, selectors and date navigation share measured hit geometry.
use super::{
    state::{Action, CalendarState},
    ui::{accent, base, bright, clipped_title, dim, hit, row, separator, source_label, styled_row},
};
use crate::app::common::theme;
use chrono::TimeZone;
use late_core::models::calendar::CalendarView;
use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
};

fn mnemonic_line(label: &str, key: char) -> Line<'static> {
    let style = bright().add_modifier(Modifier::BOLD);
    let Some((index, mnemonic)) = label
        .char_indices()
        .find(|(_, c)| c.eq_ignore_ascii_case(&key))
    else {
        return Line::styled(label.to_owned(), style);
    };
    let end = index + mnemonic.len_utf8();
    Line::from(vec![
        Span::styled(label[..index].to_owned(), style),
        Span::styled(label[index..end].to_owned(), accent()),
        Span::styled(label[end..].to_owned(), style),
    ])
}

fn action_line(label: &str, key: char, padded: bool) -> Line<'static> {
    let mut line = mnemonic_line(label, key);
    if padded {
        line.spans.insert(0, Span::raw(" "));
        line.spans.push(Span::raw(" "));
    }
    line
}

fn control(frame: &mut Frame, s: &CalendarState, area: Rect, line: Line<'static>, action: Action) {
    if area.width == 0 {
        return;
    }
    styled_row(frame, area, line);
    hit(s, area, action);
}

#[allow(clippy::too_many_arguments)] // Each selector owns its label, value and arrow hits.
fn selector(
    frame: &mut Frame,
    s: &CalendarState,
    area: Rect,
    label: &str,
    key: char,
    value: &str,
    picker: Action,
    cycle: [Action; 2],
) {
    let label_width = 9;
    let arrow_width = if area.width >= 33 { 3 } else { 2 };
    let value_width = area.width.saturating_sub(label_width + arrow_width * 2);
    styled_row(
        frame,
        Rect::new(area.x, area.y, label_width, 1),
        mnemonic_line(label, key),
    );
    hit(s, Rect::new(area.x, area.y, label_width, 1), picker.clone());
    let mut x = area.x + label_width;
    for (text, width, action) in [
        ("◂".to_owned(), arrow_width, cycle[0].clone()),
        (value.to_owned(), value_width, picker),
        ("▸".to_owned(), arrow_width, cycle[1].clone()),
    ] {
        let style = if matches!(action, Action::CycleSource(_) | Action::CycleView(_)) {
            accent()
        } else {
            bright()
        };
        let line = clipped_title(Line::styled(text, style), width).centered();
        control(frame, s, Rect::new(x, area.y, width, 1), line, action);
        x += width;
    }
}

fn selectors(frame: &mut Frame, s: &CalendarState, area: Rect) {
    selector(
        frame,
        s,
        Rect::new(area.x, area.y, area.width, 1),
        "Calendar",
        'c',
        &source_label(s),
        Action::Source,
        [Action::CycleSource(-1), Action::CycleSource(1)],
    );
    let view = if area.width < 33 && s.view == CalendarView::List {
        "List"
    } else {
        s.view.label()
    };
    selector(
        frame,
        s,
        Rect::new(area.x, area.y + 1, area.width, 1),
        "View",
        'v',
        view,
        Action::View,
        [Action::CycleView(-1), Action::CycleView(1)],
    );
}

fn period_arrows(frame: &mut Frame, s: &CalendarState, x: u16, y: u16) {
    for (offset, label, action) in [(0, "‹ [", Action::Previous), (4, "] ›", Action::Next)] {
        control(
            frame,
            s,
            Rect::new(x + offset, y, 3, 1),
            Line::styled(label, accent()),
            action,
        );
    }
}

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
        line.spans.push(Span::styled(" …", dim()));
    }
    clipped_title(line, width)
}

/// Wide layouts match the three groups; compact layouts pair selectors with
/// actions and keep navigation plus the period on a third row.
pub(super) fn draw(frame: &mut Frame, area: Rect, s: &CalendarState) -> u16 {
    let settings = action_line("Settings", 's', true);
    let new = action_line("New event", 'n', true);
    let go = action_line("Go to date", 'g', true);
    let today = action_line("Today", 't', true);
    let actions_width = new.width() as u16;
    let selector_width = 35;
    let navigation_width = go.width() as u16 + 2 + 7;
    let wide_width = 2 + actions_width + 4 + selector_width + 4 + navigation_width;
    let mut height = 3;
    if area.width >= wide_width {
        let x = area.x + 1;
        let y = area.y + 1;
        let selector_x = x + actions_width + 4;
        let navigation_x = selector_x + selector_width + 4;
        styled_row(
            frame,
            Rect::new(x, area.y, area.width - 2, 1),
            context(s, area.width - 2),
        );
        for (offset, line, action) in [(0, settings, Action::Settings), (1, new, Action::New)] {
            let width = line.width() as u16;
            control(frame, s, Rect::new(x, y + offset, width, 1), line, action);
        }
        selectors(frame, s, Rect::new(selector_x, y, selector_width, 2));
        let go_width = go.width() as u16;
        control(
            frame,
            s,
            Rect::new(navigation_x, y, go_width, 1),
            go,
            Action::Go,
        );
        period_arrows(frame, s, navigation_x + go_width + 2, y);
        let today_width = today.width() as u16;
        control(
            frame,
            s,
            Rect::new(navigation_x, y + 1, today_width, 1),
            today,
            Action::Today,
        );
        // One breathing row is affordable on normal-height terminals.
        height += u16::from(area.height >= 20);
    } else if area.width >= 40 {
        let settings = action_line("Settings", 's', false);
        let new = action_line("New event", 'n', false);
        let actions_width = new.width() as u16;
        let selector_width = (area.width - actions_width - 1).min(35);
        let actions_x = area.x + selector_width + 1;
        selectors(frame, s, Rect::new(area.x, area.y, selector_width, 2));
        for (offset, line, action) in [(0, settings, Action::Settings), (1, new, Action::New)] {
            let width = line.width() as u16;
            control(
                frame,
                s,
                Rect::new(actions_x, area.y + offset, width, 1),
                line,
                action,
            );
        }
        let y = area.y + 2;
        control(
            frame,
            s,
            Rect::new(area.x, y, 3, 1),
            Line::styled("‹ [", accent()),
            Action::Previous,
        );
        let mut x = area.x + 4;
        for (label, key, action) in [
            ("Today", 't', Action::Today),
            ("Go to date", 'g', Action::Go),
        ] {
            let line = action_line(label, key, false);
            let width = line.width() as u16;
            control(frame, s, Rect::new(x, y, width, 1), line, action);
            x += width + 1;
        }
        control(
            frame,
            s,
            Rect::new(x, y, 3, 1),
            Line::styled("] ›", accent()),
            Action::Next,
        );
        x += 4;
        styled_row(
            frame,
            Rect::new(x, y, area.right() - x, 1),
            context(s, area.right() - x),
        );
    } else {
        let selector_width = area.width.saturating_sub(1) / 2;
        for (offset, label, key, value, action) in [
            (0, "Calendar", 'c', source_label(s), Action::Source),
            (
                selector_width + 1,
                "View",
                'v',
                s.view.label().to_owned(),
                Action::View,
            ),
        ] {
            let mut line = mnemonic_line(label, key);
            line.spans.push(Span::styled(format!(" {value}"), bright()));
            control(
                frame,
                s,
                Rect::new(area.x + offset, area.y, selector_width, 1),
                clipped_title(line, selector_width),
                action,
            );
        }
        let mut x = area.x;
        let settings_label = if area.width >= 25 { "Settings" } else { "Set" };
        for (label, key, action) in [
            ("[", '[', Action::Previous),
            ("]", ']', Action::Next),
            ("Today", 't', Action::Today),
            ("Go", 'g', Action::Go),
            ("New", 'n', Action::New),
            (settings_label, 's', Action::Settings),
        ] {
            let line = action_line(label, key, false);
            let width = (line.width() as u16).min(area.right().saturating_sub(x));
            control(
                frame,
                s,
                Rect::new(x, area.y + 1, width, 1),
                clipped_title(line, width),
                action,
            );
            x = (x + width + 1).min(area.right());
        }
        styled_row(
            frame,
            Rect::new(area.x, area.y + 2, area.width, 1),
            context(s, area.width),
        );
    }
    if let Some(error) = &s.error {
        let y = if area.width >= wide_width {
            area.y
        } else {
            area.y + height
        };
        row(
            frame,
            Rect::new(area.x, y, area.width, 1),
            error,
            base().fg(theme::ERROR()),
        );
        if y != area.y {
            height += 1;
        }
    }
    height
}
