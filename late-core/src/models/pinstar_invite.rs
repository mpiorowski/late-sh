use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

crate::model! {
    table = "pinstar_invites";
    params = PinstarInviteParams;
    struct PinstarInvite {
        @data
        pub diagram_id: Uuid,
        pub token: String,
        pub role: String,
        pub uses_left: Option<i32>,
        pub expires_at: Option<DateTime<Utc>>,
    }
}

impl PinstarInvite {
    pub async fn find_by_token(client: &Client, token: &str) -> Result<Option<Self>> {
        let row = client
            .query_opt("SELECT * FROM pinstar_invites WHERE token = $1", &[&token])
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn find_by_diagram(client: &Client, diagram_id: Uuid) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT * FROM pinstar_invites WHERE diagram_id = $1 ORDER BY created DESC",
                &[&diagram_id],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn decrement_uses(client: &Client, id: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE pinstar_invites SET uses_left = uses_left - 1 WHERE id = $1 AND uses_left IS NOT NULL",
                &[&id],
            )
            .await?;
        // Delete if uses_left reached 0
        client
            .execute(
                "DELETE FROM pinstar_invites WHERE id = $1 AND uses_left = 0",
                &[&id],
            )
            .await?;
        Ok(())
    }

    pub async fn delete_expired(client: &Client) -> Result<u64> {
        let count = client
            .execute(
                "DELETE FROM pinstar_invites WHERE expires_at IS NOT NULL AND expires_at < CURRENT_TIMESTAMP",
                &[],
            )
            .await?;
        Ok(count)
    }

    pub async fn delete_by_id(client: &Client, id: Uuid) -> Result<u64> {
        Self::delete(client, id).await
    }

    /// Generate a random invite token with the `pi_` prefix.
    pub fn generate_token() -> String {
        format!("pi_{}", Uuid::new_v4().simple())
    }

    pub fn is_valid(&self) -> bool {
        // Check not expired
        if let Some(expires) = self.expires_at {
            if expires < Utc::now() {
                return false;
            }
        }
        // Check uses
        if let Some(uses) = self.uses_left {
            if uses <= 0 {
                return false;
            }
        }
        true
    }
}
