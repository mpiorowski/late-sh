//! Nightcap's per-session state: which seat (if any) this user holds, the
//! latest shared snapshot for rendering, and the roster-refresh cadence.
//! See `lobby.rs` for the actual shared seat state.

use uuid::Uuid;

use super::lobby::{SEAT_COUNT, SeatView, SharedSeats};

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

    /// Drop anyone no longer connected from the shared seats.
    pub fn refresh_roster(&mut self, roster_ids: &[Uuid]) {
        if let Some(lobby) = &self.lobby {
            lobby.sync(roster_ids);
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

    /// Sit in / stand from the given 0-based seat.
    pub fn toggle_seat(&mut self, seat: usize) {
        let Some(lobby) = &self.lobby else {
            return;
        };
        lobby.toggle_seat(self.user_id, &self.username, seat);
        self.refresh_snapshot();
        self.last_message = None;
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
