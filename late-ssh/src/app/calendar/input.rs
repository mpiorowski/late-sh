use super::{
    date_entry,
    editor::{self, EditorCommand},
    ical,
    navigation::{ClickTarget, MenuAction, Selection},
    state::{Action, CalendarState, Editor, Modal, Pane},
};
use crate::app::{
    common::{
        primitives::{Banner, Screen},
        textarea_input::handle_single_line_edit,
    },
    input::{MouseButton, MouseEvent, MouseEventKind, ParsedInput},
    state::App,
};
use chrono::Duration;
use late_core::models::calendar::{
    CalendarSource, CalendarView, CreationTier, EventAccess, event_access,
};
use ratatui_textarea::TextArea;
use std::time::Instant;

fn key(event: &ParsedInput) -> Option<char> {
    match event {
        ParsedInput::Byte(b) if b.is_ascii() => Some(*b as char),
        ParsedInput::Char(c) => Some(*c),
        _ => None,
    }
}

fn ascii_command(event: &ParsedInput) -> Option<ParsedInput> {
    match event {
        ParsedInput::Char(c) if c.is_ascii() => Some(ParsedInput::Byte(*c as u8)),
        _ => None,
    }
}

pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    let handled = handle_calendar_event(app, event);
    if let Some(text) = app.calendar.clipboard.take() {
        app.pending_clipboard = Some(text);
        app.banner = Some(Banner::success("Event copied as iCalendar"));
    }
    handled
}

fn handle_calendar_event(app: &mut App, event: &ParsedInput) -> bool {
    // VTE emits printable ASCII as Char, while control keys arrive as Byte.
    // Calendar command dispatch uses one representation; Unicode text and paste
    // remain unchanged and the editor still owns its field input first.
    let normalized = ascii_command(event);
    let event = normalized.as_ref().unwrap_or(event);
    if matches!(event, ParsedInput::Mouse(_)) && !app.interaction_mode.mouse_enabled() {
        return true;
    }
    let page = app.screen == Screen::Calendars;
    let s = &mut app.calendar;
    if s.context_menu.is_some() {
        handle_menu(s, event);
        return true;
    }
    if s.modal.is_some() {
        handle_modal(s, event);
        return true;
    }
    if let ParsedInput::Mouse(m) = event {
        return handle_mouse(s, m);
    }
    if !page {
        if app.screen == Screen::Dashboard && !app.chat.composing && key(event) == Some('u') {
            act(s, Action::Upcoming);
            return true;
        }
        return false;
    }
    s.clear_click();
    let action = match key(event) {
        Some('j') => {
            s.move_selected_event(1, false);
            return true;
        }
        Some('k') => {
            s.move_selected_event(-1, false);
            return true;
        }
        Some('c' | 'C') => Some(Action::Source),
        Some('v' | 'V') => Some(Action::View),
        Some('[') => Some(Action::Previous),
        Some(']') => Some(Action::Next),
        Some('t' | 'T') => Some(Action::Today),
        Some('g' | 'G') => Some(Action::Go),
        Some('n' | 'N') => Some(Action::New),
        Some('y' | 'Y') => Some(Action::Copy),
        Some('e') => Some(Action::Edit),
        Some('u') => Some(Action::Upcoming),
        Some('s' | 'S') => Some(Action::Settings),
        Some('\r') => Some(match s.selection {
            Selection::Event(id) => Action::Event(id),
            Selection::Slot(_) => Action::New,
            Selection::Date => Action::Agenda(s.selected),
        }),
        _ => None,
    };
    if let Some(action) = action {
        act(s, action);
        return true;
    }
    match event {
        ParsedInput::Delete => act(s, Action::Delete),
        ParsedInput::CtrlArrow(b'C') => s
            .day_scroll
            .set((s.day_scroll.get() + 12).min(s.max_days.get())),
        ParsedInput::CtrlArrow(b'D') => s.day_scroll.set(s.day_scroll.get().saturating_sub(12)),
        ParsedInput::PageUp | ParsedInput::PageDown => {
            let pane = if s.view == CalendarView::List {
                Pane::List
            } else if s.view == CalendarView::Month {
                Pane::Agenda
            } else {
                Pane::Grid
            };
            scroll(
                s,
                pane,
                if matches!(event, ParsedInput::PageUp) {
                    -6
                } else {
                    6
                },
            );
        }
        ParsedInput::Arrow(k) => {
            if s.view == CalendarView::List {
                s.move_selected_event(if matches!(k, b'A' | b'D') { -1 } else { 1 }, false);
            } else if s.is_timed_view() {
                let date = s.selected
                    + Duration::days(match k {
                        b'D' => -1,
                        b'C' => 1,
                        _ => 0,
                    });
                let minute = (i32::from(s.slot_minute)
                    + match k {
                        b'A' => -30,
                        b'B' => 30,
                        _ => 0,
                    })
                .clamp(0, 1410) as u16;
                let changed = date != s.selected;
                s.select_slot(date, minute);
                if changed {
                    s.refresh();
                }
            } else {
                let delta = match k {
                    b'A' => -7,
                    b'B' => 7,
                    b'D' => -1,
                    b'C' => 1,
                    _ => 0,
                };
                s.select_date(s.selected + Duration::days(delta));
                s.refresh();
            }
        }
        _ => return false,
    }
    true
}

