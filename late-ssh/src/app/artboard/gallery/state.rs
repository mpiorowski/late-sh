//! Per-session gallery state: the page rail, the four listings, the piece
//! being looked at, and the hang flow. Pure apart from handing tasks to the
//! service and draining what they send back in `tick`.

use std::cell::Cell;

use late_core::models::artboard_piece::{
    ApplauseOutcome, ListingCounts, PIECE_TITLE_MAX_CHARS, PieceListing,
};
use ratatui::layout::Rect;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::app::artboard::svc::ArtboardSnapshotKind;

use super::frame::FramedPiece;
use super::svc::{GalleryPiece, GalleryResult, GalleryService, HangRefusal, applause_label};

/// The gallery's listings, in rail order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GallerySection {
    ThisMonth,
    Newest,
    HallOfFame,
    Mine,
}

impl GallerySection {
    pub const ALL: [Self; 4] = [Self::ThisMonth, Self::Newest, Self::HallOfFame, Self::Mine];

    pub fn label(self) -> &'static str {
        match self {
            Self::ThisMonth => "This month",
            Self::Newest => "New",
            Self::HallOfFame => "Hall of fame",
            Self::Mine => "Mine",
        }
    }

    /// The line under the listing's title.
    pub fn hint(self) -> &'static str {
        match self {
            Self::ThisMonth => {
                "this month's pieces, most applauded first; top 3 at month end win ART badges and chips"
            }
            Self::Newest => "the latest pieces hung, any month",
            Self::HallOfFame => "each past month's winner",
            Self::Mine => "everything you have hung",
        }
    }

    pub fn listing(self) -> PieceListing {
        match self {
            Self::ThisMonth => PieceListing::ThisMonth,
            Self::Newest => PieceListing::Newest,
            Self::HallOfFame => PieceListing::HallOfFame,
            Self::Mine => PieceListing::Mine,
        }
    }

    fn from_listing(listing: PieceListing) -> Self {
        match listing {
            PieceListing::ThisMonth => Self::ThisMonth,
            PieceListing::Newest => Self::Newest,
            PieceListing::HallOfFame => Self::HallOfFame,
            PieceListing::Mine => Self::Mine,
        }
    }

    fn index(self) -> usize {
        match self {
            Self::ThisMonth => 0,
            Self::Newest => 1,
            Self::HallOfFame => 2,
            Self::Mine => 3,
        }
    }
}

/// One row of the Artboard page rail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailRow {
    /// The live board (or the archive being viewed): the default.
    Board,
    Gallery(GallerySection),
    /// Starts the framing flow on the board.
    Hang,
    /// One archive kind; Enter turns the rail into its list.
    Archive(ArtboardSnapshotKind),
}

impl RailRow {
    /// The rail, top to bottom. The gallery rows and the hang row only
    /// exist while the switch is on; the archives close the rail.
    pub fn rows(gallery_enabled: bool) -> Vec<Self> {
        let mut rows = vec![Self::Board];
        if gallery_enabled {
            rows.extend(GallerySection::ALL.iter().copied().map(Self::Gallery));
            rows.push(Self::Hang);
        }
        rows.extend(ArtboardSnapshotKind::ALL.iter().copied().map(Self::Archive));
        rows
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Board => "Board",
            Self::Gallery(section) => section.label(),
            Self::Hang => "Hang a piece",
            Self::Archive(kind) => kind.title(),
        }
    }
}

/// Where keys go on the Artboard page in view mode. The page lands on the
/// rail; the board is one Enter away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    /// The board cursor, as before the rail existed.
    Canvas,
    Rail,
    /// The piece list of the selected gallery section.
    List,
    /// One piece, full frame.
    Piece,
    /// An archive kind's key list, drawn in the rail's place; the board
    /// shows the key under the cursor.
    Archive,
}

/// The hang flow, from the rail row to the row in the database.
#[derive(Clone, Debug, PartialEq)]
pub enum HangFlow {
    Idle,
    /// Selecting the frame on the board.
    Framing,
    /// The frame passed the local rails; naming it.
    Confirm {
        framed: Box<FramedPiece>,
        title: String,
    },
    /// Sent to the database; waiting for the answer.
    Submitting,
}

