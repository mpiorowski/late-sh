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

impl ModerationAuditLog {
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