fn scroll(s: &mut CalendarState, pane: Pane, delta: i32) {
    s.clear_click();
    match pane {
        Pane::Grid => {
            let step = delta.unsigned_abs().min(s.hour_rows.get().max(1) as u32) as isize;
            let max = 48usize.saturating_sub(s.hour_rows.get().min(48));
            s.hour_scroll = s
                .hour_scroll
                .saturating_add_signed(if delta < 0 { -step } else { step })
                .min(max);
        }
        Pane::Agenda => {
            s.agenda_scroll = s
                .agenda_scroll
                .saturating_add_signed(delta as isize)
                .min(s.max_agenda.get())
        }
        Pane::List | Pane::Upcoming => {
            s.scroll = s
                .scroll
                .saturating_add_signed(delta as isize)
                .min(s.max_scroll.get())
        }
        Pane::Picker => {
            s.picker_reveal.set(false);
            s.picker_scroll.set(
                s.picker_scroll
                    .get()
                    .saturating_add_signed(delta as isize)
                    .min(s.max_picker.get()),
            );
        }
    }
}

fn click_target(action: &Action) -> Option<ClickTarget> {
    match action {
        Action::Date(date) => Some(ClickTarget::Date(*date)),
        Action::Slot(date, minute) => Some(ClickTarget::Slot {
            date: *date,
            minute: *minute,
        }),
        Action::Event(id) => Some(ClickTarget::Event(*id)),
        Action::EventAt(id, date) => Some(ClickTarget::EventAt(*id, *date)),
        _ => None,
    }
}

fn select_target(s: &mut CalendarState, target: &ClickTarget) {
    match *target {
        ClickTarget::Date(date) => {
            let changed = date != s.selected;
            s.select_date(date);
            if changed {
                s.refresh();
            }
        }
        ClickTarget::Slot { date, minute } => {
            let changed = date != s.selected;
            s.select_slot(date, minute);
            if changed {
                s.refresh();
            }
        }
        ClickTarget::EventAt(id, date) => {
            s.select_event(id, Some(date), matches!(s.modal, Some(Modal::Upcoming)));
        }
        ClickTarget::Event(id) => {
            s.select_event(id, None, matches!(s.modal, Some(Modal::Upcoming)));
        }
    }
}

