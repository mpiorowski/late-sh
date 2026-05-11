#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HubTab {
    Leaderboard,
    Dailies,
    Shop,
    Events,
}

impl HubTab {
    pub const ALL: [Self; 4] = [Self::Leaderboard, Self::Dailies, Self::Shop, Self::Events];

    pub fn label(self) -> &'static str {
        match self {
            Self::Leaderboard => "Leaderboard",
            Self::Dailies => "Dailies",
            Self::Shop => "Shop",
            Self::Events => "Events",
        }
    }
}

#[derive(Clone, Debug)]
pub struct HubState {
    selected_tab: HubTab,
}

impl HubState {
    pub fn new() -> Self {
        Self {
            selected_tab: HubTab::Leaderboard,
        }
    }

    pub fn open(&mut self, tab: HubTab) {
        self.selected_tab = tab;
    }

    pub fn selected_tab(&self) -> HubTab {
        self.selected_tab
    }

    pub fn select_next_tab(&mut self) {
        self.selected_tab = tab_at_offset(self.selected_tab, 1);
    }

    pub fn select_previous_tab(&mut self) {
        self.selected_tab = tab_at_offset(self.selected_tab, HubTab::ALL.len() - 1);
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
