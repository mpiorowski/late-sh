use crate::app::chat::svc::DiscoverRoomItem;

pub struct State {
    items: Vec<DiscoverRoomItem>,
    selected: usize,
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
        }
    }

    pub fn set_items(&mut self, items: Vec<DiscoverRoomItem>) {
        self.items = items;
        self.selected = clamp_index(self.selected, self.items.len());
    }

    pub fn all_items(&self) -> &[DiscoverRoomItem] {
        &self.items
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
