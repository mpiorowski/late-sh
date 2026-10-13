use super::{
    editor::{self, EditorCommand},
    navigation::{ClickTarget, MenuAction, Selection},
    state::{Action, CalendarState, Editor, Modal, Pane},
};
use crate::app::{
    common::primitives::Screen,
    input::{MouseButton, MouseEvent, MouseEventKind, ParsedInput},
    state::App,
};
use chrono::Duration;
use late_core::models::calendar::{CalendarSource, CalendarView, event_access};
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

/// The calendar's share of a session's input: everything while one of its
/// modals or menus is up, the page's keys and clicks on screen 7, nothing
/// anywhere else.
pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
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
    if !page {
        return false;
    }
    if let ParsedInput::Mouse(m) = event {
        return handle_mouse(s, m);
    }
    handle_key(s, event)
}

/// A key on the page itself: no modal, no menu up.
pub(super) fn handle_key(s: &mut CalendarState, event: &ParsedInput) -> bool {
    s.clear_click();
    let action = match key(event) {
        Some('j') => {
            s.move_selected_event(1);
            return true;
        }
        Some('k') => {
            s.move_selected_event(-1);
            return true;
        }
        Some('v' | 'V') => Some(Action::View),
        Some('b' | 'B') => Some(Action::Board),
        Some('[') => Some(Action::Previous),
        Some(']') => Some(Action::Next),
        Some('t' | 'T') => Some(Action::Today),
        Some('n' | 'N') => Some(Action::New),
        Some('e') => Some(Action::Edit),
        Some('d') => Some(Action::Delete),
        Some('i' | 'I') => Some(Action::Rsvp),
        Some('\r') => Some(enter(s)),
        _ => None,
    };
    if let Some(action) = action {
        act(s, action);
        return true;
    }
    match event {
        ParsedInput::Delete => act(s, Action::Delete),
        ParsedInput::PageUp | ParsedInput::PageDown => {
            let pane = if s.view == CalendarView::List {
                Pane::List
            } else {
                Pane::Agenda
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
                s.move_selected_event(if matches!(k, b'A' | b'D') { -1 } else { 1 });
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

/// What Enter and a double click do: open the marked event, or on an empty
/// day start a new one there.
fn enter(s: &CalendarState) -> Action {
    match s.selection {
        Selection::Event(id) => Action::Event(id),
        Selection::Date => Action::New,
    }
}
pub(super) fn scroll(s: &mut CalendarState, pane: Pane, delta: i32) {
    s.clear_click();
    match pane {
        Pane::Agenda => {
            s.agenda_scroll = s
                .agenda_scroll
                .saturating_add_signed(delta as isize)
                .min(s.max_agenda.get())
        }
        Pane::List => {
            s.scroll = s
                .scroll
                .saturating_add_signed(delta as isize)
                .min(s.max_scroll.get())
        }
    }
}

fn click_target(action: &Action) -> Option<ClickTarget> {
    match action {
        Action::Date(date) => Some(ClickTarget::Date(*date)),
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
        ClickTarget::EventAt(id, date) => {
            s.select_event(id, Some(date));
        }
        ClickTarget::Event(id) => {
            s.select_event(id, None);
        }
    }
}

pub(super) fn handle_mouse(s: &mut CalendarState, m: &MouseEvent) -> bool {
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
                                ClickTarget::Date(_) => act(s, enter(s)),
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
            _ => return false,
        }
        return true;
    }
    false
}

pub(super) fn handle_menu(s: &mut CalendarState, event: &ParsedInput) {
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
        EditorCommand::Discard => act(s, Action::Discard),
        EditorCommand::Keep => act(s, Action::Keep),
        EditorCommand::Reload => act(s, Action::Reload),
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
        Action::View => s.toggle_view(),
        Action::Board => s.toggle_board(),
        Action::Date(date) => {
            s.select_date(date);
            s.refresh();
        }
        Action::EventAt(id, date) => {
            s.select_event(id, Some(date));
            s.clear_click();
            s.open(id);
        }
        Action::Event(id) => {
            s.select_event(id, None);
            s.clear_click();
            s.open(id);
        }
        Action::New => {
            // A new draft is private until the poster flips it to the board:
            // nobody posts to everyone by mistake.
            s.push_modal(Modal::Editor(Box::new(Editor::new(
                CalendarSource::Personal,
                s.selected,
                s.tz,
            ))));
        }
        Action::Rsvp => s.toggle_rsvp(),
        Action::Edit | Action::Delete => {
            let event = if let Some(Modal::Details(e)) = &s.modal {
                Some(e.clone())
            } else {
                s.selected_event()
            };
            if let Some(e) = event {
                let access = event_access(&e, s.viewer, s.staff);
                let allowed = match action {
                    Action::Edit => access.edit,
                    _ => access.delete,
                };
                if allowed {
                    let modal = if action == Action::Edit {
                        Modal::Editor(Box::new(Editor::from_event(&e, s.viewer, s.staff, s.tz)))
                    } else {
                        Modal::Delete(e)
                    };
                    s.push_modal(modal);
                } else {
                    s.error = Some("This event is not yours to change".into());
                }
            } else {
                s.error = Some("Select an event first".into());
            }
        }
        Action::Save => {
            if matches!(s.modal, Some(Modal::Delete(_))) {
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
            if let Some(Modal::Editor(e)) = &s.modal
                && let Some((id, _)) = e.existing
            {
                s.service_reload(id);
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
                    ClickTarget::Date(_) => {}
                },
                MenuAction::New => act(s, Action::New),
                MenuAction::Edit => act(s, Action::Edit),
                MenuAction::Delete => act(s, Action::Delete),
                MenuAction::Rsvp(_) => act(s, Action::Rsvp),
            }
        }
    }
}

pub(super) fn handle_modal(s: &mut CalendarState, event: &ParsedInput) {
    let normalized = ascii_command(event);
    let event = normalized.as_ref().unwrap_or(event);
    if s.pending {
        return;
    }
    if let Some(Modal::Editor(e)) = &mut s.modal {
        let today = e.today();
        let command = if let ParsedInput::Mouse(m) = event {
            let (Some(x), Some(y)) = (m.x.checked_sub(1), m.y.checked_sub(1)) else {
                return;
            };
            match m.kind {
                MouseEventKind::Down if m.button == Some(MouseButton::Left) => {
                    editor::click(e, x, y, today).unwrap_or(EditorCommand::None)
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
            editor::handle_key(e, event, today)
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
    match &s.modal {
        Some(Modal::Details(_)) => match event {
            ParsedInput::Byte(b'e') => act(s, Action::Edit),
            ParsedInput::Byte(b'd') | ParsedInput::Delete => act(s, Action::Delete),
            ParsedInput::Byte(b'i' | b'I') => act(s, Action::Rsvp),
            ParsedInput::Byte(b'q') => escape(s),
            ParsedInput::PageDown | ParsedInput::Arrow(b'B') => scroll(s, Pane::List, 3),
            ParsedInput::PageUp | ParsedInput::Arrow(b'A') => scroll(s, Pane::List, -3),
            _ => {}
        },
        Some(Modal::Delete(_)) => match key(event) {
            Some('y') => s.confirm_delete(),
            Some('n') => escape(s),
            _ => {}
        },
        Some(Modal::Editor(_)) | None => {}
    }
}
