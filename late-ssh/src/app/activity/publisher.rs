use late_core::{db::Db, models::profile::fetch_username};
use uuid::Uuid;

use super::{
    channel::ActivitySender,
    event::{ActivityEvent, ActivityGame},
};

#[derive(Clone)]
pub struct ActivityPublisher {
    db: Db,
    tx: ActivitySender,
}

impl ActivityPublisher {
    pub fn new(db: Db, tx: ActivitySender) -> Self {
        Self { db, tx }
    }

    pub fn game_won_task(
        &self,
        user_id: Uuid,
        game: ActivityGame,
        detail: Option<String>,
        score: Option<i32>,
    ) {
        let publisher = self.clone();
        tokio::spawn(async move {
            let Ok(client) = publisher.db.get().await else {
                tracing::warn!(%user_id, ?game, "failed to publish activity: db unavailable");
                return;
            };
            let username = fetch_username(&client, user_id).await;
            let _ = publisher.tx.send(ActivityEvent::game_won(
                user_id, username, game, detail, score,
            ));
        });
    }
}
