use anyhow::Result;
use chrono::NaiveDate;
use tokio_postgres::Client;
use uuid::Uuid;

crate::user_scoped_model! {
    table = "rubiks_cube_daily_wins";
    user_field = user_id;
    params = DailyWinParams;
    struct DailyWin {
        @data
        pub user_id: Uuid,
        pub puzzle_date: NaiveDate,
    }
}

impl DailyWin {
    pub async fn record_win(
        client: &Client,
        user_id: Uuid,
        puzzle_date: NaiveDate,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "INSERT INTO rubiks_cube_daily_wins (user_id, puzzle_date)
                 VALUES ($1, $2)
                 ON CONFLICT (user_id, puzzle_date) DO NOTHING
                 RETURNING *",
                &[&user_id, &puzzle_date],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn has_won_today(
        client: &Client,
        user_id: Uuid,
        puzzle_date: NaiveDate,
    ) -> Result<bool> {
        let row = client
            .query_opt(
                "SELECT id FROM rubiks_cube_daily_wins WHERE user_id = $1 AND puzzle_date = $2",
                &[&user_id, &puzzle_date],
            )
            .await?;
        Ok(row.is_some())
    }
}
