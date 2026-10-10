pub use super::editor::Editor;
use super::{
    import::Import,
    navigation::{ClickRecord, ContextMenu, NavigationFrame, Selection},
    svc::{CalendarService, Query, Reply},
};
use chrono::{Datelike, Duration, NaiveDate, NaiveTime, Utc};
use chrono_tz::Tz;
use late_core::models::calendar::{
    CalendarEvent, CalendarPreferences, CalendarSource, CalendarView, CreationTier, EventTiming,
    PublicCalendar,
};
use ratatui::layout::Rect;
use ratatui_textarea::TextArea;
use std::{
    cell::{Cell, RefCell},
    time::{Duration as StdDuration, Instant},
};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Grid,
    Agenda,
    List,
    Upcoming,
    Picker,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Source,
    CycleSource(i8),
    View,
    CycleView(i8),
    Previous,
    Next,
    Today,
    Go,
    New,
    Import,
    Copy,
    Edit,
    Delete,
    Upcoming,
    Settings,
    Date(NaiveDate),
    Agenda(NaiveDate),
    Slot(NaiveDate, u16),
    MenuChoice(usize),
    Event(Uuid),
    EventAt(Uuid, NaiveDate),
    Field(usize),
    Save,
    Cancel,
    Toggle,
    ToggleField(usize),
    Choice(usize),
    Reload,
    Discard,
    Keep,
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
    Source(usize),
    View(usize),
    Go(Box<TextArea<'static>>),
    Import(Box<Import>),
    Settings {
        draft: CalendarPreferences,
        focus: usize,
    },
    Details(CalendarEvent),
    Editor(Box<Editor>),
    Delete(CalendarEvent),
    Agenda,
    Upcoming,
}
pub fn parse_duration(s: &str) -> anyhow::Result<i64> {
    let s = s.trim();
    let duration = match s.parse::<u64>() {
        Ok(seconds) => StdDuration::from_secs(seconds),
        Err(_) => humantime::parse_duration(s).map_err(|_| {
            anyhow::anyhow!("Lead time: use a nonnegative duration, e.g. 24h or 1 day")
        })?,
    };
    anyhow::ensure!(
        duration.subsec_nanos() == 0,
        "Lead time must be whole seconds"
    );
    anyhow::ensure!(
        duration.as_secs() <= 315360000,
        "Lead time cannot exceed 3650 days"
    );
    Ok(duration.as_secs() as i64)
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
pub fn week_start(d: NaiveDate, start: u8) -> NaiveDate {
    d - Duration::days((d.weekday().num_days_from_monday() as i64 + 7 - start as i64) % 7)
}
pub struct CalendarState {
    pub viewer: Uuid,
    pub source: CalendarSource,
    pub view: CalendarView,
    pub selected: NaiveDate,
    pub tz: Tz,
    pub preferences: CalendarPreferences,
    pub public: Vec<PublicCalendar>,
    pub events: Vec<CalendarEvent>,
    pub notices: Vec<CalendarEvent>,
    pub role: CreationTier,
    pub modal: Option<Modal>,
    pub error: Option<String>,
    pub pending: bool,
    pub loading: bool,
    pub selection: Selection,
    pub slot_minute: u16,
    pub reveal_event: Cell<bool>,
    pub modal_parents: Vec<NavigationFrame>,
    pub context_menu: Option<ContextMenu>,
    pub last_click: RefCell<Option<ClickRecord>>,
    pub picker_scroll: Cell<usize>,
    pub picker_rows: Cell<usize>,
    pub max_picker: Cell<usize>,
    pub picker_reveal: Cell<bool>,
    pub reload_request: bool,
    pub event_index: usize,
    pub scroll: usize,
    pub agenda_scroll: usize,
    pub hour_scroll: usize,
    pub hour_rows: Cell<usize>,
    pub list_rows: Cell<usize>,
    pub agenda_rows: Cell<usize>,
    pub day_scroll: Cell<usize>,
    pub reveal_selected: Cell<bool>,
    pub hours_geometry: Cell<Rect>,
    pub hits: RefCell<Vec<Hit>>,
    pub panes: RefCell<Vec<ScrollPane>>,
    pub geometry: Cell<Rect>,
    pub max_scroll: Cell<usize>,
    pub max_agenda: Cell<usize>,
    pub max_days: Cell<usize>,
    service: CalendarService,
    changed: watch::Receiver<u64>,
    server: watch::Receiver<Vec<CalendarEvent>>,
    tx: mpsc::UnboundedSender<Reply>,
    rx: mpsc::UnboundedReceiver<Reply>,
    pub generation: u64,
    pub open_generation: u64,
    pub(super) import_generation: u64,
    pub(super) clipboard: Option<String>,
    needs_refresh: bool,
    last_refresh: Instant,
    initialized: bool,
}
impl CalendarState {
    pub fn new(service: CalendarService, viewer: Uuid) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let changed = service.subscribe();
        let server = service.server_notices();
        let mut s = Self {
            viewer,
            source: CalendarSource::Server,
            view: CalendarView::Month,
            selected: Utc::now().date_naive(),
            tz: chrono_tz::UTC,
            preferences: Default::default(),
            public: Vec::new(),
            events: Vec::new(),
            notices: Vec::new(),
            role: CreationTier::User,
            modal: None,
            error: None,
            pending: false,
            loading: false,
            selection: Selection::Date,
            slot_minute: 540,
            reveal_event: Cell::new(false),
            modal_parents: Vec::new(),
            context_menu: None,
            last_click: RefCell::new(None),
            picker_scroll: Cell::new(0),
            picker_rows: Cell::new(1),
            max_picker: Cell::new(0),
            picker_reveal: Cell::new(true),
            reload_request: false,
            event_index: 0,
            scroll: 0,
            agenda_scroll: 0,
            hour_scroll: 16,
            hour_rows: Cell::new(6),
            list_rows: Cell::new(1),
            agenda_rows: Cell::new(1),
            day_scroll: Cell::new(0),
            reveal_selected: Cell::new(true),
            hours_geometry: Cell::new(Rect::default()),
            hits: RefCell::new(Vec::new()),
            panes: RefCell::new(Vec::new()),
            geometry: Cell::new(Rect::default()),
            max_scroll: Cell::new(0),
            max_agenda: Cell::new(0),
            max_days: Cell::new(0),
            service,
            changed,
            server,
            tx,
            rx,
            generation: 0,
            open_generation: 0,
            import_generation: 0,
            clipboard: None,
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
                let first = week_start(month_start(self.selected), self.preferences.week_start);
                (first, first + Duration::days(42))
            }
            CalendarView::List => {
                let first = month_start(self.selected);
                (first, shift_month(first, 1))
            }
            CalendarView::Week => {
                let first = week_start(self.selected, self.preferences.week_start);
                (first, first + Duration::days(7))
            }
            CalendarView::ThreeDay => (self.selected, self.selected + Duration::days(3)),
            CalendarView::Day => (self.selected, self.selected + Duration::days(1)),
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
                source: self.source,
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
        self.selected = match self.view {
            CalendarView::Month | CalendarView::List => shift_month(self.selected, delta),
            CalendarView::Week => self.selected + Duration::days(7 * delta as i64),
            CalendarView::ThreeDay => self.selected + Duration::days(3 * delta as i64),
            CalendarView::Day => self.selected + Duration::days(delta as i64),
        };
        self.reset_scroll();
        self.refresh();
    }
    pub fn reset_scroll(&mut self) {
        self.scroll = 0;
        self.agenda_scroll = 0;
        self.day_scroll.set(0);
        self.reveal_selected.set(true);
        self.event_index = 0;
        self.selection = if self.is_timed_view() {
            Selection::Slot(self.slot_minute)
        } else {
            Selection::Date
        };
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
    pub fn open(&mut self, id: Uuid) {
        self.open_generation += 1;
        self.reload_request = false;
        self.service
            .open(self.viewer, id, self.open_generation, self.tx.clone());
        self.error = None;
    }
    pub fn service_reload(&mut self, id: Uuid) {
        self.open_generation += 1;
        self.reload_request = true;
        self.service
            .open(self.viewer, id, self.open_generation, self.tx.clone());
    }
    pub fn cancel_open(&mut self) {
        self.open_generation += 1;
    }
    pub fn save_editor(&mut self) {
        if self.pending {
            return;
        }
        let today = self.today();
        if let Some(Modal::Editor(e)) = &mut self.modal {
            match e.draft(today, self.tz) {
                Ok(d) => {
                    self.pending = true;
                    self.service
                        .save(self.viewer, e.source, e.existing, d, self.tx.clone());
                }
                Err(err) => e.error = Some(err.to_string()),
            }
        }
    }
    pub(super) fn read_import(&mut self) {
        if let Some(Modal::Import(import)) = &mut self.modal {
            if import.loading {
                return;
            }
            if import.candidates.is_some() {
                let selected = import.selected;
                self.choose_import(selected);
                return;
            }
            self.import_generation += 1;
            import.loading = true;
            import.error = None;
            self.service.import(
                import.input.lines().join("\n"),
                self.tz,
                self.import_generation,
                self.tx.clone(),
            );
        }
    }
    pub(super) fn choose_import(&mut self, index: usize) {
        let Some(Modal::Editor(editor)) = self
            .modal_parents
            .last()
            .and_then(|frame| frame.modal.as_ref())
        else {
            return;
        };
        let can_import = editor.access.edit
            && (editor.existing.is_some()
                || editor.source == CalendarSource::Personal(self.viewer)
                || (editor.source == CalendarSource::Server && self.role != CreationTier::User));
        let Some(Modal::Import(import)) = &mut self.modal else {
            return;
        };
        if !can_import {
            import.error = Some("This calendar is read-only".into());
            return;
        }
        let Some(candidate) = import.candidates.as_ref().and_then(|c| c.get(index)) else {
            return;
        };
        let draft = match &candidate.draft {
            Ok(draft) => draft.clone(),
            Err(error) => {
                import.error = Some(error.clone());
                return;
            }
        };
        self.import_generation += 1;
        self.pop_modal();
        if let Some(Modal::Editor(editor)) = &mut self.modal {
            editor.apply_import(&draft, self.tz);
        }
        self.invalidate_geometry();
    }
    pub fn save_settings(&mut self) {
        if self.pending {
            return;
        }
        if let Some(Modal::Settings { draft, .. }) = &self.modal {
            self.pending = true;
            self.service
                .preferences(self.viewer, draft.clone(), self.tx.clone());
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
            self.cancel_open();
            self.changed.borrow_and_update();
            // Modal unwinding must not reconcile against the temporary empty
            // cache. Preserve browser UUIDs until the authorized load arrives.
            self.loading = true;
            self.events.clear();
            self.notices.clear();
            self.public.clear();
            self.clear_click();
            self.clear_shared_navigation();
            self.invalidate_geometry();
            self.generation += 1;
            self.needs_refresh = true;
            if visible {
                self.refresh();
            }
            changed = true;
        } else if visible
            && (self.needs_refresh || self.last_refresh.elapsed() >= StdDuration::from_secs(60))
        {
            self.refresh();
            changed = true;
        }
        if self.server.has_changed().unwrap_or(false) {
            self.server.borrow_and_update();
            changed = true;
        }
        while let Ok(reply) = self.rx.try_recv() {
            changed |= self.apply(reply);
        }
        let now = Utc::now();
        let before = self.notices.len();
        self.notices.retain(|e| e.upcoming(now));
        changed |= before != self.notices.len();
        if matches!(self.modal, Some(Modal::Upcoming)) {
            let before = (self.selection, self.event_index);
            self.reconcile_selection();
            changed |= before != (self.selection, self.event_index);
        }
        changed
    }
    pub fn upcoming(&self) -> Vec<CalendarEvent> {
        let mut events: Vec<_> = self
            .server
            .borrow()
            .iter()
            .chain(self.notices.iter())
            .filter(|e| e.upcoming(Utc::now()))
            .cloned()
            .collect();
        events.sort_by_key(|e| event_order(e, self.tz));
        events
    }
    pub fn apply(&mut self, reply: Reply) -> bool {
        match reply {
            Reply::Imported { generation, result }
                if generation == self.import_generation
                    && matches!(&self.modal, Some(Modal::Import(import)) if import.loading) =>
            {
                let Some(Modal::Import(import)) = &mut self.modal else {
                    return false;
                };
                import.loading = false;
                let mut single = false;
                match result {
                    Ok(candidates) => {
                        single = candidates.len() == 1 && candidates[0].draft.is_ok();
                        import.candidates = Some(candidates);
                        import.selected = 0;
                        self.picker_reveal.set(true);
                    }
                    Err(error) => import.error = Some(error),
                }
                if single {
                    self.choose_import(0);
                }
                true
            }
            Reply::Loaded { generation, result } if generation == self.generation => {
                self.loading = false;
                match result {
                    Ok(s) => {
                        let previous_range = self.range();
                        self.events = s.events;
                        self.notices = s.personal_notices;
                        self.public = s.public;
                        self.role = s.role;
                        let first = !self.initialized;
                        self.initialized = true;
                        self.preferences = s.preferences;
                        if first && self.view != self.preferences.default_view {
                            self.view = self.preferences.default_view;
                        }
                        if self.range() != previous_range {
                            self.events.clear();
                            self.refresh();
                        }
                        self.error = s.event_error;
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
                match result {
                    Ok(latest) => {
                        if self.reload_request {
                            if let Some(Modal::Editor(e)) = &mut self.modal {
                                e.existing = Some((latest.id, latest.revision));
                                e.error = Some(format!(
                                    "Reloaded revision {}: {}. Your draft is retained; review before saving.",
                                    latest.revision, latest.title
                                ));
                            }
                        } else {
                            self.push_modal(Modal::Details(latest));
                        }
                    }
                    Err(e) => self.error = Some(e),
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
                    Err(err) => {
                        if let Some(Modal::Editor(e)) = &mut self.modal {
                            e.error = Some(err)
                        } else {
                            self.error = Some(err)
                        }
                    }
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
            Reply::Preferences(result) => {
                self.pending = false;
                match result {
                    Ok(p) => {
                        self.preferences = p;
                        self.pop_modal();
                        self.refresh();
                    }
                    Err(e) => self.error = Some(e),
                }
                true
            }
            _ => false,
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
