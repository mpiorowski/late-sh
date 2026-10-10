//! Event form state, input, and the geometry shared by rendering and mouse input.
use super::{
    date_entry, parser,
    state::{Action, CalendarState, parse_duration},
    ui,
};
use crate::app::{
    common::{
        textarea_input::{handle_multiline_edit, handle_single_line_edit},
        theme,
    },
    input::ParsedInput,
};
use chrono::{Duration, LocalResult, NaiveDate, NaiveTime, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use late_core::models::calendar::{
    CalendarEvent, CalendarSource, CreationTier, EventAccess, EventDraft, EventTiming, Occurrence,
    event_access, local_instant,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use ratatui_textarea::{CursorMove, TextArea};
use std::{
    cell::{Cell, RefCell},
    time::Duration as StdDuration,
};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorControl {
    Title,
    Description,
    AllDay,
    StartDate,
    StartTime,
    StartOccurrence,
    EndDate,
    EndTime,
    EndOccurrence,
    Notifications,
    LeadTime,
    Delegation,
    Save,
    Cancel,
    Import,
}

impl EditorControl {
    fn field(self) -> Option<usize> {
        match self {
            Self::Title => Some(0),
            Self::Description => Some(1),
            Self::StartDate => Some(2),
            Self::StartTime => Some(3),
            Self::EndDate => Some(4),
            Self::EndTime => Some(5),
            Self::LeadTime => Some(6),
            _ => None,
        }
    }

    fn label(self, all_day: bool) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::Description => "Description",
            Self::AllDay => "All day",
            Self::StartDate => "Start date",
            Self::StartTime => "Start time (24-hour)",
            Self::StartOccurrence => "Start time occurs twice (DST)",
            Self::EndDate if all_day => "End date (inclusive)",
            Self::EndDate => "End date (optional)",
            Self::EndTime => "End time (optional)",
            Self::EndOccurrence => "End time occurs twice (DST)",
            Self::Notifications => "Notifications",
            Self::LeadTime => "Lead time (e.g. 1 day, 1h 30m)",
            Self::Delegation => "Moderator delegation",
            Self::Save => "Save",
            Self::Cancel => "Cancel",
            Self::Import => "Import iCal",
        }
    }

    fn value_rows(self, viewport_rows: usize) -> usize {
        if self == Self::Description {
            3.min(viewport_rows.saturating_sub(1).max(1))
        } else {
            1
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorCommand {
    None,
    Save,
    Cancel,
    Import,
    Discard,
    Keep,
    Reload,
}

#[derive(Clone, Copy, Debug)]
enum TargetAction {
    Focus(EditorControl),
    Value(EditorControl, usize, usize),
    Toggle(EditorControl),
    Command(EditorCommand),
    ScrollTo(usize),
}

#[derive(Clone, Copy, Debug)]
struct Target {
    area: Rect,
    action: TargetAction,
}

#[derive(Clone, Default)]
struct Geometry {
    targets: Vec<Target>,
    viewport: Rect,
    max_scroll: usize,
}

#[derive(Clone)]
pub struct Editor {
    pub existing: Option<(Uuid, i64)>,
    pub source: CalendarSource,
    pub fields: Vec<TextArea<'static>>,
    pub focus: EditorControl,
    pub all_day: bool,
    pub notifications: bool,
    pub delegated: bool,
    pub occurrence: Option<Occurrence>,
    pub end_occurrence: Option<Occurrence>,
    pub ever_assigned: bool,
    pub provisional: NaiveDate,
    pub initial: String,
    pub error: Option<String>,
    pub discard_prompt: bool,
    pub access: EventAccess,
    scroll: Cell<usize>,
    reveal_focus: Cell<bool>,
    last_area: Cell<Rect>,
    field_viewports: RefCell<[(usize, usize); 7]>,
    geometry: RefCell<Geometry>,
    discard_selected: bool,
}

fn field(s: impl Into<String>) -> TextArea<'static> {
    let s = s.into();
    let mut field = TextArea::from(s.split('\n').map(str::to_owned).collect::<Vec<_>>());
    field.move_cursor(CursorMove::Bottom);
    field.move_cursor(CursorMove::End);
    field
}

fn occurrence_for(instant: chrono::DateTime<Utc>, tz: Tz) -> Option<Occurrence> {
    match tz.from_local_datetime(&instant.with_timezone(&tz).naive_local()) {
        LocalResult::Ambiguous(a, b) => Some(if instant == a.min(b).with_timezone(&Utc) {
            Occurrence::Earlier
        } else {
            Occurrence::Later
        }),
        _ => None,
    }
}

fn clock(time: impl Timelike) -> String {
    format!(
        "{:02}:{:02}{}",
        time.hour(),
        time.minute(),
        if time.second() == 0 {
            String::new()
        } else {
            format!(":{:02}", time.second())
        }
    )
}

fn parse_clock(text: &str) -> Result<NaiveTime, chrono::ParseError> {
    NaiveTime::parse_from_str(text.trim(), "%H:%M")
        .or_else(|_| NaiveTime::parse_from_str(text.trim(), "%H:%M:%S"))
}

impl Editor {
    pub fn new(source: CalendarSource, date: NaiveDate, access: EventAccess) -> Self {
        let mut e = Self {
            existing: None,
            source,
            fields: vec![
                field(""),
                field(""),
                field(date.to_string()),
                field("09:00"),
                field(""),
                field(""),
                field("24h"),
            ],
            focus: EditorControl::Title,
            all_day: true,
            notifications: false,
            delegated: false,
            occurrence: None,
            end_occurrence: None,
            ever_assigned: false,
            provisional: date,
            initial: String::new(),
            error: None,
            discard_prompt: false,
            access,
            scroll: Cell::new(0),
            reveal_focus: Cell::new(true),
            last_area: Cell::new(Rect::default()),
            field_viewports: RefCell::new([(0, 0); 7]),
            geometry: RefCell::new(Geometry::default()),
            discard_selected: false,
        };
        e.initial = e.fingerprint();
        e
    }

    pub fn from_event(event: &CalendarEvent, viewer: Uuid, role: CreationTier, tz: Tz) -> Self {
        let access = event_access(event, viewer, role);
        let mut e = Self::new(
            event
                .owner_id
                .map(CalendarSource::Personal)
                .unwrap_or(CalendarSource::Server),
            event.timing.dates(tz).0,
            access,
        );
        e.existing = Some((event.id, event.revision));
        e.fields[0] = field(&event.title);
        e.fields[1] = field(&event.description);
        e.set_timing(&event.timing, tz);
        e.notifications = event.notice_lead_seconds.is_some();
        e.fields[6] = field(
            humantime::format_duration(StdDuration::from_secs(
                event.notice_lead_seconds.unwrap_or(86400) as u64,
            ))
            .to_string(),
        );
        e.delegated = event.mod_editable;
        e.ever_assigned = true;
        e.initial = e.fingerprint();
        e
    }

    pub(super) fn apply_import(&mut self, draft: &EventDraft, tz: Tz) {
        let e = self;
        e.fields[0] = field(&draft.title);
        e.fields[1] = field(&draft.description);
        // Review starts at the beginning, including on horizontally clipped fields.
        for input in &mut e.fields[..2] {
            input.move_cursor(CursorMove::Top);
            input.move_cursor(CursorMove::Head);
        }
        e.set_timing(&draft.timing, tz);
        e.ever_assigned = true;
        if e.access.notifications {
            e.notifications = draft.notice_lead_seconds.is_some();
            e.fields[6] = field(
                humantime::format_duration(StdDuration::from_secs(
                    draft.notice_lead_seconds.unwrap_or(86400) as u64,
                ))
                .to_string(),
            );
        }
        e.error = None;
        e.focus = EditorControl::Title;
        e.scroll.set(0);
        e.reveal_focus.set(true);
    }

    pub fn text(&self, n: usize) -> String {
        self.fields[n].lines().join("\n")
    }

    pub fn fingerprint(&self) -> String {
        format!(
            "{:?}|{}|{}|{}|{:?}|{:?}",
            self.fields.iter().map(|f| f.lines()).collect::<Vec<_>>(),
            self.all_day,
            self.notifications,
            self.delegated,
            self.occurrence,
            self.end_occurrence
        )
    }

    pub fn dirty(&self) -> bool {
        self.fingerprint() != self.initial
    }

    pub fn invalidate_geometry(&self) {
        *self.geometry.borrow_mut() = Geometry::default();
    }

    pub fn set_timing(&mut self, t: &EventTiming, tz: Tz) {
        self.occurrence = None;
        self.end_occurrence = None;
        match t {
            EventTiming::AllDay {
                start,
                end_exclusive,
            } => {
                self.all_day = true;
                self.fields[2] = field(start.to_string());
                self.fields[4] = field((*end_exclusive - Duration::days(1)).to_string());
            }
            EventTiming::Timed { start, end } => {
                self.all_day = false;
                self.occurrence = occurrence_for(*start, tz);
                self.end_occurrence = end.and_then(|t| occurrence_for(t, tz));
                let start = start.with_timezone(&tz);
                self.fields[2] = field(start.date_naive().to_string());
                self.fields[3] = field(clock(start));
                self.fields[4] = field(
                    end.map(|t| t.with_timezone(&tz).date_naive().to_string())
                        .unwrap_or_default(),
                );
                self.fields[5] =
                    field(end.map(|t| clock(t.with_timezone(&tz))).unwrap_or_default());
            }
        }
    }

    pub fn blur_title(&mut self, today: NaiveDate, tz: Tz) {
        if self.ever_assigned {
            return;
        }
        if let Some(i) = parser::infer(&self.text(0), self.provisional, today, tz) {
            self.fields[0] = field(i.title);
            self.set_timing(&i.timing, tz);
            self.ever_assigned = true;
        }
    }

    pub fn focus(&mut self, control: EditorControl, today: NaiveDate, tz: Tz) {
        if self.focus == EditorControl::Title && control != self.focus {
            self.blur_title(today, tz);
        }
        if matches!(
            self.focus,
            EditorControl::StartDate | EditorControl::EndDate
        ) && control != self.focus
        {
            let n = self.focus.field().unwrap();
            let prefix = if n == 2 { "Start date:" } else { "End date:" };
            match self.date_field(n, today) {
                Ok(_) => {
                    if self.error.as_deref().is_some_and(|e| e.starts_with(prefix)) {
                        self.error = None;
                    }
                }
                Err(error) => self.error = Some(error.to_string()),
            }
        }
        let controls = self.visible_controls(today, tz);
        self.focus = if controls.contains(&control) {
            control
        } else {
            EditorControl::AllDay
        };
        self.reveal_focus.set(true);
    }

    pub fn visible_controls(&self, today: NaiveDate, tz: Tz) -> Vec<EditorControl> {
        use EditorControl::{
            AllDay, Cancel, Delegation, Description, EndDate, EndOccurrence, EndTime, LeadTime,
            Notifications, Save, StartDate, StartOccurrence, StartTime, Title,
        };
        let mut controls = vec![Title, Description, AllDay, StartDate];
        if !self.all_day {
            controls.push(StartTime);
            if self.time_repeats(false, today, tz) {
                controls.push(StartOccurrence);
            }
        }
        controls.push(EndDate);
        if !self.all_day {
            controls.push(EndTime);
            if self.time_repeats(true, today, tz) {
                controls.push(EndOccurrence);
            }
        }
        if self.access.notifications {
            controls.push(Notifications);
            if self.notifications {
                controls.push(LeadTime);
            }
        }
        if self.access.delegate {
            controls.push(Delegation);
        }
        controls.extend([Save, Cancel]);
        if self.access.edit {
            controls.push(EditorControl::Import);
        }
        controls
    }

    fn time_repeats(&self, end: bool, today: NaiveDate, tz: Tz) -> bool {
        let date = if end && !self.text(4).trim().is_empty() {
            4
        } else {
            2
        };
        let Ok(date) = date_entry::parse(&self.text(date), today, today) else {
            return false;
        };
        let Ok(time) = parse_clock(&self.text(if end { 5 } else { 3 })) else {
            return false;
        };
        matches!(
            tz.from_local_datetime(&date.and_time(time)),
            LocalResult::Ambiguous(..)
        )
    }

    fn advance(&mut self, backwards: bool, today: NaiveDate, tz: Tz) {
        if self.focus == EditorControl::Title {
            self.blur_title(today, tz);
        }
        let controls = self.visible_controls(today, tz);
        let i = controls.iter().position(|c| *c == self.focus).unwrap_or(0);
        let next = if backwards {
            (i + controls.len() - 1) % controls.len()
        } else {
            (i + 1) % controls.len()
        };
        self.focus(controls[next], today, tz);
    }

    fn toggle(&mut self, control: EditorControl) {
        match control {
            EditorControl::AllDay => {
                self.all_day = !self.all_day;
                self.ever_assigned = true;
                if !self.all_day && self.text(3).is_empty() {
                    self.fields[3] = field("09:00");
                }
            }
            EditorControl::Notifications if self.access.notifications => {
                self.notifications = !self.notifications
            }
            EditorControl::Delegation if self.access.delegate => self.delegated = !self.delegated,
            EditorControl::StartOccurrence => self.occurrence = next_occurrence(self.occurrence),
            EditorControl::EndOccurrence => {
                self.end_occurrence = next_occurrence(self.end_occurrence)
            }
            _ => {}
        }
        self.reveal_focus.set(true);
    }

    fn date_field(&mut self, n: usize, today: NaiveDate) -> anyhow::Result<Option<NaiveDate>> {
        let text = self.text(n);
        if n == 4 && text.trim().is_empty() {
            return Ok(None);
        }
        let date = date_entry::parse(&text, today, today).map_err(|error| {
            anyhow::anyhow!("{} date: {error}", if n == 2 { "Start" } else { "End" })
        })?;
        self.fields[n] = field(date.to_string());
        Ok(Some(date))
    }

    pub fn draft(&mut self, today: NaiveDate, tz: Tz) -> anyhow::Result<EventDraft> {
        self.blur_title(today, tz);
        let start = self.date_field(2, today)?.unwrap();
        let end_date = self.date_field(4, today)?;
        let end_time = self.text(5);
        let timing = if self.all_day {
            let inclusive = end_date.unwrap_or(start);
            EventTiming::AllDay {
                start,
                end_exclusive: inclusive
                    .succ_opt()
                    .ok_or_else(|| anyhow::anyhow!("End date out of range"))?,
            }
        } else {
            let time = parse_clock(&self.text(3))
                .map_err(|_| anyhow::anyhow!("Start time must be HH:MM or HH:MM:SS (24-hour)"))?;
            let begin = local_instant(start.and_time(time), tz, self.occurrence)?;
            let end = if end_date.is_none() && end_time.trim().is_empty() {
                None
            } else {
                let d = end_date.unwrap_or(start);
                let t = parse_clock(&end_time)
                    .map_err(|_| anyhow::anyhow!("End time must be HH:MM or HH:MM:SS"))?;
                Some(local_instant(d.and_time(t), tz, self.end_occurrence)?)
            };
            EventTiming::Timed { start: begin, end }
        };
        let draft = EventDraft {
            title: self.text(0),
            description: self.text(1),
            timing,
            notice_lead_seconds: if self.notifications {
                Some(parse_duration(&self.text(6))?)
            } else {
                None
            },
            mod_editable: self.delegated,
        };
        draft.validate()?;
        Ok(draft)
    }
}

fn next_occurrence(occurrence: Option<Occurrence>) -> Option<Occurrence> {
    match occurrence {
        None => Some(Occurrence::Earlier),
        Some(Occurrence::Earlier) => Some(Occurrence::Later),
        Some(Occurrence::Later) => None,
    }
}

pub fn handle_key(e: &mut Editor, event: &ParsedInput, today: NaiveDate, tz: Tz) -> EditorCommand {
    use EditorCommand::{Cancel, Discard, Keep, None, Save};
    // VTE reports printable ASCII as Char; direct callers can also provide Byte.
    let normalized = match event {
        ParsedInput::Char(ch) if ch.is_ascii() => Some(ParsedInput::Byte(*ch as u8)),
        _ => Option::None,
    };
    let event = normalized.as_ref().unwrap_or(event);
    if e.discard_prompt {
        return match event {
            ParsedInput::Byte(0x1b | b'k' | b'n') => Keep,
            ParsedInput::Byte(b'd' | b'y') => Discard,
            ParsedInput::Byte(b'\r') => {
                if e.discard_selected {
                    Discard
                } else {
                    Keep
                }
            }
            ParsedInput::Byte(b'\t') | ParsedInput::BackTab | ParsedInput::Arrow(_) => {
                e.discard_selected = !e.discard_selected;
                None
            }
            _ => None,
        };
    }
    match event {
        ParsedInput::Byte(0x1b) => {
            e.discard_selected = false;
            return Cancel;
        }
        ParsedInput::Byte(0x13) => return Save,
        ParsedInput::Byte(b'\t') => {
            e.advance(false, today, tz);
            return None;
        }
        ParsedInput::BackTab => {
            e.advance(true, today, tz);
            return None;
        }
        ParsedInput::PageUp | ParsedInput::PageDown => {
            let geometry = e.geometry.borrow();
            let delta = geometry.viewport.height.max(1) as isize;
            let delta = if matches!(event, ParsedInput::PageUp) {
                -delta
            } else {
                delta
            };
            e.scroll.set(
                e.scroll
                    .get()
                    .saturating_add_signed(delta)
                    .min(geometry.max_scroll),
            );
            e.reveal_focus.set(false);
            return None;
        }
        ParsedInput::Byte(b'\r') if e.focus != EditorControl::Description => {
            return match e.focus {
                EditorControl::Save => Save,
                EditorControl::Cancel => Cancel,
                EditorControl::Import => EditorCommand::Import,
                control if control.field().is_none() => {
                    e.toggle(control);
                    None
                }
                _ => {
                    e.advance(false, today, tz);
                    None
                }
            };
        }
        ParsedInput::Byte(b' ') if e.focus.field().is_none() => {
            return match e.focus {
                EditorControl::Save => Save,
                EditorControl::Cancel => Cancel,
                EditorControl::Import => EditorCommand::Import,
                control => {
                    e.toggle(control);
                    None
                }
            };
        }
        _ => {}
    }
    if let Some(n) = e.focus.field() {
        let before = e.text(n);
        if n == 1 {
            let adapted = match event {
                ParsedInput::Byte(b'\r') => ParsedInput::AltEnter,
                ParsedInput::Byte(b) if b.is_ascii_graphic() || *b == b' ' => {
                    ParsedInput::Char(*b as char)
                }
                _ => event.clone(),
            };
            handle_multiline_edit(&mut e.fields[n], &adapted, 10000);
        } else {
            handle_single_line_edit(&mut e.fields[n], event, if n == 0 { 300 } else { 40 });
        }
        if (2..=5).contains(&n) && before != e.text(n) {
            e.ever_assigned = true;
        }
        e.reveal_focus.set(true);
    }
    None
}

/// Coordinates are zero-based terminal cells, matching the recorded render.
pub fn click(e: &mut Editor, x: u16, y: u16, today: NaiveDate, tz: Tz) -> Option<EditorCommand> {
    let target = e
        .geometry
        .borrow()
        .targets
        .iter()
        .rev()
        .find(|t| t.area.contains((x, y).into()))
        .copied()?;
    match target.action {
        TargetAction::Focus(control) => e.focus(control, today, tz),
        TargetAction::Toggle(control) => {
            e.focus(control, today, tz);
            e.toggle(control);
        }
        TargetAction::Value(control, top, left) => {
            let n = control.field().unwrap();
            let row = (top + usize::from(y - target.area.y))
                .min(e.fields[n].lines().len().saturating_sub(1));
            let col = column_at_cell(
                &e.fields[n].lines()[row],
                left + usize::from(x - target.area.x),
            );
            e.focus(control, today, tz);
            e.fields[n].cancel_selection();
            e.fields[n].move_cursor(CursorMove::Jump(row as u16, col as u16));
        }
        TargetAction::Command(command) => {
            if command == EditorCommand::Cancel {
                e.discard_selected = false;
            }
            return Some(command);
        }
        TargetAction::ScrollTo(offset) => {
            e.scroll.set(offset);
            e.reveal_focus.set(false);
        }
    }
    Some(EditorCommand::None)
}

pub fn scroll(e: &mut Editor, x: u16, y: u16, delta: isize) -> bool {
    let geometry = e.geometry.borrow();
    if !geometry.viewport.contains((x, y).into()) {
        return false;
    }
    e.scroll.set(
        e.scroll
            .get()
            .saturating_add_signed(delta)
            .min(geometry.max_scroll),
    );
    e.reveal_focus.set(false);
    true
}

/// TextArea stores character offsets; terminal coordinates are grapheme cell widths.
fn column_at_cell(text: &str, cell: usize) -> usize {
    let span = Span::raw(text);
    let mut width = 0;
    let mut col = 0;
    for grapheme in span.styled_graphemes(Style::default()) {
        let next = width + Span::raw(grapheme.symbol).width();
        if cell < next {
            return col;
        }
        width = next;
        col += grapheme.symbol.chars().count();
    }
    col
}

fn ensure_visible(top: usize, cursor: usize, size: usize) -> usize {
    if cursor < top {
        cursor
    } else if cursor >= top + size.max(1) {
        cursor + 1 - size.max(1)
    } else {
        top
    }
}

fn record(e: &Editor, area: Rect, action: TargetAction) {
    if area.width > 0 && area.height > 0 {
        e.geometry
            .borrow_mut()
            .targets
            .push(Target { area, action });
    }
}

fn value_text(e: &Editor, control: EditorControl) -> String {
    let flag = match control {
        EditorControl::AllDay => Some(e.all_day),
        EditorControl::Notifications => Some(e.notifications),
        EditorControl::Delegation => Some(e.delegated),
        _ => None,
    };
    if let Some(flag) = flag {
        return if flag { "[x]" } else { "[ ]" }.into();
    }
    let occurrence = if control == EditorControl::StartOccurrence {
        e.occurrence
    } else {
        e.end_occurrence
    };
    match occurrence {
        None => "Choose earlier or later",
        Some(Occurrence::Earlier) => "Earlier occurrence",
        Some(Occurrence::Later) => "Later occurrence",
    }
    .into()
}

fn draw_field(
    frame: &mut Frame,
    e: &Editor,
    control: EditorControl,
    area: Rect,
    crop: usize,
    full_height: usize,
) {
    let n = control.field().unwrap();
    let cursor = e.fields[n].cursor();
    let prefix: String = e.fields[n].lines()[cursor.0]
        .chars()
        .take(cursor.1)
        .collect();
    let cursor_cell = Line::raw(prefix).width();
    let (top, left) = e.field_viewports.borrow()[n];
    let top = ensure_visible(top, cursor.0, full_height);
    let left = ensure_visible(left, cursor_cell, usize::from(area.width));
    e.field_viewports.borrow_mut()[n] = (top, left);
    let style = if control == EditorControl::Title {
        ui::bright()
    } else {
        ui::base()
    };
    let first_row = top + crop;
    let lines: Vec<_> = e.fields[n]
        .lines()
        .iter()
        .skip(first_row)
        .take(area.height as usize)
        .map(|line| Line::styled(line.clone(), style))
        .collect();
    frame.render_widget(
        Paragraph::new(lines).style(style).scroll((0, left as u16)),
        area,
    );
    if e.focus == control
        && cursor.0 >= first_row
        && cursor.0 < first_row + usize::from(area.height)
        && cursor_cell >= left
        && cursor_cell < left + usize::from(area.width)
    {
        let x = area.x + (cursor_cell - left) as u16;
        let y = area.y + (cursor.0 - first_row) as u16;
        // A reversed bright cell remains visible even when the cursor follows the text.
        frame.buffer_mut()[(x, y)]
            .set_style(ui::bright().add_modifier(Modifier::REVERSED | Modifier::BOLD));
    }
    record(e, area, TargetAction::Value(control, first_row, left));
}

#[allow(clippy::too_many_arguments)] // The button and its hit target share the same geometry.
fn draw_command(
    frame: &mut Frame,
    s: &CalendarState,
    e: &Editor,
    x: &mut u16,
    y: u16,
    right: u16,
    label: &str,
    command: EditorCommand,
    selected: bool,
) {
    let action = match command {
        EditorCommand::Save => Action::Save,
        EditorCommand::Cancel => Action::Cancel,
        EditorCommand::Import => Action::Import,
        EditorCommand::Discard => Action::Discard,
        EditorCommand::Keep => Action::Keep,
        EditorCommand::Reload => Action::Reload,
        EditorCommand::None => return,
    };
    let start = *x;
    ui::button(frame, s, x, y, right, label, action, selected);
    if *x > start {
        record(
            e,
            Rect::new(start, y, *x - start, 1),
            TargetAction::Command(command),
        );
    }
}

pub fn draw(frame: &mut Frame, inner: Rect, s: &CalendarState, e: &Editor) {
    *e.geometry.borrow_mut() = Geometry::default();
    if inner.is_empty() {
        return;
    }
    if e.discard_prompt {
        ui::row(frame, inner, "Discard unsaved changes?", ui::accent());
        if inner.height > 1 {
            let mut x = inner.x;
            let y = (inner.y + 2).min(inner.bottom() - 1);
            draw_command(
                frame,
                s,
                e,
                &mut x,
                y,
                inner.right(),
                "d Discard",
                EditorCommand::Discard,
                e.discard_selected,
            );
            draw_command(
                frame,
                s,
                e,
                &mut x,
                y,
                inner.right(),
                "k Keep editing",
                EditorCommand::Keep,
                !e.discard_selected,
            );
        }
        return;
    }
    let footer_height = inner.height.min(3);
    let viewport = Rect::new(inner.x, inner.y, inner.width, inner.height - footer_height);
    let controls = e.visible_controls(s.today(), s.tz);
    let mut row_offset = 0;
    let rows: Vec<_> = controls
        .into_iter()
        .filter(|c| {
            !matches!(
                c,
                EditorControl::Save | EditorControl::Cancel | EditorControl::Import
            )
        })
        .map(|control| {
            let height = control.value_rows(viewport.height as usize) + 1;
            let row = (control, row_offset, height);
            row_offset += height;
            row
        })
        .collect();
    let max_scroll = row_offset.saturating_sub(viewport.height as usize);
    let mut offset = e.scroll.get().min(max_scroll);
    let resized = e.last_area.replace(inner) != inner;
    if (e.reveal_focus.replace(false) || resized)
        && let Some((_, start, height)) = rows.iter().find(|(control, _, _)| *control == e.focus)
    {
        if *start < offset {
            offset = *start;
        }
        if start + height > offset + viewport.height as usize {
            offset = (start + height).saturating_sub(viewport.height as usize);
        }
    }
    offset = offset.min(max_scroll);
    e.scroll.set(offset);
    {
        let mut geometry = e.geometry.borrow_mut();
        geometry.viewport = viewport;
        geometry.max_scroll = max_scroll;
    }
    let content_width = viewport.width.saturating_sub(u16::from(max_scroll > 0));
    let visible_end = offset + viewport.height as usize;
    for (control, start, height) in rows {
        if start >= visible_end || start + height <= offset || content_width == 0 {
            continue;
        }
        if start >= offset {
            let label = Rect::new(
                viewport.x,
                viewport.y + (start - offset) as u16,
                content_width,
                1,
            );
            ui::row(
                frame,
                label,
                format!(
                    "{}{}",
                    if e.focus == control { "▸ " } else { "  " },
                    control.label(e.all_day)
                ),
                if e.focus == control {
                    ui::accent()
                } else {
                    ui::muted()
                },
            );
            record(e, label, TargetAction::Focus(control));
        }
        let value_start = start + 1;
        let clipped_start = value_start.max(offset);
        let clipped_end = (start + height).min(visible_end);
        if clipped_start >= clipped_end || content_width <= 2 {
            continue;
        }
        let value = Rect::new(
            viewport.x + 2,
            viewport.y + (clipped_start - offset) as u16,
            content_width - 2,
            (clipped_end - clipped_start) as u16,
        );
        if control.field().is_some() {
            draw_field(
                frame,
                e,
                control,
                value,
                clipped_start - value_start,
                height - 1,
            );
        } else {
            ui::row(
                frame,
                value,
                value_text(e, control),
                if e.focus == control {
                    ui::selection_style()
                } else {
                    ui::base()
                },
            );
            record(e, value, TargetAction::Toggle(control));
        }
    }
    if max_scroll > 0 && viewport.width > 0 && viewport.height > 0 {
        let thumb_len = ((viewport.height as usize * viewport.height as usize) / row_offset).max(1);
        let travel = viewport.height as usize - thumb_len;
        let thumb_start = offset * travel / max_scroll;
        for i in 0..viewport.height as usize {
            let area = Rect::new(viewport.right() - 1, viewport.y + i as u16, 1, 1);
            let thumb = i >= thumb_start && i < thumb_start + thumb_len;
            ui::row(
                frame,
                area,
                if thumb { "█" } else { "│" },
                if thumb { ui::accent() } else { ui::dim() },
            );
            let target = if i < thumb_start {
                offset.saturating_sub(viewport.height as usize)
            } else if i >= thumb_start + thumb_len {
                (offset + viewport.height as usize).min(max_scroll)
            } else {
                offset
            };
            record(e, area, TargetAction::ScrollTo(target));
        }
    }
    let stacked_buttons = e.access.edit
        && inner.width < Line::from(" Save  Cancel  Import iCal ").width() as u16
        && footer_height >= 2;
    if footer_height >= 3 && !stacked_buttons {
        ui::row(
            frame,
            Rect::new(inner.x, inner.bottom() - 3, inner.width, 1),
            format!("{} · Tab fields · Ctrl+S save", s.tz),
            ui::dim(),
        );
    }
    if footer_height >= 2 && (!stacked_buttons || footer_height >= 3) {
        let area = Rect::new(
            inner.x,
            inner.bottom() - if stacked_buttons { 3 } else { 2 },
            inner.width,
            1,
        );
        if let Some(error) = &e.error {
            ui::row(frame, area, error, ui::base().fg(theme::ERROR()));
            if e.existing.is_some() {
                record(e, area, TargetAction::Command(EditorCommand::Reload));
                ui::hit(s, area, Action::Reload);
            }
        } else {
            let hint = match e.focus {
                EditorControl::StartDate | EditorControl::EndDate => {
                    "Dates: Oct 2 or +2w · offsets from today"
                }
                EditorControl::LeadTime => "Lead: 1 day, 24h or 1h 30m",
                EditorControl::StartOccurrence | EditorControl::EndOccurrence => {
                    "DST clock change: Space chooses occurrence"
                }
                _ => "Space toggles · PgUp/PgDn or wheel scroll",
            };
            ui::row(frame, area, hint, ui::muted());
        }
    }
    let mut x = inner.x;
    let save_y = inner.bottom() - if stacked_buttons { 2 } else { 1 };
    draw_command(
        frame,
        s,
        e,
        &mut x,
        save_y,
        inner.right(),
        "Save",
        EditorCommand::Save,
        e.focus == EditorControl::Save,
    );
    draw_command(
        frame,
        s,
        e,
        &mut x,
        save_y,
        inner.right(),
        "Cancel",
        EditorCommand::Cancel,
        e.focus == EditorControl::Cancel,
    );
    if e.access.edit {
        if stacked_buttons {
            x = inner.x;
        }
        draw_command(
            frame,
            s,
            e,
            &mut x,
            inner.bottom() - 1,
            inner.right(),
            "Import iCal",
            EditorCommand::Import,
            e.focus == EditorControl::Import,
        );
    }
}

#[cfg(test)]
#[path = "editor_test.rs"]
mod tests;
