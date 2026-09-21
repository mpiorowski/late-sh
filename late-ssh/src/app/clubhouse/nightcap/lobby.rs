//! Shared seat state for Nightcap, the small bar out back of the Clubhouse.
//! Deliberately much smaller than the Clubhouse's `SharedLobby`: there is no
//! walking, no floor plan, and no drunk map of its own (the tavern's
//! `SharedLobby` carries drunk state for both rooms). Just a fixed row of
//! seats, who is in which one, when they sat down, and how many drinks the
//! house has actually poured them this sitting (`svc.rs` records a pour
//! only once the chips have moved).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use late_core::MutexRecover;
use uuid::Uuid;

pub const SEAT_COUNT: usize = 6;

struct Occupant {
    user_id: Uuid,
    username: String,
    sat_at: Instant,
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeatView {
    pub user_id: Uuid,
    pub username: String,
    /// How long they have held the stool, as of the snapshot.
    pub seated_for: Duration,
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
            sat_at: Instant::now(),
            drinks: 0,
        });
        SeatChange::SatDown
    }

    pub fn seat_of(&self, user_id: Uuid) -> Option<usize> {
        self.inner.lock_recover().seat_of(user_id)
    }

    /// A drink landed for this patron (chips moved, or a credit was cashed).
    /// Returns the new count for the sitting, or `None` when they stood up
    /// while the order was in flight: the drink still happened, the stool
    /// just has nobody to show it on.
    pub fn record_pour(&self, user_id: Uuid) -> Option<u32> {
        let mut inner = self.inner.lock_recover();
        let seat = inner.seat_of(user_id)?;
        let occupant = inner.seats[seat].as_mut()?;
        occupant.drinks += 1;
        Some(occupant.drinks)
    }

    /// Everyone else on a stool right now: who a round at this bar is for.
    pub fn seated_ids_excluding(&self, user_id: Uuid) -> Vec<Uuid> {
        let inner = self.inner.lock_recover();
        inner
            .seats
            .iter()
            .flatten()
            .map(|o| o.user_id)
            .filter(|id| *id != user_id)
            .collect()
    }

    pub fn snapshot(&self) -> [Option<SeatView>; SEAT_COUNT] {
        let inner = self.inner.lock_recover();
        let now = Instant::now();
        std::array::from_fn(|i| {
            inner.seats[i].as_ref().map(|o| SeatView {
                user_id: o.user_id,
                username: o.username.clone(),
                seated_for: now.saturating_duration_since(o.sat_at),
                drinks: o.drinks,
            })
        })
    }
}

#[cfg(test)]
#[path = "lobby_test.rs"]
mod lobby_test;
