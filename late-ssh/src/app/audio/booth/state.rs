use uuid::Uuid;

use crate::app::audio::svc::{FavoriteItemView, HistoryItemView, QueueItemView};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum BoothFocus {
    #[default]
    Submit,
    Queue,
    History,
}

/// One row of the History list: the listener's favorites on top, then the
/// community History without the tracks already shown as favorites.
#[derive(Clone, Copy, Debug)]
pub(crate) enum HistoryRow<'a> {
    Favorite {
        favorite: &'a FavoriteItemView,
        /// The same track's History row, while it is still in the 200.
        played: Option<&'a HistoryItemView>,
    },
    Played(&'a HistoryItemView),
}

impl<'a> HistoryRow<'a> {
    pub(crate) fn video_id(&self) -> &'a str {
        match self {
            HistoryRow::Favorite { favorite, .. } => &favorite.video_id,
            HistoryRow::Played(item) => &item.video_id,
        }
    }

    pub(crate) fn title(&self) -> Option<&'a str> {
        match self {
            HistoryRow::Favorite { favorite, .. } => favorite.title.as_deref(),
            HistoryRow::Played(item) => item.title.as_deref(),
        }
    }

    pub(crate) fn channel(&self) -> Option<&'a str> {
        match self {
            HistoryRow::Favorite { favorite, .. } => favorite.channel.as_deref(),
            HistoryRow::Played(item) => item.channel.as_deref(),
        }
    }

    pub(crate) fn duration_ms(&self) -> Option<i32> {
        match self {
            HistoryRow::Favorite { favorite, .. } => favorite.duration_ms,
            HistoryRow::Played(item) => item.duration_ms,
        }
    }

    pub(crate) fn is_stream(&self) -> bool {
        match self {
            HistoryRow::Favorite { favorite, .. } => favorite.is_stream,
            HistoryRow::Played(item) => item.is_stream,
        }
    }

    /// How often the room has played it; unknown for a favorite that fell
    /// out of History.
    pub(crate) fn play_count(&self) -> Option<i32> {
        match self {
            HistoryRow::Favorite { played, .. } => played.map(|item| item.play_count),
            HistoryRow::Played(item) => Some(item.play_count),
        }
    }
}

/// Upper bound on the history filter query, mirroring the rooms search cap.
const HISTORY_FILTER_MAX_LEN: usize = 32;

#[derive(Clone, Debug, Default)]
pub(crate) struct BoothModalState {
    open: bool,
    submit_input: String,
    selected_queue: usize,
    selected_history: usize,
    focus: BoothFocus,
    /// True while the History list `/` filter input is capturing keystrokes.
    history_filter_active: bool,
    /// Case-insensitive substring applied to the History list. Kept even when
    /// the input is inactive so the list stays filtered after Enter.
    history_filter_query: String,
}

