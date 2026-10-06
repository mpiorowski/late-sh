use anyhow::{Result, bail};
use deadpool_postgres::GenericClient;
use std::collections::HashMap;
use tokio_postgres::Client;
use uuid::Uuid;

pub const TARGET_CHAT_ROOM: &str = "chat_room";

crate::model! {
    table = "voice_channels";
    params = VoiceChannelParams;
    struct VoiceChannel {
        @data
        pub target_kind: String,
        pub target_id: Uuid,
        pub enabled: bool,
        pub display_name: String,
    }
}

impl VoiceChannel {
    pub async fn find_by_id(client: &Client, id: Uuid) -> Result<Option<Self>> {
        let row = client
            .query_opt("SELECT * FROM voice_channels WHERE id = $1", &[&id])
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn find_enabled_by_id(client: &Client, id: Uuid) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM voice_channels WHERE id = $1 AND enabled = true",
                &[&id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn find_for_target(
        client: &impl GenericClient,
        target_kind: &str,
        target_id: Uuid,
    ) -> Result<Option<Self>> {
        validate_target_kind(target_kind)?;
        let row = client
            .query_opt(
                "SELECT *
                 FROM voice_channels
                 WHERE target_kind = $1 AND target_id = $2",
                &[&target_kind, &target_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn upsert_for_target(
        client: &impl GenericClient,
        target_kind: &str,
        target_id: Uuid,
        display_name: &str,
        enabled: bool,
    ) -> Result<Self> {
        validate_target_kind(target_kind)?;
        let display_name = display_name.trim();
        if display_name.is_empty() {
            bail!("voice channel display name is required");
        }
        let row = client
            .query_one(
                "INSERT INTO voice_channels (target_kind, target_id, display_name, enabled)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (target_kind, target_id)
                 DO UPDATE
                    SET display_name = EXCLUDED.display_name,
                        enabled = EXCLUDED.enabled,
                        updated = current_timestamp
                 RETURNING *",
                &[&target_kind, &target_id, &display_name, &enabled],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn enabled_for_chat_rooms(
        client: &Client,
        chat_room_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Self>> {
        Self::enabled_for_targets(client, TARGET_CHAT_ROOM, chat_room_ids).await
    }

    async fn enabled_for_targets(
        client: &Client,
        target_kind: &str,
        target_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Self>> {
        validate_target_kind(target_kind)?;
        if target_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = client
            .query(
                "SELECT *
                 FROM voice_channels
                 WHERE target_kind = $1
                   AND target_id = ANY($2)
                   AND enabled = true",
                &[&target_kind, &target_ids],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(Self::from)
            .map(|channel| (channel.target_id, channel))
            .collect())
    }
}

fn validate_target_kind(target_kind: &str) -> Result<()> {
    match target_kind {
        TARGET_CHAT_ROOM => Ok(()),
        _ => bail!("unknown voice target kind: {target_kind}"),
    }
}
