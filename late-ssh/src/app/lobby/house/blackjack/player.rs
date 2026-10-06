use std::collections::HashMap;

use late_core::{db::Db, models::blackjack::BlackjackPlayer};
use uuid::Uuid;

#[derive(Clone)]
pub struct BlackjackPlayerDirectory {
    db: Db,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlackjackPlayerInfo {
    pub user_id: Uuid,
    pub username: String,
    pub balance: i64,
}

impl BlackjackPlayerDirectory {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn player_info(&self, user_id: Uuid) -> anyhow::Result<BlackjackPlayerInfo> {
        let mut infos = self.player_infos(&[user_id]).await?;
        infos
            .remove(&user_id)
            .ok_or_else(|| anyhow::anyhow!("player not found: {user_id}"))
    }

    async fn player_infos(
        &self,
        user_ids: &[Uuid],
    ) -> anyhow::Result<HashMap<Uuid, BlackjackPlayerInfo>> {
        let client = self.db.get().await?;
        BlackjackPlayer::find_many(&client, user_ids)
            .await
            .map(|players| {
                players
                    .into_iter()
                    .map(|(id, player)| (id, player.into()))
                    .collect()
            })
    }
}

impl From<BlackjackPlayer> for BlackjackPlayerInfo {
    fn from(player: BlackjackPlayer) -> Self {
        Self {
            user_id: player.user_id,
            username: player.username,
            balance: player.balance,
        }
    }
}
