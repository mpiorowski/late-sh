use late_core::{
    db::Db,
    models::notification::{Notification, NotificationView},
};
use tokio::sync::{broadcast, watch};
use tracing::{Instrument, info_span};
use uuid::Uuid;

use crate::app::common::mentions;

#[derive(Clone, Default)]
pub struct NotificationSnapshot {
    pub user_id: Option<Uuid>,
    pub items: Vec<NotificationView>,
}

#[derive(Clone, Debug)]
pub enum NotificationEvent {
    UnreadCountUpdated {
        user_id: Uuid,
        unread_count: i64,
        last_read_at: Option<chrono::DateTime<chrono::Utc>>,
    },
    NewMention {
        user_id: Uuid,
        unread_count: i64,
    },
}

#[derive(Clone)]
pub struct NotificationService {
    db: Db,
    snapshot_tx: watch::Sender<NotificationSnapshot>,
    snapshot_rx: watch::Receiver<NotificationSnapshot>,
    evt_tx: broadcast::Sender<NotificationEvent>,
}

impl NotificationService {
    pub fn new(db: Db) -> Self {
        let (snapshot_tx, snapshot_rx) = watch::channel(NotificationSnapshot::default());
        let (evt_tx, _) = broadcast::channel(256);

        Self {
            db,
            snapshot_tx,
            snapshot_rx,
            evt_tx,
        }
    }

    pub fn subscribe_snapshot(&self) -> watch::Receiver<NotificationSnapshot> {
        self.snapshot_rx.clone()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<NotificationEvent> {
        self.evt_tx.subscribe()
    }

    pub fn list_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = svc.do_list(user_id).await {
                    late_core::error_span!(
                        "notification_list_failed",
                        error = ?e,
                        user_id = %user_id,
                        "failed to list notifications"
                    );
                }
            }
            .instrument(info_span!("notification.list", user_id = %user_id)),
        );
    }

    async fn do_list(&self, user_id: Uuid) -> anyhow::Result<()> {
        let client = self.db.get().await?;
        let items = Notification::list_for_user(&client, user_id, 50).await?;
        self.snapshot_tx.send(NotificationSnapshot {
            user_id: Some(user_id),
            items,
        })?;
        Ok(())
    }

    pub fn refresh_unread_count_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.publish_unread_count(user_id).await {
                late_core::error_span!(
                    "notification_unread_refresh_failed",
                    error = ?e,
                    user_id = %user_id,
                    "failed to refresh notification unread count"
                );
            }
        });
    }

    async fn publish_unread_count(&self, user_id: Uuid) -> anyhow::Result<()> {
        let client = self.db.get().await?;
        let unread_count = Notification::unread_count(&client, user_id).await?;
        let last_read_at = Notification::last_read_at(&client, user_id).await?;
        let _ = self.evt_tx.send(NotificationEvent::UnreadCountUpdated {
            user_id,
            unread_count,
            last_read_at,
        });
        Ok(())
    }

    pub fn mark_all_read_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = svc.do_mark_all_read(user_id).await {
                    late_core::error_span!(
                        "notification_mark_read_failed",
                        error = ?e,
                        user_id = %user_id,
                        "failed to mark notifications read"
                    );
                }
            }
            .instrument(info_span!(
                "notification.mark_all_read",
                user_id = %user_id
            )),
        );
    }

    async fn do_mark_all_read(&self, user_id: Uuid) -> anyhow::Result<()> {
        let client = self.db.get().await?;
        Notification::mark_all_read(&client, user_id).await?;
        let last_read_at = Notification::last_read_at(&client, user_id).await?;
        let _ = self.evt_tx.send(NotificationEvent::UnreadCountUpdated {
            user_id,
            unread_count: 0,
            last_read_at,
        });
        Ok(())
    }

    /// Parse a message body for @mentions, resolve to user IDs, insert notification rows,
    /// and broadcast unread count updates.
    pub fn create_mentions_task(
        &self,
        actor_id: Uuid,
        message_id: Uuid,
        room_id: Uuid,
        body: String,
    ) {
        let usernames = mentions::extract_mentions(&body);
        if usernames.is_empty() {
            return;
        }

        let svc = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = svc
                    .do_create_mentions(actor_id, message_id, room_id, &usernames)
                    .await
                {
                    late_core::error_span!(
                        "notification_create_mentions_failed",
                        error = ?e,
                        actor_id = %actor_id,
                        message_id = %message_id,
                        "failed to create mention notifications"
                    );
                }
            }
            .instrument(info_span!(
                "notification.create_mentions",
                actor_id = %actor_id,
                message_id = %message_id
            )),
        );
    }

    async fn do_create_mentions(
        &self,
        actor_id: Uuid,
        message_id: Uuid,
        room_id: Uuid,
        usernames: &[String],
    ) -> anyhow::Result<()> {
        let client = self.db.get().await?;

        let user_ids =
            Notification::resolve_mentioned_user_ids(&client, usernames, actor_id, room_id).await?;
        if user_ids.is_empty() {
            return Ok(());
        }

        Notification::create_mentions_batch(&client, &user_ids, actor_id, message_id, room_id)
            .await?;

        // Broadcast updated unread counts for each mentioned user.
        for &uid in &user_ids {
            let count = Notification::unread_count(&client, uid).await?;
            let _ = self.evt_tx.send(NotificationEvent::NewMention {
                user_id: uid,
                unread_count: count,
            });
        }

        Ok(())
    }
}
