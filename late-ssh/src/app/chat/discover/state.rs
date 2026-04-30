use crate::app::chat::svc::DiscoverRoomItem;

pub struct State {
    items: Vec<DiscoverRoomItem>,
    selected: usize,
    loading: bool,
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
        }
    }

    pub fn start_loading(&mut self) {
        self.items.clear();
        self.selected = 0;
        self.loading = true;
    }

    pub fn set_items(&mut self, items: Vec<DiscoverRoomItem>) {
        self.items = items;
        self.selected = clamp_index(self.selected, self.items.len());
        self.loading = false;
    }

    pub fn finish_loading(&mut self) {
        self.loading = false;
    }

    pub fn all_items(&self) -> &[DiscoverRoomItem] {
        &self.items
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn selected_index(&self) -> usize {
        clamp_index(self.selected, self.items.len())
    }

    pub fn move_selection(&mut self, delta: isize) {
        self.selected = move_index(self.selected_index(), delta, self.items.len());
    }

    pub fn selected_item(&self) -> Option<&DiscoverRoomItem> {
        self.items.get(self.selected_index())
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

    #[test]
    fn start_loading_clears_empty_state_until_items_arrive() {
        let mut state = State::new();

        state.start_loading();

        assert!(state.is_loading());
        assert!(state.all_items().is_empty());
    }

    #[test]
    fn set_items_marks_loading_complete() {
        let mut state = State::new();
        state.start_loading();

        state.set_items(Vec::new());

        assert!(!state.is_loading());
        assert!(state.all_items().is_empty());
    }
}
