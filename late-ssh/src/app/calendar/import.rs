//! Paste/URL entry and the single-event chooser share one cancellable dialog.
use super::{
    ical::{Candidate, MAX_BYTES},
    state::{Action, CalendarState},
    ui,
};
use crate::app::{
    common::{
        textarea_input::{EditOutcome, handle_multiline_edit},
        theme,
    },
    input::{MouseButton, MouseEventKind, ParsedInput},
};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use ratatui_textarea::{CursorMove, TextArea};
use std::cell::Cell;

pub struct Import {
    pub(super) input: TextArea<'static>,
    pub(super) candidates: Option<Vec<Candidate>>,
    pub(super) selected: usize,
    pub(super) focus: usize,
    pub(super) loading: bool,
    pub(super) error: Option<String>,
    input_area: Cell<Rect>,
}

impl Default for Import {
    fn default() -> Self {
        Self {
            input: TextArea::default(),
            candidates: None,
            selected: 0,
            focus: 0,
            loading: false,
            error: None,
            input_area: Cell::new(Rect::default()),
        }
    }
}

impl Import {
    pub(super) fn handle(&mut self, event: &ParsedInput, picker_rows: usize) -> Option<Action> {
        if matches!(event, ParsedInput::Byte(0x1b)) {
            return Some(Action::Cancel);
        }
        if let ParsedInput::Mouse(m) = event {
            let (Some(x), Some(y)) = (m.x.checked_sub(1), m.y.checked_sub(1)) else {
                return None;
            };
            if m.kind == MouseEventKind::Down
                && m.button == Some(MouseButton::Left)
                && self.input_area.get().contains((x, y).into())
                && !self.loading
            {
                self.focus = 0;
                self.input.move_cursor(CursorMove::Bottom);
                self.input.move_cursor(CursorMove::End);
            }
            return None;
        }
        if self.loading {
            return None;
        }
        if let Some(candidates) = &self.candidates {
            match event {
                ParsedInput::Arrow(b'A') | ParsedInput::Byte(b'k') | ParsedInput::BackTab => {
                    self.selected = self.selected.saturating_sub(1)
                }
                ParsedInput::Arrow(b'B') | ParsedInput::Byte(b'j' | b'\t') => {
                    self.selected = (self.selected + 1) % candidates.len()
                }
                ParsedInput::PageUp => {
                    self.selected = self.selected.saturating_sub(picker_rows.max(1))
                }
                ParsedInput::PageDown => {
                    self.selected = (self.selected + picker_rows.max(1)).min(candidates.len() - 1)
                }
                ParsedInput::Byte(b'\r' | 0x13) => return Some(Action::Choice(self.selected)),
                ParsedInput::Byte(b'b') => return Some(Action::Reload),
                _ => {}
            }
            self.error = None;
            return None;
        }
        match event {
            ParsedInput::Byte(0x13) => return Some(Action::Save),
            ParsedInput::Byte(b'\t') => self.focus = (self.focus + 1) % 3,
            ParsedInput::BackTab => self.focus = (self.focus + 2) % 3,
            ParsedInput::Byte(b'\r') if self.focus == 2 => return Some(Action::Cancel),
            _ if self.focus != 0 => {
                if matches!(event, ParsedInput::Byte(b'\r')) {
                    return Some(Action::Save);
                }
            }
            _ => {
                // Reject an oversized paste intact; never import a silently truncated file.
                if let ParsedInput::Paste(bytes) = event
                    && bytes.len() + self.input.lines().iter().map(String::len).sum::<usize>()
                        > MAX_BYTES
                {
                    self.error = Some("iCalendar is limited to 1 MiB".into());
                    return None;
                }
                if let ParsedInput::Paste(bytes) = event
                    && std::str::from_utf8(bytes).is_err()
                {
                    self.error = Some("iCalendar must be UTF-8 text".into());
                    return None;
                }
                let printable = match event {
                    ParsedInput::Byte(b) if b.is_ascii() && !b.is_ascii_control() => {
                        Some(ParsedInput::Char(*b as char))
                    }
                    _ => None,
                };
                match handle_multiline_edit(
                    &mut self.input,
                    printable.as_ref().unwrap_or(event),
                    MAX_BYTES,
                ) {
                    EditOutcome::Submit => return Some(Action::Save),
                    EditOutcome::Cancel => return Some(Action::Cancel),
                    EditOutcome::Handled => self.error = None,
                    _ => {}
                }
            }
        }
        None
    }
}