fn handle_mouse(s: &mut CalendarState, m: &MouseEvent) -> bool {
    let (Some(x), Some(y)) = (m.x.checked_sub(1), m.y.checked_sub(1)) else {
        return false;
    };
    if s.pending {
        return true;
    }
    if matches!(m.kind, MouseEventKind::Down) {
        let action = s
            .hits
            .borrow()
            .iter()
            .rev()
            .find(|h| h.area.contains((x, y).into()))
            .map(|h| h.action.clone());
        if let Some(action) = action {
            if let Some(target) = click_target(&action) {
                match m.button {
                    Some(MouseButton::Right) => {
                        select_target(s, &target);
                        s.open_context_menu(target, (x, y));
                    }
                    Some(MouseButton::Left) => {
                        let double = s.register_click(target, Instant::now());
                        select_target(s, &target);
                        if double {
                            match target {
                                ClickTarget::Date(date) => act(s, Action::Agenda(date)),
                                ClickTarget::Slot { .. } => act(s, Action::New),
                                ClickTarget::Event(id) => act(s, Action::Event(id)),
                                ClickTarget::EventAt(id, date) => act(s, Action::EventAt(id, date)),
                            }
                        }
                    }
                    _ => return false,
                }
                return true;
            }
            if m.button == Some(MouseButton::Left) {
                s.clear_click();
                act(s, action);
                return true;
            }
        }
        // An intervening click on empty padding is not part of a double-click
        // on a previously selected row.
        s.clear_click();
    }
    let pane = s
        .panes
        .borrow()
        .iter()
        .find(|p| p.area.contains((x, y).into()))
        .map(|p| p.pane);
    if let Some(pane) = pane {
        match m.kind {
            MouseEventKind::ScrollUp => scroll(s, pane, -3),
            MouseEventKind::ScrollDown => scroll(s, pane, 3),
            MouseEventKind::ScrollLeft => {
                s.clear_click();
                s.day_scroll.set(s.day_scroll.get().saturating_sub(12));
            }
            MouseEventKind::ScrollRight => {
                s.clear_click();
                s.day_scroll
                    .set((s.day_scroll.get() + 12).min(s.max_days.get()));
            }
            _ => return false,
        }
        return true;
    }
    false
}

fn handle_menu(s: &mut CalendarState, event: &ParsedInput) {
    let normalized = ascii_command(event);
    let event = normalized.as_ref().unwrap_or(event);
    if s.pending {
        return;
    }
    if let ParsedInput::Mouse(m) = event {
        if m.kind == MouseEventKind::Down {
            let (Some(x), Some(y)) = (m.x.checked_sub(1), m.y.checked_sub(1)) else {
                return;
            };
            if m.button == Some(MouseButton::Left) {
                let choice = s
                    .hits
                    .borrow()
                    .iter()
                    .rev()
                    .find(|h| h.area.contains((x, y).into()))
                    .and_then(|h| {
                        if let Action::MenuChoice(i) = h.action {
                            Some(i)
                        } else {
                            None
                        }
                    });
                if let Some(i) = choice {
                    act(s, Action::MenuChoice(i));
                    return;
                }
            }
            s.context_menu = None;
            s.invalidate_geometry();
            s.clear_click();
        } else if matches!(
            m.kind,
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
        ) && let Some(menu) = &mut s.context_menu
        {
            menu.move_selection(if m.kind == MouseEventKind::ScrollUp {
                -1
            } else {
                1
            });
        }
        return;
    }
    match event {
        ParsedInput::Arrow(b'A') | ParsedInput::Byte(b'k') => {
            if let Some(menu) = &mut s.context_menu {
                menu.move_selection(-1);
            }
        }
        ParsedInput::Arrow(b'B') | ParsedInput::Byte(b'j' | b'\t') => {
            if let Some(menu) = &mut s.context_menu {
                menu.move_selection(1);
            }
        }
        ParsedInput::Byte(b'\r') => {
            if let Some(menu) = &s.context_menu {
                act(s, Action::MenuChoice(menu.selected));
            }
        }
        ParsedInput::Byte(0x1b) => escape(s),
        _ => {}
    }
}

