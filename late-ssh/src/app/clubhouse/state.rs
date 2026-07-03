//! Per-session clubhouse view state. The crowd itself lives in the shared
//! [`lobby`](super::lobby): every active human holds a seat until their
//! first step frees it, walkers carry live positions, and every session
//! renders the same room. This struct owns the session-local bits: the
//! camera target (your own cell, mirrored from the lobby), animation clock,
//! the latest lobby snapshot, door arrival/departure ambience, and the
//! first-visit tutorial state machine.

use std::collections::{HashSet, VecDeque};

use uuid::Uuid;

use super::lobby::{Emote, LobbySnapshot, SharedLobby};
use super::map;

/// Refresh the roster from the active-users map once a second (15 ticks).
const ROSTER_REFRESH_TICKS: u64 = 15;
/// How long a door ambience line lingers, in ticks (~5s).
const DOOR_EVENT_TICKS: u64 = 75;
/// How many ambience lines can stack by the door.
const DOOR_EVENT_MAX: usize = 4;

/// A live human from the active-users map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occupant {
    pub user_id: Uuid,
    pub username: String,
}

/// `* name slipped in` / `* name headed out`, shown near the door.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoorEvent {
    pub username: String,
    pub arrived: bool,
    pub until_tick: u64,
}

/// The first-visit walkthrough. `Pending` arms it until the screen is first
/// opened; every step is skippable and `Done` is persisted once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tutorial {
    /// Nothing to run (returning user).
    Off,
    /// Armed, fires on the first clubhouse entry this session.
    Pending,
    /// Box over your head at the door: how to walk, go see the bartender.
    Welcome,
    /// Walking; a hint points at the bar until you reach it.
    GoToBar,
    /// At the bar: the chat lesson popup.
    BarLesson,
    /// Last box: the landmarks and Ctrl+O, then you're on your own.
    SendOff,
    Done,
}

#[derive(Debug)]
pub struct State {
    pub player_x: u16,
    pub player_y: u16,
    pub anim_tick: u64,
    lobby: Option<SharedLobby>,
    /// Latest crowd view, cloned from the lobby every tick while on screen.
    pub snapshot: LobbySnapshot,
    user_id: Uuid,
    username: String,
    pub graybeard_online: bool,
    pub bartender_online: bool,
    last_roster_tick: u64,
    force_roster_refresh: bool,
    /// Roster ids from the last refresh, for arrival/departure diffs.
    seen: HashSet<Uuid>,
    /// The first refresh only primes `seen`; it must not announce the whole
    /// room as arrivals.
    seen_primed: bool,
    pub door_events: VecDeque<DoorEvent>,
    pub tutorial: Tutorial,
}

impl State {
    pub fn new(
        lobby: Option<SharedLobby>,
        user_id: Uuid,
        username: String,
        tutorial_pending: bool,
    ) -> Self {
        Self {
            player_x: map::SPAWN.0,
            player_y: map::SPAWN.1,
            anim_tick: 0,
            lobby,
            snapshot: LobbySnapshot::default(),
            user_id,
            username,
            graybeard_online: false,
            bartender_online: false,
            last_roster_tick: 0,
            force_roster_refresh: false,
            seen: HashSet::new(),
            seen_primed: false,
            door_events: VecDeque::new(),
            tutorial: if tutorial_pending {
                Tutorial::Pending
            } else {
                Tutorial::Off
            },
        }
    }

    /// Advance the animation clock and expire door ambience. Called every
    /// world tick.
    pub fn tick(&mut self, _on_screen: bool) {
        self.anim_tick = self.anim_tick.wrapping_add(1);
        let now = self.anim_tick;
        self.door_events.retain(|e| e.until_tick > now);
    }

    /// Screen entry hook: refresh the crowd immediately and, on the very
    /// first visit ever, start the tutorial at the door.
    pub fn enter_screen(&mut self) {
        self.force_roster_refresh = true;
        if self.tutorial == Tutorial::Pending {
            self.tutorial = Tutorial::Welcome;
            if let Some(lobby) = &self.lobby {
                lobby.place(self.user_id, &self.username, map::SPAWN.0, map::SPAWN.1);
            }
            self.player_x = map::SPAWN.0;
            self.player_y = map::SPAWN.1;
        }
    }

    pub fn roster_refresh_due(&mut self) -> bool {
        if !self.force_roster_refresh
            && self.anim_tick.wrapping_sub(self.last_roster_tick) < ROSTER_REFRESH_TICKS
        {
            return false;
        }
        self.force_roster_refresh = false;
        self.last_roster_tick = self.anim_tick;
        true
    }

