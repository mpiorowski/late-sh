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

    pub fn game_started_task(&self, user_id: Uuid, game: ActivityGame) {
        let publisher = self.clone();
        tokio::spawn(async move {
            let username = publisher.username_for(user_id).await;
            let _ = publisher
                .tx
                .send(ActivityEvent::game_started(user_id, username, game));
        });
    }

    pub fn boss_slain_task(&self, user_id: Uuid, game: ActivityGame, boss: String) {
        let publisher = self.clone();
        tokio::spawn(async move {
            let username = publisher.username_for(user_id).await;
            let _ = publisher
                .tx
                .send(ActivityEvent::boss_slain(user_id, username, game, boss));
        });
    }

    /// Announce a finished daily match to #lounge. `winner_id` is `None` for a
    /// draw; otherwise it must be one of the two players. Resolves both names,
    /// then emits a single `DailyResult` event (one line per match).
    pub fn daily_result_task(
        &self,
        game_label: &'static str,
        challenger_id: Uuid,
        opponent_id: Uuid,
        winner_id: Option<Uuid>,
    ) {
        let publisher = self.clone();
        tokio::spawn(async move {
            let event = match winner_id {
                Some(winner) => {
                    let loser = if winner == challenger_id {
                        opponent_id
                    } else {
                        challenger_id
                    };
                    let winner_name = publisher.username_for(winner).await;
                    let loser_name = publisher.username_for(loser).await;
                    ActivityEvent::daily_win(winner, winner_name, loser_name, game_label)
                }
                None => {
                    let challenger_name = publisher.username_for(challenger_id).await;
                    let opponent_name = publisher.username_for(opponent_id).await;
                    ActivityEvent::daily_draw(
                        challenger_id,
                        challenger_name,
                        opponent_name,
                        game_label,
                    )
                }
            };
            let _ = publisher.tx.send(event);
        });
    }

    pub fn sat_down_task(&self, user_id: Uuid, game: ActivityGame) {
        let publisher = self.clone();
        tokio::spawn(async move {
            let username = publisher.username_for(user_id).await;
            let _ = publisher
                .tx
                .send(ActivityEvent::sat_down(user_id, username, game));
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