pub fn escape(s: &mut CalendarState) {
    if matches!(s.modal, Some(Modal::Import(_))) {
        s.import_generation += 1;
    }
    s.cancel_open();
    s.clear_click();
    if s.pending {
        return;
    }
    if s.context_menu.take().is_some() {
        s.invalidate_geometry();
        return;
    }
    if let Some(Modal::Editor(e)) = &mut s.modal {
        if e.discard_prompt {
            e.discard_prompt = false;
            return;
        }
        if e.dirty() {
            e.discard_prompt = true;
            return;
        }
    }
    s.pop_modal();
    s.error = None;
}

fn editor_command(s: &mut CalendarState, command: EditorCommand) {
    match command {
        EditorCommand::None => {}
        EditorCommand::Save => act(s, Action::Save),
        EditorCommand::Cancel => escape(s),
        EditorCommand::Import => act(s, Action::Import),
        EditorCommand::Discard => act(s, Action::Discard),
        EditorCommand::Keep => act(s, Action::Keep),
        EditorCommand::Reload => act(s, Action::Reload),
    }
}

fn source_index(s: &CalendarState) -> usize {
    match s.source {
        CalendarSource::Server => 0,
        CalendarSource::Personal(id) if id == s.viewer => 1,
        CalendarSource::Personal(id) => s
            .public
            .iter()
            .position(|p| p.owner_id == id)
            .map(|n| n + 2)
            .unwrap_or(0),
    }
}

fn choose_source(s: &mut CalendarState, index: usize) {
    let source = match index {
        0 => CalendarSource::Server,
        1 => CalendarSource::Personal(s.viewer),
        _ => {
            let Some(calendar) = s.public.get(index - 2) else {
                return;
            };
            CalendarSource::Personal(calendar.owner_id)
        }
    };
    s.source = source;
    s.events.clear();
    s.reset_scroll();
    s.refresh();
}

fn choose_view(s: &mut CalendarState, index: usize) {
    if let Some(view) = CalendarView::ALL.get(index) {
        s.view = *view;
        s.reset_scroll();
        s.refresh();
    }
}

