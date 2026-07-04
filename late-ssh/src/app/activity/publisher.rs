use late_core::{db::Db, models::profile::fetch_username};
use uuid::Uuid;

use crate::usernames::UsernameDirectory;

use super::{
    channel::ActivitySender,
    event::{ActivityEvent, ActivityGame},
};

#[derive(Clone)]
pub struct ActivityPublisher {
    db: Db,
    tx: ActivitySender,
    username_directory: Option<UsernameDirectory>,
}

impl ActivityPublisher {
    pub fn new(db: Db, tx: ActivitySender) -> Self {
        Self {
            db,
            tx,
            username_directory: None,
        }
    }

    pub fn with_username_directory(mut self, username_directory: UsernameDirectory) -> Self {
        self.username_directory = Some(username_directory);
        self
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
            let username = publisher.username_for(user_id).await;
            let _ = publisher.tx.send(ActivityEvent::game_won(
                user_id, username, game, detail, score,
            ));
        });
    }

    pub fn game_event_task(&self, user_id: Uuid, game: ActivityGame, action: String) {
        let publisher = self.clone();
        tokio::spawn(async move {
            let username = publisher.username_for(user_id).await;
            let _ = publisher
                .tx
                .send(ActivityEvent::game_event(user_id, username, game, action));
        });
    }

    pub fn game_played_task(&self, user_id: Uuid, game: ActivityGame, detail: Option<String>) {
        let publisher = self.clone();
        tokio::spawn(async move {
            let username = publisher.username_for(user_id).await;
            let _ = publisher
                .tx
                .send(ActivityEvent::game_played(user_id, username, game, detail));
        });
    }

    pub fn game_scored_task(
        &self,
        user_id: Uuid,
        game: ActivityGame,
        score: i32,
        level: Option<i32>,
    ) {
        let publisher = self.clone();
        tokio::spawn(async move {
            let username = publisher.username_for(user_id).await;
            let _ = publisher.tx.send(ActivityEvent::game_scored(
                user_id, username, game, score, level,
            ));
        });
    }

    async fn username_for(&self, user_id: Uuid) -> String {
        if let Some(directory) = &self.username_directory
            && let Some(username) = crate::usernames::get(directory, user_id)
        {
            return username;
        }

        match self.db.get().await {
            Ok(client) => fetch_username(&client, user_id).await,
            Err(error) => {
                tracing::warn!(%user_id, ?error, "publishing activity with fallback username");
                "someone".to_string()
            }
        }
    }
}
