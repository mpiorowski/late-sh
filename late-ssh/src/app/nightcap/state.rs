//! Nightcap's per-session state: which seat (if any) this user holds, the
//! latest shared snapshot for rendering, and the roster-refresh cadence.
//! See `lobby.rs` for the actual shared seat state.

use uuid::Uuid;

use super::lobby::{SEAT_COUNT, SeatChange, SeatView, SharedSeats};

/// How often (in `tick` calls) the live roster is reconciled against the
/// shared seats, mirroring the Clubhouse's own cadence.
const ROSTER_REFRESH_TICKS: u64 = 20;

pub struct State {
    lobby: Option<SharedSeats>,
    user_id: Uuid,
    username: String,
    anim_tick: u64,
    last_roster_tick: u64,
    force_roster_refresh: bool,
    snapshot: [Option<SeatView>; SEAT_COUNT],
    /// Flavor line shown in the footer after the last seat/drink action.
    pub last_message: Option<String>,
}

impl State {
    pub fn new(lobby: Option<SharedSeats>, user_id: Uuid, username: String) -> Self {
        Self {
            lobby,
            user_id,
            username,
            anim_tick: 0,
            last_roster_tick: 0,
            force_roster_refresh: true,
            snapshot: std::array::from_fn(|_| None),
            last_message: None,
        }
    }

    /// Screen entry hook: refresh the seat snapshot immediately rather than
    /// waiting for the next scheduled roster tick.
    pub fn enter_screen(&mut self) {
        self.force_roster_refresh = true;
    }

    /// Screen exit hook: hand the stool back. Esc is not the only way out
    /// (digits, Tab, `0` all leave), so this hangs off `set_screen` rather
    /// than the Esc path, the same shape the other contextual screens use.
    pub fn leave_screen(&mut self) {
        if let Some(lobby) = &self.lobby {
            lobby.vacate(self.user_id);
        }
        self.last_message = None;
        self.refresh_snapshot();
    }

    pub fn tick(&mut self, anim_tick: u64) {
        self.anim_tick = anim_tick;
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

    /// Drop anyone no longer connected from the shared seats, and relabel
    /// the patrons still here from the roster's names.
    pub fn refresh_roster(&mut self, roster: &[(Uuid, String)]) {
        if let Some(lobby) = &self.lobby {
            lobby.sync(roster);
        }
    }

    pub fn refresh_snapshot(&mut self) {
        let Some(lobby) = &self.lobby else {
            return;
        };
        self.snapshot = lobby.snapshot();
    }

    pub fn snapshot(&self) -> &[Option<SeatView>; SEAT_COUNT] {
        &self.snapshot
    }

    pub fn my_seat(&self) -> Option<usize> {
        self.lobby.as_ref()?.seat_of(self.user_id)
    }

    /// Sit in / stand from the given 0-based seat, reporting what the press
    /// did. A stool someone else holds is the press this room bounces most
    /// often, and saying nothing there is indistinguishable from a key that
    /// never arrived.
    pub fn toggle_seat(&mut self, seat: usize) {
        let Some(lobby) = &self.lobby else {
            return;
        };
        self.last_message = match lobby.toggle_seat(self.user_id, &self.username, seat) {
            // Sitting and standing show themselves: the seat row picks up
            // (or drops) the `(you)` label on the next draw.
            SeatChange::SatDown | SeatChange::StoodUp => None,
            SeatChange::Taken => Some("that stool is taken.".to_string()),
            // Not reachable from the keymap, which only sends `1`-`6`; the
            // variant exists because `SharedSeats` bounds-checks for any
            // caller, not just this one.
            SeatChange::OutOfRange => None,
        };
        self.refresh_snapshot();
    }

    pub fn order_drink(&mut self) {
        let Some(lobby) = &self.lobby else {
            return;
        };
        self.last_message = Some(match lobby.order_drink(self.user_id) {
            Some(1) => "you order a drink.".to_string(),
            Some(count) => format!("you order round {count}."),
            None => "take a seat first.".to_string(),
        });
        self.refresh_snapshot();
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
