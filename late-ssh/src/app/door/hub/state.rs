//! Games hub: the dedicated landing screen for the immersive door games
//! (Lateania, DCSS, NetHack, Green Dragon, ...). It is a selector, a grouped
//! sidebar of games on the left with the selected game's full landing page
//! rendered beside it — not a scroll. Up/down (or j/k, h/l) change the
//! selection; Enter launches the selected game; Ctrl+J/K (or Ctrl+Down/Up)
//! scroll a landing too long for the terminal; `s` on a watchable card opens
//! the read-only watch view (`door::spectate`), which replaces the selector
//! until Esc. Adding a future door game is a
//! new `HubGame` entry with a `group()` arm plus a `draw_landing` for it, not a
//! new top-level screen. Minecraft is the one card with nothing to launch: the
//! server is played from the game client, so its landing is information only.
//! Night City heads the sidebar for runners only ([`HubGame::roster`]): its
//! card is the night city's front door, and Enter takes the same descent as
//! `0` on the Lounge (`deadchannel/city/input.rs`).

use std::cell::Cell;

use crate::app::common::primitives::Screen;
use crate::app::state::App;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HubGame {
    NightCity,
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
    Servers,
    Doors,
}

impl HubGroup {
    pub fn label(self) -> &'static str {
        match self {
            HubGroup::House => "the house",
            HubGroup::Roguelikes => "roguelikes",
            HubGroup::Remakes => "remakes",
            HubGroup::Servers => "servers",
            HubGroup::Doors => "doors",
        }
    }
}

impl HubGame {
    /// Selector order, top to bottom: the house games first (Night City,
    /// the runners' own card, sliced off for everyone else by
    /// [`HubGame::roster`], then Lateania, ours from the ground up), the
    /// roguelikes by stature, the remakes (our own
    /// build of A Dark Room), the servers we host and you play from a game
    /// client (Minecraft), then the doors: Green Dragon (our native LORD
    /// remake, filed with the BBS doors it descends from) and the foreign
    /// upstream terminal games hosted on a PTY.
    pub const ALL: [HubGame; 13] = [
        HubGame::NightCity,
        HubGame::Lateania,
        HubGame::Dcss,
        HubGame::Nethack,
        HubGame::Brogue,
        HubGame::Darkroom,
        HubGame::Minecraft,
        HubGame::GreenDragon,
        HubGame::Usurper,
        HubGame::Dopewars,
        HubGame::Bashquest,
        HubGame::Rebels,
        HubGame::Codekeep,
    ];

    /// The sidebar a session sees. Runners (`App::is_runner`) get every
    /// card with Night City on top; everyone else gets the rest, so the card
    /// is not there to select, let alone launch.
    pub fn roster(is_runner: bool) -> &'static [HubGame] {
        match is_runner {
            true => &HubGame::ALL,
            false => &HubGame::ALL[1..],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            HubGame::NightCity => "Night City",
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
            HubGame::NightCity | HubGame::Lateania => HubGroup::House,
            HubGame::Dcss | HubGame::Nethack | HubGame::Brogue => HubGroup::Roguelikes,
            HubGame::Darkroom => HubGroup::Remakes,
            HubGame::Minecraft => HubGroup::Servers,
            HubGame::GreenDragon
            | HubGame::Usurper
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
            HubGame::NightCity
            | HubGame::Lateania
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

    /// The watchable door behind this card, for the doors whose hosts serve
    /// watch sessions (the hub's `s` key).
    pub fn spectate_game(self) -> Option<crate::app::door::spectate::state::SpectateGame> {
        use crate::app::door::spectate::state::SpectateGame;
        match self {
            HubGame::Dcss => Some(SpectateGame::Dcss),
            HubGame::NightCity
            | HubGame::Lateania
            | HubGame::Minecraft
            | HubGame::Rebels
            | HubGame::Nethack
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
    /// has no session here at all, and Night City is not a door: the street
    /// keeps a runner standing on its own (`deadchannel/street`).
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
            HubGame::NightCity
            | HubGame::Minecraft
            | HubGame::Usurper
            | HubGame::Dopewars
            | HubGame::Bashquest
            | HubGame::Rebels
            | HubGame::Codekeep => None,
        }
    }
}

// `roster` slices Night City off the front for non-runners: it must be the
// first card, or the slice would hide some other game.
const _: () = assert!(matches!(HubGame::ALL[0], HubGame::NightCity));

/// Whether the hub paints on the ambience edge: Night City's landing rains
/// and flickers like the street it opens onto (`tick.rs` drives both).
pub(crate) fn animates(app: &App) -> bool {
    app.screen == Screen::Games
        && app
            .games_hub_state
            .selected_game(HubGame::roster(app.is_runner()))
            == HubGame::NightCity
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
/// landing is scrolled. Every selection call takes the session's roster
/// ([`HubGame::roster`]): it changes when a runner joins or leaves
/// #deadchannel, and the selection is a game, not a row, so it stays put.
#[derive(Default)]
pub struct State {
    /// The chosen card; `None` until one is chosen, which shows the top of
    /// the roster. A choice the roster no longer holds (Night City after
    /// leaving #deadchannel) shows the top too.
    selected: Option<HubGame>,
    /// Rows the selected landing is scrolled down. Back to the top whenever the
    /// selection changes.
    scroll: u16,
    /// How far the selected landing could scroll on the last frame, recorded by
    /// `hub::ui::draw_games_hub`. Input has no layout to measure, so it clamps
    /// to what was last drawn.
    max_scroll: Cell<u16>,
}

impl State {
    /// The selected card's row in `roster`.
    pub fn selected(&self, roster: &[HubGame]) -> usize {
        match self.selected {
            Some(game) => roster.iter().position(|g| *g == game).unwrap_or(0),
            None => 0,
        }
    }

    pub fn selected_game(&self, roster: &[HubGame]) -> HubGame {
        roster[self.selected(roster)]
    }

    /// Move the selection one game down the sidebar, clamped at the last game.
    pub fn select_next(&mut self, roster: &[HubGame]) {
        let last = roster.len() - 1;
        let index = self.selected(roster).saturating_add(1).min(last);
        self.select_game(roster, roster[index]);
    }

    /// Move the selection one game up the sidebar, clamped at the first game.
    pub fn select_prev(&mut self, roster: &[HubGame]) {
        let index = self.selected(roster).saturating_sub(1);
        self.select_game(roster, roster[index]);
    }

    pub fn select(&mut self, roster: &[HubGame], index: usize) {
        if let Some(game) = roster.get(index) {
            self.select_game(roster, *game);
        }
    }

    /// A different game starts at the top of its landing; re-selecting the
    /// one `roster` already shows (the top card too, before any choice was
    /// made) keeps the reader's place.
    pub fn select_game(&mut self, roster: &[HubGame], game: HubGame) {
        if self.selected_game(roster) != game {
            self.scroll = 0;
        }
        self.selected = Some(game);
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
