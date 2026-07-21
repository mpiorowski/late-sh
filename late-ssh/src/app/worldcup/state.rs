//! Per-session UI state for the World Cup screen.
//!
//! This is purely the local view model — which sub-view is showing and how far
//! each is scrolled. The tournament data lives in the service snapshot; the
//! viewer guard that gates polling lives on `App` (see `App::set_screen`).

/// Which sub-view the screen is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    /// Live/upcoming matches plus group standings.
    #[default]
    Overview,
    /// The knockout bracket.
    Bracket,
}

#[derive(Debug, Default)]
pub struct State {
    pub view: View,
    /// Scroll offsets, tracked independently per sub-view so toggling back and
    /// forth doesn't lose your place.
    pub overview_scroll: u16,
    pub bracket_scroll: u16,
}

impl State {
    /// Switches between the overview and the bracket.
    pub fn toggle_view(&mut self) {
        self.view = match self.view {
            View::Overview => View::Bracket,
            View::Bracket => View::Overview,
        };
    }

    fn active_scroll(&mut self) -> &mut u16 {
        match self.view {
            View::Overview => &mut self.overview_scroll,
            View::Bracket => &mut self.bracket_scroll,
        }
    }

    pub fn scroll_up(&mut self) {
        let s = self.active_scroll();
        *s = s.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        let s = self.active_scroll();
        *s = s.saturating_add(1);
    }

    /// Applies a signed scroll delta (positive = toward the top, matching the
    /// `PageUp`/wheel convention used elsewhere in the app).
    pub fn scroll(&mut self, delta: isize) {
        if delta >= 0 {
            for _ in 0..delta {
                self.scroll_up();
            }
        } else {
            for _ in 0..-delta {
                self.scroll_down();
            }
        }
    }
}
