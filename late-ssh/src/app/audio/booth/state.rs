use uuid::Uuid;

use crate::app::audio::svc::{HistoryItemView, QueueItemView};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum BoothFocus {
    #[default]
    Submit,
    Queue,
    History,
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

    /// The History rows visible under the current filter, in snapshot order.
    /// When the query is empty this is every row.
    pub(crate) fn filtered_history<'a>(
        &self,
        history: &'a [HistoryItemView],
    ) -> Vec<&'a HistoryItemView> {
        let query = self.history_filter_query.trim().to_lowercase();
        if query.is_empty() {
            return history.iter().collect();
        }
        history
            .iter()
            .filter(|item| history_item_matches(item, &query))
            .collect()
    }

    /// Number of History rows visible under the current filter.
    pub(crate) fn filtered_history_len(&self, history: &[HistoryItemView]) -> usize {
        if self.history_filter_query.trim().is_empty() {
            return history.len();
        }
        let query = self.history_filter_query.trim().to_lowercase();
        history
            .iter()
            .filter(|item| history_item_matches(item, &query))
            .count()
    }

    pub(crate) fn selected_history_item<'a>(
        &self,
        history: &'a [HistoryItemView],
    ) -> Option<&'a HistoryItemView> {
        self.filtered_history(history)
            .into_iter()
            .nth(self.selected_history)
    }

    pub(crate) fn selected_history_item_id(&self, history: &[HistoryItemView]) -> Option<Uuid> {
        self.selected_history_item(history).map(|item| item.id)
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

/// Case-insensitive match of a history row against an already-lowercased query,
/// checking the title, channel, and raw video id.
fn history_item_matches(item: &HistoryItemView, query: &str) -> bool {
    item.title
        .as_deref()
        .is_some_and(|title| title.to_lowercase().contains(query))
        || item
            .channel
            .as_deref()
            .is_some_and(|channel| channel.to_lowercase().contains(query))
        || item.video_id.to_lowercase().contains(query)
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
