//! The `/status` picker's state machine: which status is highlighted and
//! which duration is armed. Pure, no I/O; `input.rs` turns Enter into the
//! `App::set_status` call.

use crate::app::common::status::{SessionStatus, Status};

/// The durations the picker offers, in cycle order. `None` leads because it
/// is the safe choice: an open-ended status clears itself the next time the
/// owner posts, where a countdown outlives everything until it runs out.
///
/// A closed list rather than a free-typed number: the typed path
/// (`/status working 90`) already covers any minute count up to the cap, and
/// a picker that needs a text field stops being a two-keystroke picker.
pub(crate) const DURATIONS: [Option<u32>; 5] = [None, Some(15), Some(25), Some(50), Some(90)];

#[derive(Clone, Debug, Default)]
pub(crate) struct StatusPickerState {
    open: bool,
    /// Index into `Status::ALL`.
    status: usize,
    /// Index into [`DURATIONS`].
    duration: usize,
}

impl StatusPickerState {
    /// Open on the session's current status when there is one, so reopening
    /// the picker to change your mind starts where you left off rather than
    /// back at the top of the list.
    pub(crate) fn open(&mut self, current: Option<SessionStatus>) {
        self.open = true;
        self.status = current
            .and_then(|current| {
                Status::ALL
                    .into_iter()
                    .position(|status| status == current.status)
            })
            .unwrap_or(0);
        self.duration = 0;
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn selected_index(&self) -> usize {
        self.status
    }

    pub(crate) fn selected_status(&self) -> Status {
        Status::ALL[self.status]
    }

    pub(crate) fn duration_index(&self) -> usize {
        self.duration
    }

    pub(crate) fn selected_minutes(&self) -> Option<u32> {
        DURATIONS[self.duration]
    }

    /// Move the highlight by `delta`, wrapping at both ends: the list is
    /// eight rows, so wrapping is faster than clamping in both directions.
    pub(crate) fn move_selection(&mut self, delta: isize) {
        self.status = wrap(self.status, delta, Status::ALL.len());
    }

    pub(crate) fn cycle_duration(&mut self, delta: isize) {
        self.duration = wrap(self.duration, delta, DURATIONS.len());
    }

    /// Jump straight to a status by its 1-based row number. Out-of-range
    /// digits are ignored rather than clamped: `9` is a typo, not a request
    /// for the last row.
    pub(crate) fn select_row(&mut self, row: usize) -> bool {
        let Some(index) = row.checked_sub(1).filter(|i| *i < Status::ALL.len()) else {
            return false;
        };
        self.status = index;
        true
    }
}

fn wrap(current: usize, delta: isize, len: usize) -> usize {
    let len = len as isize;
    (((current as isize + delta) % len + len) % len) as usize
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
