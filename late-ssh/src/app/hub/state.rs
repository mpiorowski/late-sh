use std::cell::Cell;
use std::time::Instant;

use ratatui::layout::Rect;

use crate::app::hub::trophy_sixel::TrophyTier;

/// Max gap between two left-clicks (on the same tab) to count as a double-click.
pub const HUB_DOUBLE_CLICK_WINDOW_MS: u128 = 400;

/// One trophy placement slot — area on screen plus which tier (gold /
/// silver / bronze) the renderer should paint there. Populated by
/// `leaderboard::draw` each frame; read back after `hub::draw` returns
/// so the placements can be pushed into the terminal-image frame.
#[derive(Clone, Copy, Debug)]
pub struct TrophySlot {
    pub area: Rect,
    pub tier: TrophyTier,
}

/// Number of trophy slots tracked per frame — two ranked panels (chips
/// + arcade) on the top row × three trophy tiers each.
pub const HUB_TROPHY_SLOT_COUNT: usize = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HubTab {
    Leaderboard,
    Dailies,
    Shop,
    Events,
    Guide,
}

impl HubTab {
    pub const ALL: [Self; 5] = [
        Self::Shop,
        Self::Leaderboard,
        Self::Dailies,
        Self::Events,
        Self::Guide,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Leaderboard => "Leaderboard",
            Self::Dailies => "Dailies",
            Self::Shop => "Shop",
            Self::Events => "Events",
            Self::Guide => "Guide",
        }
    }
}

#[derive(Clone, Debug)]
pub struct HubState {
    selected_tab: HubTab,
    guide_scroll: u16,
    /// Per-tab on-screen rectangles, populated by the renderer each frame.
    /// `tab_rects[i]` corresponds to `HubTab::ALL[i]`. Indexed in 0-based
    /// ratatui coords.
    tab_rects: Cell<[Rect; 5]>,
    /// Bounds of the body area (whichever tab is showing). Used to gate
    /// scroll-wheel events so wheel ticks outside the modal body don't
    /// scroll the guide.
    body_area: Cell<Rect>,
    /// `(time, tab)` of the previous left-click on a tab, for double-click
    /// detection.
    last_click: Option<(Instant, HubTab)>,
    /// On-screen trophy placements for the current frame's Leaderboard
    /// tab. Cleared at the top of each leaderboard render and refilled
    /// for ranks 1-3 of each ranked panel. `None` slots are skipped by
    /// the placement push step.
    leaderboard_trophy_slots: Cell<[Option<TrophySlot>; HUB_TROPHY_SLOT_COUNT]>,
    /// On-screen burst area for the current frame's Shop celebration,
    /// if a purchase is being celebrated. Cleared every frame.
    shop_celebration_area: Cell<Option<Rect>>,
}

impl HubState {
    pub fn new() -> Self {
        Self {
            selected_tab: HubTab::Shop,
            guide_scroll: 0,
            tab_rects: Cell::new([Rect::new(0, 0, 0, 0); 5]),
            body_area: Cell::new(Rect::new(0, 0, 0, 0)),
            last_click: None,
            leaderboard_trophy_slots: Cell::new([None; HUB_TROPHY_SLOT_COUNT]),
            shop_celebration_area: Cell::new(None),
        }
    }

    pub fn open(&mut self, tab: HubTab) {
        self.selected_tab = tab;
    }

    pub fn selected_tab(&self) -> HubTab {
        self.selected_tab
    }

    pub fn guide_scroll(&self) -> u16 {
        self.guide_scroll
    }

    pub fn select_next_tab(&mut self) {
        self.selected_tab = tab_at_offset(self.selected_tab, 1);
    }

    pub fn select_previous_tab(&mut self) {
        self.selected_tab = tab_at_offset(self.selected_tab, HubTab::ALL.len() - 1);
    }

    pub fn scroll_guide(&mut self, delta: i16) {
        if delta.is_negative() {
            self.guide_scroll = self.guide_scroll.saturating_sub(delta.unsigned_abs());
        } else {
            let max_scroll = crate::app::hub::guide::content_line_count() as u16;
            self.guide_scroll = self
                .guide_scroll
                .saturating_add(delta as u16)
                .min(max_scroll);
        }
    }

    pub fn jump_guide_to_top(&mut self) {
        self.guide_scroll = 0;
    }

    pub fn jump_guide_to_bottom(&mut self) {
        self.guide_scroll = crate::app::hub::guide::content_line_count() as u16;
    }

    pub fn set_tab_rects(&self, rects: [Rect; 5]) {
        self.tab_rects.set(rects);
    }

    pub fn set_body_area(&self, area: Rect) {
        self.body_area.set(area);
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

    pub fn body_contains(&self, x: u16, y: u16) -> bool {
        rect_contains(self.body_area.get(), x, y)
    }

    /// Reset and accept the current frame's trophy placements. Called at
    /// the top of `leaderboard::draw` so stale placements from a previous
    /// frame never leak through.
    pub fn set_leaderboard_trophy_slots(&self, slots: [Option<TrophySlot>; HUB_TROPHY_SLOT_COUNT]) {
        self.leaderboard_trophy_slots.set(slots);
    }

    pub fn leaderboard_trophy_slots(&self) -> [Option<TrophySlot>; HUB_TROPHY_SLOT_COUNT] {
        self.leaderboard_trophy_slots.get()
    }

    pub fn set_shop_celebration_area(&self, area: Option<Rect>) {
        self.shop_celebration_area.set(area);
    }

    pub fn shop_celebration_area(&self) -> Option<Rect> {
        self.shop_celebration_area.get()
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

fn tab_at_offset(current: HubTab, offset: usize) -> HubTab {
    let index = HubTab::ALL
        .iter()
        .position(|tab| *tab == current)
        .unwrap_or_default();
    HubTab::ALL[(index + offset) % HubTab::ALL.len()]
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
        let mut rects = [Rect::new(0, 0, 0, 0); 5];
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
        assert!(!state.click_tab(HubTab::Guide));
        assert_eq!(state.selected_tab(), HubTab::Guide);
    }
}
