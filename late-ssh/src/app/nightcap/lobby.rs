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

/// What a seat press did. The caller says it out loud, so a press that
/// bounces off an occupied stool is never silent: in a six-stool room that
/// is the common case, and no feedback there reads as a dropped key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatChange {
    SatDown,
    StoodUp,
    Taken,
    OutOfRange,
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

    /// Reconcile the stools against the live human roster: drop anyone who
    /// disconnected, and relabel everyone still here. The roster owns the
    /// name (root `CONTEXT.md` §8.1: no per-feature username caches for seat
    /// labels), so a rename lands on the stool instead of sitting stale
    /// until its owner stands up and sits back down.
    pub fn sync(&self, roster: &[(Uuid, String)]) {
        let mut inner = self.inner.lock_recover();
        for slot in inner.seats.iter_mut() {
            let Some(seated) = slot.as_ref().map(|occupant| occupant.user_id) else {
                continue;
            };
            match roster.iter().find(|(id, _)| *id == seated) {
                Some((_, username)) => {
                    if let Some(occupant) = slot.as_mut() {
                        occupant.username.clone_from(username);
                    }
                }
                None => *slot = None,
            }
        }
    }

    /// Give up this user's stool, if they hold one. A seat is only held
    /// while its owner is in the room: leaving the screen is a departure,
    /// not a reservation. Without this a stool is a durable claim that lives
    /// until the session disconnects, and six of them close the bar for
    /// everyone else.
    pub fn vacate(&self, user_id: Uuid) {
        let mut inner = self.inner.lock_recover();
        if let Some(seat) = inner.seat_of(user_id) {
            inner.seats[seat] = None;
        }
    }

    /// Sit in the given seat if it's free, standing up from any other seat
    /// first. Pressing your own seat again stands you up: the whole
    /// interaction model is "press a seat's number to sit there or leave
    /// it."
    pub fn toggle_seat(&self, user_id: Uuid, username: &str, seat: usize) -> SeatChange {
        if seat >= SEAT_COUNT {
            return SeatChange::OutOfRange;
        }
        let mut inner = self.inner.lock_recover();
        if inner.seats[seat]
            .as_ref()
            .is_some_and(|o| o.user_id == user_id)
        {
            inner.seats[seat] = None;
            return SeatChange::StoodUp;
        }
        if inner.seats[seat].is_some() {
            return SeatChange::Taken;
        }
        if let Some(current) = inner.seat_of(user_id) {
            inner.seats[current] = None;
        }
        inner.seats[seat] = Some(Occupant {
            user_id,
            username: username.to_string(),
            drinks: 0,
        });
        SeatChange::SatDown
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
