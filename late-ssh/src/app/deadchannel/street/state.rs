//! Who stands on the night city street, derived from presence
//! (`app/presence`), and this session's part of it. Pure: no I/O, no clock
//! reads (every `now_ms` is handed in).
//!
//! The first descent puts the runner on the street and it stays there,
//! lit while the session looks at the page and dim while it is on another,
//! until the session ends (presence drops the record) or the runner stops
//! being one (`leave`, from the runner-directory edge in `tick.rs`).

use std::collections::HashMap;

use late_core::models::presence::{PresenceRecord, StreetStand};
use uuid::Uuid;

/// One runner on the street as a session draws it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreetRunner {
    pub x: u16,
    pub y: u16,
    /// Some session of this user is looking at the street right now.
    /// Otherwise the runner stands where it was left, dim.
    pub present: bool,
}

/// What the city draws from: user id to runner, one per user.
pub type StreetView = HashMap<Uuid, StreetRunner>;

/// One runner per user across every session on every replica: the cell of
/// the latest mover (the session id breaks a same-millisecond tie, so every
/// replica picks the same), present if any session is looking.
pub fn street_view(records: &[PresenceRecord]) -> StreetView {
    let mut latest: HashMap<Uuid, (StreetStand, Uuid)> = HashMap::new();
    let mut present: Vec<Uuid> = Vec::new();
    for record in records {
        let Some(stand) = record.street else {
            continue;
        };
        if stand.present {
            present.push(record.user_id);
        }
        match latest.get(&record.user_id) {
            Some((kept, kept_session))
                if (kept.moved_at_ms, *kept_session) >= (stand.moved_at_ms, record.session_id) => {}
            Some(_) | None => {
                latest.insert(record.user_id, (stand, record.session_id));
            }
        }
    }
    latest
        .into_iter()
        .map(|(user_id, (stand, _))| {
            (
                user_id,
                StreetRunner {
                    x: stand.x,
                    y: stand.y,
                    present: present.contains(&user_id),
                },
            )
        })
        .collect()
}

/// This session's runner on the street, and its copy of everyone's.
#[derive(Debug, Default)]
pub struct StreetPresence {
    on_street: bool,
    stand: Option<StreetStand>,
    /// The derived view the renderer reads.
    pub view: StreetView,
}

impl StreetPresence {
    pub fn new() -> Self {
        Self::default()
    }

    /// The runner goes down: on the street from here until the session
    /// ends. The next `sync` puts it there.
    pub fn descend(&mut self) {
        self.on_street = true;
    }

    /// Off the street: the runner stopped being one. A later descent (a
    /// runner again) puts it back.
    pub fn leave(&mut self) {
        self.on_street = false;
        self.stand = None;
    }

    /// Where the runner stands and whether the session is looking. The
    /// move stamp only moves when the runner does (or arrives), so looking
    /// away does not win the latest-mover tie for a second device. Nothing
    /// before the first descent.
    pub fn sync(&mut self, x: u16, y: u16, present: bool, now_ms: i64) {
        if !self.on_street {
            return;
        }
        let moved_at_ms = match self.stand {
            Some(old) if (old.x, old.y) == (x, y) => old.moved_at_ms,
            Some(_) | None => now_ms,
        };
        self.stand = Some(StreetStand {
            x,
            y,
            present,
            moved_at_ms,
        });
    }

    /// This session's part of its presence record.
    pub fn stand(&self) -> Option<StreetStand> {
        self.stand
    }

    pub fn set_records(&mut self, records: &[PresenceRecord]) {
        self.view = street_view(records);
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
