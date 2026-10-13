//! Calendar-local selection and modal history. Browser state survives drill-downs.
use super::state::{CalendarState, Modal, month_start};
use chrono::NaiveDate;
use late_core::models::calendar::{CalendarEvent, CalendarView, event_access};
use ratatui::layout::Rect;
use std::{cell::Cell, time::Instant};
use uuid::Uuid;

const DOUBLE_CLICK_WINDOW: std::time::Duration = std::time::Duration::from_millis(400);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Selection {
    #[default]
    Date,
    Event(Uuid),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickTarget {
    Date(NaiveDate),
    Event(Uuid),
    EventAt(Uuid, NaiveDate),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClickSurface {
    Page,
    Details(Uuid),
    Other,
}

#[derive(Clone, Copy, Debug)]
pub struct ClickRecord {
    target: ClickTarget,
    surface: ClickSurface,
    depth: usize,
    time: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Open,
    New,
    Edit,
    Delete,
    /// Say you are in (`true`) or take it back.
    Rsvp(bool),
}

impl MenuAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::New => "New event",
            Self::Edit => "Edit",
            Self::Delete => "Delete",
            Self::Rsvp(true) => "I'm in",
            Self::Rsvp(false) => "I'm out",
        }
    }
}

pub struct ContextMenu {
    pub target: ClickTarget,
    pub anchor: (u16, u16),
    pub selected: usize,
    pub items: Vec<MenuAction>,
    pub area: Cell<Rect>,
}

impl ContextMenu {
    pub fn selected_action(&self) -> Option<MenuAction> {
        self.items.get(self.selected).copied()
    }

    pub fn move_selection(&mut self, delta: isize) {
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(self.items.len().saturating_sub(1));
    }
}

/// A modal and the browser cursor it temporarily owns. The first frame has no
/// modal: it is the page underneath the complete modal chain.
pub struct NavigationFrame {
    pub modal: Option<Modal>,
    pub selected: NaiveDate,
    pub selection: Selection,
    pub event_index: usize,
    pub scroll: usize,
    pub agenda_scroll: usize,
    reveal_selected: bool,
    reveal_event: bool,
    neighbor: Option<Uuid>,
}

impl CalendarState {
    pub fn clear_click(&self) {
        self.last_click.borrow_mut().take();
    }

    /// First click selects immediately. Only a second click on the same semantic
    /// target and surface opens it; callers supply time so tests need no sleeps.
    pub fn register_click(&self, target: ClickTarget, now: Instant) -> bool {
        let surface = match &self.modal {
            None => ClickSurface::Page,
            Some(Modal::Details(e)) => ClickSurface::Details(e.id),
            _ => ClickSurface::Other,
        };
        let depth = self.modal_parents.len();
        let mut previous = self.last_click.borrow_mut();
        let double = previous.as_ref().is_some_and(|record| {
            record.target == target
                && record.surface == surface
                && record.depth == depth
                && now.saturating_duration_since(record.time) <= DOUBLE_CLICK_WINDOW
        });
        *previous = (!double).then_some(ClickRecord {
            target,
            surface,
            depth,
            time: now,
        });
        double
    }

    /// Land on a day. Its first event is selected with it, marked, so Enter
    /// opens it and `e`, `d`, `i` act on it; an empty day selects itself.
    pub fn select_date(&mut self, date: NaiveDate) {
        self.cancel_open();
        self.selected = date;
        self.selection = Selection::Date;
        self.event_index = 0;
        self.agenda_scroll = 0;
        self.reveal_selected.set(true);
        self.reveal_event.set(false);
        self.context_menu = None;
        if let Some(first) = self.day_events(date).first().map(|e| e.id) {
            self.select_event(first, Some(date));
        }
    }

    /// The active target is exact: the marked event, never a silent fallback
    /// to a day's first event, so Edit and Delete act on what is marked.
    pub fn selected_target_event(&self) -> Option<CalendarEvent> {
        let Selection::Event(id) = self.selection else {
            return None;
        };
        if let Some(Modal::Details(e) | Modal::Delete(e)) = &self.modal
            && e.id == id
        {
            return Some(e.clone());
        }
        self.events.iter().find(|e| e.id == id).cloned()
    }

    pub fn select_event(&mut self, id: Uuid, date: Option<NaiveDate>) -> bool {
        let Some(event) = self.events.iter().find(|e| e.id == id).cloned() else {
            return false;
        };
        self.cancel_open();
        let (start, end) = event.timing.dates(self.tz);
        let contains = |date| start <= date && date < end;
        let selected = date
            .filter(|date| contains(*date))
            .or_else(|| contains(self.selected).then_some(self.selected))
            .unwrap_or_else(|| {
                // A month list can include an event that began last month.
                // Selecting it should stay on its visible part of this month.
                if self.view == CalendarView::List {
                    start.max(month_start(self.selected))
                } else {
                    start
                }
            });
        self.selected = selected;
        self.selection = Selection::Event(id);
        self.context_menu = None;
        self.reveal_selected.set(true);
        self.reveal_event.set(true);
        self.event_index = self
            .selection_events()
            .iter()
            .position(|e| e.id == id)
            .unwrap_or(0);
        self.reveal_event_row();
        true
    }

