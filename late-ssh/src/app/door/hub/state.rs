//! Games hub: the dedicated landing screen for the immersive door games
//! (Lateania, DCSS, NetHack, Green Dragon, ...). It is a selector — a grouped
//! sidebar of games on the left with the selected game's full landing page
//! rendered beside it — not a scroll. Up/down (or j/k, h/l) change the
//! selection; Enter launches the selected game; Ctrl+J/K (or Ctrl+Down/Up)
//! scroll a landing too long for the terminal. Adding a future door game is a
//! new `HubGame` entry with a `group()` arm plus a `draw_landing` for it, not a
//! new top-level screen. Minecraft is the one card with nothing to launch: the
//! server is played from the game client, so its landing is information only.

use std::cell::Cell;

use crate::app::common::primitives::Screen;
use crate::app::state::App;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HubGame {
    Lateania,
    Minecraft,
    Rebels,
    Nethack,
    Dcss,
    Brogue,
    Usurper,
    GreenDragon,
    Dopewars,
    Bashquest,
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
    Remakes,
    Doors,
}

impl HubGroup {
    pub fn label(self) -> &'static str {
        match self {
            HubGroup::House => "the house",
            HubGroup::Roguelikes => "roguelikes",
            HubGroup::Remakes => "remakes",
            HubGroup::Doors => "doors",
        }
    }
}

impl HubGame {
    /// Selector order, top to bottom: the house games first (Lateania, ours
    /// from the ground up, then the Minecraft server we host), the roguelikes by stature, the remakes (our own
    /// builds of A Dark Room and Green Dragon), then the doors (foreign upstream
    /// terminal games hosted on a PTY).
    pub const ALL: [HubGame; 12] = [
        HubGame::Lateania,
        HubGame::Minecraft,
        HubGame::Dcss,
        HubGame::Nethack,
        HubGame::Brogue,
        HubGame::Darkroom,
        HubGame::GreenDragon,
        HubGame::Usurper,
        HubGame::Dopewars,
        HubGame::Bashquest,
        HubGame::Rebels,
        HubGame::Codekeep,
    ];

