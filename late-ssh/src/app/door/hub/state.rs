//! Games hub: the dedicated landing screen for the immersive door games
//! (Lateania, NetHack, Green Dragon, Rebels). It is a selector — a tab row of games with the
//! selected game's full landing page rendered below it — not a scroll. Left/right
//! (or h/l) change the selection; Enter launches the selected game. Adding a
//! future door game is a new `HubGame` entry plus a `draw_landing` for it, not a
//! new top-level screen.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HubGame {
    Lateania,
    Rebels,
    Nethack,
    Dcss,
    Usurper,
    GreenDragon,
    Dopewars,
}

impl HubGame {
    /// Selector order, left to right.
    pub const ALL: [HubGame; 7] = [
        HubGame::Lateania,
        HubGame::Nethack,
        HubGame::Dcss,
        HubGame::Usurper,
        HubGame::GreenDragon,
        HubGame::Rebels,
        HubGame::Dopewars,
    ];

    pub fn label(self) -> &'static str {
        match self {
            HubGame::Lateania => "Lateania",
            HubGame::Rebels => "Rebels",
            HubGame::Nethack => "NetHack",
            HubGame::Dcss => "DCSS",
            HubGame::Usurper => "Usurper",
            HubGame::GreenDragon => "Green Dragon",
            HubGame::Dopewars => "dopewars",
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

    /// Move the selection one card right, clamped at the last game.
    pub fn select_next(&mut self) {
        let last = HubGame::ALL.len() - 1;
        self.selected = self.selected().saturating_add(1).min(last);
    }

    /// Move the selection one card left, clamped at the first game.
    pub fn select_prev(&mut self) {
        self.selected = self.selected().saturating_sub(1);
    }

    pub fn select(&mut self, index: usize) {
        if index < HubGame::ALL.len() {
            self.selected = index;
        }
    }
}


