use std::cell::Cell;
use std::collections::HashMap;

use chrono::{DateTime, Utc};
use late_core::models::chat_message::{ChatMessage, HistoryDirection};
use uuid::Uuid;

/// What the modal has to show right now. Loading and the two settled
/// failures are distinct states because they read differently to the user:
/// a spinner that never resolves is worse than "that message is gone".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HistoryStatus {
    Loading,
    Ready,
    /// The anchor was hard-deleted, or sits in a room this viewer may not
    /// read. Settled: retrying cannot help.
    AnchorMissing,
    /// The opening load failed. Transient, so the modal says so rather than
    /// claiming the room is empty.
    Failed,
}

/// Where the next rendered frame should park the viewport. An open (or an
/// End press) knows where it wants to land, the tail or the anchor, but not
/// how many wrapped lines the pane fits, so the parking is deferred to the
/// renderer's `resolve_viewport`, the first place both are known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Park {
    Bottom,
    Anchor,
}

/// Scroll-driven room history: a window over
/// `ChatMessage::list_page_for_viewer`, opened either at a room's tail
/// (`/history`) or centered on one message that is older than the live room
/// tail can reach (a search hit or a mention).
///
/// The modal keeps its own `usernames` rather than borrowing `ChatState`'s.
/// A page walked far enough back turns up authors the live session never
/// loaded, and rendering those as `?` would be a visible hole.
///
/// Nothing is ever evicted from `messages`: scrolling back and forth within
/// one sitting refetches nothing, and the run stays contiguous so the two
/// cursors are always just its ends.
pub(crate) struct ChatHistoryModalState {
    open: bool,
    room_id: Option<Uuid>,
    room_label: String,
    /// Chronological, oldest first.
    messages: Vec<ChatMessage>,
    usernames: HashMap<Uuid, String>,
    /// The message the modal was opened on, drawn highlighted. `None` when
    /// opened at the tail, where there is nothing to point at.
    anchor_id: Option<Uuid>,
    status: HistoryStatus,
    /// Index into `messages` of the first message drawn. Scrolling is tracked
    /// in messages rather than rendered lines so that splicing a page onto
    /// the top can hold the viewport still with an exact `+= len`; a line
    /// offset would drift every time a body wrapped to a different height.
    /// Interior-mutable because the renderer settles pending parks and
    /// aligns the index with the back-filled row it actually drew first,
    /// both from `&self`.
    scroll_index: Cell<usize>,
    /// A parking request for the next rendered frame; see [`Park`].
    park: Cell<Option<Park>>,
    /// The viewer, for the unread divider's "authored by someone else".
    viewer_id: Option<Uuid>,
    /// Draw the `new messages` divider before the first loaded message
    /// authored by someone else and created after this instant. Captured
    /// from the session's own unread marker at open, because the server-side
    /// read cursor has usually already advanced by then (opening the room is
    /// what marks it read). `None` (no marker, or a never-read room, mirroring
    /// the live tail's divider) means no divider.
    unread_cutoff: Option<DateTime<Utc>>,
    /// Request id of the in-flight page for each edge, at most one apiece, so
    /// holding a scroll key queues one fetch instead of one per key repeat.
    pending_older: Option<Uuid>,
    pending_newer: Option<Uuid>,
    /// Request id of the in-flight opening load.
    pending_open: Option<Uuid>,
    /// An empty page came back, so that end of the room is reached. Set from
    /// an empty page only, never a short one: page filters (ignored users,
    /// system lines) can shorten a page that still has more behind it.
    exhausted_older: bool,
    exhausted_newer: bool,
    /// Messages the last frame fully fit, recorded by the renderer: bodies
    /// wrap to a variable number of terminal rows, so only a rendered frame
    /// knows how many messages make a screenful. Paging keys, the bottom-edge
    /// test, and the scroll clamp all count in messages through this.
    /// Interior-mutable because rendering takes `&self`; the conservative
    /// default of 1 is deliberate (see the renderer's `draw_messages`).
    visible_rows: Cell<usize>,
}

impl Default for ChatHistoryModalState {
    fn default() -> Self {
        Self {
            open: false,
            room_id: None,
            room_label: String::new(),
            messages: Vec::new(),
            usernames: HashMap::new(),
            anchor_id: None,
            status: HistoryStatus::Loading,
            scroll_index: Cell::new(0),
            park: Cell::new(None),
            viewer_id: None,
            unread_cutoff: None,
            pending_older: None,
            pending_newer: None,
            pending_open: None,
            exhausted_older: false,
            exhausted_newer: false,
            visible_rows: Cell::new(1),
        }
    }
}

impl ChatHistoryModalState {
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn room_id(&self) -> Option<Uuid> {
        self.room_id
    }

    pub(crate) fn room_label(&self) -> &str {
        &self.room_label
    }

    pub(crate) fn status(&self) -> HistoryStatus {
        self.status
    }

    pub(crate) fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub(crate) fn usernames(&self) -> &HashMap<Uuid, String> {
        &self.usernames
    }