/// What pressing Enter on a rail row asks the page to do. The page acts on
/// it because three of the answers need things the gallery does not hold
/// (the live board, the ban gate, the archive lists).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailActivation {
    FocusCanvas,
    OpenGallery,
    BeginHang,
    OpenArchive(ArtboardSnapshotKind),
}

#[derive(Debug, Default)]
struct SectionState {
    pieces: Vec<GalleryPiece>,
    loaded: bool,
    loading: bool,
    error: Option<String>,
    selected: usize,
    scroll: usize,
}

pub struct GalleryState {
    service: GalleryService,
    viewer_id: Uuid,
    results_tx: mpsc::UnboundedSender<GalleryResult>,
    results_rx: mpsc::UnboundedReceiver<GalleryResult>,
    focus: Focus,
    rail_index: usize,
    sections: [SectionState; 4],
    /// The rail's numbers, fetched on entry so they are there before any
    /// listing is opened. A loaded listing's own length wins over them.
    counts: Option<ListingCounts>,
    hang: HangFlow,
    /// The last thing the gallery had to say: a refusal, a landed hang, a
    /// failed applause. Shown in the gallery pane's notice line and on the
    /// framing bar; replaced by the next one.
    notice: Option<String>,
    /// The piece whose applause is in flight, so a held `v` sends one.
    pending_applause: Option<Uuid>,
    /// Published by the draw path so hit tests and the viewport math agree
    /// with what is on screen.
    rail_visible: Cell<bool>,
    rail_area: Cell<Rect>,
    list_area: Cell<Rect>,
    list_visible_height: Cell<usize>,
}

