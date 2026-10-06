use crate::app::chat::svc::DiscoverRoomItem;

/// How the room list is ordered. Activity is what the query already returns and
/// stays the default: it surfaces rooms worth walking into today. Members
/// answers the other question people ask of a directory, which is where
/// everyone actually is (user feedback).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortMode {
    #[default]
    Activity,
    Members,
}

impl SortMode {
    pub fn label(self) -> &'static str {
        match self {
            SortMode::Activity => "activity",
            SortMode::Members => "members",
        }
    }

    fn next(self) -> Self {
        match self {
            SortMode::Activity => SortMode::Members,
            SortMode::Members => SortMode::Activity,
        }
    }
}

pub struct State {
    items: Vec<DiscoverRoomItem>,
    /// Index into the *filtered* (visible) list, not the full `items`.
    selected: usize,
    loading: bool,
    /// Live substring filter applied to room slugs. Empty means "show all".
    query: String,
    /// Whether the footer filter input is capturing keystrokes.
    filtering: bool,
    sort: SortMode,
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
            sort: SortMode::default(),
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

    /// Rooms matching the current filter, in the current sort order. When the
    /// query is empty this is every room.
    pub fn visible_items(&self) -> Vec<&DiscoverRoomItem> {
        let query = self.query.trim().to_lowercase();
        let mut items: Vec<&DiscoverRoomItem> = match query.is_empty() {
            true => self.items.iter().collect(),
            false => self
                .items
                .iter()
                .filter(|item| item.slug.to_lowercase().contains(&query))
                .collect(),
        };
        // Activity order is the query's own, so only the members sort reorders
        // anything here. Slug breaks ties so equal-sized rooms hold a stable
        // position instead of shuffling between frames.
        if self.sort == SortMode::Members {
            items.sort_by(|a, b| {
                b.member_count
                    .cmp(&a.member_count)
                    .then_with(|| a.slug.cmp(&b.slug))
            });
        }
        items
    }

    pub fn sort(&self) -> SortMode {
        self.sort
    }

    /// Flip the ordering. The selection resets to the top: after a re-sort the
    /// old index points at an unrelated room, and silently landing the cursor
    /// somewhere new is worse than starting from the front.
    pub fn cycle_sort(&mut self) {
        self.sort = self.sort.next();
        self.selected = 0;
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