    fn selection_events(&self) -> Vec<CalendarEvent> {
        if self.view == CalendarView::List {
            self.ordered_events().into_iter().cloned().collect()
        } else {
            self.day_events(self.selected)
                .into_iter()
                .cloned()
                .collect()
        }
    }

    /// Called after a completed refresh, not while invalidation has temporarily
    /// emptied the visible data. Keep UUID identity through sorting and updates.
    pub fn reconcile_selection(&mut self) {
        if self.loading
            || matches!(
                self.modal,
                Some(Modal::Details(_) | Modal::Editor(_) | Modal::Delete(_))
            )
        {
            return;
        }
        let events = self.selection_events();
        if !matches!(self.selection, Selection::Event(_)) {
            // A day landed on before its month loaded: mark its first event
            // now that it is here.
            if let Some(first) = events.first() {
                self.selection = Selection::Event(first.id);
                self.event_index = 0;
                self.reveal_event.set(true);
            }
            return;
        }
        if let Selection::Event(id) = self.selection
            && let Some(index) = events.iter().position(|e| e.id == id)
        {
            self.event_index = index;
            self.sync_event_cursor(&events[index]);
            return;
        }
        let index = self.event_index.min(events.len().saturating_sub(1));
        self.event_index = index;
        self.selection = events
            .get(index)
            .map_or(Selection::Date, |e| Selection::Event(e.id));
        if let Some(event) = events.get(index) {
            self.sync_event_cursor(event);
        }
    }

    fn sync_event_cursor(&mut self, event: &CalendarEvent) {
        let (start, end) = event.timing.dates(self.tz);
        if self.selected < start || self.selected >= end {
            self.selected = if self.view == CalendarView::List {
                start.max(month_start(self.selected))
            } else {
                start
            };
            self.reveal_selected.set(true);
        }
    }

    pub fn move_selected_event(&mut self, delta: isize) {
        self.clear_click();
        let events = self.selection_events();
        if events.is_empty() {
            self.selection = Selection::Date;
            self.event_index = 0;
            return;
        }
        let current = match self.selection {
            Selection::Event(id) => events.iter().position(|e| e.id == id),
            _ => None,
        };
        let index = current.map_or_else(
            || if delta < 0 { events.len() - 1 } else { 0 },
            |index| index.saturating_add_signed(delta).min(events.len() - 1),
        );
        self.select_event(events[index].id, None);
    }

    fn reveal_event_row(&mut self) {
        let list = self.view == CalendarView::List;
        let rows = if list {
            self.list_rows.get()
        } else {
            self.agenda_rows.get()
        };
        if rows == 0 {
            return;
        }
        let (start, end) = if list {
            let events = self.selection_events();
            let mut row: usize = 0;
            let mut last_date = None;
            for e in events.iter().take(self.event_index + 1) {
                let date = e.timing.dates(self.tz).0.max(month_start(self.selected));
                if last_date != Some(date) {
                    row += 1;
                    last_date = Some(date);
                }
                row += 1;
            }
            (row.saturating_sub(1), row)
        } else {
            let start = self.event_index * 2;
            (start, start + 2)
        };
        let scroll = if list {
            &mut self.scroll
        } else {
            &mut self.agenda_scroll
        };
        if start < *scroll {
            *scroll = start;
        } else if end > scroll.saturating_add(rows) {
            *scroll = end.saturating_sub(rows).min(start);
        }
    }

    pub fn push_modal(&mut self, modal: Modal) {
        self.context_menu = None;
        self.clear_click();
        let events = self.selection_events();
        let neighbor = if let Selection::Event(id) = self.selection {
            events.iter().position(|e| e.id == id).and_then(|index| {
                events
                    .get(index + 1)
                    .or_else(|| index.checked_sub(1).and_then(|i| events.get(i)))
                    .map(|e| e.id)
            })
        } else {
            None
        };
        self.modal_parents.push(NavigationFrame {
            modal: self.modal.take(),
            selected: self.selected,
            selection: self.selection,
            event_index: self.event_index,
            scroll: self.scroll,
            agenda_scroll: self.agenda_scroll,
            reveal_selected: self.reveal_selected.get(),
            reveal_event: self.reveal_event.get(),
            neighbor,
        });
        self.modal = Some(modal);
        self.scroll = 0;
        self.agenda_scroll = 0;
        self.error = None;
        self.invalidate_geometry();
    }

