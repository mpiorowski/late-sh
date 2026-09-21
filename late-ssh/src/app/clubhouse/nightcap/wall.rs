//! The wall of the bar out back: what the muted TV has to show, the tab
//! board, and the lines carved into the stools. Process-global and
//! read-only for sessions: one background task (`svc.rs`) refreshes it
//! every few minutes for the whole process, and a carve updates it in
//! place the moment it lands, so nobody pays a DB read per frame or per
//! visit.

use std::sync::{Arc, Mutex};

use late_core::MutexRecover;
use late_core::models::artboard_piece::NewestPiece;
use late_core::models::chips::RoundBuyer;
use late_core::models::nightcap_carving::Carving;

use super::lobby::SEAT_COUNT;

/// Lines on the tab board.
pub const TAB_BOARD_SIZE: i64 = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WallSnapshot {
    /// The newest article's title, for the TV.
    pub headline: Option<String>,
    /// The newest piece hanging on the Artboard, for the TV.
    pub newest_piece: Option<NewestPiece>,
    /// The house's biggest round buyers, best first.
    pub tab: Vec<RoundBuyer>,
    /// What is carved into each stool, by stool index.
    pub carvings: [Option<Carving>; SEAT_COUNT],
}

impl Default for WallSnapshot {
    fn default() -> Self {
        Self {
            headline: None,
            newest_piece: None,
            tab: Vec::new(),
            carvings: std::array::from_fn(|_| None),
        }
    }
}

#[derive(Clone)]
pub struct SharedWall {
    inner: Arc<Mutex<WallSnapshot>>,
}

impl Default for SharedWall {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedWall {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(WallSnapshot::default())),
        }
    }

    pub fn snapshot(&self) -> WallSnapshot {
        self.inner.lock_recover().clone()
    }

    /// Wholesale replace from a refresh pass.
    pub fn set(&self, snapshot: WallSnapshot) {
        *self.inner.lock_recover() = snapshot;
    }

    /// A carve just landed: show it now rather than on the next refresh.
    pub fn set_carving(&self, carving: Carving) {
        let mut inner = self.inner.lock_recover();
        let Some(slot) = inner.carvings.get_mut(carving.stool as usize) else {
            return;
        };
        *slot = Some(carving);
    }
}
