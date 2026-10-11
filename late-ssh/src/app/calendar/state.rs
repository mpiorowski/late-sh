//! The session's side of the board: a render mirror of what the service
//! handed it, the cursor, and the modal stack. Nothing here writes; every
//! action goes to `svc.rs` and comes back as a `Reply` on the tick.
pub use super::editor::Editor;
use super::{
    navigation::{ClickRecord, ContextMenu, NavigationFrame, Selection},
    svc::{CalendarService, Query, Reply, SaveError},
};
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, Utc};
use chrono_tz::Tz;
use late_core::models::calendar::{CalendarEvent, CalendarView, EventTiming};
use ratatui::layout::Rect;
use std::{
    cell::{Cell, RefCell},
    time::{Duration as StdDuration, Instant},
};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Agenda,
    List,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    View,
    Board,
    Previous,
    Next,
    Today,
    New,
    Edit,
    Delete,
    Rsvp,
    Date(NaiveDate),
    MenuChoice(usize),
    Event(Uuid),
    EventAt(Uuid, NaiveDate),
    Save,
    Cancel,
    Discard,
    Keep,
    Reload,
}
#[derive(Clone, Debug)]
pub struct Hit {
    pub area: Rect,
    pub action: Action,
}
#[derive(Clone, Debug)]
pub struct ScrollPane {
    pub area: Rect,
    pub pane: Pane,
}
pub enum Modal {
    Details(CalendarEvent),
    Editor(Box<Editor>),
    Delete(CalendarEvent),
}
pub fn month_start(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap()
}
pub fn shift_month(d: NaiveDate, delta: i32) -> NaiveDate {
    let index = d.year() * 12 + d.month0() as i32 + delta;
    let first = NaiveDate::from_ymd_opt(index.div_euclid(12), index.rem_euclid(12) as u32 + 1, 1)
        .unwrap_or(d);
    let next = if first.month() == 12 {
        NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)
    };
    first
        .with_day(
            d.day()
                .min(next.map(|n| (n - first).num_days() as u32).unwrap_or(28)),
        )
        .unwrap()
}
/// Weeks start on Monday.
pub fn week_start(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}
pub struct CalendarState {
    pub viewer: Uuid,
    /// Moderator or admin, as the last load read it: staff delete any post.
    pub staff: bool,
    /// Whether the board is drawn over the viewer's own events.
    pub show_board: bool,
    pub view: CalendarView,
    pub selected: NaiveDate,
    pub tz: Tz,
    pub events: Vec<CalendarEvent>,
    /// The board events the viewer said they are in.
    pub rsvps: Vec<Uuid>,
    pub modal: Option<Modal>,
    pub error: Option<String>,
    pub pending: bool,
    pub loading: bool,
    pub selection: Selection,
    pub reveal_event: Cell<bool>,
    pub modal_parents: Vec<NavigationFrame>,
    pub context_menu: Option<ContextMenu>,
    pub last_click: RefCell<Option<ClickRecord>>,
    pub reload_request: bool,
    pub event_index: usize,
    pub scroll: usize,
    pub agenda_scroll: usize,
    pub list_rows: Cell<usize>,
    pub agenda_rows: Cell<usize>,
    pub reveal_selected: Cell<bool>,
    pub hits: RefCell<Vec<Hit>>,
    pub panes: RefCell<Vec<ScrollPane>>,
    pub geometry: Cell<Rect>,
    pub max_scroll: Cell<usize>,
    pub max_agenda: Cell<usize>,
    service: CalendarService,
    changed: watch::Receiver<u64>,
    board: watch::Receiver<Vec<CalendarEvent>>,
    tx: mpsc::UnboundedSender<Reply>,
    rx: mpsc::UnboundedReceiver<Reply>,
    pub generation: u64,
    pub open_generation: u64,
    /// The event a details refresh was asked for after an invalidation, so
    /// the answer replaces the open modal instead of stacking a new one.
    refreshing: Option<Uuid>,
    needs_refresh: bool,
    last_refresh: Instant,
    initialized: bool,
}
impl CalendarState {
    pub fn new(service: CalendarService, viewer: Uuid) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let changed = service.subscribe();
        let board = service.board_live();
        let mut s = Self {
            viewer,
            staff: false,
            show_board: true,
            view: CalendarView::Month,
            selected: Utc::now().date_naive(),
            tz: chrono_tz::UTC,
            events: Vec::new(),
            rsvps: Vec::new(),
            modal: None,
            error: None,
            pending: false,
            loading: false,
            selection: Selection::Date,
            reveal_event: Cell::new(false),
            modal_parents: Vec::new(),
            context_menu: None,
            last_click: RefCell::new(None),
            reload_request: false,
            event_index: 0,
            scroll: 0,
            agenda_scroll: 0,
            list_rows: Cell::new(1),
            agenda_rows: Cell::new(1),
            reveal_selected: Cell::new(true),
            hits: RefCell::new(Vec::new()),
            panes: RefCell::new(Vec::new()),
            geometry: Cell::new(Rect::default()),
            max_scroll: Cell::new(0),
            max_agenda: Cell::new(0),
            service,
            changed,
            board,
            tx,
            rx,
            generation: 0,
            open_generation: 0,
            refreshing: None,
            needs_refresh: false,
            last_refresh: Instant::now(),
            initialized: false,
        };
        s.refresh();
        s
    }
    pub fn today(&self) -> NaiveDate {
        Utc::now().with_timezone(&self.tz).date_naive()
    }
    pub fn range(&self) -> (NaiveDate, NaiveDate) {
        match self.view {
            CalendarView::Month => {
                let first = week_start(month_start(self.selected));
                (first, first + Duration::days(42))
            }
            CalendarView::List => {
                let first = month_start(self.selected);
                (first, shift_month(first, 1))
            }
        }
    }
    pub fn refresh(&mut self) {
        self.needs_refresh = false;
        self.generation += 1;
        self.loading = true;
        self.last_refresh = Instant::now();
        let (from, to) = self.range();
        self.service.load(
            Query {
                viewer: self.viewer,
                board: self.show_board,
                from,
                to,
                tz: self.tz,
                generation: self.generation,
            },
            self.tx.clone(),
        );
    }
    pub fn invalidate_geometry(&self) {
        self.hits.borrow_mut().clear();
        self.panes.borrow_mut().clear();
        self.geometry.set(Rect::default());
        if let Some(Modal::Editor(editor)) = &self.modal {
            editor.invalidate_geometry();
        }
    }
    pub fn navigate(&mut self, delta: i32) {
        self.selected = shift_month(self.selected, delta);
        self.reset_scroll();
        self.refresh();
    }
    pub fn toggle_view(&mut self) {
        self.view = self.view.toggled();
        self.reset_scroll();
        self.refresh();
    }
    pub fn toggle_board(&mut self) {
        self.show_board = !self.show_board;
        self.events.clear();
        self.reset_scroll();
        self.refresh();
    }
    pub fn reset_scroll(&mut self) {
        self.scroll = 0;
        self.agenda_scroll = 0;
        self.reveal_selected.set(true);
        self.event_index = 0;
        self.selection = Selection::Date;
        self.context_menu = None;
        self.invalidate_geometry();
    }
    pub fn day_events(&self, date: NaiveDate) -> Vec<&CalendarEvent> {
        let mut e: Vec<_> = self
            .events
            .iter()
            .filter(|e| {
                let (a, b) = e.timing.dates(self.tz);
                a <= date && date < b
            })
            .collect();
        e.sort_by_key(|e| event_order(e, self.tz));
        e
    }
    pub fn ordered_events(&self) -> Vec<&CalendarEvent> {
        let mut e: Vec<_> = self.events.iter().collect();
        e.sort_by_key(|e| event_order(e, self.tz));
        e
    }
    pub fn selected_event(&self) -> Option<CalendarEvent> {
        self.selected_target_event()
    }
    /// Whether the viewer said they are in.
    pub fn going(&self, id: Uuid) -> bool {
        self.rsvps.contains(&id)
    }
    pub fn open(&mut self, id: Uuid) {
        self.open_generation += 1;
        self.reload_request = false;
        self.refreshing = None;
        self.service
            .open(self.viewer, id, self.open_generation, self.tx.clone());
        self.error = None;
    }
    /// The replica service behind this session's mirror.
    pub fn service(&self) -> &CalendarService {
        &self.service
    }
    pub fn service_reload(&mut self, id: Uuid) {
        self.open_generation += 1;
        self.reload_request = true;
        self.refreshing = None;
        self.service
            .open(self.viewer, id, self.open_generation, self.tx.clone());
    }
    pub fn cancel_open(&mut self) {
        self.open_generation += 1;
        self.refreshing = None;
    }
    pub fn save_editor(&mut self) {
        if self.pending {
            return;
        }
        if let Some(Modal::Editor(e)) = &mut self.modal {
            let today = e.today();
            match e.draft(today) {
                Ok(d) => {
                    self.pending = true;
                    self.service
                        .save(self.viewer, e.source, e.existing, d, self.tx.clone());
                }
                Err(err) => e.error = Some(err.to_string()),
            }
        }
    }
    pub fn confirm_delete(&mut self) {
        if self.pending {
            return;
        }
        if let Some(Modal::Delete(e)) = &self.modal {
            self.pending = true;
            self.service
                .delete(self.viewer, e.id, e.revision, self.tx.clone());
        }
    }
    /// Say you are in, or take it back, on the details event or the
    /// selection. A personal event takes nothing.
    pub fn toggle_rsvp(&mut self) {
        if self.pending {
            return;
        }
        let event = match &self.modal {
            Some(Modal::Details(e)) => Some(e.clone()),
            _ => self.selected_event(),
        };
        let Some(event) = event else {
            self.error = Some("Select an event first".into());
            return;
        };
        if !event.is_board() {
            self.error = Some("Only board events take an I'm in".into());
            return;
        }
        self.pending = true;
        let going = !self.going(event.id);
        self.service
            .rsvp(self.viewer, event.id, going, self.tx.clone());
    }
    /// Open an event the strip or the panel showed: the day it starts, then
    /// its details over the page. The copy is the shared snapshot's, so the
    /// details are up before the page's own load lands.
    pub fn show_event(&mut self, event: CalendarEvent) {
        let start = event.timing.dates(self.tz).0;
        if self.modal.is_some() || !self.modal_parents.is_empty() {
            self.modal = None;
            self.modal_parents.clear();
        }
        self.select_date(start);
        self.selection = Selection::Event(event.id);
        self.push_modal(Modal::Details(event));
    }
    pub fn tick(&mut self, visible: bool, tz: Tz) -> bool {
        let mut changed = false;
        if self.tz != tz {
            let was_today = self.selected == self.today();
            self.tz = tz;
            if !self.initialized || was_today {
                self.selected = self.today();
            }
            self.refresh();
            changed = true;
        }
        let invalid = self.changed.has_changed().unwrap_or(false);
        if invalid {
            self.changed.borrow_and_update();
            // Modal unwinding must not reconcile against the temporary empty
            // cache. Preserve browser UUIDs until the authorized load arrives.
            self.loading = true;
            self.events.clear();
            self.clear_click();
            self.invalidate_geometry();
            self.generation += 1;
            self.needs_refresh = true;
            if visible {
                self.refresh();
            }
            // Details the viewer has open may have changed under them, or
            // been deleted by staff: read it again and swap it in.
            if let Some(Modal::Details(e)) = &self.modal {
                let id = e.id;
                self.open_generation += 1;
                self.reload_request = false;
                self.refreshing = Some(id);
                self.service
                    .open(self.viewer, id, self.open_generation, self.tx.clone());
            }
            changed = true;
        } else if visible
            && (self.needs_refresh || self.last_refresh.elapsed() >= StdDuration::from_secs(60))
        {
            self.refresh();
            changed = true;
        }
        if self.board.has_changed().unwrap_or(false) {
            self.board.borrow_and_update();
            changed = true;
        }
        while let Ok(reply) = self.rx.try_recv() {
            changed |= self.apply(reply);
        }
        changed
    }
    /// The board's live posts, soonest first: the shared snapshot minus
    /// what ended since it was read. Personal events are never here.
    pub fn live(&self) -> Vec<CalendarEvent> {
        self.live_at(Utc::now())
    }
    pub fn live_at(&self, now: DateTime<Utc>) -> Vec<CalendarEvent> {
        self.board
            .borrow()
            .iter()
            .filter(|e| e.live(now))
            .cloned()
            .collect()
    }
    pub fn apply(&mut self, reply: Reply) -> bool {
        match reply {
            Reply::Loaded { generation, result } if generation == self.generation => {
                self.loading = false;
                match result {
                    Ok(s) => {
                        let previous_range = self.range();
                        self.events = s.events;
                        self.rsvps = s.rsvps;
                        self.staff = s.staff;
                        self.initialized = true;
                        if self.range() != previous_range {
                            self.events.clear();
                            self.refresh();
                        }
                        self.error = None;
                        if !self.loading {
                            self.reconcile_selection();
                        }
                    }
                    Err(e) => {
                        self.events.clear();
                        self.error = Some(e);
                    }
                }
                true
            }
            Reply::Opened { generation, result } if generation == self.open_generation => {
                let refreshing = self.refreshing.take();
                match result {
                    Ok(latest) => {
                        if self.reload_request {
                            if let Some(Modal::Editor(e)) = &mut self.modal {
                                e.existing = Some((latest.id, latest.revision));
                                e.conflict = false;
                                e.error = Some(format!(
                                    "Reloaded revision {}: {}. Your draft is retained; review before saving.",
                                    latest.revision, latest.title
                                ));
                            }
                        } else if refreshing == Some(latest.id) {
                            if let Some(Modal::Details(open)) = &mut self.modal
                                && open.id == latest.id
                            {
                                *open = latest;
                            }
                        } else {
                            self.push_modal(Modal::Details(latest));
                        }
                    }
                    Err(e) => {
                        if let Some(id) = refreshing
                            && matches!(&self.modal, Some(Modal::Details(open)) if open.id == id)
                        {
                            self.finish_delete(id);
                        }
                        self.error = Some(e);
                    }
                }
                true
            }
            Reply::Saved(result) => {
                self.pending = false;
                match result {
                    Ok(e) => {
                        self.replace_saved(e);
                        self.refresh();
                    }
                    Err(SaveError::Conflict(message)) => match &mut self.modal {
                        Some(Modal::Editor(e)) => {
                            e.error = Some(message);
                            e.conflict = true;
                        }
                        _ => self.error = Some(message),
                    },
                    // Another refusal leaves a conflict standing: the
                    // revision stays stale until a reload.
                    Err(SaveError::Other(message)) => match &mut self.modal {
                        Some(Modal::Editor(e)) => e.error = Some(message),
                        _ => self.error = Some(message),
                    },
                }
                true
            }
            Reply::Deleted(result) => {
                self.pending = false;
                match result {
                    Ok(()) => {
                        if let Some(Modal::Delete(e)) = &self.modal {
                            let id = e.id;
                            self.finish_delete(id);
                        }
                        self.refresh();
                    }
                    Err(e) => self.error = Some(e),
                }
                true
            }
            Reply::Rsvp(result) => {
                self.pending = false;
                match result {
                    Ok(e) => {
                        let going = self.going(e.id);
                        // The reply is the toggle's answer: it carries the
                        // count, and the viewer's own side flips here.
                        if going {
                            self.rsvps.retain(|id| *id != e.id);
                        } else {
                            self.rsvps.push(e.id);
                        }
                        self.replace_event(e);
                        // A load started before this write committed may
                        // still land with the old answer; the generation
                        // bump drops it and reads again.
                        self.refresh();
                    }
                    Err(e) => self.error = Some(e),
                }
                true
            }
            _ => false,
        }
    }
    /// Swap a fresh copy of an event into every surface holding one.
    fn replace_event(&mut self, event: CalendarEvent) {
        for loaded in &mut self.events {
            if loaded.id == event.id {
                *loaded = event.clone();
            }
        }
        if let Some(Modal::Details(open)) = &mut self.modal
            && open.id == event.id
        {
            *open = event.clone();
        }
        for frame in &mut self.modal_parents {
            if let Some(Modal::Details(previous)) = &mut frame.modal
                && previous.id == event.id
            {
                *previous = event.clone();
            }
        }
    }
}
pub fn event_order(e: &CalendarEvent, tz: Tz) -> (NaiveDate, NaiveTime, Uuid) {
    match e.timing {
        EventTiming::AllDay { start, .. } => (start, NaiveTime::MIN, e.id),
        EventTiming::Timed { start, .. } => {
            let t = start.with_timezone(&tz);
            (t.date_naive(), t.time(), e.id)
        }
    }
}
