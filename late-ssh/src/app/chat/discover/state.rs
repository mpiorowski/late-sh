use crate::app::chat::svc::DiscoverRoomItem;

pub struct State {
    items: Vec<DiscoverRoomItem>,
    /// Index into the *filtered* (visible) list, not the full `items`.
    selected: usize,
    loading: bool,
    /// Live substring filter applied to room slugs. Empty means "show all".
    query: String,
    /// Whether the footer filter input is capturing keystrokes.
    filtering: bool,
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

impl State {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            selected: 0,
            loading: false,
            query: String::new(),
            filtering: false,
        }
    }

    pub fn start_loading(&mut self) {
        self.items.clear();
        self.selected = 0;
        self.loading = true;
    }

    pub fn set_items(&mut self, items: Vec<DiscoverRoomItem>) {
        self.items = items;
        self.selected = clamp_index(self.selected, self.visible_len());
        self.loading = false;
    }

    pub fn finish_loading(&mut self) {
        self.loading = false;
    }

    /// Rooms matching the current filter, in list order. When the query is
    /// empty this is every room.
    pub fn visible_items(&self) -> Vec<&DiscoverRoomItem> {
        let query = self.query.trim().to_lowercase();
        if query.is_empty() {
            return self.items.iter().collect();
        }
        self.items
            .iter()
            .filter(|item| item.slug.to_lowercase().contains(&query))
            .collect()
    }

    fn visible_len(&self) -> usize {
        self.visible_items().len()
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn is_filtering(&self) -> bool {
        self.filtering
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn selected_index(&self) -> usize {
        clamp_index(self.selected, self.visible_len())
    }

    pub fn move_selection(&mut self, delta: isize) {
        self.selected = move_index(self.selected_index(), delta, self.visible_len());
    }

    pub fn selected_item(&self) -> Option<&DiscoverRoomItem> {
        self.visible_items().into_iter().nth(self.selected_index())
    }

    /// Enter filter mode; keystrokes now edit the query.
    pub fn start_filter(&mut self) {
        self.filtering = true;
    }

    /// Leave filter mode and clear the query back to the full list.
    pub fn cancel_filter(&mut self) {
        self.filtering = false;
        self.query.clear();
        self.selected = 0;
    }

    pub fn push_char(&mut self, ch: char) {
        if !ch.is_control() {
            self.query.push(ch);
            self.selected = 0;
        }
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.selected = 0;
    }

    pub fn clear_query(&mut self) {
        self.query.clear();
        self.selected = 0;
    }
}

fn clamp_index(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { index.min(len - 1) }
}

fn move_index(current: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (current as isize + delta).clamp(0, len as isize - 1) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn item(slug: &str) -> DiscoverRoomItem {
        DiscoverRoomItem {
            room_id: Uuid::from_u128(1),
            slug: slug.to_string(),
            member_count: 1,
            message_count: 0,
            last_message_at: Some(Utc::now()),
            recent: Vec::new(),
        }
    }

    #[test]
    fn start_loading_clears_empty_state_until_items_arrive() {
        let mut state = State::new();

        state.start_loading();

        assert!(state.is_loading());
        assert!(state.visible_items().is_empty());
    }

    #[test]
    fn set_items_marks_loading_complete() {
        let mut state = State::new();
        state.start_loading();

        state.set_items(Vec::new());

        assert!(!state.is_loading());
        assert!(state.visible_items().is_empty());
    }

    #[test]
    fn filter_narrows_visible_items_case_insensitively() {
        let mut state = State::new();
        state.set_items(vec![item("rust"), item("Python"), item("rust-gamedev")]);

        state.start_filter();
        for ch in "RUST".chars() {
            state.push_char(ch);
        }

        let visible: Vec<_> = state
            .visible_items()
            .iter()
            .map(|i| i.slug.clone())
            .collect();
        assert_eq!(visible, vec!["rust", "rust-gamedev"]);
    }

    #[test]
    fn selection_tracks_filtered_list() {
        let mut state = State::new();
        state.set_items(vec![item("alpha"), item("beta"), item("betamax")]);

        state.start_filter();
        for ch in "beta".chars() {
            state.push_char(ch);
        }
        // Query reset selection to the top of the filtered list.
        assert_eq!(state.selected_index(), 0);
        assert_eq!(
            state.selected_item().map(|i| i.slug.clone()),
            Some("beta".into())
        );

        state.move_selection(1);
        assert_eq!(
            state.selected_item().map(|i| i.slug.clone()),
            Some("betamax".into())
        );
        // Cannot move past the end of the filtered list.
        state.move_selection(1);
        assert_eq!(
            state.selected_item().map(|i| i.slug.clone()),
            Some("betamax".into())
        );
    }

    #[test]
    fn cancel_filter_restores_full_list() {
        let mut state = State::new();
        state.set_items(vec![item("alpha"), item("beta")]);

        state.start_filter();
        state.push_char('z');
        assert!(state.visible_items().is_empty());

        state.cancel_filter();
        assert!(!state.is_filtering());
        assert_eq!(state.visible_items().len(), 2);
    }
}