    pub fn label(self) -> &'static str {
        match self {
            HubGame::Lateania => "Lateania",
            HubGame::Minecraft => "Minecraft",
            HubGame::Rebels => "Rebels",
            HubGame::Nethack => "NetHack",
            HubGame::Dcss => "DCSS",
            HubGame::Brogue => "Brogue",
            HubGame::Usurper => "Usurper",
            HubGame::GreenDragon => "Green Dragon",
            HubGame::Dopewars => "dopewars",
            HubGame::Bashquest => "BashQuest",
            HubGame::Codekeep => "CodeKeep",
            HubGame::Darkroom => crate::app::door::darkroom::data::TITLE,
        }
    }

    pub fn group(self) -> HubGroup {
        match self {
            HubGame::Lateania | HubGame::Minecraft => HubGroup::House,
            HubGame::Dcss | HubGame::Nethack | HubGame::Brogue => HubGroup::Roguelikes,
            HubGame::Darkroom | HubGame::GreenDragon => HubGroup::Remakes,
            HubGame::Usurper
            | HubGame::Dopewars
            | HubGame::Bashquest
            | HubGame::Rebels
            | HubGame::Codekeep => HubGroup::Doors,
        }
    }

    /// The pushed-config slot this game reads from, for the doors that take
    /// one. Brogue keeps its config per-player upstream already; the rest have
    /// no config file at all.
    pub fn rc_game(self) -> Option<late_core::models::door_rc::DoorRcGame> {
        use late_core::models::door_rc::DoorRcGame;
        match self {
            HubGame::Nethack => Some(DoorRcGame::Nethack),
            HubGame::Dcss => Some(DoorRcGame::Dcss),
            HubGame::Lateania
            | HubGame::Minecraft
            | HubGame::Rebels
            | HubGame::Brogue
            | HubGame::Usurper
            | HubGame::GreenDragon
            | HubGame::Dopewars
            | HubGame::Bashquest
            | HubGame::Codekeep
            | HubGame::Darkroom => None,
        }
    }

    /// The screen a live session of this game resumes on, or `None` when the
    /// game is not live right now. This is the one definition of door
    /// liveness: the backtick cycle's door leg ([`live_doors`]) and the
    /// sidebar's in-progress pips both read it, so they cannot drift. Three
    /// models. Lateania has no detached session, so its test is the recency
    /// window a backtick detach arms (`App::lateania_recently_active`): hop
    /// out and the door stays live for a few minutes, hopping in re-joins the
    /// saved character. The roguelikes count a running (attached or detached)
    /// game: a door sitting on its launcher is not live. Dark Room and Green
    /// Dragon keep their loaded state across a hop, so being loaded is the
    /// test; `App::tick` saves and drops them once the player has been away
    /// past [`crate::app::door::game::IDLE_WINDOW`], which is what ends it.
    /// The PTY doors (Usurper, dopewars, BashQuest, Rebels, CodeKeep) end
    /// their session on leaving the screen, so they are never live. Minecraft
    /// has no session here at all.
    pub(crate) fn live_screen(self, app: &App) -> Option<Screen> {
        match self {
            HubGame::Lateania => app.lateania_recently_active().then_some(Screen::Lateania),
            HubGame::Dcss => app
                .dcss_state
                .as_ref()
                .is_some_and(|state| state.is_running())
                .then_some(Screen::Dcss),
            HubGame::Nethack => app
                .nethack_state
                .as_ref()
                .is_some_and(|state| state.is_running())
                .then_some(Screen::Nethack),
            HubGame::Brogue => app
                .brogue_state
                .as_ref()
                .is_some_and(|state| state.is_running())
                .then_some(Screen::Brogue),
            HubGame::Darkroom => app.darkroom_state.is_some().then_some(Screen::Darkroom),
            HubGame::GreenDragon => app
                .greendragon_state
                .is_some()
                .then_some(Screen::GreenDragon),
            HubGame::Minecraft
            | HubGame::Usurper
            | HubGame::Dopewars
            | HubGame::Bashquest
            | HubGame::Rebels
            | HubGame::Codekeep => None,
        }
    }
}

/// The door games with a live session, by their live-game screens, in sidebar
/// order ([`HubGame::ALL`]). The backtick cycle's door leg.
pub(crate) fn live_doors(app: &App) -> Vec<Screen> {
    HubGame::ALL
        .into_iter()
        .filter_map(|game| game.live_screen(app))
        .collect()
}

/// Per-session hub state: which game card is selected, and how far its
/// landing is scrolled.
#[derive(Default)]
pub struct State {
    selected: usize,
    /// Rows the selected landing is scrolled down. Back to the top whenever the
    /// selection changes.
    scroll: u16,
    /// How far the selected landing could scroll on the last frame, recorded by
    /// `hub::ui::draw_games_hub`. Input has no layout to measure, so it clamps
    /// to what was last drawn.
    max_scroll: Cell<u16>,
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
        self.set_selected(self.selected().saturating_add(1).min(last));
    }

    /// Move the selection one game up the sidebar, clamped at the first game.
    pub fn select_prev(&mut self) {
        self.set_selected(self.selected().saturating_sub(1));
    }

    pub fn select(&mut self, index: usize) {
        if index < HubGame::ALL.len() {
            self.set_selected(index);
        }
    }

    /// A different game starts at the top of its landing; re-selecting the
    /// same one keeps the reader's place.
    fn set_selected(&mut self, index: usize) {
        if index != self.selected() {
            self.scroll = 0;
        }
        self.selected = index;
    }

    pub fn scroll(&self) -> u16 {
        self.scroll
    }

    /// Where the renderer records the selected landing's scroll range.
    pub fn max_scroll(&self) -> &Cell<u16> {
        &self.max_scroll
    }

    /// Scroll the landing one row down, stopping at the range last drawn.
    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1).min(self.max_scroll.get());
    }

    /// Scroll the landing one row up. Clamps to the range last drawn first, so
    /// after a resize shortens the landing the first press already moves it.
    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.min(self.max_scroll.get()).saturating_sub(1);
    }
}
