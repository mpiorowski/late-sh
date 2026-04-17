use std::collections::HashMap;

use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

use super::chips::{ChipLeader, UserChips};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeTier {
    Bronze,
    Silver,
    Gold,
}

impl BadgeTier {
    pub fn from_streak(streak: u32) -> Option<Self> {
        if streak >= 14 {
            Some(Self::Gold)
        } else if streak >= 7 {
            Some(Self::Silver)
        } else if streak >= 3 {
            Some(Self::Bronze)
        } else {
            None
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Bronze => "\u{2605}",
            Self::Silver => "\u{2605}\u{2605}",
            Self::Gold => "\u{2605}\u{2605}\u{2605}",
        }
    }

    pub fn tier_name(&self) -> &'static str {
        match self {
            Self::Bronze => "bronze",
            Self::Silver => "silver",
            Self::Gold => "gold",
        }
    }
}

#[derive(Clone)]
pub struct LeaderboardEntry {
    pub username: String,
    pub user_id: Uuid,
    pub count: u32,
}

#[derive(Clone)]
pub struct HighScoreEntry {
    pub game: &'static str,
    pub username: String,
    pub user_id: Uuid,
    pub score: i32,
}

#[derive(Clone, Default)]
pub struct LeaderboardData {
    pub today_champions: Vec<LeaderboardEntry>,
    pub streak_leaders: Vec<LeaderboardEntry>,
    pub user_streaks: HashMap<Uuid, u32>,
    pub high_scores: Vec<HighScoreEntry>,
    pub chip_leaders: Vec<ChipLeader>,
    pub user_chips: HashMap<Uuid, i64>,
}

impl LeaderboardData {
    pub fn badge_for(&self, user_id: &Uuid) -> Option<BadgeTier> {
        self.user_streaks
            .get(user_id)
            .and_then(|&s| BadgeTier::from_streak(s))
    }

    pub fn badges(&self) -> HashMap<Uuid, BadgeTier> {
        self.user_streaks
            .iter()
            .filter_map(|(&uid, &streak)| BadgeTier::from_streak(streak).map(|t| (uid, t)))
            .collect()
    }
}

pub async fn fetch_leaderboard_data(client: &Client) -> Result<LeaderboardData> {
    let (champions, streaks, high_scores, chip_leaders, all_chips) = tokio::try_join!(
        fetch_today_champions(client, 10),
        fetch_all_streaks(client),
        fetch_high_scores(client, 3),
        UserChips::top_balances(client, 10),
        UserChips::all_balances(client),
    )?;

    let user_streaks: HashMap<Uuid, u32> = streaks.iter().map(|e| (e.user_id, e.count)).collect();
    let mut streak_leaders: Vec<LeaderboardEntry> = streaks;
    streak_leaders.truncate(10);

    Ok(LeaderboardData {
        today_champions: champions,
        streak_leaders,
        user_streaks,
        high_scores,
        chip_leaders,
        user_chips: all_chips,
    })
}

async fn fetch_high_scores(client: &Client, limit: i64) -> Result<Vec<HighScoreEntry>> {
    let mut entries = Vec::new();

    // Tetris top scores
    let rows = client
        .query(
            "SELECT u.username, h.user_id, h.score
             FROM tetris_high_scores h
             JOIN users u ON u.id = h.user_id
             ORDER BY h.score DESC
             LIMIT $1",
            &[&limit],
        )
        .await?;
    for row in rows {
        entries.push(HighScoreEntry {
            game: "Tetris",
            username: row.get("username"),
            user_id: row.get("user_id"),
            score: row.get("score"),
        });
    }

    // 2048 top scores
    let rows = client
        .query(
            "SELECT u.username, h.user_id, h.score
             FROM twenty_forty_eight_high_scores h
             JOIN users u ON u.id = h.user_id
             ORDER BY h.score DESC
             LIMIT $1",
            &[&limit],
        )
        .await?;
    for row in rows {
        entries.push(HighScoreEntry {
            game: "2048",
            username: row.get("username"),
            user_id: row.get("user_id"),
            score: row.get("score"),
        });
    }

    Ok(entries)
}