    pub fn pop_modal(&mut self) {
        let previous_range = self.range();
        self.cancel_open();
        self.context_menu = None;
        self.clear_click();
        if let Some(frame) = self.modal_parents.pop() {
            self.modal = frame.modal;
            self.selected = frame.selected;
            self.selection = frame.selection;
            self.event_index = frame.event_index;
            self.scroll = frame.scroll;
            self.agenda_scroll = frame.agenda_scroll;
            self.reveal_selected.set(frame.reveal_selected);
            self.reveal_event.set(frame.reveal_event);
        } else {
            self.modal = None;
            self.reveal_event.set(false);
        }
        self.error = None;
        self.invalidate_geometry();
        if self.range() != previous_range {
            // Details opened from the strip may have moved the temporary date
            // into another month. Restoring the original date reloads it.
            self.refresh();
        }
        self.reconcile_selection();
    }

    /// Save from Details returns to that same Details surface. Direct New/Edit
    /// becomes Details with the original browser still underneath.
    pub fn replace_saved(&mut self, event: CalendarEvent) {
        let id = event.id;
        for loaded in &mut self.events {
            if loaded.id == id {
                *loaded = event.clone();
            }
        }
        for frame in &mut self.modal_parents {
            if let Some(Modal::Details(previous)) = &mut frame.modal
                && previous.id == id
            {
                *previous = event.clone();
            }
        }
        let details_parent = self
            .modal_parents
            .last()
            .is_some_and(|frame| matches!(&frame.modal, Some(Modal::Details(e)) if e.id == id));
        if details_parent {
            self.pop_modal();
        } else {
            self.modal = Some(Modal::Details(event));
            self.scroll = 0;
            self.context_menu = None;
            self.clear_click();
            self.invalidate_geometry();
        }
        self.selection = Selection::Event(id);
        self.error = None;
    }

    pub fn finish_delete(&mut self, id: Uuid) {
        // A DB invalidation can empty the cache before the successful delete
        // reply arrives. Retain the originating browser's adjacent UUID so that
        // completion still selects it when the authorized reload finishes.
        let neighbor = self
            .modal_parents
            .iter()
            .rev()
            .find(|frame| frame.modal.is_none() && frame.selection == Selection::Event(id))
            .and_then(|frame| frame.neighbor);
        self.events.retain(|e| e.id != id);
        self.rsvps.retain(|going| *going != id);
        self.pop_modal();
        while matches!(&self.modal, Some(Modal::Details(e) | Modal::Delete(e)) if e.id == id) {
            self.pop_modal();
        }
        if self.selection == Selection::Event(id) {
            // The shared snapshot can lag the successful write. Never keep
            // a just-deleted target actionable while its invalidation arrives.
            let events: Vec<_> = self
                .selection_events()
                .into_iter()
                .filter(|e| e.id != id)
                .collect();
            self.event_index = self.event_index.min(events.len().saturating_sub(1));
            self.selection = events.get(self.event_index).map_or_else(
                || {
                    if self.loading {
                        neighbor.map_or(Selection::Date, Selection::Event)
                    } else {
                        Selection::Date
                    }
                },
                |e| Selection::Event(e.id),
            );
        }
        self.reconcile_selection();
        if let Selection::Event(selected) = self.selection {
            self.reveal_event.set(true);
            self.select_event(selected, None);
        }
    }

    pub fn open_context_menu(&mut self, target: ClickTarget, anchor: (u16, u16)) {
        self.clear_click();
        let items = match target {
            ClickTarget::Event(id) | ClickTarget::EventAt(id, _) => {
                let event = self
                    .events
                    .iter()
                    .find(|e| e.id == id)
                    .cloned()
                    .or_else(|| self.selected_target_event().filter(|e| e.id == id));
                let Some(event) = event else { return };
                let date = match target {
                    ClickTarget::EventAt(_, date) => Some(date),
                    _ => None,
                };
                self.select_event(id, date);
                let access = event_access(&event, self.viewer, self.staff);
                let mut items = vec![MenuAction::Open];
                if access.rsvp {
                    items.push(MenuAction::Rsvp(!self.going(id)));
                }
                if access.edit {
                    items.push(MenuAction::Edit);
                }
                if access.delete {
                    items.push(MenuAction::Delete);
                }
                items
            }
            ClickTarget::Date(date) => {
                self.select_date(date);
                vec![MenuAction::New]
            }
        };
        self.context_menu = Some(ContextMenu {
            target,
            anchor,
            selected: 0,
            items,
            area: Cell::new(Rect::default()),
        });
    }
}