impl BoothModalState {
    pub(crate) fn open(&mut self, submit_enabled: bool) {
        self.open = true;
        self.submit_input.clear();
        self.selected_queue = 0;
        self.selected_history = 0;
        self.history_filter_active = false;
        self.history_filter_query.clear();
        self.focus = if submit_enabled {
            BoothFocus::Submit
        } else {
            BoothFocus::Queue
        };
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.submit_input.clear();
        self.selected_queue = 0;
        self.selected_history = 0;
        self.history_filter_active = false;
        self.history_filter_query.clear();
        self.focus = BoothFocus::Submit;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn submit_input(&self) -> &str {
        &self.submit_input
    }

    pub(crate) fn focus(&self) -> BoothFocus {
        self.focus
    }

    pub(crate) fn selected(&self) -> usize {
        match self.focus {
            BoothFocus::Submit | BoothFocus::Queue => self.selected_queue,
            BoothFocus::History => self.selected_history,
        }
    }

    pub(crate) fn selected_queue(&self) -> usize {
        self.selected_queue
    }

    pub(crate) fn selected_history(&self) -> usize {
        self.selected_history
    }

    pub(crate) fn push(&mut self, ch: char) {
        if !ch.is_control() {
            self.submit_input.push(ch);
        }
    }

    pub(crate) fn backspace(&mut self) {
        self.submit_input.pop();
    }

    pub(crate) fn take_input(&mut self) -> String {
        std::mem::take(&mut self.submit_input)
    }

    pub(crate) fn move_selection(&mut self, delta: isize, len: usize) {
        if len == 0 {
            self.set_selected_for_focus(0);
            return;
        }
        let current = self.selected();
        let next = (current as isize + delta).rem_euclid(len as isize) as usize;
        self.set_selected_for_focus(next);
    }

    pub(crate) fn clamp(&mut self, queue_len: usize, history_len: usize) {
        if queue_len == 0 {
            self.selected_queue = 0;
        } else {
            self.selected_queue = self.selected_queue.min(queue_len - 1);
        }
        if history_len == 0 {
            self.selected_history = 0;
        } else {
            self.selected_history = self.selected_history.min(history_len - 1);
        }
    }

    pub(crate) fn cycle_focus(&mut self, submit_enabled: bool) {
        self.focus = match self.focus {
            BoothFocus::Submit => BoothFocus::Queue,
            BoothFocus::Queue => BoothFocus::History,
            BoothFocus::History if submit_enabled => BoothFocus::Submit,
            BoothFocus::History => BoothFocus::Queue,
        };
    }

    pub(crate) fn set_focus(&mut self, focus: BoothFocus) {
        self.focus = focus;
    }

    pub(crate) fn selected_item<'a>(
        &self,
        queue: &'a [QueueItemView],
    ) -> Option<&'a QueueItemView> {
        queue.get(self.selected_queue)
    }

    pub(crate) fn selected_item_id(&self, queue: &[QueueItemView]) -> Option<Uuid> {
        self.selected_item(queue).map(|item| item.id)
    }

    /// The History list rows visible under the current filter: favorites
    /// newest first, then History in snapshot order without the tracks
    /// already shown as favorites. When the query is empty this is every row.
    pub(crate) fn history_rows<'a>(
        &self,
        favorites: &'a [FavoriteItemView],
        history: &'a [HistoryItemView],
    ) -> Vec<HistoryRow<'a>> {
        let query = self.history_filter_query.trim().to_lowercase();
        let favorite_rows = favorites.iter().map(|favorite| HistoryRow::Favorite {
            favorite,
            played: history
                .iter()
                .find(|item| item.video_id == favorite.video_id),
        });
        let played_rows = history
            .iter()
            .filter(|item| {
                !favorites
                    .iter()
                    .any(|favorite| favorite.video_id == item.video_id)
            })
            .map(HistoryRow::Played);
        favorite_rows
            .chain(played_rows)
            .filter(|row| query.is_empty() || history_row_matches(row, &query))
            .collect()
    }

    pub(crate) fn selected_history_row<'a>(
        &self,
        favorites: &'a [FavoriteItemView],
        history: &'a [HistoryItemView],
    ) -> Option<HistoryRow<'a>> {
        self.history_rows(favorites, history)
            .into_iter()
            .nth(self.selected_history)
    }

    /// Put the History cursor on a track, by video id, wherever the list
    /// shows it now. A track the list no longer shows leaves the cursor
    /// where it is.
    pub(crate) fn select_history_track(
        &mut self,
        favorites: &[FavoriteItemView],
        history: &[HistoryItemView],
        video_id: &str,
    ) {
        if let Some(index) = self
            .history_rows(favorites, history)
            .iter()
            .position(|row| row.video_id() == video_id)
        {
            self.selected_history = index;
        }
    }

    pub(crate) fn history_filter_active(&self) -> bool {
        self.history_filter_active
    }

    pub(crate) fn history_filter_query(&self) -> &str {
        &self.history_filter_query
    }

    /// True when a filter query is set, whether or not the input is focused.
    pub(crate) fn history_filter_engaged(&self) -> bool {
        !self.history_filter_query.trim().is_empty()
    }

    pub(crate) fn enter_history_filter(&mut self) {
        self.history_filter_active = true;
    }

    /// Deactivate the input but keep the query, so the list stays filtered.
    pub(crate) fn apply_history_filter(&mut self) {
        self.history_filter_active = false;
    }

    /// Deactivate the input and drop the query, restoring the full list.
    pub(crate) fn cancel_history_filter(&mut self) {
        self.history_filter_active = false;
        self.history_filter_query.clear();
        self.selected_history = 0;
    }

    pub(crate) fn push_history_filter(&mut self, ch: char) {
        if ch.is_control() || self.history_filter_query.chars().count() >= HISTORY_FILTER_MAX_LEN {
            return;
        }
        self.history_filter_query.push(ch);
        self.selected_history = 0;
    }

    pub(crate) fn backspace_history_filter(&mut self) {
        self.history_filter_query.pop();
        self.selected_history = 0;
    }

    pub(crate) fn clear_history_filter_query(&mut self) {
        self.history_filter_query.clear();
        self.selected_history = 0;
    }

    fn set_selected_for_focus(&mut self, selected: usize) {
        match self.focus {
            BoothFocus::Submit | BoothFocus::Queue => self.selected_queue = selected,
            BoothFocus::History => self.selected_history = selected,
        }
    }
}

/// Case-insensitive match of a History list row against an already-lowercased
/// query, checking the title, channel, and raw video id.
fn history_row_matches(row: &HistoryRow<'_>, query: &str) -> bool {
    row.title()
        .is_some_and(|title| title.to_lowercase().contains(query))
        || row
            .channel()
            .is_some_and(|channel| channel.to_lowercase().contains(query))
        || row.video_id().to_lowercase().contains(query)
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
