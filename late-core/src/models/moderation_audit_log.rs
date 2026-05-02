use anyhow::Result;
use deadpool_postgres::GenericClient;
use serde_json::Value;
use uuid::Uuid;

crate::model! {
    table = "moderation_audit_log";
    params = ModerationAuditLogParams;
    struct ModerationAuditLog {
        @data
        pub actor_user_id: Uuid,
        pub action: String,
        pub target_kind: String,
        pub target_id: Option<Uuid>,
        pub metadata: Value,
    }
}

pub struct ModerationAuditLogListItem {
    pub log: ModerationAuditLog,
    pub actor_username: Option<String>,
    pub target_username: Option<String>,
}

impl ModerationAuditLog {
    pub async fn recent_with_usernames(
        client: &tokio_postgres::Client,
        limit: i64,
    ) -> Result<Vec<ModerationAuditLogListItem>> {
        let rows = client
            .query(
                "SELECT mal.*, actor.username AS actor_username, target.username AS target_username
                 FROM moderation_audit_log mal
                 LEFT JOIN users actor ON actor.id = mal.actor_user_id
                 LEFT JOIN users target
                   ON target.id = mal.target_id AND mal.target_kind = 'user'
                 ORDER BY mal.created DESC
                 LIMIT $1",
                &[&limit],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let actor_username: Option<String> = row.get("actor_username");
                let target_username: Option<String> = row.get("target_username");
                ModerationAuditLogListItem {
                    log: Self::from(row),
                    actor_username,
                    target_username,
                }
            })
            .collect())
    }

    pub async fn record_if(
        client: &impl GenericClient,
        should_record: bool,
        actor_user_id: Uuid,
        action: impl Into<String>,
        target_kind: impl Into<String>,
        target_id: Option<Uuid>,
        metadata: Value,
    ) -> Result<Option<Self>> {
        if !should_record {
            return Ok(None);
        }

        Ok(Some(
            Self::record(
                client,
                actor_user_id,
                action,
                target_kind,
                target_id,
                metadata,
            )
            .await?,
        ))
    }

    pub async fn record(
        client: &impl GenericClient,
        actor_user_id: Uuid,
        action: impl Into<String>,
        target_kind: impl Into<String>,
        target_id: Option<Uuid>,
        metadata: Value,
    ) -> Result<Self> {
        let action = action.into();
        let target_kind = target_kind.into();
        let row = client
            .query_one(
                "INSERT INTO moderation_audit_log
                 (actor_user_id, action, target_kind, target_id, metadata)
                 VALUES ($1, $2, $3, $4, $5)
                 RETURNING *",
                &[&actor_user_id, &action, &target_kind, &target_id, &metadata],
            )
            .await?;
        Ok(Self::from(row))
    }
}
