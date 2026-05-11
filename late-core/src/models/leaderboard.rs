use std::collections::{HashMap, HashSet};

use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

use super::chips::{ChipLeader, UserChips};

#[derive(Clone)]
pub struct LeaderboardEntry {
    pub username: String,
    pub user_id: Uuid,
    pub count: u32,
}

#[derive(Clone)]
pub struct RankedEntry {
    pub username: String,
    pub user_id: Uuid,
    pub rank: i64,
    pub value: i64,
}

#[derive(Clone)]
pub struct HighScoreEntry {
    pub game: &'static str,
    pub username: String,
    pub user_id: Uuid,
    pub rank: i64,
    pub score: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DailyGame {
    Sudoku,
    Nonogram,
    Solitaire,
    Minesweeper,
}

#[derive(Clone, Debug, Default)]
pub struct DailyCompletionStatus {
    pub completed_games: HashSet<DailyGame>,
}

impl DailyCompletionStatus {
    pub fn completed(&self, game: DailyGame) -> bool {
        self.completed_games.contains(&game)
    }

    fn mark_completed(&mut self, game: DailyGame) {
        self.completed_games.insert(game);
    }
}

#[derive(Clone, Default)]
pub struct LeaderboardData {
    pub today_champions: Vec<LeaderboardEntry>,
    pub user_daily_statuses: HashMap<Uuid, DailyCompletionStatus>,
    pub high_scores: Vec<HighScoreEntry>,
    pub chip_leaders: Vec<ChipLeader>,
    pub user_chips: HashMap<Uuid, i64>,
    pub monthly_chip_earners: Vec<RankedEntry>,
    pub arcade_champions: Vec<RankedEntry>,
    pub monthly_tetris_high_scores: Vec<HighScoreEntry>,
    pub monthly_2048_high_scores: Vec<HighScoreEntry>,
    pub monthly_snake_high_scores: Vec<HighScoreEntry>,
}

pub async fn fetch_leaderboard_data(client: &Client) -> Result<LeaderboardData> {
    let (
        champions,
        daily_statuses,
        high_scores,
        chip_leaders,
        all_chips,
        monthly_chip_earners,
        arcade_champions,
        monthly_tetris_high_scores,
        monthly_2048_high_scores,
        monthly_snake_high_scores,
    ) = tokio::try_join!(
        fetch_today_champions(client, 10),
        fetch_today_daily_statuses(client),
        fetch_high_scores(client, 500),
        UserChips::top_balances(client, 10),
        UserChips::all_balances(client),
        fetch_monthly_chip_earners(client, 500),
        fetch_arcade_champions(client, 500),
        fetch_monthly_tetris_high_scores(client, 500),
        fetch_monthly_2048_high_scores(client, 500),
        fetch_monthly_snake_high_scores(client, 500),
    )?;

    Ok(LeaderboardData {
        today_champions: champions,
        user_daily_statuses: daily_statuses,
        high_scores,
        chip_leaders,
        user_chips: all_chips,
        monthly_chip_earners,
        arcade_champions,
        monthly_tetris_high_scores,
        monthly_2048_high_scores,
        monthly_snake_high_scores,
    })
}

async fn fetch_monthly_chip_earners(client: &Client, limit: i64) -> Result<Vec<RankedEntry>> {
    let rows = client
        .query(
            "WITH totals AS (
                SELECT user_id, SUM(delta)::bigint AS earned
                FROM chip_ledger
                WHERE delta > 0
                  AND reason <> 'floor_restore'
                  AND created_at >= date_trunc('month', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
                GROUP BY user_id
            ),
            ranked AS (
                SELECT u.username,
                       t.user_id,
                       t.earned,
                       RANK() OVER (ORDER BY t.earned DESC) AS rank
                FROM totals t
                JOIN users u ON u.id = t.user_id
            )
            SELECT username, user_id, earned, rank
            FROM ranked
            ORDER BY rank ASC, username ASC
            LIMIT $1",
            &[&limit],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| RankedEntry {
            username: row.get("username"),
            user_id: row.get("user_id"),
            rank: row.get("rank"),
            value: row.get("earned"),
        })
        .collect())
}

