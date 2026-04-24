use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use tokio_postgres::{Client, Row};
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ChatMessageReaction {
    pub message_id: Uuid,
    pub user_id: Uuid,
    pub kind: i16,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

impl From<Row> for ChatMessageReaction {
    fn from(row: Row) -> Self {
        Self {
            message_id: row.get("message_id"),
            user_id: row.get("user_id"),
            kind: row.get("kind"),
            created: row.get("created"),
            updated: row.get("updated"),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
pub struct ChatMessageReactionSummary {
    pub kind: i16,
    pub count: i64,
}

impl ChatMessageReaction {
    pub async fn get_by_user_and_message(
        client: &Client,
        message_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT *
                 FROM chat_message_reactions
                 WHERE message_id = $1 AND user_id = $2",
                &[&message_id, &user_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn toggle(client: &Client, message_id: Uuid, user_id: Uuid, kind: i16) -> Result<()> {
        if !(1..=8).contains(&kind) {
            bail!("reaction kind must be between 1 and 8");
        }

        let existing = Self::get_by_user_and_message(client, message_id, user_id).await?;
        match existing {
            Some(reaction) if reaction.kind == kind => {
                client
                    .execute(
                        "DELETE FROM chat_message_reactions
                         WHERE message_id = $1 AND user_id = $2",
                        &[&message_id, &user_id],
                    )
                    .await?;
            }
            Some(_) => {
                client
                    .execute(
                        "UPDATE chat_message_reactions
                         SET kind = $3, updated = current_timestamp
                         WHERE message_id = $1 AND user_id = $2",
                        &[&message_id, &user_id, &kind],
                    )
                    .await?;
            }
            None => {
                client
                    .execute(
                        "INSERT INTO chat_message_reactions (message_id, user_id, kind)
                         VALUES ($1, $2, $3)",
                        &[&message_id, &user_id, &kind],
                    )
                    .await?;
            }
        }

        Ok(())
    }

    pub async fn list_summaries_for_messages(
        client: &Client,
        message_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Vec<ChatMessageReactionSummary>>> {
        if message_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = client
            .query(
                "SELECT message_id,
                        kind,
                        COUNT(*)::bigint AS count
                 FROM chat_message_reactions
                 WHERE message_id = ANY($1)
                 GROUP BY message_id, kind
                 ORDER BY message_id, kind",
                &[&message_ids],
            )
            .await?;

        let mut summaries: HashMap<Uuid, Vec<ChatMessageReactionSummary>> = HashMap::new();
        for row in rows {
            summaries
                .entry(row.get("message_id"))
                .or_default()
                .push(ChatMessageReactionSummary {
                    kind: row.get("kind"),
                    count: row.get("count"),
                });
        }

        Ok(summaries)
    }
}
