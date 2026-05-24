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
        Self::Leaderboard,
        Self::Dailies,
        Self::Shop,
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
}

impl HubState {
    pub fn new() -> Self {
        Self {
            selected_tab: HubTab::Leaderboard,
            guide_scroll: 0,
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
