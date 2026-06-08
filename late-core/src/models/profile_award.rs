use anyhow::Result;
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct ProfileAward {
    pub id: Uuid,
    pub user_id: Uuid,
    pub category: String,
    pub period_month: NaiveDate,
    pub rank: i32,
    pub score_value: i64,
    pub awarded_at: DateTime<Utc>,
}

impl ProfileAward {
    pub fn badge(&self) -> String {
        award_badge(&self.category, self.rank)
    }

    pub fn label(&self) -> &'static str {
        award_category_label(&self.category)
    }

    pub fn month_label(&self) -> String {
        month_label(self.period_month)
    }

    pub fn description(&self) -> String {
        format!(
            "{} #{} · {} · {}",
            self.label(),
            self.rank,
            format_score_value(&self.category, self.score_value),
            self.month_label()
        )
    }
}

pub async fn list_profile_awards_for_user(
    client: &Client,
    user_id: Uuid,
) -> Result<Vec<ProfileAward>> {
    let rows = client
        .query(
            "SELECT id, user_id, category, period_month, rank, score_value, awarded_at
             FROM profile_awards
             WHERE user_id = $1
             ORDER BY period_month DESC,
                      rank ASC,
                      CASE category
                        WHEN 'arcade_wins' THEN 0
                        WHEN 'top_chips' THEN 1
                        WHEN 'tetris' THEN 2
                        WHEN 'twenty_forty_eight' THEN 3
                        WHEN 'snake' THEN 4
                        ELSE 99
                      END,
                      awarded_at DESC",
            &[&user_id],
        )
        .await?;

    Ok(rows.into_iter().map(ProfileAward::from).collect())
}

pub async fn snapshot_previous_month_profile_awards(client: &Client) -> Result<u64> {
    let inserted = client
        .execute(
            "INSERT INTO profile_awards (user_id, category, period_month, rank, score_value)
             WITH bounds AS (
                SELECT
                    (date_trunc('month', now() AT TIME ZONE 'UTC')::date - INTERVAL '1 month')::date AS period_month,
                    ((date_trunc('month', now() AT TIME ZONE 'UTC')::date - INTERVAL '1 month') AT TIME ZONE 'UTC') AS period_start,
                    (date_trunc('month', now() AT TIME ZONE 'UTC')::date AT TIME ZONE 'UTC') AS period_end
             ),
             chip_totals AS (
                SELECT user_id, SUM(delta)::bigint AS value
                FROM chip_ledger, bounds
                WHERE reason NOT IN ('floor_restore', 'shop_purchase')
                  AND created_at >= bounds.period_start
                  AND created_at < bounds.period_end
                GROUP BY user_id
                HAVING SUM(delta) > 0
             ),
             arcade_wins AS (
                SELECT user_id, difficulty_key
                FROM sudoku_daily_wins, bounds
                WHERE puzzle_date >= bounds.period_month
                  AND puzzle_date < (bounds.period_month + INTERVAL '1 month')::date
                UNION ALL
                SELECT user_id, difficulty_key
                FROM nonogram_daily_wins, bounds
                WHERE puzzle_date >= bounds.period_month
                  AND puzzle_date < (bounds.period_month + INTERVAL '1 month')::date
                UNION ALL
                SELECT user_id, difficulty_key
                FROM solitaire_daily_wins, bounds
                WHERE puzzle_date >= bounds.period_month
                  AND puzzle_date < (bounds.period_month + INTERVAL '1 month')::date
                UNION ALL
                SELECT user_id, difficulty_key
                FROM minesweeper_daily_wins, bounds
                WHERE puzzle_date >= bounds.period_month
                  AND puzzle_date < (bounds.period_month + INTERVAL '1 month')::date
             ),
             arcade_totals AS (
                SELECT user_id,
                       SUM(CASE difficulty_key
                         WHEN 'easy' THEN 1
                         WHEN 'draw-1' THEN 1
                         WHEN 'medium' THEN 3
                         WHEN 'hard' THEN 5
                         WHEN 'draw-3' THEN 5
                         ELSE 1
                       END)::bigint AS value
                FROM arcade_wins
                GROUP BY user_id
             ),
             score_events AS (
                SELECT user_id, game, score
                FROM game_score_events, bounds
                WHERE game IN ('tetris', '2048', 'snake')
                  AND created_at >= bounds.period_start
                  AND created_at < bounds.period_end
                UNION ALL
                SELECT user_id, 'tetris' AS game, score
                FROM tetris_high_scores, bounds
                WHERE updated >= bounds.period_start
                  AND updated < bounds.period_end
                UNION ALL
                SELECT user_id, '2048' AS game, score
                FROM twenty_forty_eight_high_scores, bounds
                WHERE updated >= bounds.period_start
                  AND updated < bounds.period_end
                UNION ALL
                SELECT user_id, 'snake' AS game, score
                FROM snake_high_scores, bounds
                WHERE updated >= bounds.period_start
                  AND updated < bounds.period_end
             ),
             score_totals AS (
                SELECT user_id,
                       CASE game
                         WHEN 'tetris' THEN 'tetris'
                         WHEN '2048' THEN 'twenty_forty_eight'
                         WHEN 'snake' THEN 'snake'
                       END AS category,
                       MAX(score)::bigint AS value
                FROM score_events
                GROUP BY user_id, game
             ),
             ranked AS (
                SELECT user_id,
                       'top_chips'::text AS category,
                       value,
                       RANK() OVER (ORDER BY value DESC) AS rank
                FROM chip_totals
                UNION ALL
                SELECT user_id,
                       'arcade_wins'::text AS category,
                       value,
                       RANK() OVER (ORDER BY value DESC) AS rank
                FROM arcade_totals
                UNION ALL
                SELECT user_id,
                       category,
                       value,
                       RANK() OVER (PARTITION BY category ORDER BY value DESC) AS rank
                FROM score_totals
             )
             SELECT ranked.user_id, ranked.category, bounds.period_month, ranked.rank::int, ranked.value
             FROM ranked
             CROSS JOIN bounds
             WHERE ranked.rank <= 5
             ON CONFLICT (user_id, category, period_month)
             DO NOTHING",
            &[],
        )
        .await?;

    Ok(inserted)
}

