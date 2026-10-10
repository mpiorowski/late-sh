//! Calendar-local selection and modal history. Browser state survives drill-downs.
use super::state::{CalendarState, Modal, month_start};
use chrono::{NaiveDate, Timelike};
use late_core::models::calendar::{
    CalendarEvent, CalendarSource, CalendarView, CreationTier, EventTiming, event_access,
};
use ratatui::layout::Rect;
use std::{cell::Cell, time::Instant};
use uuid::Uuid;

const DOUBLE_CLICK_WINDOW: std::time::Duration = std::time::Duration::from_millis(400);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Selection {
    #[default]
    Date,
    Slot(u16),
    Event(Uuid),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickTarget {
    Date(NaiveDate),
    Slot { date: NaiveDate, minute: u16 },
    Event(Uuid),
    EventAt(Uuid, NaiveDate),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClickSurface {
    Page,
    Agenda,
    Upcoming,
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
    Agenda,
    New,
    Edit,
    Delete,
    Copy,
}

impl MenuAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Agenda => "Agenda",
            Self::New => "New event",
            Self::Edit => "Edit",
            Self::Delete => "Delete",
            Self::Copy => "Copy as iCal",
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
    pub hour_scroll: usize,
    pub day_scroll: usize,
    pub slot_minute: u16,
    pub picker_scroll: usize,
    reveal_selected: bool,
    reveal_event: bool,
    neighbor: Option<Uuid>,
}

impl CalendarState {
    pub fn is_timed_view(&self) -> bool {
        matches!(
            self.view,
            CalendarView::Week | CalendarView::ThreeDay | CalendarView::Day
        )
    }

    pub fn clear_click(&self) {
        self.last_click.borrow_mut().take();
    }

    /// First click selects immediately. Only a second click on the same semantic
    /// target and surface opens it; callers supply time so tests need no sleeps.
    pub fn register_click(&self, target: ClickTarget, now: Instant) -> bool {
        let surface = match &self.modal {
            None => ClickSurface::Page,
            Some(Modal::Agenda) => ClickSurface::Agenda,
            Some(Modal::Upcoming) => ClickSurface::Upcoming,
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

    pub fn can_create_event(&self) -> bool {
        self.source == CalendarSource::Personal(self.viewer)
            || (self.source == CalendarSource::Server && self.role != CreationTier::User)
    }

    pub fn select_date(&mut self, date: NaiveDate) {
        self.cancel_open();
        self.selected = date;
        self.selection = Selection::Date;
        self.event_index = 0;
        self.agenda_scroll = 0;
        self.reveal_selected.set(true);
        self.reveal_event.set(false);
        self.context_menu = None;
    }

    pub fn select_slot(&mut self, date: NaiveDate, minute: u16) {
        self.select_date(date);
        let minute = minute.min(1439) / 30 * 30;
        self.selection = Selection::Slot(minute);
        self.slot_minute = minute;
        self.reveal_time(minute);
    }

    /// The active target is exact. A selected date or blank slot must never
    /// silently resolve to the first event, especially for Edit and Delete.
    pub fn selected_target_event(&self) -> Option<CalendarEvent> {
        let Selection::Event(id) = self.selection else {
            return None;
        };
        if let Some(Modal::Details(e) | Modal::Delete(e)) = &self.modal
            && e.id == id
        {
            return Some(e.clone());
        }
        self.events
            .iter()
            .find(|e| e.id == id)
            .cloned()
            .or_else(|| self.upcoming().into_iter().find(|e| e.id == id))
    }

    pub fn select_event(&mut self, id: Uuid, date: Option<NaiveDate>, upcoming: bool) -> bool {
        let event = self
            .events
            .iter()
            .find(|e| e.id == id)
            .cloned()
            .or_else(|| self.upcoming().into_iter().find(|e| e.id == id));
        let Some(event) = event else {
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
                if !upcoming && self.view == CalendarView::List {
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
            .selection_events(upcoming)
            .iter()
            .position(|e| e.id == id)
            .unwrap_or(0);
        if let EventTiming::Timed { start, .. } = event.timing {
            let start = start.with_timezone(&self.tz);
            let minute = if start.date_naive() == selected {
                (start.hour() * 60 + start.minute()) as u16 / 30 * 30
            } else {
                0
            };
            self.slot_minute = minute;
            self.reveal_time(minute);
        } else if self.modal.is_none()
            && self.is_timed_view()
            && self
                .day_events(selected)
                .into_iter()
                .filter(|e| matches!(e.timing, EventTiming::AllDay { .. }))
                .position(|e| e.id == id)
                .is_some_and(|index| index >= 2)
        {
            self.push_modal(Modal::Agenda);
        }
        self.reveal_event_row(upcoming);
        true
    }

    fn reveal_time(&mut self, minute: u16) {
        let row = usize::from(minute / 30);
        let rows = self.hour_rows.get().max(1);
        if row < self.hour_scroll {
            self.hour_scroll = row;
        } else if row >= self.hour_scroll.saturating_add(rows) {
            self.hour_scroll = row.saturating_add(1).saturating_sub(rows);
        }
        self.hour_scroll = self.hour_scroll.min(48usize.saturating_sub(rows).min(47));
    }

    fn selection_events(&self, upcoming: bool) -> Vec<CalendarEvent> {
        if upcoming {
            self.upcoming()
        } else if self.view == CalendarView::List && !matches!(self.modal, Some(Modal::Agenda)) {
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
        if !matches!(self.selection, Selection::Event(_)) {
            return;
        }
        let upcoming = matches!(self.modal, Some(Modal::Upcoming));
        let events = self.selection_events(upcoming);
        if let Selection::Event(id) = self.selection
            && let Some(index) = events.iter().position(|e| e.id == id)
        {
            self.event_index = index;
            self.sync_event_cursor(&events[index], upcoming);
            return;
        }
        let index = self.event_index.min(events.len().saturating_sub(1));
        self.event_index = index;
        self.selection = events
            .get(index)
            .map_or(Selection::Date, |e| Selection::Event(e.id));
        if let Some(event) = events.get(index) {
            self.sync_event_cursor(event, upcoming);
        }
    }

    fn sync_event_cursor(&mut self, event: &CalendarEvent, upcoming: bool) {
        let (start, end) = event.timing.dates(self.tz);
        if self.selected < start || self.selected >= end {
            self.selected = if !upcoming && self.view == CalendarView::List {
                start.max(month_start(self.selected))
            } else {
                start
            };
            self.reveal_selected.set(true);
        }
        if let EventTiming::Timed { start, .. } = event.timing {
            let start = start.with_timezone(&self.tz);
            self.slot_minute = if start.date_naive() == self.selected {
                (start.hour() * 60 + start.minute()) as u16 / 30 * 30
            } else {
                0
            };
        }
    }

    pub fn move_selected_event(&mut self, delta: isize, upcoming: bool) {
        self.clear_click();
        let events = self.selection_events(upcoming);
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
        self.select_event(events[index].id, None, upcoming);
    }

    fn reveal_event_row(&mut self, upcoming: bool) {
        let list = upcoming
            || (self.view == CalendarView::List && !matches!(self.modal, Some(Modal::Agenda)));
        let rows = if list {
            self.list_rows.get()
        } else {
            self.agenda_rows.get()
        };
        if rows == 0 {
            return;
        }
        let (start, end) = if list {
            let events = self.selection_events(upcoming);
            let mut row: usize = 0;
            let mut last_date = None;
            for e in events.iter().take(self.event_index + 1) {
                let date = e.timing.dates(self.tz).0.max(if upcoming {
                    NaiveDate::MIN
                } else {
                    month_start(self.selected)
                });
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
        let events = self.selection_events(matches!(self.modal, Some(Modal::Upcoming)));
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
            hour_scroll: self.hour_scroll,
            day_scroll: self.day_scroll.get(),
            slot_minute: self.slot_minute,
            picker_scroll: self.picker_scroll.get(),
            reveal_selected: self.reveal_selected.get(),
            reveal_event: self.reveal_event.get(),
            neighbor,
        });
        self.modal = Some(modal);
        self.scroll = 0;
        self.agenda_scroll = 0;
        self.picker_scroll.set(0);
        self.picker_reveal.set(true);
        if matches!(self.modal, Some(Modal::Upcoming)) {
            self.event_index = 0;
            self.selection = self
                .upcoming()
                .first()
                .map_or(Selection::Date, |e| Selection::Event(e.id));
        }
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
            self.hour_scroll = frame.hour_scroll;
            self.day_scroll.set(frame.day_scroll);
            self.slot_minute = frame.slot_minute;
            self.picker_scroll.set(frame.picker_scroll);
            self.reveal_selected.set(frame.reveal_selected);
            self.reveal_event.set(frame.reveal_event);
        } else {
            self.modal = None;
            self.reveal_event.set(false);
        }
        self.error = None;
        self.invalidate_geometry();
        if self.range() != previous_range {
            // An Upcoming event may have moved the temporary date into another
            // range. A save/periodic refresh there must not replace page data
            // permanently when its original date is restored.
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
            .find(|frame| {
                matches!(frame.modal, None | Some(Modal::Agenda | Modal::Upcoming))
                    && frame.selection == Selection::Event(id)
            })
            .and_then(|frame| frame.neighbor);
        self.events.retain(|e| e.id != id);
        self.notices.retain(|e| e.id != id);
        self.pop_modal();
        while matches!(&self.modal, Some(Modal::Details(e) | Modal::Delete(e)) if e.id == id) {
            self.pop_modal();
        }
        if self.selection == Selection::Event(id) {
            // The shared notice watch can lag the successful write. Never keep
            // a just-deleted target actionable while its invalidation arrives.
            let events: Vec<_> = self
                .selection_events(matches!(self.modal, Some(Modal::Upcoming)))
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
            let upcoming = matches!(self.modal, Some(Modal::Upcoming));
            self.select_event(selected, None, upcoming);
        }
    }

    /// An invalidation may revoke access while Details is hidden beneath another
    /// modal. Unwind through every retained shared snapshot before reloading.
    pub fn clear_shared_navigation(&mut self) {
        self.context_menu = None;
        self.clear_click();
        let shared = |modal: &Option<Modal>| matches!(modal, Some(Modal::Details(e) | Modal::Delete(e)) if e.owner_id.is_some_and(|id| id != self.viewer));
        if let Some(index) = self
            .modal_parents
            .iter()
            .position(|frame| shared(&frame.modal))
        {
            while self.modal_parents.len() > index {
                self.pop_modal();
            }
        }
        if matches!(&self.modal, Some(Modal::Details(e) | Modal::Delete(e)) if e.owner_id.is_some_and(|id| id != self.viewer))
        {
            self.pop_modal();
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
                    .or_else(|| self.upcoming().into_iter().find(|e| e.id == id))
                    .or_else(|| self.selected_target_event().filter(|e| e.id == id));
                let Some(event) = event else { return };
                let upcoming = matches!(self.modal, Some(Modal::Upcoming));
                let date = match target {
                    ClickTarget::EventAt(_, date) => Some(date),
                    _ => None,
                };
                self.select_event(id, date, upcoming);
                if event_access(&event, self.viewer, self.role).edit {
                    vec![
                        MenuAction::Open,
                        MenuAction::Edit,
                        MenuAction::Delete,
                        MenuAction::Copy,
                    ]
                } else {
                    vec![MenuAction::Open, MenuAction::Copy]
                }
            }
            ClickTarget::Date(date) => {
                self.select_date(date);
                if self.can_create_event() {
                    vec![MenuAction::Agenda, MenuAction::New]
                } else {
                    vec![MenuAction::Agenda]
                }
            }
            ClickTarget::Slot { date, minute } => {
                self.select_slot(date, minute);
                if self.can_create_event() {
                    vec![MenuAction::New, MenuAction::Agenda]
                } else {
                    vec![MenuAction::Agenda]
                }
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