async fn fetch_arcade_champions(client: &Client, limit: i64) -> Result<Vec<RankedEntry>> {
    let rows = client
        .query(
            "WITH monthly AS (
                SELECT user_id, difficulty_key
                FROM sudoku_daily_wins
                WHERE puzzle_date >= date_trunc('month', now() AT TIME ZONE 'UTC')::date
                UNION ALL
                SELECT user_id, size_key AS difficulty_key
                FROM nonogram_daily_wins
                WHERE puzzle_date >= date_trunc('month', now() AT TIME ZONE 'UTC')::date
                UNION ALL
                SELECT user_id, difficulty_key
                FROM solitaire_daily_wins
                WHERE puzzle_date >= date_trunc('month', now() AT TIME ZONE 'UTC')::date
                UNION ALL
                SELECT user_id, difficulty_key
                FROM minesweeper_daily_wins
                WHERE puzzle_date >= date_trunc('month', now() AT TIME ZONE 'UTC')::date
            ),
            scored AS (
                SELECT user_id,
                       CASE difficulty_key
                         WHEN 'easy' THEN 1
                         WHEN 'draw-1' THEN 1
                         WHEN 'medium' THEN 3
                         WHEN 'hard' THEN 5
                         WHEN 'draw-3' THEN 5
                         ELSE 1
                       END AS points
                FROM monthly
            ),
            totals AS (
                SELECT user_id, SUM(points)::bigint AS points
                FROM scored
                GROUP BY user_id
            ),
            ranked AS (
                SELECT u.username,
                       t.user_id,
                       t.points,
                       RANK() OVER (ORDER BY t.points DESC) AS rank
                FROM totals t
                JOIN users u ON u.id = t.user_id
            )
            SELECT username, user_id, points, rank
            FROM ranked
            ORDER BY rank ASC, username ASC
            LIMIT $1",
            &[&limit],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| RankedEntry {
            username: row.get("username"),
            user_id: row.get("user_id"),
            rank: row.get("rank"),
            value: row.get("points"),
        })
        .collect())
}

async fn fetch_high_scores(client: &Client, limit: i64) -> Result<Vec<HighScoreEntry>> {
    let mut entries = Vec::new();

    // Tetris top scores
    let rows = client
        .query(
            "WITH ranked AS (
                SELECT u.username,
                       h.user_id,
                       h.score,
                       RANK() OVER (ORDER BY h.score DESC) AS rank
                FROM tetris_high_scores h
                JOIN users u ON u.id = h.user_id
             )
             SELECT username, user_id, score, rank
             FROM ranked
             ORDER BY rank ASC, username ASC
             LIMIT $1",
            &[&limit],
        )
        .await?;
    for row in rows {
        entries.push(HighScoreEntry {
            game: "Tetris",
            username: row.get("username"),
            user_id: row.get("user_id"),
            rank: row.get("rank"),
            score: row.get("score"),
        });
    }

    // 2048 top scores
    let rows = client
        .query(
            "WITH ranked AS (
                SELECT u.username,
                       h.user_id,
                       h.score,
                       RANK() OVER (ORDER BY h.score DESC) AS rank
                FROM twenty_forty_eight_high_scores h
                JOIN users u ON u.id = h.user_id
             )
             SELECT username, user_id, score, rank
             FROM ranked
             ORDER BY rank ASC, username ASC
             LIMIT $1",
            &[&limit],
        )
        .await?;
    for row in rows {
        entries.push(HighScoreEntry {
            game: "2048",
            username: row.get("username"),
            user_id: row.get("user_id"),
            rank: row.get("rank"),
            score: row.get("score"),
        });
    }

    // Snake top scores
    let rows = client
        .query(
            "WITH ranked AS (
                SELECT u.username,
                       h.user_id,
                       h.score,
                       RANK() OVER (ORDER BY h.score DESC) AS rank
                FROM snake_high_scores h
                JOIN users u ON u.id = h.user_id
             )
             SELECT username, user_id, score, rank
             FROM ranked
             ORDER BY rank ASC, username ASC
             LIMIT $1",
            &[&limit],
        )
        .await?;
    for row in rows {
        entries.push(HighScoreEntry {
            game: "Snake",
            username: row.get("username"),
            user_id: row.get("user_id"),
            rank: row.get("rank"),
            score: row.get("score"),
        });
    }

    Ok(entries)
}