    pub(crate) fn anchor_id(&self) -> Option<Uuid> {
        self.anchor_id
    }

    pub(crate) fn scroll_index(&self) -> usize {
        self.scroll_index.get()
    }

    /// Consume the pending parking request. Renderer only; a taken park is
    /// settled by writing the landing spot back through `sync_scroll_index`.
    pub(crate) fn take_park(&self) -> Option<Park> {
        self.park.take()
    }

    /// Align the scroll index with the first row the frame actually drew.
    /// Renderer only: this is how a park lands and how a back-filled tail
    /// keeps the next keypress moving from what the user sees.
    pub(crate) fn sync_scroll_index(&self, index: usize) {
        self.scroll_index.set(index);
    }

    pub(crate) fn set_visible_rows(&self, rows: usize) {
        self.visible_rows.set(rows.max(1));
    }

    /// Open at the room's newest messages. Nothing is newer than the tail, so
    /// that edge starts exhausted and never asks for a page.
    pub(crate) fn open_at_tail(
        &mut self,
        room_id: Uuid,
        room_label: String,
        request_id: Uuid,
        viewer_id: Uuid,
        unread_cutoff: Option<DateTime<Utc>>,
    ) {
        *self = Self {
            open: true,
            room_id: Some(room_id),
            room_label,
            status: HistoryStatus::Loading,
            pending_open: Some(request_id),
            exhausted_newer: true,
            viewer_id: Some(viewer_id),
            unread_cutoff,
            ..Self::default()
        };
    }

    /// Open centered on one message, which will be drawn highlighted once the
    /// opening page resolves.
    pub(crate) fn open_at_message(
        &mut self,
        room_id: Uuid,
        room_label: String,
        anchor_id: Uuid,
        request_id: Uuid,
        viewer_id: Uuid,
        unread_cutoff: Option<DateTime<Utc>>,
    ) {
        *self = Self {
            open: true,
            room_id: Some(room_id),
            room_label,
            anchor_id: Some(anchor_id),
            status: HistoryStatus::Loading,
            pending_open: Some(request_id),
            viewer_id: Some(viewer_id),
            unread_cutoff,
            ..Self::default()
        };
    }

    /// Open at the room's first unread message when the service can find
    /// one, else at the tail. Which of the two happened arrives with the
    /// answer: an anchor load (`apply_anchor`) centers and highlights the
    /// first unread, a plain page (`apply_page`) is the tail fallback. The
    /// newer edge starts unexhausted because an anchor landing mid-history
    /// must be able to page forward; for the tail fallback the first
    /// newer-edge probe comes back empty and settles the flag.
    pub(crate) fn open_at_unread(
        &mut self,
        room_id: Uuid,
        room_label: String,
        request_id: Uuid,
        viewer_id: Uuid,
        unread_cutoff: Option<DateTime<Utc>>,
    ) {
        *self = Self {
            open: true,
            room_id: Some(room_id),
            room_label,
            status: HistoryStatus::Loading,
            pending_open: Some(request_id),
            viewer_id: Some(viewer_id),
            unread_cutoff,
            ..Self::default()
        };
    }

    /// The message the `new messages` divider is drawn before: the first
    /// loaded message authored by someone else past the unread cutoff. The
    /// target can shift as older pages splice in; it converges on the true
    /// first unread once the run reaches back past the cutoff.
    pub(crate) fn unread_divider_target(&self) -> Option<Uuid> {
        let cutoff = self.unread_cutoff?;
        let viewer = self.viewer_id?;
        self.messages
            .iter()
            .find(|m| m.created > cutoff && m.user_id != viewer)
            .map(|m| m.id)
    }

    pub(crate) fn close(&mut self) {
        *self = Self::default();
    }

    /// The `(created, id)` cursor for the older edge, `None` while empty.
    pub(crate) fn older_cursor(&self) -> Option<(DateTime<Utc>, Uuid)> {
        self.messages.first().map(|m| (m.created, m.id))
    }

    /// The `(created, id)` cursor for the newer edge, `None` while empty.
    pub(crate) fn newer_cursor(&self) -> Option<(DateTime<Utc>, Uuid)> {
        self.messages.last().map(|m| (m.created, m.id))
    }

    /// Whether the viewport has reached an edge that still has messages
    /// behind it and no fetch already in flight. Checked after every scroll;
    /// the caller turns a `true` into a request.
    pub(crate) fn wants_page(&self, direction: HistoryDirection) -> bool {
        if !self.open || self.status != HistoryStatus::Ready {
            return false;
        }
        match direction {
            HistoryDirection::Older => {
                self.scroll_index.get() == 0
                    && !self.exhausted_older
                    && self.pending_older.is_none()
            }
            HistoryDirection::Newer => {
                self.at_bottom() && !self.exhausted_newer && self.pending_newer.is_none()
            }
        }
    }

    fn at_bottom(&self) -> bool {
        self.scroll_index.get() + self.visible_rows.get() >= self.messages.len()
    }

