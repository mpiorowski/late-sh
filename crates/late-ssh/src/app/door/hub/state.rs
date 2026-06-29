//! Games hub: the dedicated landing screen for the immersive door games
//! (Lateania, Rebels, NetHack). It is a selector — a tab row of games with the
//! selected game's full landing page rendered below it — not a scroll. Left/right
//! (or h/l) change the selection; Enter launches the selected game. Adding a
//! future door game is a new `HubGame` entry plus a `draw_landing` for it, not a
//! new top-level screen.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HubGame {
    Lateania,
    Rebels,
    Nethack,
}

impl HubGame {
    /// Selector order, left to right.
    pub const ALL: [HubGame; 3] = [HubGame::Lateania, HubGame::Rebels, HubGame::Nethack];

    pub fn label(self) -> &'static str {
        match self {
            HubGame::Lateania => "Lateania",
            HubGame::Rebels => "Rebels",
            HubGame::Nethack => "NetHack",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_clamps_at_both_ends() {
        let mut s = State::default();
        assert_eq!(s.selected_game(), HubGame::Lateania);
        s.select_prev();
        assert_eq!(s.selected_game(), HubGame::Lateania);
        s.select_next();
        assert_eq!(s.selected_game(), HubGame::Rebels);
        s.select_next();
        assert_eq!(s.selected_game(), HubGame::Nethack);
        s.select_next();
        assert_eq!(s.selected_game(), HubGame::Nethack);
    }

    #[test]
    fn select_jumps_directly() {
        let mut s = State::default();
        s.select(2);
        assert_eq!(s.selected_game(), HubGame::Nethack);
        s.select(99);
        assert_eq!(s.selected_game(), HubGame::Nethack);
    }

    #[test]
    fn all_games_are_listed_in_order() {
        assert_eq!(
            HubGame::ALL.map(HubGame::label),
            ["Lateania", "Rebels", "NetHack"],
        );
    }
}