pub fn act(s: &mut CalendarState, action: Action) {
    if s.pending {
        return;
    }
    let today = s.today();
    if !matches!(
        action,
        Action::Event(_) | Action::EventAt(_, _) | Action::Reload
    ) {
        s.cancel_open();
    }
    match action {
        Action::Previous => {
            s.clear_click();
            s.navigate(-1);
        }
        Action::Next => {
            s.clear_click();
            s.navigate(1);
        }
        Action::Today => {
            s.selected = today;
            s.reset_scroll();
            s.refresh();
        }
        Action::Source => {
            s.push_modal(Modal::Source(source_index(s)));
        }
        Action::CycleSource(delta) => {
            let index = (source_index(s) as isize + delta as isize)
                .rem_euclid((s.public.len() + 2) as isize) as usize;
            choose_source(s, index);
        }
        Action::CycleView(delta) => {
            let index = CalendarView::ALL
                .iter()
                .position(|v| *v == s.view)
                .unwrap_or(0);
            let index = (index as isize + delta as isize)
                .rem_euclid(CalendarView::ALL.len() as isize) as usize;
            choose_view(s, index);
        }
        Action::View => s.push_modal(Modal::View(
            CalendarView::ALL
                .iter()
                .position(|v| *v == s.view)
                .unwrap_or(0),
        )),
        Action::Go => {
            s.error = None;
            s.push_modal(Modal::Go(Box::new(TextArea::default())));
        }
        Action::Upcoming => s.push_modal(Modal::Upcoming),
        Action::Settings => s.push_modal(Modal::Settings {
            draft: s.preferences.clone(),
            focus: 0,
        }),
        Action::Date(date) => {
            s.select_date(date);
            s.refresh();
        }
        Action::Slot(date, minute) => {
            s.select_slot(date, minute);
            s.refresh();
        }
        Action::Agenda(date) => {
            if date != s.selected {
                s.select_date(date);
                s.refresh();
            }
            s.push_modal(Modal::Agenda);
        }
        Action::EventAt(id, date) => {
            s.select_event(id, Some(date), matches!(s.modal, Some(Modal::Upcoming)));
            s.clear_click();
            s.open(id);
        }
        Action::Event(id) => {
            s.select_event(id, None, matches!(s.modal, Some(Modal::Upcoming)));
            s.clear_click();
            s.open(id);
        }
        Action::New => {
            let personal = s.source == CalendarSource::Personal(s.viewer);
            if personal || (s.source == CalendarSource::Server && s.role != CreationTier::User) {
                let mut e = Editor::new(
                    s.source,
                    s.selected,
                    EventAccess {
                        edit: true,
                        notifications: personal || s.role == CreationTier::Admin,
                        delegate: !personal && s.role == CreationTier::Admin,
                    },
                );
                if s.is_timed_view() && !matches!(s.modal, Some(Modal::Agenda)) {
                    e.all_day = false;
                    e.fields[3] = TextArea::from(vec![format!(
                        "{:02}:{:02}",
                        s.slot_minute / 60,
                        s.slot_minute % 60
                    )]);
                    e.ever_assigned = true;
                    e.initial = e.fingerprint();
                }
                s.push_modal(Modal::Editor(Box::new(e)));
            } else {
                s.error = Some("This calendar is read-only".into());
            }
        }
        Action::Import => {
            if matches!(&s.modal, Some(Modal::Editor(editor)) if editor.access.edit && !editor.discard_prompt)
            {
                s.import_generation += 1;
                s.error = None;
                s.push_modal(Modal::Import(Box::default()));
            }
        }
        Action::Copy => {
            let event = if let Some(Modal::Details(e)) = &s.modal {
                Some(e.clone())
            } else {
                s.selected_event()
            };
            if let Some(event) = event {
                s.clipboard = Some(ical::export(&event));
            } else {
                s.error = Some("Select an event to copy".into());
            }
        }
        Action::Edit | Action::Delete => {
            let event = if let Some(Modal::Details(e)) = &s.modal {
                Some(e.clone())
            } else {
                s.selected_event()
            };
            if let Some(e) = event {
                if event_access(&e, s.viewer, s.role).edit {
                    let modal = if action == Action::Edit {
                        Modal::Editor(Box::new(Editor::from_event(&e, s.viewer, s.role, s.tz)))
                    } else {
                        Modal::Delete(e)
                    };
                    s.push_modal(modal);
                } else {
                    s.error = Some("This event is read-only".into());
                }
            }
        }
        Action::Field(n) => {
            if let Some(Modal::Settings { focus, .. }) = &mut s.modal {
                *focus = n;
            }
            if let Some(Modal::Import(import)) = &mut s.modal {
                import.focus = n.min(2);
            }
        }
        Action::ToggleField(n) => {
            act(s, Action::Field(n));
            act(s, Action::Toggle);
        }
        Action::Toggle => {
            if let Some(Modal::Settings { draft, focus }) = &mut s.modal {
                match focus {
                    0 => draft.week_start = if draft.week_start == 0 { 6 } else { 0 },
                    1 => {
                        let i = CalendarView::ALL
                            .iter()
                            .position(|v| *v == draft.default_view)
                            .unwrap_or(0);
                        draft.default_view = CalendarView::ALL[(i + 1) % 5];
                    }
                    2 => draft.server_overlay = !draft.server_overlay,
                    3 => draft.public = !draft.public,
                    _ => {}
                }
            }
        }
        Action::Save => {
            if matches!(s.modal, Some(Modal::Import(_))) {
                s.read_import();
            } else if let Some(Modal::Go(input)) = &s.modal {
                match date_entry::parse(&input.lines().join(""), s.selected, today) {
                    Ok(date) => {
                        s.pop_modal();
                        s.selected = date;
                        s.error = None;
                        s.reset_scroll();
                        s.refresh();
                    }
                    Err(error) => s.error = Some(error.to_string()),
                }
            } else if matches!(s.modal, Some(Modal::Settings { .. })) {
                s.save_settings();
            } else if matches!(s.modal, Some(Modal::Delete(_))) {
                s.confirm_delete();
            } else {
                s.save_editor();
            }
        }
        Action::Cancel => escape(s),
        Action::Discard => {
            s.pop_modal();
        }
        Action::Keep => {
            if let Some(Modal::Editor(e)) = &mut s.modal {
                e.discard_prompt = false;
            }
        }
        Action::Reload => {
            if let Some(Modal::Import(import)) = &mut s.modal {
                import.candidates = None;
                import.error = None;
                import.focus = 0;
            }
            if let Some(Modal::Editor(e)) = &s.modal
                && let Some((id, _)) = e.existing
            {
                s.service_reload(id);
            }
        }
        Action::Choice(i) => {
            if matches!(s.modal, Some(Modal::Import(_))) {
                s.choose_import(i);
            } else if matches!(s.modal, Some(Modal::Source(_))) && i < s.public.len() + 2 {
                s.pop_modal();
                choose_source(s, i);
            } else if matches!(s.modal, Some(Modal::View(_))) && i < CalendarView::ALL.len() {
                s.pop_modal();
                choose_view(s, i);
            }
        }
        Action::MenuChoice(i) => {
            let Some(menu) = s.context_menu.take() else {
                return;
            };
            let Some(action) = menu.items.get(i).copied() else {
                return;
            };
            s.invalidate_geometry();
            s.clear_click();
            match action {
                MenuAction::Open => match menu.target {
                    ClickTarget::Event(id) => act(s, Action::Event(id)),
                    ClickTarget::EventAt(id, date) => act(s, Action::EventAt(id, date)),
                    _ => {}
                },
                MenuAction::Agenda => match menu.target {
                    ClickTarget::Date(date) | ClickTarget::Slot { date, .. } => {
                        act(s, Action::Agenda(date))
                    }
                    _ => {}
                },
                MenuAction::New => act(s, Action::New),
                MenuAction::Edit => act(s, Action::Edit),
                MenuAction::Delete => act(s, Action::Delete),
                MenuAction::Copy => act(s, Action::Copy),
            }
        }
    }
}

