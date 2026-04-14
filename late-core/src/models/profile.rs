use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

crate::user_scoped_model! {
    table = "profiles";
    user_field = user_id;
    params = ProfileParams;
    struct Profile {
        @data
        pub user_id: Uuid,
        pub username: String,
        pub enable_ghost: bool,
        pub notify_kinds: Vec<String>,
        pub notify_cooldown_mins: i32,
    }
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            id: Uuid::nil(),
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
            user_id: Uuid::nil(),
            username: String::new(),
            enable_ghost: true,
            notify_kinds: Vec::new(),
            notify_cooldown_mins: 0,
        }
    }
}

impl Profile {
    /// Find existing profile for user, or create with defaults.
    pub async fn find_or_create_by_user(client: &Client, user_id: Uuid) -> Result<Self> {
        if let Some(row) = client
            .query_opt("SELECT * FROM profiles WHERE user_id = $1", &[&user_id])
            .await?
        {
            return Ok(Self::from(row));
        }

        let username = next_available_username(client, user_id).await?;
        let row = client
            .query_one(
                "INSERT INTO profiles (user_id, username) VALUES ($1, $2)
                 ON CONFLICT (user_id) DO UPDATE SET updated = profiles.updated
                 RETURNING *",
                &[&user_id, &username],
            )
            .await?;
        Ok(Self::from(row))
    }
}

/// Look up a user's display name by user_id. Returns "someone" on failure.
pub async fn fetch_username(client: &Client, user_id: Uuid) -> String {
    client
        .query_opt(
            "SELECT username FROM profiles WHERE user_id = $1",
            &[&user_id],
        )
        .await
        .ok()
        .flatten()
        .map(|row| row.get::<_, String>("username"))
        .unwrap_or_else(|| "someone".to_string())
}

async fn next_available_username(client: &Client, user_id: Uuid) -> Result<String> {
    let base_username = client
        .query_one("SELECT username FROM users WHERE id = $1", &[&user_id])
        .await?
        .get::<_, String>("username");
    let base_username = normalize_username_seed(&base_username);

    let mut candidate = base_username.clone();
    let mut suffix = 2usize;

    loop {
        let row = client
            .query_opt(
                "SELECT 1 FROM profiles WHERE LOWER(username) = LOWER($1)",
                &[&candidate],
            )
            .await?;
        if row.is_none() {
            return Ok(candidate);
        }

        let suffix_text = format!("-{suffix}");
        let max_base_len = 32usize.saturating_sub(suffix_text.len());
        candidate = format!(
            "{}{}",
            truncate_to_boundary(&base_username, max_base_len),
            suffix_text
        );
        suffix += 1;
    }
}

fn normalize_username_seed(username: &str) -> String {
    let trimmed = username.trim();
    if trimmed.is_empty() {
        return "user".to_string();
    }

    truncate_to_boundary(trimmed, 32)
}

fn truncate_to_boundary(value: &str, max_len: usize) -> String {
    value.chars().take(max_len).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_username_seed_trims_and_falls_back() {
        assert_eq!(normalize_username_seed("  night-owl  "), "night-owl");
        assert_eq!(normalize_username_seed("   "), "user");
    }

    #[test]
    fn truncate_to_boundary_respects_char_boundaries() {
        assert_eq!(truncate_to_boundary("abcdef", 4), "abcd");
        assert_eq!(truncate_to_boundary("żółw", 3), "żół");
    }
}
