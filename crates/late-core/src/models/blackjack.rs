use std::collections::HashMap;

use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

use crate::models::chips::CHIP_FLOOR;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlackjackPlayer {
    pub user_id: Uuid,
    pub username: String,
    pub balance: i64,
}

impl BlackjackPlayer {
    pub async fn find(client: &Client, user_id: Uuid) -> Result<Option<Self>> {
        let mut players = Self::find_many(client, &[user_id]).await?;
        Ok(players.remove(&user_id))
    }

    pub async fn find_many(client: &Client, user_ids: &[Uuid]) -> Result<HashMap<Uuid, Self>> {
        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = client
            .query(
                "SELECT u.id,
                        NULLIF(u.username, '') AS username,
                        COALESCE(c.balance, $2) AS balance
                 FROM users u
                 LEFT JOIN user_chips c ON c.user_id = u.id
                 WHERE u.id = ANY($1)",
                &[&user_ids, &CHIP_FLOOR],
            )
            .await?;

        let mut players = HashMap::with_capacity(rows.len());
        for row in rows {
            let user_id = row.get("id");
            players.insert(
                user_id,
                Self {
                    user_id,
                    username: row
                        .get::<_, Option<String>>("username")
                        .unwrap_or_else(|| "player".to_string()),
                    balance: row.get("balance"),
                },
            );
        }
        Ok(players)
    }
}