    /// Reconcile the shared lobby with a fresh human roster (including this
    /// session's user) and record arrival/departure ambience.
    pub fn refresh_roster(&mut self, roster: Vec<Occupant>) {
        if let Some(own) = roster.iter().find(|o| o.user_id == self.user_id) {
            self.username = own.username.clone();
        }

        let ids: HashSet<Uuid> = roster.iter().map(|o| o.user_id).collect();
        if self.seen_primed {
            for who in &roster {
                if !self.seen.contains(&who.user_id) && who.user_id != self.user_id {
                    self.push_door_event(who.username.clone(), true);
                }
            }
            // Departures need the old names; look them up in the previous
            // snapshot before it is replaced.
            let departed: Vec<String> = self
                .seen
                .difference(&ids)
                .filter_map(|gone| self.snapshot.find(*gone))
                .map(|p| p.username.clone())
                .collect();
            for name in departed {
                self.push_door_event(name, false);
            }
        }
        self.seen = ids;
        self.seen_primed = true;

        if let Some(lobby) = &self.lobby {
            let pairs: Vec<(Uuid, String)> = roster
                .into_iter()
                .map(|o| (o.user_id, o.username))
                .collect();
            lobby.sync(&pairs);
        }
    }

    /// Pull the latest crowd view and mirror our own cell for the camera.
    /// Called every world tick while the screen is visible.
    pub fn refresh_snapshot(&mut self) {
        let Some(lobby) = &self.lobby else {
            return;
        };
        self.snapshot = lobby.snapshot();
        if let Some(own) = self.snapshot.find(self.user_id) {
            let (x, y) = own.placement.position();
            self.player_x = x;
            self.player_y = y;
        }
    }

    fn push_door_event(&mut self, username: String, arrived: bool) {
        if self.door_events.len() >= DOOR_EVENT_MAX {
            self.door_events.pop_front();
        }
        self.door_events.push_back(DoorEvent {
            username,
            arrived,
            until_tick: self.anim_tick.wrapping_add(DOOR_EVENT_TICKS),
        });
    }

    /// True while an arrival is fresh, so the door sign can glow.
    pub fn door_glow(&self) -> bool {
        self.door_events.iter().any(|e| e.arrived)
    }

    /// Try to walk one step; the first step frees your seat in the shared
    /// lobby. Also advances the tutorial off the welcome box.
    pub fn walk(&mut self, dx: i32, dy: i32) {
        if let Some(lobby) = &self.lobby {
            let (x, y) = lobby.walk(self.user_id, &self.username, dx, dy);
            self.player_x = x;
            self.player_y = y;
        } else {
            // Headless/test sessions still walk locally.
            let nx = self.player_x.saturating_add_signed(dx as i16);
            let ny = self.player_y.saturating_add_signed(dy as i16);
            if map::walkable(nx, ny) {
                self.player_x = nx;
                self.player_y = ny;
            }
        }
        if self.tutorial == Tutorial::Welcome {
            self.tutorial = Tutorial::GoToBar;
        }
    }

    pub fn emote(&self, emote: Emote) {
        if let Some(lobby) = &self.lobby {
            lobby.emote(self.user_id, emote);
        }
    }

    pub fn pet_dog(&self) {
        if let Some(lobby) = &self.lobby {
            lobby.pet_dog(&self.username);
        }
    }

    /// The prop within reach of the player, if any.
    pub fn nearby(&self) -> Option<map::Interactive> {
        map::nearest_interactive(self.player_x, self.player_y)
    }

    /// Everyone in the room (the lobby roster includes this session's user
    /// once the first refresh lands).
    pub fn headcount(&self) -> usize {
        self.snapshot.headcount().max(1)
    }

    pub fn own_user_id(&self) -> Uuid {
        self.user_id
    }

    /// GoToBar -> BarLesson when the player reaches the counter. Returns
    /// true exactly once, so the caller can trigger the bartender greeting.
    pub fn tutorial_reached_bar(&mut self) -> bool {
        if self.tutorial == Tutorial::GoToBar && self.nearby() == Some(map::Interactive::Bartender)
        {
            self.tutorial = Tutorial::BarLesson;
            return true;
        }
        false
    }

    /// Advance past the current tutorial popup (Enter). Returns true when
    /// the tutorial just finished and should be persisted.
    pub fn tutorial_advance(&mut self) -> bool {
        match self.tutorial {
            Tutorial::BarLesson => {
                self.tutorial = Tutorial::SendOff;
                false
            }
            Tutorial::SendOff => {
                self.tutorial = Tutorial::Done;
                true
            }
            _ => false,
        }
    }

    /// Esc: skip the rest of the walkthrough. Returns true when this ended
    /// a live tutorial and should be persisted.
    pub fn tutorial_skip(&mut self) -> bool {
        match self.tutorial {
            Tutorial::Welcome | Tutorial::GoToBar | Tutorial::BarLesson | Tutorial::SendOff => {
                self.tutorial = Tutorial::Done;
                true
            }
            _ => false,
        }
    }