    pub(crate) fn begin_page(&mut self, direction: HistoryDirection, request_id: Uuid) {
        match direction {
            HistoryDirection::Older => self.pending_older = Some(request_id),
            HistoryDirection::Newer => self.pending_newer = Some(request_id),
        }
    }

    /// Move the viewport by whole messages, clamped so the pane cannot
    /// scroll past its last screenful of the loaded run. A manual scroll
    /// overrides any parking still waiting on a frame.
    pub(crate) fn scroll(&mut self, delta: i32) {
        self.park.set(None);
        let max = self.messages.len().saturating_sub(self.visible_rows.get());
        let next = self.scroll_index.get() as i32 + delta;
        self.scroll_index.set(next.clamp(0, max as i32) as usize);
    }

    pub(crate) fn scroll_page(&mut self, pages: i32) {
        let rows = self.visible_rows.get().max(1) as i32;
        self.scroll(pages * rows);
    }

    /// Take a loaded page. A tail open and an older-edge scroll arrive as the
    /// same event, so which one this is comes from the request id, not the
    /// direction: the opening page installs the run, later pages splice onto
    /// it. Stale responses (a request id outstanding for neither) are
    /// dropped, so a page arriving after the modal was reopened cannot bleed
    /// into the new room.
    pub(crate) fn apply_page(
        &mut self,
        request_id: Uuid,
        direction: HistoryDirection,
        messages: Vec<ChatMessage>,
        usernames: HashMap<Uuid, String>,
    ) {
        if self.pending_open == Some(request_id) {
            self.apply_open_page(request_id, messages, usernames);
            return;
        }
        match direction {
            HistoryDirection::Older => {
                if self.pending_older != Some(request_id) {
                    return;
                }
                self.pending_older = None;
                if messages.is_empty() {
                    self.exhausted_older = true;
                    return;
                }
                // Hold the viewport on the message the user was looking at:
                // everything spliced above it shifts its index by the page
                // length, so the same row stays under their eyes.
                let added = messages.len();
                let mut merged = messages;
                merged.append(&mut self.messages);
                self.messages = merged;
                self.scroll_index.set(self.scroll_index.get() + added);
            }
            HistoryDirection::Newer => {
                if self.pending_newer != Some(request_id) {
                    return;
                }
                self.pending_newer = None;
                if messages.is_empty() {
                    self.exhausted_newer = true;
                    return;
                }
                self.messages.extend(messages);
            }
        }
        self.usernames.extend(usernames);
    }

    /// Install the opening window and ask the next frame to park the
    /// viewport with the anchor mid-pane: centering needs wrapped line
    /// heights, which only the renderer knows.
    pub(crate) fn apply_anchor(
        &mut self,
        request_id: Uuid,
        anchor_id: Uuid,
        messages: Vec<ChatMessage>,
        usernames: HashMap<Uuid, String>,
    ) {
        if self.pending_open != Some(request_id) {
            return;
        }
        self.pending_open = None;
        self.status = HistoryStatus::Ready;
        self.anchor_id = Some(anchor_id);
        self.usernames = usernames;
        self.messages = messages;
        self.park.set(Some(Park::Anchor));
    }

    /// Install the opening page for a tail open, parked at the newest
    /// message.
    fn apply_open_page(
        &mut self,
        request_id: Uuid,
        messages: Vec<ChatMessage>,
        usernames: HashMap<Uuid, String>,
    ) {
        if self.pending_open != Some(request_id) {
            return;
        }
        self.pending_open = None;
        self.status = HistoryStatus::Ready;
        if messages.is_empty() {
            self.exhausted_older = true;
        }
        self.usernames = usernames;
        self.messages = messages;
        self.scroll_to_bottom();
    }

    /// Park at the newest loaded message. Deferred to the next frame like an
    /// open: how many messages a bottom-anchored pane holds depends on their
    /// wrapped heights.
    pub(crate) fn scroll_to_bottom(&mut self) {
        self.park.set(Some(Park::Bottom));
    }

    pub(crate) fn apply_anchor_missing(&mut self, request_id: Uuid) {
        if self.pending_open != Some(request_id) {
            return;
        }
        self.pending_open = None;
        self.status = HistoryStatus::AnchorMissing;
    }

    /// A page request failed. An opening failure is a visible dead end; an
    /// edge failure just clears the slot so the next scroll can retry.
    pub(crate) fn apply_failed(&mut self, request_id: Uuid, direction: HistoryDirection) {
        if self.pending_open == Some(request_id) {
            self.pending_open = None;
            self.status = HistoryStatus::Failed;
            return;
        }
        match direction {
            HistoryDirection::Older if self.pending_older == Some(request_id) => {
                self.pending_older = None;
            }
            HistoryDirection::Newer if self.pending_newer == Some(request_id) => {
                self.pending_newer = None;
            }
            HistoryDirection::Older | HistoryDirection::Newer => {}
        }
    }

    /// Whether a fetch is in flight, so the footer can say so instead of the
    /// view looking stuck at an edge.
    pub(crate) fn is_fetching(&self) -> bool {
        self.pending_older.is_some() || self.pending_newer.is_some()
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