fn handle_modal(s: &mut CalendarState, event: &ParsedInput) {
    let normalized = ascii_command(event);
    let event = normalized.as_ref().unwrap_or(event);
    if s.pending {
        return;
    }
    let today = s.today();
    if let Some(Modal::Import(import)) = &mut s.modal {
        let action = import.handle(event, s.picker_rows.get());
        s.picker_reveal.set(true);
        if let Some(action) = action {
            act(s, action);
        } else if let ParsedInput::Mouse(m) = event {
            handle_mouse(s, m);
        }
        return;
    }
    if let Some(Modal::Editor(e)) = &mut s.modal {
        let command = if let ParsedInput::Mouse(m) = event {
            let (Some(x), Some(y)) = (m.x.checked_sub(1), m.y.checked_sub(1)) else {
                return;
            };
            match m.kind {
                MouseEventKind::Down if m.button == Some(MouseButton::Left) => {
                    editor::click(e, x, y, today, s.tz).unwrap_or(EditorCommand::None)
                }
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                    editor::scroll(
                        e,
                        x,
                        y,
                        if m.kind == MouseEventKind::ScrollUp {
                            -3
                        } else {
                            3
                        },
                    );
                    EditorCommand::None
                }
                _ => EditorCommand::None,
            }
        } else {
            editor::handle_key(e, event, today, s.tz)
        };
        editor_command(s, command);
        return;
    }
    if let ParsedInput::Mouse(m) = event {
        handle_mouse(s, m);
        return;
    }
    s.clear_click();
    if key(event) == Some('\u{1b}') {
        escape(s);
        return;
    }
    if let Some(Modal::Go(input)) = &mut s.modal {
        if key(event) == Some('\r') {
            act(s, Action::Save);
        } else {
            let before = input.lines().join("");
            handle_single_line_edit(input, event, 100);
            if input.lines().join("") != before {
                s.error = None;
            }
        }
        return;
    }
    let picker_len = if matches!(s.modal, Some(Modal::View(_))) {
        5
    } else {
        s.public.len() + 2
    };
    match &mut s.modal {
        Some(Modal::Source(i)) | Some(Modal::View(i)) => {
            let action = match event {
                ParsedInput::Arrow(b'A') | ParsedInput::Byte(b'k') | ParsedInput::BackTab => {
                    *i = i.saturating_sub(1);
                    s.picker_reveal.set(true);
                    None
                }
                ParsedInput::Arrow(b'B') | ParsedInput::Byte(b'j' | b'\t') => {
                    *i = (*i + 1) % picker_len;
                    s.picker_reveal.set(true);
                    None
                }
                ParsedInput::PageUp => {
                    *i = i.saturating_sub(s.picker_rows.get());
                    s.picker_reveal.set(true);
                    None
                }
                ParsedInput::PageDown => {
                    *i = (*i + s.picker_rows.get()).min(picker_len - 1);
                    s.picker_reveal.set(true);
                    None
                }
                ParsedInput::Byte(b'\r') => Some(Action::Choice(*i)),
                _ => None,
            };
            if let Some(action) = action {
                act(s, action);
            }
        }
        Some(Modal::Settings { focus, .. }) => {
            let action = match event {
                ParsedInput::Arrow(b'A') | ParsedInput::BackTab => {
                    *focus = (*focus + 5) % 6;
                    None
                }
                ParsedInput::Arrow(b'B') | ParsedInput::Byte(b'\t') => {
                    *focus = (*focus + 1) % 6;
                    None
                }
                ParsedInput::Byte(b'\r') => Some(if *focus == 4 {
                    Action::Save
                } else if *focus == 5 {
                    Action::Cancel
                } else {
                    Action::Toggle
                }),
                ParsedInput::Byte(b' ') => Some(Action::Toggle),
                ParsedInput::Byte(0x13) => Some(Action::Save),
                _ => None,
            };
            if let Some(action) = action {
                act(s, action);
            }
        }
        Some(Modal::Details(_)) => match event {
            ParsedInput::Byte(b'e') => act(s, Action::Edit),
            ParsedInput::Byte(b'y' | b'Y') => act(s, Action::Copy),
            ParsedInput::Byte(b'q') => escape(s),
            ParsedInput::Delete => act(s, Action::Delete),
            ParsedInput::PageDown | ParsedInput::Arrow(b'B') => scroll(s, Pane::List, 3),
            ParsedInput::PageUp | ParsedInput::Arrow(b'A') => scroll(s, Pane::List, -3),
            _ => {}
        },
        Some(Modal::Delete(_)) => match key(event) {
            Some('y') => s.confirm_delete(),
            Some('n') => escape(s),
            _ => {}
        },
        Some(Modal::Upcoming) | Some(Modal::Agenda) => {
            let upcoming = matches!(s.modal, Some(Modal::Upcoming));
            match event {
                ParsedInput::Arrow(b'A') | ParsedInput::Byte(b'k') => {
                    s.move_selected_event(-1, upcoming)
                }
                ParsedInput::Arrow(b'B') | ParsedInput::Byte(b'j') => {
                    s.move_selected_event(1, upcoming)
                }
                ParsedInput::PageUp => scroll(
                    s,
                    if upcoming {
                        Pane::Upcoming
                    } else {
                        Pane::Agenda
                    },
                    -5,
                ),
                ParsedInput::PageDown => scroll(
                    s,
                    if upcoming {
                        Pane::Upcoming
                    } else {
                        Pane::Agenda
                    },
                    5,
                ),
                ParsedInput::Byte(b'\r') => {
                    if let Some(e) = s.selected_event() {
                        act(s, Action::Event(e.id));
                    }
                }
                ParsedInput::Byte(b'n') if !upcoming => act(s, Action::New),
                ParsedInput::Byte(b'e') => act(s, Action::Edit),
                ParsedInput::Byte(b'y' | b'Y') => act(s, Action::Copy),
                ParsedInput::Delete => act(s, Action::Delete),
                _ => {}
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "input_test.rs"]
mod tests;
