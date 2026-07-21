use std::cell::Cell;
use std::time::Instant;

use ratatui::layout::Rect;

/// Max gap between two left-clicks (on the same tab) to count as a double-click.
pub(crate) const HUB_DOUBLE_CLICK_WINDOW_MS: u128 = 400;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HubTab {
    Leaderboard,
    Dailies,
    Shop,
    Events,
    Admin,
}

impl HubTab {
    pub(crate) const ALL: [Self; 5] = [
        Self::Dailies,
        Self::Shop,
        Self::Leaderboard,
        Self::Events,
        Self::Admin,
    ];
    pub(crate) const PUBLIC: [Self; 4] = [Self::Dailies, Self::Shop, Self::Leaderboard, Self::Events];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Leaderboard => "Leaderboard",
            Self::Dailies => "Quests",
            Self::Shop => "Shop",
            Self::Events => "Events",
            Self::Admin => "Admin",
        }
    }

    pub(crate) fn visible_tabs(is_admin: bool) -> &'static [Self] {
        if is_admin { &Self::ALL } else { &Self::PUBLIC }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct HubState {
    selected_tab: HubTab,
    /// Per-tab on-screen rectangles, populated by the renderer each frame.
    /// `tab_rects[i]` corresponds to `HubTab::ALL[i]`. Indexed in 0-based
    /// ratatui coords.
    tab_rects: Cell<[Rect; HubTab::ALL.len()]>,
    /// `(time, tab)` of the previous left-click on a tab, for double-click
    /// detection.
    last_click: Option<(Instant, HubTab)>,
}

impl HubState {
    pub(crate) fn new() -> Self {
        Self {
            selected_tab: HubTab::Dailies,
            tab_rects: Cell::new([Rect::new(0, 0, 0, 0); HubTab::ALL.len()]),
            last_click: None,
        }
    }

    pub(crate) fn open(&mut self, tab: HubTab) {
        self.selected_tab = tab;
    }

    pub(crate) fn selected_tab(&self) -> HubTab {
        self.selected_tab
    }

    pub(crate) fn select_next_tab(&mut self, is_admin: bool) {
        self.selected_tab = tab_at_offset(self.selected_tab, 1, is_admin);
    }

    pub(crate) fn select_previous_tab(&mut self, is_admin: bool) {
        let len = HubTab::visible_tabs(is_admin).len();
        self.selected_tab = tab_at_offset(self.selected_tab, len - 1, is_admin);
    }

    pub(crate) fn ensure_visible_tab(&mut self, is_admin: bool) {
        if !HubTab::visible_tabs(is_admin).contains(&self.selected_tab) {
            self.selected_tab = HubTab::Shop;
        }
    }

    pub(crate) fn set_tab_rects(&self, rects: [Rect; HubTab::ALL.len()]) {
        self.tab_rects.set(rects);
    }

    /// Return the tab whose tab-strip cell contains the (0-based ratatui)
    /// point, if any.
    pub(crate) fn tab_at_point(&self, x: u16, y: u16) -> Option<HubTab> {
        let rects = self.tab_rects.get();
        rects.iter().enumerate().find_map(|(idx, rect)| {
            if rect_contains(*rect, x, y) {
                Some(HubTab::ALL[idx])
            } else {
                None
            }
        })
    }

    /// Switch to the clicked tab, returning `true` if this click chained with
    /// the previous click on the same tab inside the double-click window.
    pub(crate) fn click_tab(&mut self, tab: HubTab) -> bool {
        let now = Instant::now();
        let is_double = match self.last_click {
            Some((prev_time, prev_tab)) => {
                prev_tab == tab
                    && now.duration_since(prev_time).as_millis() <= HUB_DOUBLE_CLICK_WINDOW_MS
            }
            None => false,
        };
        self.selected_tab = tab;
        self.last_click = if is_double { None } else { Some((now, tab)) };
        is_double
    }
}

impl Default for HubState {
    fn default() -> Self {
        Self::new()
    }
}

fn tab_at_offset(current: HubTab, offset: usize, is_admin: bool) -> HubTab {
    let tabs = HubTab::visible_tabs(is_admin);
    let index = tabs
        .iter()
        .position(|tab| *tab == current)
        .unwrap_or_default();
    tabs[(index + offset) % tabs.len()]
}

fn rect_contains(rect: Rect, x: u16, y: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && x >= rect.x
        && x < rect.x + rect.width
        && y >= rect.y
        && y < rect.y + rect.height
}
