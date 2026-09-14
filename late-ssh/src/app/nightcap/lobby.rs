//! Shared seat state for Nightcap, the small bar out back of the Clubhouse.
//! Deliberately much smaller than the Clubhouse's `SharedLobby`: there is no
//! walking, no floor plan, no drunk-decay tracking against the DB — just a
//! fixed row of seats, who is in which one, and a per-session drink count
//! for flavor. Wiring "order a drink" into the real chip economy
//! (`app::games::chips::svc::ChipService::buy_drink`) is a natural
//! follow-up if this room earns its keep; for now it is cosmetic.

use std::sync::{Arc, Mutex};

use late_core::MutexRecover;
use uuid::Uuid;

pub const SEAT_COUNT: usize = 6;

struct Occupant {
    user_id: Uuid,
    username: String,
    drinks: u32,
}

#[derive(Default)]
struct SeatsInner {
    seats: [Option<Occupant>; SEAT_COUNT],
}

impl SeatsInner {
    fn seat_of(&self, user_id: Uuid) -> Option<usize> {
        self.seats
            .iter()
            .position(|slot| slot.as_ref().is_some_and(|o| o.user_id == user_id))
    }
}

/// One seat's occupant as shown to the renderer.
#[derive(Clone)]
pub struct SeatView {
    pub username: String,
    pub drinks: u32,
}

#[derive(Clone)]
pub struct SharedSeats {
    inner: Arc<Mutex<SeatsInner>>,
}

impl Default for SharedSeats {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedSeats {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(SeatsInner::default())),
        }
    }

    /// Drop anyone no longer in the live human roster (disconnected).
    pub fn sync(&self, roster_ids: &[Uuid]) {
        let mut inner = self.inner.lock_recover();
        for slot in inner.seats.iter_mut() {
            let stale = slot
                .as_ref()
                .is_some_and(|occupant| !roster_ids.contains(&occupant.user_id));
            if stale {
                *slot = None;
            }
        }
    }

    /// Sit in the given seat if it's free, standing up from any other seat
    /// first. Pressing your own seat again stands you up — the whole
    /// interaction model is "press a seat's number to sit there or leave
    /// it." Returns `false` only when the seat is out of range or already
    /// taken by someone else.
    pub fn toggle_seat(&self, user_id: Uuid, username: &str, seat: usize) -> bool {
        if seat >= SEAT_COUNT {
            return false;
        }
        let mut inner = self.inner.lock_recover();
        if inner.seats[seat]
            .as_ref()
            .is_some_and(|o| o.user_id == user_id)
        {
            inner.seats[seat] = None;
            return true;
        }
        if inner.seats[seat].is_some() {
            return false;
        }
        if let Some(current) = inner.seat_of(user_id) {
            inner.seats[current] = None;
        }
        inner.seats[seat] = Some(Occupant {
            user_id,
            username: username.to_string(),
            drinks: 0,
        });
        true
    }

    pub fn seat_of(&self, user_id: Uuid) -> Option<usize> {
        self.inner.lock_recover().seat_of(user_id)
    }

    /// Order a round. Only works while seated; returns the new drink count.
    pub fn order_drink(&self, user_id: Uuid) -> Option<u32> {
        let mut inner = self.inner.lock_recover();
        let seat = inner.seat_of(user_id)?;
        let occupant = inner.seats[seat].as_mut()?;
        occupant.drinks += 1;
        Some(occupant.drinks)
    }

    pub fn snapshot(&self) -> [Option<SeatView>; SEAT_COUNT] {
        let inner = self.inner.lock_recover();
        std::array::from_fn(|i| {
            inner.seats[i].as_ref().map(|o| SeatView {
                username: o.username.clone(),
                drinks: o.drinks,
            })
        })
    }
}

#[cfg(test)]
#[path = "lobby_test.rs"]
mod lobby_test;