pub fn award_badge(category: &str, rank: i32) -> String {
    let prefix = award_category_code(category);
    format!("{prefix}{rank}")
}

pub fn award_category_code(category: &str) -> &'static str {
    match category {
        "top_chips" => "LC",
        "arcade_wins" => "AW",
        "tetris" => "LA",
        "twenty_forty_eight" => "24#",
        "snake" => "SN",
        _ => "LB",
    }
}

pub fn award_category_label(category: &str) -> &'static str {
    match category {
        "top_chips" => "Top Chips",
        "arcade_wins" => "Arcade Wins",
        "tetris" => "Lateris",
        "twenty_forty_eight" => "2048",
        "snake" => "Snake",
        _ => "Leaderboard",
    }
}

pub fn award_category_priority(category: &str) -> i32 {
    match category {
        "arcade_wins" => 0,
        "top_chips" => 1,
        "tetris" => 2,
        "twenty_forty_eight" => 3,
        "snake" => 4,
        _ => 99,
    }
}

pub fn month_label(month: NaiveDate) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month_name = MONTHS
        .get(month.month0() as usize)
        .copied()
        .unwrap_or("???");
    format!("{month_name}'{:02}", month.year().rem_euclid(100))
}

pub fn format_score_value(category: &str, value: i64) -> String {
    match category {
        "top_chips" => format!("{value} chips"),
        "arcade_wins" => format!("{value} pts"),
        _ => format!("{value} score"),
    }
}

impl From<tokio_postgres::Row> for ProfileAward {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            id: row.get("id"),
            user_id: row.get("user_id"),
            category: row.get("category"),
            period_month: row.get("period_month"),
            rank: row.get("rank"),
            score_value: row.get("score_value"),
            awarded_at: row.get("awarded_at"),
        }
    }
}
