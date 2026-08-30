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
    /// draw; otherwise it must be one of the two players. Emits a single
    /// `DailyResult` event (one line per match; `match_id` keys the #lounge
    /// repeat throttle so distinct matches never collapse). A decisive result
    /// names only the winner (the loser is never resolved); a draw names both
    /// players, so `opponent_id` is only looked up on the draw path.
    pub fn daily_result_task(
        &self,
        match_id: Uuid,
        game_label: &'static str,
        challenger_id: Uuid,
        opponent_id: Uuid,
        winner_id: Option<Uuid>,
    ) {
        let publisher = self.clone();
        tokio::spawn(async move {
            let event = match winner_id {
                // Decisive: name only the winner — the loser is never resolved.
                Some(winner) => {
                    let winner_name = publisher.username_for(winner).await;
                    ActivityEvent::daily_win(winner, winner_name, game_label, match_id)
                }
                // Draw: nobody lost, so name both players.
                None => {
                    let challenger_name = publisher.username_for(challenger_id).await;
                    let opponent_name = publisher.username_for(opponent_id).await;
                    ActivityEvent::daily_draw(
                        challenger_id,
                        challenger_name,
                        opponent_name,
                        game_label,
                        match_id,
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

    pub fn username_effect_task(
        &self,
        user_id: Uuid,
        effect: late_core::models::username_effect::UsernameEffect,
        duration_secs: i64,
    ) {
        let publisher = self.clone();
        tokio::spawn(async move {
            let username = publisher.username_for(user_id).await;
            let _ = publisher.tx.send(ActivityEvent::username_effect_applied(
                user_id,
                username,
                effect,
                duration_secs,
            ));
        });
    }

    pub fn badge_rented_task(&self, user_id: Uuid, emoji: String, duration_secs: i64) {
        let publisher = self.clone();
        tokio::spawn(async move {
            let username = publisher.username_for(user_id).await;
            let _ = publisher.tx.send(ActivityEvent::badge_rented(
                user_id,
                username,
                emoji,
                duration_secs,
            ));
        });
    }

    pub fn burn_milestone_task(&self, user_id: Uuid, name: String, emoji: String, price: i64) {
        let publisher = self.clone();
        tokio::spawn(async move {
            let username = publisher.username_for(user_id).await;
            let _ = publisher.tx.send(ActivityEvent::burn_milestone(
                user_id, username, name, emoji, price,
            ));
        });
    }

    pub fn title_applied_task(&self, user_id: Uuid, title: String, duration_secs: i64) {
        let publisher = self.clone();
        tokio::spawn(async move {
            let username = publisher.username_for(user_id).await;
            let _ = publisher.tx.send(ActivityEvent::title_applied(
                user_id,
                username,
                title,
                duration_secs,
            ));
        });
    }

    /// `author_user_id` is whose message was gilded; the line is attributed
    /// to them, and nobody who paid is resolved at all.
    pub fn message_gilded_task(
        &self,
        author_user_id: Uuid,
        message_id: Uuid,
        count: i64,
        room_slug: Option<String>,
    ) {
        let publisher = self.clone();
        tokio::spawn(async move {
            let username = publisher.username_for(author_user_id).await;
            let _ = publisher.tx.send(ActivityEvent::message_gilded(
                author_user_id,
                username,
                message_id,
                count,
                room_slug,
            ));
        });
    }

    /// `from` is the deposed holder's username, already resolved by the
    /// crown service: it reads the previous reign inside the take
    /// transaction, so there is nothing left here to look up.
    pub fn crown_taken_task(
        &self,
        taker_id: Uuid,
        reign_id: Uuid,
        price: i64,
        next_price: i64,
        from: Option<String>,
    ) {
        let publisher = self.clone();
        tokio::spawn(async move {
            let username = publisher.username_for(taker_id).await;
            let _ = publisher.tx.send(ActivityEvent::crown_taken(
                taker_id, username, reign_id, price, next_price, from,
            ));
        });
    }

    /// The pot drew. `winner_id` is resolved to a username here like every
    /// other line; the odds come from the draw, which already counted them.
    pub fn pot_drawn_task(
        &self,
        winner_id: Uuid,
        pot_id: Uuid,
        payout: i64,
        winner_tickets: i64,
        total_tickets: i64,
    ) {
        let publisher = self.clone();
        tokio::spawn(async move {
            let username = publisher.username_for(winner_id).await;
            let _ = publisher.tx.send(ActivityEvent::pot_drawn(
                winner_id,
                username,
                pot_id,
                payout,
                winner_tickets,
                total_tickets,
            ));
        });
    }

    pub fn went_live_task(&self, user_id: Uuid, title: Option<String>) {
        let publisher = self.clone();
        tokio::spawn(async move {
            let username = publisher.username_for(user_id).await;
            let _ = publisher
                .tx
                .send(ActivityEvent::went_live(user_id, username, title));
        });
    }

    /// `viewer_id` is who showed up; `streamer` is the broadcaster's
    /// username, already resolved by the registry that owns the stream.
    pub fn watching_stream_task(&self, viewer_id: Uuid, streamer: String) {
        let publisher = self.clone();
        tokio::spawn(async move {
            let username = publisher.username_for(viewer_id).await;
            let _ = publisher.tx.send(ActivityEvent::watching_stream(
                viewer_id, username, streamer,
            ));
        });
    }

    pub fn cyberspace_posted_task(&self, user_id: Uuid, title: Option<String>) {
        let publisher = self.clone();
        tokio::spawn(async move {
            let username = publisher.username_for(user_id).await;
            let _ = publisher
                .tx
                .send(ActivityEvent::cyberspace_posted(user_id, username, title));
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