pub(super) fn draw(frame: &mut Frame, inner: Rect, s: &CalendarState, import: &Import) {
    import.input_area.set(Rect::default());
    let footer = inner.height.min(4);
    let content = Rect::new(inner.x, inner.y, inner.width, inner.height - footer);
    let y = inner.bottom() - footer;
    if let Some(candidates) = &import.candidates {
        let labels: Vec<_> = candidates
            .iter()
            .map(|c| {
                let date = c
                    .draft
                    .as_ref()
                    .map(|d| d.timing.dates(s.tz).0.to_string())
                    .unwrap_or_else(|_| "unavailable".into());
                Line::from(vec![
                    Span::styled(c.title.clone(), ui::bright()),
                    ui::separator(),
                    Span::styled(date, ui::dim()),
                ])
            })
            .collect();
        ui::picker_rows(frame, content, s, &labels, import.selected);
        if let Some(candidate) = candidates.get(import.selected) {
            let preview = match &candidate.draft {
                Ok(d) => {
                    let (start, _) = d.timing.dates(s.tz);
                    format!("{start} · Replace form fields, then review")
                }
                Err(e) => e.clone(),
            };
            ui::row(
                frame,
                Rect::new(inner.x, y, inner.width, 1),
                &preview,
                if candidate.draft.is_ok() {
                    ui::dim()
                } else {
                    ui::base().fg(theme::ERROR())
                },
            );
        }
    } else {
        if content.height > 0 {
            ui::row(
                frame,
                Rect::new(content.x, content.y, content.width, 1),
                "Paste .ics content or a calendar URL",
                ui::bright(),
            );
        }
        if content.height > 1 {
            let area = Rect::new(content.x, content.y + 1, content.width, content.height - 1);
            import.input_area.set(area);
            ui::hit(s, area, Action::Field(0));
            let mut input = import.input.clone();
            input.set_style(ui::base());
            input.set_cursor_line_style(ui::base());
            input.set_cursor_style(if import.focus == 0 {
                ui::selection_style()
            } else {
                ui::base()
            });
            frame.render_widget(&input, area);
        }
        ui::row(
            frame,
            Rect::new(inner.x, y, inner.width, 1),
            if import.loading {
                "Fetching iCalendar… Escape cancels"
            } else {
                "Enter reads · Alt+Enter newline · Tab focus"
            },
            ui::dim(),
        );
    }
    if footer >= 2 {
        let mut x = inner.x;
        let choose = import.candidates.is_some();
        ui::button(
            frame,
            s,
            &mut x,
            y + 1,
            inner.right(),
            if choose {
                "Enter Review"
            } else {
                "Enter Read iCal"
            },
            if choose {
                Action::Choice(import.selected)
            } else {
                Action::Save
            },
            import.focus == 1,
        );
        if choose {
            ui::button(
                frame,
                s,
                &mut x,
                y + 1,
                inner.right(),
                "b Back",
                Action::Reload,
                false,
            );
        }
        ui::button(
            frame,
            s,
            &mut x,
            y + 1,
            inner.right(),
            "Esc Cancel",
            Action::Cancel,
            import.focus == 2,
        );
    }
    if footer >= 3
        && let Some(error) = &import.error
    {
        frame.render_widget(
            Paragraph::new(error.as_str())
                .style(ui::base().fg(theme::ERROR()))
                .wrap(Wrap { trim: false }),
            Rect::new(inner.x, y + 2, inner.width, footer - 2),
        );
    }
}
