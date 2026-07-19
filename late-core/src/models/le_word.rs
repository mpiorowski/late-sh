use anyhow::Result;
use chrono::NaiveDate;
use serde_json::Value;
use tokio_postgres::{Client, GenericClient};
use uuid::Uuid;

crate::model! {
    table = "le_word_daily_words";
    params = DailyWordParams;
    struct DailyWord {
        @data
        pub puzzle_date: NaiveDate,
        pub answer_word: String,
    }
}

crate::user_scoped_model! {
    table = "le_word_games";
    user_field = user_id;
    params = GameParams;
    struct Game {
        @data
        pub user_id: Uuid,
        pub puzzle_date: NaiveDate,
        pub answer_word: String,
        pub guesses: Value,
        pub current_guess: String,
        pub is_game_over: bool,
        pub won: bool,
    }
}

crate::user_scoped_model! {
    table = "le_word_daily_wins";
    user_field = user_id;
    params = DailyWinParams;
    struct DailyWin {
        @data
        pub user_id: Uuid,
        pub puzzle_date: NaiveDate,
        pub score: i32,
    }
}

impl DailyWord {
    pub async fn find_by_date(
        client: &impl GenericClient,
        puzzle_date: NaiveDate,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM le_word_daily_words WHERE puzzle_date = $1",
                &[&puzzle_date],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// Take the transaction-scoped advisory lock guarding daily-word creation,
    /// so concurrent `ensure_daily_word` callers serialize on the check-then-
    /// insert instead of racing. Released automatically when the transaction
    /// ends, so the caller must hold an open transaction.
    pub async fn lock_daily_creation(client: &impl GenericClient) -> Result<()> {
        client
            .query_one(
                "SELECT pg_advisory_xact_lock(hashtextextended('le_word_daily_word', 0))",
                &[],
            )
            .await?;
        Ok(())
    }

    pub async fn used_answer_words(client: &impl GenericClient) -> Result<Vec<String>> {
        let rows = client
            .query("SELECT answer_word FROM le_word_daily_words", &[])
            .await?;
        Ok(rows.into_iter().map(|row| row.get("answer_word")).collect())
    }

    pub async fn insert_for_date(
        client: &impl GenericClient,
        puzzle_date: NaiveDate,
        answer_word: &str,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO le_word_daily_words (puzzle_date, answer_word)
                 VALUES ($1, $2)
                 ON CONFLICT (puzzle_date) DO UPDATE SET answer_word = le_word_daily_words.answer_word
                 RETURNING *",
                &[&puzzle_date, &answer_word],
            )
            .await?;
        Ok(Self::from(row))
    }
}

impl Game {
    pub async fn find_by_user_id_for_date(
        client: &Client,
        user_id: Uuid,
        puzzle_date: NaiveDate,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM le_word_games WHERE user_id = $1 AND puzzle_date = $2",
                &[&user_id, &puzzle_date],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn upsert(client: &Client, params: GameParams) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO le_word_games
                   (user_id, puzzle_date, answer_word, guesses, current_guess, is_game_over, won)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT (user_id, puzzle_date) DO UPDATE SET
                   answer_word = $3,
                   guesses = $4,
                   current_guess = $5,
                   is_game_over = $6,
                   won = $7,
                   updated = current_timestamp
                 RETURNING *",
                &[
                    &params.user_id,
                    &params.puzzle_date,
                    &params.answer_word,
                    &params.guesses,
                    &params.current_guess,
                    &params.is_game_over,
                    &params.won,
                ],
            )
            .await?;
        Ok(Self::from(row))
    }
}

impl DailyWin {
    pub async fn record_win(
        client: &Client,
        user_id: Uuid,
        puzzle_date: NaiveDate,
        score: i32,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO le_word_daily_wins (user_id, puzzle_date, score)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (user_id, puzzle_date) DO UPDATE SET
                   score = LEAST(le_word_daily_wins.score, $3),
                   updated = current_timestamp
                 RETURNING *",
                &[&user_id, &puzzle_date, &score],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn has_won_today(
        client: &Client,
        user_id: Uuid,
        puzzle_date: NaiveDate,
    ) -> Result<bool> {
        let row = client
            .query_opt(
                "SELECT id FROM le_word_daily_wins WHERE user_id = $1 AND puzzle_date = $2",
                &[&user_id, &puzzle_date],
            )
            .await?;
        Ok(row.is_some())
    }
}
