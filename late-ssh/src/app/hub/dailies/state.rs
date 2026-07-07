use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::app::common::primitives::Banner;

use super::svc::{QuestEvent, QuestService, QuestSnapshot};

pub struct QuestState {
    user_id: Uuid,
    snapshot_rx: watch::Receiver<QuestSnapshot>,
    event_rx: broadcast::Receiver<QuestEvent>,
    snapshot: QuestSnapshot,
}

pub struct QuestTick {
    pub banner: Option<Banner>,
}

impl QuestState {
    pub fn new(
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

    pub fn tick(&mut self) -> QuestTick {
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
                    streak_reward_chips,
                    streak_bonus_level,
                } if user_id == self.user_id => {
                    let message = if streak_reward_chips > 0 {
                        let bonus_level = streak_bonus_level.unwrap_or_default();
                        format!(
                            "Quest complete: {title} (+{reward_chips} chips, streak {bonus_level} +{streak_reward_chips})"
                        )
                    } else if reward_chips > 0 {
                        format!("Quest complete: {title} (+{reward_chips} chips)")
                    } else {
                        format!("Quest complete: {title}")
                    };
                    banner = Some(Banner::success(&message));
                }
                _ => {}
            }
        }

        QuestTick { banner }
    }

    pub fn snapshot(&self) -> &QuestSnapshot {
        &self.snapshot
    }

    pub fn is_loaded(&self) -> bool {
        self.snapshot.user_id == Some(self.user_id)
    }
}
