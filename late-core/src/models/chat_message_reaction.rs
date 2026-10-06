use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use tokio_postgres::{Client, Row};
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ChatMessageReaction {
    pub message_id: Uuid,
    pub user_id: Uuid,
    pub icon: String,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

impl From<Row> for ChatMessageReaction {
    fn from(row: Row) -> Self {
        Self {
            message_id: row.get("message_id"),
            user_id: row.get("user_id"),
            icon: row.get("icon"),
            created: row.get("created"),
            updated: row.get("updated"),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
pub struct ChatMessageReactionSummary {
    pub icon: String,
    pub count: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ChatMessageReactionOwners {
    pub icon: String,
    pub user_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum ChatMessageReactionAction {
    React,
    Unreact,
    Replace,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ChatMessageReactionToggle {
    pub action: ChatMessageReactionAction,
    pub icon: String,
    pub previous_icon: Option<String>,
}

impl ChatMessageReaction {
    fn clean_icon(icon: &str) -> Result<&str> {
        let icon = icon.trim();
        if icon.is_empty() {
            bail!("reaction icon must not be empty");
        }
        if icon.chars().count() > 64 {
            bail!("reaction icon is too long");
        }
        Ok(icon)
    }

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

    pub async fn toggle(
        client: &Client,
        message_id: Uuid,
        user_id: Uuid,
        icon: &str,
    ) -> Result<ChatMessageReactionToggle> {
        let icon = Self::clean_icon(icon)?;

        let existing = Self::get_by_user_and_message(client, message_id, user_id).await?;
        let result = match existing {
            Some(reaction) if reaction.icon == icon => {
                client
                    .execute(
                        "DELETE FROM chat_message_reactions
                         WHERE message_id = $1 AND user_id = $2",
                        &[&message_id, &user_id],
                    )
                    .await?;
                ChatMessageReactionToggle {
                    action: ChatMessageReactionAction::Unreact,
                    icon: icon.to_string(),
                    previous_icon: Some(reaction.icon),
                }
            }
            Some(reaction) => {
                client
                    .execute(
                        "UPDATE chat_message_reactions
                         SET icon = $3, updated = current_timestamp
                         WHERE message_id = $1 AND user_id = $2",
                        &[&message_id, &user_id, &icon],
                    )
                    .await?;
                ChatMessageReactionToggle {
                    action: ChatMessageReactionAction::Replace,
                    icon: icon.to_string(),
                    previous_icon: Some(reaction.icon),
                }
            }
            None => {
                client
                    .execute(
                        "INSERT INTO chat_message_reactions (message_id, user_id, icon)
                         VALUES ($1, $2, $3)",
                        &[&message_id, &user_id, &icon],
                    )
                    .await?;
                ChatMessageReactionToggle {
                    action: ChatMessageReactionAction::React,
                    icon: icon.to_string(),
                    previous_icon: None,
                }
            }
        };

        Ok(result)
    }

    pub async fn unreact_matching(
        client: &Client,
        message_id: Uuid,
        user_id: Uuid,
        icon: &str,
    ) -> Result<Option<ChatMessageReactionToggle>> {
        let icon = Self::clean_icon(icon)?;
        let Some(reaction) = Self::get_by_user_and_message(client, message_id, user_id).await?
        else {
            return Ok(None);
        };
        if reaction.icon != icon {
            return Ok(None);
        }

        client
            .execute(
                "DELETE FROM chat_message_reactions
                 WHERE message_id = $1 AND user_id = $2",
                &[&message_id, &user_id],
            )
            .await?;

        Ok(Some(ChatMessageReactionToggle {
            action: ChatMessageReactionAction::Unreact,
            icon: icon.to_string(),
            previous_icon: Some(reaction.icon),
        }))
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
                        icon,
                        COUNT(*)::bigint AS count
                 FROM chat_message_reactions
                 WHERE message_id = ANY($1)
                 GROUP BY message_id, icon
                 ORDER BY message_id, MIN(created), icon",
                &[&message_ids],
            )
            .await?;

        let mut summaries: HashMap<Uuid, Vec<ChatMessageReactionSummary>> = HashMap::new();
        for row in rows {
            summaries
                .entry(row.get("message_id"))
                .or_default()
                .push(ChatMessageReactionSummary {
                    icon: row.get("icon"),
                    count: row.get("count"),
                });
        }

        Ok(summaries)
    }

    pub async fn list_owners_for_message(
        client: &Client,
        message_id: Uuid,
    ) -> Result<Vec<ChatMessageReactionOwners>> {
        let rows = client
            .query(
                "SELECT icon,
                        ARRAY_AGG(user_id ORDER BY created, user_id) AS user_ids
                 FROM chat_message_reactions
                 WHERE message_id = $1
                 GROUP BY icon
                 ORDER BY MIN(created), icon",
                &[&message_id],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| ChatMessageReactionOwners {
                icon: row.get("icon"),
                user_ids: row.get("user_ids"),
            })
            .collect())
    }
}
