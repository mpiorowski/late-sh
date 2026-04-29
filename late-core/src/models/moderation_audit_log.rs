use anyhow::Result;
use serde_json::Value;
use tokio_postgres::Client;
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
        client: &Client,
        actor_user_id: Uuid,
        action: impl Into<String>,
        target_kind: impl Into<String>,
        target_id: Option<Uuid>,
        metadata: Value,
    ) -> Result<Self> {
        Self::create(
            client,
            ModerationAuditLogParams {
                actor_user_id,
                action: action.into(),
                target_kind: target_kind.into(),
                target_id,
                metadata,
            },
        )
        .await
    }
}
