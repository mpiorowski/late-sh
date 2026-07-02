use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

/// One user's best normalized score (0..1000) for a single track.
pub struct TrackScore {
    pub track_key: String,
    pub score: i32,
}

/// A user's aggregate Traffic high score: the sum of their per-track bests.
pub struct HighScore {
    pub user_id: Uuid,
    pub score: i32,
}

impl TrackScore {
    /// Every per-track best the user has recorded.
    pub async fn list_for_user(client: &Client, user_id: Uuid) -> Result<Vec<TrackScore>> {
        let rows = client
            .query(
                "SELECT track_key, score FROM traffic_track_scores WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| TrackScore {
                track_key: row.get("track_key"),
                score: row.get("score"),
            })
            .collect())
    }
}

impl HighScore {
    pub async fn find_by_user_id(client: &Client, user_id: Uuid) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT user_id, score FROM traffic_high_scores WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(row.map(|row| Self {
            user_id: row.get("user_id"),
            score: row.get("score"),
        }))
    }

    /// Record a track finish: keep the best per-track score, then recompute the
    /// aggregate Traffic high score as the sum of the user's per-track bests.
    /// Returns the new aggregate total.
    ///
    /// All three statements run in one transaction so the `traffic_high_scores`
    /// aggregate can never persist a total that disagrees with the source-of-
    /// truth `traffic_track_scores` sum, even under concurrent finishes for the
    /// same user.
    pub async fn update_track_score_if_higher(
        client: &mut Client,
        user_id: Uuid,
        track_key: &str,
        new_score: i32,
    ) -> Result<i32> {
        let tx = client.transaction().await?;

        tx.execute(
            "INSERT INTO traffic_track_scores (user_id, track_key, score)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (user_id, track_key) DO UPDATE
                    SET score = GREATEST(traffic_track_scores.score, $3),
                        updated = current_timestamp",
            &[&user_id, &track_key, &new_score],
        )
        .await?;

        let total: i32 = tx
            .query_one(
                "SELECT COALESCE(SUM(score), 0)::int AS total
                 FROM traffic_track_scores WHERE user_id = $1",
                &[&user_id],
            )
            .await?
            .get("total");

        tx.execute(
            "INSERT INTO traffic_high_scores (user_id, score)
                 VALUES ($1, $2)
                 ON CONFLICT (user_id) DO UPDATE
                    SET score = $2, updated = current_timestamp",
            &[&user_id, &total],
        )
        .await?;

        tx.commit().await?;
        Ok(total)
    }

    pub async fn record_score_event(client: &Client, user_id: Uuid, total: i32) -> Result<()> {
        client
            .execute(
                "INSERT INTO game_score_events (user_id, game, score)
                 VALUES ($1, 'traffic', $2)",
                &[&user_id, &total],
            )
            .await?;
        Ok(())
    }
}
