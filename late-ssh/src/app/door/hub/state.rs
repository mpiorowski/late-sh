//! Games hub: the dedicated landing screen for the immersive door games
//! (Lateania, DCSS, NetHack, Green Dragon, ...). It is a selector — a grouped
//! sidebar of games on the left with the selected game's full landing page
//! rendered beside it — not a scroll. Up/down (or j/k, h/l) change the
//! selection; Enter launches the selected game. Adding a future door game is a
//! new `HubGame` entry with a `group()` arm plus a `draw_landing` for it, not a
//! new top-level screen.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HubGame {
    Lateania,
    Rebels,
    Nethack,
    Dcss,
    Brogue,
    Usurper,
    GreenDragon,
    Dopewars,
    Codekeep,
    Darkroom,
}

/// Sidebar groups, in display order. Every game maps to exactly one, and
/// `HubGame::ALL` keeps each group's games adjacent so a group's header
/// renders once (asserted in `state_test.rs`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HubGroup {
    House,
    Roguelikes,
    Doors,
    BackRoom,
}

impl HubGroup {
    pub fn label(self) -> &'static str {
        match self {
            HubGroup::House => "the house",
            HubGroup::Roguelikes => "roguelikes",
            HubGroup::Doors => "bbs doors",
            HubGroup::BackRoom => "the back room",
        }
    }
}

impl HubGame {
    /// Selector order, top to bottom: the house game first, the roguelikes by
    /// stature, the BBS doors led by Green Dragon, then the back room.
    pub const ALL: [HubGame; 10] = [
        HubGame::Lateania,
        HubGame::Dcss,
        HubGame::Nethack,
        HubGame::Brogue,
        HubGame::GreenDragon,
        HubGame::Usurper,
        HubGame::Dopewars,
        HubGame::Darkroom,
        HubGame::Rebels,
        HubGame::Codekeep,
    ];

    pub fn label(self) -> &'static str {
        match self {
            HubGame::Lateania => "Lateania",
            HubGame::Rebels => "Rebels",
            HubGame::Nethack => "NetHack",
            HubGame::Dcss => "DCSS",
            HubGame::Brogue => "Brogue",
            HubGame::Usurper => "Usurper",
            HubGame::GreenDragon => "Green Dragon",
            HubGame::Dopewars => "dopewars",
            HubGame::Codekeep => "CodeKeep",
            HubGame::Darkroom => crate::app::door::darkroom::data::TITLE,
        }
    }

    pub fn group(self) -> HubGroup {
        match self {
            HubGame::Lateania => HubGroup::House,
            HubGame::Dcss | HubGame::Nethack | HubGame::Brogue => HubGroup::Roguelikes,
            HubGame::GreenDragon | HubGame::Usurper | HubGame::Dopewars => HubGroup::Doors,
            HubGame::Darkroom | HubGame::Rebels | HubGame::Codekeep => HubGroup::BackRoom,
        }
    }
}

/// Per-session hub state: which game card is currently selected.
#[derive(Default)]
pub struct State {
    selected: usize,
}

impl State {
    pub fn selected(&self) -> usize {
        self.selected.min(HubGame::ALL.len() - 1)
    }

    pub fn selected_game(&self) -> HubGame {
        HubGame::ALL[self.selected()]
    }

    /// Move the selection one game down the sidebar, clamped at the last game.
    pub fn select_next(&mut self) {
        let last = HubGame::ALL.len() - 1;
        self.selected = self.selected().saturating_add(1).min(last);
    }

    /// Move the selection one game up the sidebar, clamped at the first game.
    pub fn select_prev(&mut self) {
        self.selected = self.selected().saturating_sub(1);
    }

    pub fn select(&mut self, index: usize) {
        if index < HubGame::ALL.len() {
            self.selected = index;
        }
    }
}