impl GalleryState {
    pub fn new(service: GalleryService, viewer_id: Uuid) -> Self {
        let (results_tx, results_rx) = mpsc::unbounded_channel();
        service.counts_task(viewer_id, results_tx.clone());
        Self {
            service,
            viewer_id,
            results_tx,
            results_rx,
            focus: Focus::Rail,
            rail_index: 0,
            sections: Default::default(),
            counts: None,
            hang: HangFlow::Idle,
            notice: None,
            pending_applause: None,
            rail_visible: Cell::new(false),
            rail_area: Cell::new(Rect::default()),
            list_area: Cell::new(Rect::default()),
            list_visible_height: Cell::new(1),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.service.is_enabled()
    }

    pub fn rows(&self) -> Vec<RailRow> {
        RailRow::rows(self.is_enabled())
    }

    pub fn selected_row(&self) -> RailRow {
        let rows = self.rows();
        rows[self.rail_index.min(rows.len() - 1)]
    }

    pub fn focus(&self) -> Focus {
        self.focus
    }

    pub fn hang(&self) -> &HangFlow {
        &self.hang
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// The section the detail pane shows, if the selected row is one.
    pub fn viewed_section(&self) -> Option<GallerySection> {
        match self.selected_row() {
            RailRow::Gallery(section) => Some(section),
            RailRow::Board | RailRow::Hang | RailRow::Archive(_) => None,
        }
    }

    /// True while the gallery, not the board, owns the detail pane.
    pub fn shows_gallery_pane(&self) -> bool {
        self.rail_visible.get() && self.viewed_section().is_some()
    }

    /// True while a key press must not be read as a global hotkey: typing
    /// a title, or framing on the board.
    pub fn captures_typing(&self) -> bool {
        matches!(self.hang, HangFlow::Framing | HangFlow::Confirm { .. })
    }

    /// True when Esc has somewhere to go on this page: out of a pane to the
    /// rail, or from the board to the rail. On the rail itself Esc is not
    /// the page's.
    pub fn claims_escape(&self) -> bool {
        self.captures_typing() || self.focus != Focus::Rail
    }

    /// True when `q` is not the quit key: a piece is up, or a title is
    /// being typed.
    pub fn claims_q(&self) -> bool {
        self.captures_typing() || self.focus == Focus::Piece
    }

    // ----- rail -----

    pub fn rail_visible(&self) -> bool {
        self.rail_visible.get()
    }

    pub fn set_rail_visible(&self, visible: bool) {
        self.rail_visible.set(visible);
    }

    pub fn set_rail_area(&self, area: Rect) {
        self.rail_area.set(area);
    }

    pub fn set_list_area(&self, area: Rect, visible_height: usize) {
        self.list_area.set(area);
        self.list_visible_height.set(visible_height.max(1));
    }

    pub fn focus_rail(&mut self) {
        self.focus = Focus::Rail;
    }

    pub fn focus_archive(&mut self) {
        if matches!(self.selected_row(), RailRow::Archive(_)) {
            self.focus = Focus::Archive;
        }
    }

    pub fn focus_canvas(&mut self) {
        self.focus = Focus::Canvas;
        self.rail_index = 0;
    }

    pub fn rail_move(&mut self, delta: isize) {
        let rows = self.rows();
        let last = rows.len().saturating_sub(1) as isize;
        let next = (self.rail_index as isize + delta).clamp(0, last) as usize;
        self.rail_index = next;
        if let RailRow::Gallery(section) = rows[next] {
            self.ensure_loaded(section);
        }
    }

    pub fn rail_select(&mut self, row: RailRow) {
        if let Some(index) = self.rows().iter().position(|candidate| *candidate == row) {
            self.rail_index = index;
            if let RailRow::Gallery(section) = row {
                self.ensure_loaded(section);
            }
        }
    }

    /// The rail's line under a screen point, from the last draw.
    pub fn rail_line_at(&self, x: u16, y: u16) -> Option<usize> {
        let area = self.rail_area.get();
        if !area.contains(ratatui::layout::Position { x, y }) {
            return None;
        }
        Some((y - area.y) as usize)
    }

    /// The rail row under a screen point, from the last draw. None while
    /// the rail shows an archive list instead of its rows.
    pub fn rail_row_at(&self, x: u16, y: u16) -> Option<RailRow> {
        if self.focus == Focus::Archive {
            return None;
        }
        let line = self.rail_line_at(x, y)?;
        super::ui::rail_row_for_line(&self.rows(), line)
    }

    /// The piece list index under a screen point, from the last draw.
    pub fn list_index_at(&self, x: u16, y: u16) -> Option<usize> {
        let area = self.list_area.get();
        if !area.contains(ratatui::layout::Position { x, y }) {
            return None;
        }
        let section = self.viewed_section()?;
        let index = self.sections[section.index()].scroll + (y - area.y) as usize;
        (index < self.sections[section.index()].pieces.len()).then_some(index)
    }

    pub fn rail_activate(&mut self) -> RailActivation {
        match self.selected_row() {
            RailRow::Board => {
                self.focus = Focus::Canvas;
                RailActivation::FocusCanvas
            }
            RailRow::Gallery(section) => {
                self.ensure_loaded(section);
                self.focus = Focus::List;
                RailActivation::OpenGallery
            }
            RailRow::Hang => RailActivation::BeginHang,
            RailRow::Archive(kind) => RailActivation::OpenArchive(kind),
        }
    }

    // ----- listings -----

    fn ensure_loaded(&mut self, section: GallerySection) {
        let state = &mut self.sections[section.index()];
        if state.loaded || state.loading {
            return;
        }
        state.loading = true;
        state.error = None;
        self.service
            .list_task(self.viewer_id, section.listing(), self.results_tx.clone());
    }

    fn reload(&mut self, section: GallerySection) {
        let state = &mut self.sections[section.index()];
        state.loaded = false;
        state.loading = false;
        self.ensure_loaded(section);
    }

    pub fn section_pieces(&self, section: GallerySection) -> &[GalleryPiece] {
        &self.sections[section.index()].pieces
    }

    pub fn section_loading(&self, section: GallerySection) -> bool {
        self.sections[section.index()].loading
    }

    pub fn section_error(&self, section: GallerySection) -> Option<&str> {
        self.sections[section.index()].error.as_deref()
    }

    pub fn section_selected(&self, section: GallerySection) -> usize {
        self.sections[section.index()].selected
    }

    pub fn section_scroll(&self, section: GallerySection) -> usize {
        self.sections[section.index()].scroll
    }

    /// The count the rail shows next to a section: the listing's length
    /// once it has loaded, the entry-time count before that.
    pub fn section_count(&self, section: GallerySection) -> Option<usize> {
        let state = &self.sections[section.index()];
        if state.loaded {
            return Some(state.pieces.len());
        }
        self.counts
            .map(|counts| counts.get(section.listing()).max(0) as usize)
    }

    pub fn selected_piece(&self) -> Option<&GalleryPiece> {
        let section = self.viewed_section()?;
        let state = &self.sections[section.index()];
        state.pieces.get(state.selected)
    }

    pub fn list_move(&mut self, delta: isize) {
        let Some(section) = self.viewed_section() else {
            return;
        };
        let visible = self.list_visible_height.get().max(1);
        let state = &mut self.sections[section.index()];
        if state.pieces.is_empty() {
            state.selected = 0;
            state.scroll = 0;
            return;
        }
        let last = state.pieces.len() as isize - 1;
        state.selected = (state.selected as isize + delta).clamp(0, last) as usize;
        if state.selected < state.scroll {
            state.scroll = state.selected;
        } else if state.selected >= state.scroll + visible {
            state.scroll = state.selected + 1 - visible;
        }
    }

    pub fn list_select(&mut self, index: usize) {
        let Some(section) = self.viewed_section() else {
            return;
        };
        let state = &mut self.sections[section.index()];
        if index < state.pieces.len() {
            state.selected = index;
        }
    }

    pub fn list_page(&mut self, pages: isize) {
        let visible = self.list_visible_height.get().max(1) as isize;
        self.list_move(pages.saturating_mul(visible));
    }

    pub fn focus_list(&mut self) {
        if self.viewed_section().is_some() {
            self.focus = Focus::List;
        }
    }

    pub fn open_selected_piece(&mut self) -> bool {
        if self.selected_piece().is_none() {
            return false;
        }
        self.focus = Focus::Piece;
        true
    }

    pub fn close_piece(&mut self) {
        if self.focus == Focus::Piece {
            self.focus = Focus::List;
        }
    }

    /// Applaud the selected piece, or take the applause back. One in
    /// flight at a time per session.
    pub fn applaud_selected(&mut self) {
        let Some(piece) = self.selected_piece() else {
            return;
        };
        if piece.user_id == self.viewer_id {
            self.notice = Some("You cannot applaud your own piece.".to_string());
            return;
        }
        if self.pending_applause.is_some() {
            return;
        }
        let piece_id = piece.id;
        self.pending_applause = Some(piece_id);
        self.service
            .applaud_task(piece_id, self.viewer_id, self.results_tx.clone());
    }

    // ----- hang flow -----

    pub fn begin_framing(&mut self) {
        self.hang = HangFlow::Framing;
        self.focus = Focus::Canvas;
        self.rail_index = 0;
        self.notice =
            Some("Frame your work: Shift+arrows or drag, Enter hangs, Esc cancels.".to_string());
    }

    pub fn is_framing(&self) -> bool {
        matches!(self.hang, HangFlow::Framing)
    }

    pub fn is_confirming(&self) -> bool {
        matches!(self.hang, HangFlow::Confirm { .. })
    }

    pub fn cancel_hang(&mut self) {
        self.hang = HangFlow::Idle;
        self.notice = None;
    }

    /// The frame passed the local rails: move on to naming it.
    pub fn set_confirm(&mut self, framed: FramedPiece) {
        self.hang = HangFlow::Confirm {
            framed: Box::new(framed),
            title: String::new(),
        };
        self.notice = None;
    }

    pub fn set_framing_notice(&mut self, notice: String) {
        self.notice = Some(notice);
    }

    pub fn title_push(&mut self, ch: char) {
        if let HangFlow::Confirm { title, .. } = &mut self.hang
            && !ch.is_control()
            && title.chars().count() < PIECE_TITLE_MAX_CHARS
        {
            title.push(ch);
        }
    }

    pub fn title_pop(&mut self) {
        if let HangFlow::Confirm { title, .. } = &mut self.hang {
            title.pop();
        }
    }

    /// Send the named frame to the database. Without a title nothing is
    /// sent; the modal says so.
    pub fn submit_hang(&mut self) {
        let HangFlow::Confirm { framed, title } = &self.hang else {
            return;
        };
        let title = title.trim().to_string();
        if title.is_empty() {
            self.notice = Some("Give it a title first.".to_string());
            return;
        }
        let framed = framed.as_ref().clone();
        self.hang = HangFlow::Submitting;
        self.notice = None;
        self.service
            .hang_task(self.viewer_id, title, framed, self.results_tx.clone());
    }

    // ----- tick -----

    /// Drain what the service sent back. Returns true when anything on
    /// screen changed.
    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        while let Ok(result) = self.results_rx.try_recv() {
            changed = true;
            match result {
                GalleryResult::Counts(counts) => {
                    self.counts = Some(counts);
                }
                GalleryResult::CountsFailed(_) => {
                    // The rail shows no number; the listing's own load
                    // reports its error where it is read.
                }
                GalleryResult::Listed { listing, pieces } => {
                    let section = GallerySection::from_listing(listing);
                    let state = &mut self.sections[section.index()];
                    state.pieces = pieces;
                    state.loaded = true;
                    state.loading = false;
                    state.error = None;
                    state.selected = state.selected.min(state.pieces.len().saturating_sub(1));
                    state.scroll = state.scroll.min(state.selected);
                }
                GalleryResult::ListFailed { listing, error } => {
                    let section = GallerySection::from_listing(listing);
                    let state = &mut self.sections[section.index()];
                    state.loading = false;
                    state.error = Some(error);
                }
                GalleryResult::Hung(piece) => {
                    self.hang = HangFlow::Idle;
                    self.notice = Some(format!(
                        "\"{}\" now hangs in the gallery. Applause decides the month.",
                        piece.title
                    ));
                    for section in [
                        GallerySection::ThisMonth,
                        GallerySection::Newest,
                        GallerySection::Mine,
                    ] {
                        let state = &mut self.sections[section.index()];
                        if state.loaded {
                            self.reload(section);
                        }
                    }
                    self.service
                        .counts_task(self.viewer_id, self.results_tx.clone());
                    self.rail_select(RailRow::Gallery(GallerySection::Mine));
                    self.focus = Focus::List;
                }
                GalleryResult::HangRefused(refusal) => {
                    self.hang = HangFlow::Idle;
                    self.notice = Some(refusal.notice().to_string());
                    if refusal == HangRefusal::Disabled {
                        self.focus = Focus::Canvas;
                        self.rail_index = 0;
                    }
                }
                GalleryResult::HangFailed(error) => {
                    self.hang = HangFlow::Idle;
                    self.notice = Some(error);
                }
                GalleryResult::Applause { piece_id, outcome } => {
                    self.pending_applause = None;
                    match outcome {
                        ApplauseOutcome::Applauded(count) => {
                            self.update_piece(piece_id, |piece| {
                                piece.applause = count;
                                piece.applauded_by_viewer = true;
                            });
                            self.notice = Some(format!("Applauded. {}.", applause_label(count)));
                        }
                        ApplauseOutcome::Withdrawn(count) => {
                            self.update_piece(piece_id, |piece| {
                                piece.applause = count;
                                piece.applauded_by_viewer = false;
                            });
                            self.notice =
                                Some(format!("Applause withdrawn. {}.", applause_label(count)));
                        }
                        ApplauseOutcome::OwnPiece => {
                            self.notice = Some("You cannot applaud your own piece.".to_string());
                        }
                        ApplauseOutcome::NotFound => {
                            for state in &mut self.sections {
                                state.pieces.retain(|piece| piece.id != piece_id);
                                state.selected =
                                    state.selected.min(state.pieces.len().saturating_sub(1));
                            }
                            if self.focus == Focus::Piece {
                                self.focus = Focus::List;
                            }
                            self.notice = Some("That piece was taken down.".to_string());
                        }
                    }
                }
                GalleryResult::ApplauseFailed { piece_id: _, error } => {
                    self.pending_applause = None;
                    self.notice = Some(error);
                }
            }
        }
        changed
    }

    fn update_piece(&mut self, piece_id: Uuid, mut apply: impl FnMut(&mut GalleryPiece)) {
        for state in &mut self.sections {
            for piece in &mut state.pieces {
                if piece.id == piece_id {
                    apply(piece);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
