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
        if let Some(row) = client
            .query_opt(
                "UPDATE pinstar_invites
                    SET uses_left = uses_left - 1,
                        updated = CURRENT_TIMESTAMP
                  WHERE id = $1
                    AND uses_left IS NOT NULL
                    AND uses_left > 0
                  RETURNING uses_left",
                &[&id],
            )
            .await?
        {
            let uses_left: i32 = row.get("uses_left");
            if uses_left == 0 {
                client
                    .execute("DELETE FROM pinstar_invites WHERE id = $1", &[&id])
                    .await?;
            }
        }
        Ok(())
    }

    /// Atomically consume an invite token and upsert the user as a member.
    pub async fn redeem(client: &Client, user_id: Uuid, token: &str) -> Result<(Uuid, String)> {
        let row = client
            .query_opt(
                "WITH consumed AS (
                    UPDATE pinstar_invites
                       SET uses_left = CASE
                           WHEN uses_left IS NULL THEN NULL
                           ELSE uses_left - 1
                       END,
                           updated = CURRENT_TIMESTAMP
                     WHERE token = $1
                       AND (expires_at IS NULL OR expires_at >= CURRENT_TIMESTAMP)
                       AND (uses_left IS NULL OR uses_left > 0)
                     RETURNING id, diagram_id, role, uses_left
                 ),
                 member AS (
                    INSERT INTO pinstar_diagram_members (diagram_id, user_id, role)
                    SELECT diagram_id, $2, role FROM consumed
                    ON CONFLICT (diagram_id, user_id) DO UPDATE
                        SET role = EXCLUDED.role,
                            updated = CURRENT_TIMESTAMP
                    RETURNING diagram_id, role
                 ),
                 delete_used AS (
                    DELETE FROM pinstar_invites
                     WHERE id IN (SELECT id FROM consumed WHERE uses_left = 0)
                 )
                 SELECT diagram_id, role FROM member LIMIT 1",
                &[&token, &user_id],
            )
            .await?
            .ok_or_else(|| anyhow::anyhow!("invite not found, expired, or exhausted"))?;

        Ok((row.get("diagram_id"), row.get("role")))
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
        if let Some(expires) = self.expires_at
            && expires < Utc::now()
        {
            return false;
        }
        // Check uses
        if let Some(uses) = self.uses_left
            && uses <= 0
        {
            return false;
        }
        true
    }
}
