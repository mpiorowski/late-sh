use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

crate::user_scoped_model! {
    table = "cyberspace_accounts";
    user_field = user_id;
    params = CyberspaceAccountParams;
    struct CyberspaceAccount {
        @data
        pub user_id: Uuid,
        pub cs_user_id: String,
        pub cs_username: String,
        pub refresh_token: String,
    }
}

impl CyberspaceAccount {
    /// Link (or re-link) the user's cyberspace account. One link per user:
    /// a second login replaces the stored refresh token and identity.
    pub async fn upsert_for_user(
        client: &Client,
        user_id: Uuid,
        cs_user_id: &str,
        cs_username: &str,
        refresh_token: &str,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO cyberspace_accounts (user_id, cs_user_id, cs_username, refresh_token)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (user_id)
                 DO UPDATE SET cs_user_id = $2,
                               cs_username = $3,
                               refresh_token = $4,
                               updated = current_timestamp
                 RETURNING *",
                &[&user_id, &cs_user_id, &cs_username, &refresh_token],
            )
            .await?;
        Ok(Self::from(row))
    }

    /// Unlink: forget the account and its token. Returns true if a link existed.
    pub async fn delete_for_user(client: &Client, user_id: Uuid) -> Result<bool> {
        let n = client
            .execute(
                "DELETE FROM cyberspace_accounts WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(n > 0)
    }
}

#[cfg(test)]
#[path = "cyberspace_account_test.rs"]
mod cyberspace_account_test;