async fn fetch_monthly_tetris_high_scores(
    client: &Client,
    limit: i64,
) -> Result<Vec<HighScoreEntry>> {
    fetch_monthly_score_board(client, "Tetris", "tetris", "tetris_high_scores", limit).await
}

async fn fetch_monthly_2048_high_scores(
    client: &Client,
    limit: i64,
) -> Result<Vec<HighScoreEntry>> {
    fetch_monthly_score_board(
        client,
        "2048",
        "2048",
        "twenty_forty_eight_high_scores",
        limit,
    )
    .await
}

async fn fetch_monthly_snake_high_scores(
    client: &Client,
    limit: i64,
) -> Result<Vec<HighScoreEntry>> {
    fetch_monthly_score_board(client, "Snake", "snake", "snake_high_scores", limit).await
}

async fn fetch_monthly_score_board(
    client: &Client,
    display_game: &'static str,
    score_event_game: &str,
    legacy_table: &str,
    limit: i64,
) -> Result<Vec<HighScoreEntry>> {
    let query = format!(
        "WITH scores AS (
            SELECT user_id, score
            FROM game_score_events
            WHERE game = $1
              AND created_at >= date_trunc('month', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
            UNION ALL
            SELECT user_id, score
            FROM {legacy_table}
            WHERE updated >= date_trunc('month', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'
         ),
         best AS (
            SELECT user_id, MAX(score)::int AS score
            FROM scores
            GROUP BY user_id
         ),
         ranked AS (
            SELECT u.username,
                   b.user_id,
                   b.score,
                   RANK() OVER (ORDER BY b.score DESC) AS rank
            FROM best b
            JOIN users u ON u.id = b.user_id
         )
         SELECT username, user_id, score, rank
         FROM ranked
         ORDER BY rank ASC, username ASC
         LIMIT $2"
    );
    let rows = client.query(&query, &[&score_event_game, &limit]).await?;

    Ok(rows
        .into_iter()
        .map(|row| HighScoreEntry {
            game: display_game,
            username: row.get("username"),
            user_id: row.get("user_id"),
            rank: row.get("rank"),
            score: row.get("score"),
        })
        .collect())
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

async fn fetch_today_daily_statuses(
    client: &Client,
) -> Result<HashMap<Uuid, DailyCompletionStatus>> {
    let rows = client
        .query(
            "WITH all_today AS (
                SELECT DISTINCT user_id, 'sudoku' AS game
                FROM sudoku_daily_wins
                WHERE puzzle_date = CURRENT_DATE
                UNION ALL
                SELECT DISTINCT user_id, 'nonogram' AS game
                FROM nonogram_daily_wins
                WHERE puzzle_date = CURRENT_DATE
                UNION ALL
                SELECT DISTINCT user_id, 'solitaire' AS game
                FROM solitaire_daily_wins
                WHERE puzzle_date = CURRENT_DATE
                UNION ALL
                SELECT DISTINCT user_id, 'minesweeper' AS game
                FROM minesweeper_daily_wins
                WHERE puzzle_date = CURRENT_DATE
            )
            SELECT user_id, game FROM all_today",
            &[],
        )
        .await?;

    let mut statuses: HashMap<Uuid, DailyCompletionStatus> = HashMap::new();
    for row in rows {
        let user_id: Uuid = row.get("user_id");
        let game = match row.get::<_, &str>("game") {
            "sudoku" => DailyGame::Sudoku,
            "nonogram" => DailyGame::Nonogram,
            "solitaire" => DailyGame::Solitaire,
            "minesweeper" => DailyGame::Minesweeper,
            _ => continue,
        };
        statuses.entry(user_id).or_default().mark_completed(game);
    }

    Ok(statuses)
}
