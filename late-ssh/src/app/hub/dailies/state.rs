use chrono::NaiveDate;
use late_core::models::daily_rung::STAR_GLYPH;
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::app::common::primitives::Banner;

use super::svc::{QuestEvent, QuestService, QuestSnapshot};

/// Whether today's rung is still to be won, for the arcade week's badge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RungStanding {
    Open,
    Done,
}

/// `pub` (not `pub(crate)`) because the public `ArcadeHubView` carries a
/// reference to it for the arcade quest strip.
pub struct QuestState {
    user_id: Uuid,
    snapshot_rx: watch::Receiver<QuestSnapshot>,
    event_rx: broadcast::Receiver<QuestEvent>,
    snapshot: QuestSnapshot,
}

pub(crate) struct QuestTick {
    pub banner: Option<Banner>,
    pub snapshot_changed: bool,
}

impl QuestState {
    pub(crate) fn new(
        user_id: Uuid,
        service: QuestService,
        snapshot_rx: watch::Receiver<QuestSnapshot>,
    ) -> Self {
        let snapshot = snapshot_rx.borrow().clone();
        let event_rx = service.subscribe_events();
        Self {
            user_id,
            snapshot_rx,
            event_rx,
            snapshot,
        }
    }

    pub(crate) fn tick(&mut self) -> QuestTick {
        let snapshot_changed = self.snapshot_rx.has_changed().unwrap_or(false);
        if snapshot_changed {
            self.snapshot = self.snapshot_rx.borrow_and_update().clone();
        }

        let mut banner = None;
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                QuestEvent::Completed {
                    user_id,
                    title,
                    reward_chips,
                } if user_id == self.user_id => {
                    let message = if reward_chips > 0 {
                        format!("Quest complete: {title} (+{reward_chips} chips)")
                    } else {
                        format!("Quest complete: {title}")
                    };
                    banner = Some(Banner::success(&message));
                }
                QuestEvent::RungStar { user_id, stars } if user_id == self.user_id => {
                    banner = Some(Banner::success(&format!(
                        "{STAR_GLYPH} Today's rung, done. That is star {stars}."
                    )));
                }
                QuestEvent::Completed { .. } | QuestEvent::RungStar { .. } => {}
            }
        }

        QuestTick {
            banner,
            snapshot_changed,
        }
    }

    pub(crate) fn snapshot(&self) -> &QuestSnapshot {
        &self.snapshot
    }

    /// Whether today's rung is still to be won, off the snapshot's newest
    /// star; the hub refreshes the snapshot the moment a star lands.
    pub(crate) fn rung_standing(&self, today: NaiveDate) -> RungStanding {
        match self.snapshot.latest_star == Some(today) {
            true => RungStanding::Done,
            false => RungStanding::Open,
        }
    }

    pub(crate) fn is_loaded(&self) -> bool {
        self.snapshot.user_id == Some(self.user_id)
    }

    /// Quests still open in each cadence, for the status bar's `quests`
    /// segment. Zero until the snapshot belongs to this user, so a session
    /// that has not loaded yet reads as "nothing outstanding" rather than
    /// flashing another user's count.
    pub(crate) fn open_counts(&self) -> (usize, usize) {
        if !self.is_loaded() {
            return (0, 0);
        }
        let open =
            |items: &[super::svc::QuestItem]| items.iter().filter(|q| !q.completed()).count();
        (open(&self.snapshot.daily), open(&self.snapshot.weekly))
    }
}
