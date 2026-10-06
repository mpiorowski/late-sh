//! Nightcap's six stools, derived from presence (`app/presence`): who holds
//! which, since when, and how many drinks the house has poured them this
//! sitting. Pure: no I/O, no clock reads (every `now_ms` is handed in), so
//! every replica holding the same records draws the same row.
//!
//! A stool is part of the sitter's own presence record, never handed out.
//! Two people who take the same stool in the same breath on different
//! replicas are settled here: the earlier `sat_at_ms` keeps it (then the
//! user id), and the other session notices on its next refresh and stands
//! back up (`State::set_records`). One user on two devices holds at most
//! one stool: their earliest.

use std::collections::HashSet;
use std::time::Duration;

use late_core::models::presence::{NightcapStand, PresenceRecord};
use uuid::Uuid;

pub const SEAT_COUNT: usize = 6;

/// One seat's occupant as shown to the renderer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeatView {
    pub user_id: Uuid,
    pub username: String,
    /// How long they have held the stool, as of the derivation.
    pub seated_for: Duration,
    pub drinks: u32,
}

pub type Stools = [Option<SeatView>; SEAT_COUNT];

/// This session's own stool, laid over the records so its own sitting and
/// standing draw before they come back from the presence task.
#[derive(Debug, Clone, Copy)]
pub struct OwnStool<'a> {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub username: &'a str,
    pub stand: Option<NightcapStand>,
}

/// The row as every replica draws it from the same records.
pub fn stools(records: &[PresenceRecord], own: &OwnStool<'_>, now_ms: i64) -> Stools {
    let mut claims: Vec<(NightcapStand, Uuid, Uuid, &str)> = records
        .iter()
        .filter(|record| record.session_id != own.session_id)
        .filter_map(|record| {
            record.nightcap.map(|stand| {
                (
                    stand,
                    record.user_id,
                    record.session_id,
                    record.username.as_str(),
                )
            })
        })
        .chain(
            own.stand
                .map(|stand| (stand, own.user_id, own.session_id, own.username)),
        )
        .filter(|(stand, ..)| usize::from(stand.stool) < SEAT_COUNT)
        .collect();
    claims.sort_by_key(|(stand, user_id, session_id, _)| (stand.sat_at_ms, *user_id, *session_id));

    let mut row: Stools = std::array::from_fn(|_| None);
    let mut seated: HashSet<Uuid> = HashSet::new();
    for (stand, user_id, _, username) in claims {
        let slot = &mut row[usize::from(stand.stool)];
        if slot.is_some() || seated.contains(&user_id) {
            continue;
        }
        seated.insert(user_id);
        *slot = Some(SeatView {
            user_id,
            username: username.to_string(),
            seated_for: Duration::from_millis((now_ms - stand.sat_at_ms).max(0) as u64),
            drinks: stand.drinks,
        });
    }
    row
}

#[cfg(test)]
#[path = "stools_test.rs"]
mod stools_test;