    /// True while a tutorial popup wants Enter/Esc before anything else.
    pub fn tutorial_capturing_keys(&self) -> bool {
        matches!(self.tutorial, Tutorial::BarLesson | Tutorial::SendOff)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn occupant(n: u128, name: &str) -> Occupant {
        Occupant {
            user_id: Uuid::from_u128(n),
            username: name.to_string(),
        }
    }

    fn state_with_lobby(tutorial: bool) -> State {
        State::new(
            Some(SharedLobby::with_seed(7)),
            Uuid::from_u128(1),
            "me".to_string(),
            tutorial,
        )
    }

    #[test]
    fn refresh_seats_the_crowd_and_mirrors_own_position() {
        let mut state = state_with_lobby(false);
        state.refresh_roster(vec![occupant(1, "me"), occupant(2, "alice")]);
        state.refresh_snapshot();
        assert_eq!(state.headcount(), 2);
        // Own cell mirrors the assigned seat, not the spawn mat.
        let own = state.snapshot.find(Uuid::from_u128(1)).unwrap();
        assert_eq!(own.placement.position(), (state.player_x, state.player_y));
    }

    #[test]
    fn first_refresh_does_not_announce_the_whole_room() {
        let mut state = state_with_lobby(false);
        state.refresh_roster(vec![occupant(1, "me"), occupant(2, "alice")]);
        assert!(state.door_events.is_empty());

        state.refresh_roster(vec![
            occupant(1, "me"),
            occupant(2, "alice"),
            occupant(3, "bob"),
        ]);
        assert_eq!(state.door_events.len(), 1);
        assert!(state.door_events[0].arrived);
        assert_eq!(state.door_events[0].username, "bob");
        assert!(state.door_glow());
    }

    #[test]
    fn departures_use_the_last_known_name() {
        let mut state = state_with_lobby(false);
        state.refresh_roster(vec![occupant(1, "me"), occupant(2, "alice")]);
        state.refresh_snapshot();
        state.refresh_roster(vec![occupant(1, "me")]);
        assert_eq!(state.door_events.len(), 1);
        assert!(!state.door_events[0].arrived);
        assert_eq!(state.door_events[0].username, "alice");
    }

    #[test]
    fn door_events_expire_with_the_clock() {
        let mut state = state_with_lobby(false);
        state.refresh_roster(vec![occupant(1, "me")]);
        state.refresh_roster(vec![occupant(1, "me"), occupant(2, "alice")]);
        assert_eq!(state.door_events.len(), 1);
        for _ in 0..=DOOR_EVENT_TICKS {
            state.tick(true);
        }
        assert!(state.door_events.is_empty());
    }

    #[test]
    fn walking_moves_and_respects_walls() {
        let mut state = state_with_lobby(false);
        state.refresh_roster(vec![occupant(1, "me")]);
        state.refresh_snapshot();
        for _ in 0..80 {
            state.walk(0, 1);
        }
        assert_eq!(state.player_y, map::MAP_H - 2);
        let before = (state.player_x, state.player_y);
        state.walk(0, 1);
        assert_eq!((state.player_x, state.player_y), before);
    }

    #[test]
    fn tutorial_runs_welcome_to_done() {
        let mut state = state_with_lobby(true);
        assert_eq!(state.tutorial, Tutorial::Pending);
        state.enter_screen();
        assert_eq!(state.tutorial, Tutorial::Welcome);
        assert_eq!((state.player_x, state.player_y), map::SPAWN);

        state.walk(0, -1);
        assert_eq!(state.tutorial, Tutorial::GoToBar);

        // Not at the bar yet: no transition.
        assert!(!state.tutorial_reached_bar());

        // Teleport next to the counter (test-only shortcut via the lobby).
        state.player_x = 28;
        state.player_y = 12;
        assert!(state.tutorial_reached_bar());
        assert_eq!(state.tutorial, Tutorial::BarLesson);
        // Only fires once.
        assert!(!state.tutorial_reached_bar());

        assert!(!state.tutorial_advance());
        assert_eq!(state.tutorial, Tutorial::SendOff);
        assert!(state.tutorial_advance());
        assert_eq!(state.tutorial, Tutorial::Done);
    }

    #[test]
    fn tutorial_skip_persists_once() {
        let mut state = state_with_lobby(true);
        state.enter_screen();
        assert!(state.tutorial_skip());
        assert_eq!(state.tutorial, Tutorial::Done);
        assert!(!state.tutorial_skip());

        let mut off = state_with_lobby(false);
        off.enter_screen();
        assert_eq!(off.tutorial, Tutorial::Off);
        assert!(!off.tutorial_skip());
    }

    #[test]
    fn returning_user_spawns_seated_not_at_the_door() {
        let mut state = state_with_lobby(false);
        state.enter_screen();
        state.refresh_roster(vec![occupant(1, "me")]);
        state.refresh_snapshot();
        let own = state.snapshot.find(Uuid::from_u128(1)).unwrap();
        assert!(matches!(
            own.placement,
            super::super::lobby::Placement::Seated(_)
        ));
    }
}