async fn fetch_today_champions(client: &Client, limit: i64) -> Result<Vec<LeaderboardEntry>> {
    let rows = client
        .query(
            "WITH all_today AS (
                SELECT user_id FROM sudoku_daily_wins WHERE puzzle_date = CURRENT_DATE
                UNION ALL
                SELECT user_id FROM nonogram_daily_wins WHERE puzzle_date = CURRENT_DATE
                UNION ALL
                SELECT user_id FROM solitaire_daily_wins WHERE puzzle_date = CURRENT_DATE
                UNION ALL
                SELECT user_id FROM minesweeper_daily_wins WHERE puzzle_date = CURRENT_DATE
            )
            SELECT u.username, a.user_id, COUNT(*)::int AS wins
            FROM all_today a
            JOIN users u ON u.id = a.user_id
            GROUP BY a.user_id, u.username
            ORDER BY wins DESC
            LIMIT $1",
            &[&limit],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| LeaderboardEntry {
            username: row.get("username"),
            user_id: row.get("user_id"),
            count: row.get::<_, i32>("wins") as u32,
        })
        .collect())
}

async fn fetch_all_streaks(client: &Client) -> Result<Vec<LeaderboardEntry>> {
    let rows = client
        .query(
            "WITH all_wins AS (
                SELECT user_id, puzzle_date FROM sudoku_daily_wins
                UNION
                SELECT user_id, puzzle_date FROM nonogram_daily_wins
                UNION
                SELECT user_id, puzzle_date FROM solitaire_daily_wins
                UNION
                SELECT user_id, puzzle_date FROM minesweeper_daily_wins
            ),
            distinct_days AS (
                SELECT DISTINCT user_id, puzzle_date FROM all_wins
            ),
            with_grp AS (
                SELECT user_id, puzzle_date,
                       puzzle_date - (ROW_NUMBER() OVER (
                           PARTITION BY user_id ORDER BY puzzle_date
                       ))::int AS grp
                FROM distinct_days
            ),
            streaks AS (
                SELECT user_id, COUNT(*)::int AS streak_len, MAX(puzzle_date) AS end_date
                FROM with_grp
                GROUP BY user_id, grp
            )
            SELECT u.username, s.user_id, s.streak_len
            FROM streaks s
            JOIN users u ON u.id = s.user_id
            WHERE s.end_date >= (CURRENT_DATE - 1)
            ORDER BY s.streak_len DESC",
            &[],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| LeaderboardEntry {
            username: row.get("username"),
            user_id: row.get("user_id"),
            count: row.get::<_, i32>("streak_len") as u32,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badge_tier_thresholds() {
        assert_eq!(BadgeTier::from_streak(0), None);
        assert_eq!(BadgeTier::from_streak(2), None);
        assert_eq!(BadgeTier::from_streak(3), Some(BadgeTier::Bronze));
        assert_eq!(BadgeTier::from_streak(6), Some(BadgeTier::Bronze));
        assert_eq!(BadgeTier::from_streak(7), Some(BadgeTier::Silver));
        assert_eq!(BadgeTier::from_streak(13), Some(BadgeTier::Silver));
        assert_eq!(BadgeTier::from_streak(14), Some(BadgeTier::Gold));
        assert_eq!(BadgeTier::from_streak(100), Some(BadgeTier::Gold));
    }

    #[test]
    fn badge_labels() {
        assert_eq!(BadgeTier::Bronze.label(), "\u{2605}");
        assert_eq!(BadgeTier::Silver.label(), "\u{2605}\u{2605}");
        assert_eq!(BadgeTier::Gold.label(), "\u{2605}\u{2605}\u{2605}");
    }

    #[test]
    fn leaderboard_data_badges_filters_below_threshold() {
        let mut data = LeaderboardData::default();
        let u1 = Uuid::nil();
        let u2 = Uuid::from_u128(1);
        let u3 = Uuid::from_u128(2);
        data.user_streaks.insert(u1, 2);
        data.user_streaks.insert(u2, 7);
        data.user_streaks.insert(u3, 14);

        let badges = data.badges();
        assert_eq!(badges.len(), 2);
        assert_eq!(badges[&u2], BadgeTier::Silver);
        assert_eq!(badges[&u3], BadgeTier::Gold);
    }
}
