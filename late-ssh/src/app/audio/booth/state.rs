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
mod tests {
    use super::*;

    fn item(video_id: &str, title: Option<&str>, channel: Option<&str>) -> HistoryItemView {
        HistoryItemView {
            id: Uuid::nil(),
            video_id: video_id.to_string(),
            title: title.map(str::to_string),
            channel: channel.map(str::to_string),
            duration_ms: None,
            is_stream: false,
            play_count: 0,
            last_played_at_ms: 0,
            vote_score: 0,
        }
    }

    fn history() -> Vec<HistoryItemView> {
        vec![
            item("aaa", Some("Lofi Beats"), Some("ChillHop")),
            item("bbb", Some("Jazz Night"), Some("BlueNote")),
            item("ccc", None, Some("Lofi Radio")),
            item("ddd", Some("Synthwave"), None),
        ]
    }

    #[test]
    fn empty_query_matches_every_row() {
        let state = BoothModalState::default();
        let history = history();
        assert_eq!(state.filtered_history(&history).len(), history.len());
        assert_eq!(state.filtered_history_len(&history), history.len());
    }

    #[test]
    fn filter_matches_title_channel_and_video_id_case_insensitively() {
        let mut state = BoothModalState::default();
        state.enter_history_filter();
        for ch in "lofi".chars() {
            state.push_history_filter(ch);
        }
        let history = history();
        let filtered = state.filtered_history(&history);
        // "Lofi Beats" (title) and "Lofi Radio" (channel) both match.
        assert_eq!(filtered.len(), 2);
        assert_eq!(state.filtered_history_len(&history), 2);
        assert_eq!(filtered[0].video_id, "aaa");
        assert_eq!(filtered[1].video_id, "ccc");

        state.clear_history_filter_query();
        for ch in "DDD".chars() {
            state.push_history_filter(ch);
        }
        let filtered = state.filtered_history(&history);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].video_id, "ddd");
    }

    #[test]
    fn selected_history_item_indexes_into_filtered_list() {
        let mut state = BoothModalState::default();
        state.set_focus(BoothFocus::History);
        state.enter_history_filter();
        for ch in "lofi".chars() {
            state.push_history_filter(ch);
        }
        let history = history();
        // Move to the second filtered row.
        state.move_selection(1, state.filtered_history_len(&history));
        assert_eq!(state.selected_history_item_id(&history), Some(Uuid::nil()));
        let item = state.selected_history_item(&history).unwrap();
        assert_eq!(item.video_id, "ccc");
    }

    #[test]
    fn editing_query_resets_selection_and_cancel_clears_query() {
        let mut state = BoothModalState::default();
        state.enter_history_filter();
        state.selected_history = 3;
        state.push_history_filter('a');
        assert_eq!(state.selected_history, 0);
        assert!(state.history_filter_engaged());

        state.cancel_history_filter();
        assert!(!state.history_filter_active());
        assert!(!state.history_filter_engaged());
        assert_eq!(state.history_filter_query(), "");
    }

    #[test]
    fn filter_query_is_length_capped() {
        let mut state = BoothModalState::default();
        state.enter_history_filter();
        for _ in 0..(HISTORY_FILTER_MAX_LEN + 10) {
            state.push_history_filter('x');
        }
        assert_eq!(
            state.history_filter_query().chars().count(),
            HISTORY_FILTER_MAX_LEN
        );
    }
}
