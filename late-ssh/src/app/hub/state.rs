use std::cell::Cell;
use std::time::Instant;

use ratatui::layout::Rect;

/// Max gap between two left-clicks (on the same tab) to count as a double-click.
pub const HUB_DOUBLE_CLICK_WINDOW_MS: u128 = 400;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HubTab {
    Leaderboard,
    Dailies,
    Shop,
    Events,
    Admin,
}

impl HubTab {
    pub const ALL: [Self; 5] = [
        Self::Shop,
        Self::Leaderboard,
        Self::Dailies,
        Self::Events,
        Self::Admin,
    ];
    pub const PUBLIC: [Self; 4] = [Self::Shop, Self::Leaderboard, Self::Dailies, Self::Events];

    pub fn label(self) -> &'static str {
        match self {
            Self::Leaderboard => "Leaderboard",
            Self::Dailies => "Quests",
            Self::Shop => "Shop",
            Self::Events => "Events",
            Self::Admin => "Admin",
        }
    }

    pub fn visible_tabs(is_admin: bool) -> &'static [Self] {
        if is_admin { &Self::ALL } else { &Self::PUBLIC }
    }
}

#[derive(Clone, Debug)]
pub struct HubState {
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
    pub fn new() -> Self {
        Self {
            selected_tab: HubTab::Shop,
            tab_rects: Cell::new([Rect::new(0, 0, 0, 0); HubTab::ALL.len()]),
            last_click: None,
        }
    }

    pub fn open(&mut self, tab: HubTab) {
        self.selected_tab = tab;
    }

    pub fn selected_tab(&self) -> HubTab {
        self.selected_tab
    }

    pub fn select_next_tab(&mut self, is_admin: bool) {
        self.selected_tab = tab_at_offset(self.selected_tab, 1, is_admin);
    }

    pub fn select_previous_tab(&mut self, is_admin: bool) {
        let len = HubTab::visible_tabs(is_admin).len();
        self.selected_tab = tab_at_offset(self.selected_tab, len - 1, is_admin);
    }

    pub fn ensure_visible_tab(&mut self, is_admin: bool) {
        if !HubTab::visible_tabs(is_admin).contains(&self.selected_tab) {
            self.selected_tab = HubTab::Shop;
        }
    }

    pub fn set_tab_rects(&self, rects: [Rect; HubTab::ALL.len()]) {
        self.tab_rects.set(rects);
    }

    /// Return the tab whose tab-strip cell contains the (0-based ratatui)
    /// point, if any.
    pub fn tab_at_point(&self, x: u16, y: u16) -> Option<HubTab> {
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
    pub fn click_tab(&mut self, tab: HubTab) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_at_point_hits_set_rect() {
        let state = HubState::new();
        let mut rects = [Rect::new(0, 0, 0, 0); HubTab::ALL.len()];
        rects[0] = Rect::new(2, 5, 8, 1); // Shop
        rects[1] = Rect::new(11, 5, 14, 1); // Leaderboard
        state.set_tab_rects(rects);

        assert_eq!(state.tab_at_point(2, 5), Some(HubTab::Shop));
        assert_eq!(state.tab_at_point(9, 5), Some(HubTab::Shop));
        assert_eq!(state.tab_at_point(12, 5), Some(HubTab::Leaderboard));
        assert_eq!(state.tab_at_point(0, 5), None);
        assert_eq!(state.tab_at_point(2, 6), None);
    }

    #[test]
    fn click_tab_detects_double_within_window() {
        let mut state = HubState::new();
        assert!(!state.click_tab(HubTab::Leaderboard));
        // Second click on the same tab within the window — double.
        assert!(state.click_tab(HubTab::Leaderboard));
        // After a double, the chain resets — next click is single again.
        assert!(!state.click_tab(HubTab::Leaderboard));
    }

    #[test]
    fn click_tab_different_tab_resets_chain() {
        let mut state = HubState::new();
        state.click_tab(HubTab::Shop);
        assert!(!state.click_tab(HubTab::Events));
        assert_eq!(state.selected_tab(), HubTab::Events);
    }
}
