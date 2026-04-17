use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

use super::user::{
    User, extract_enable_ghost, extract_notify_cooldown_mins, extract_notify_kinds,
    set_enable_ghost, set_notify_cooldown_mins, set_notify_kinds,
};

pub const USERNAME_MAX_LEN: usize = 32;

#[derive(Clone, Debug)]
pub struct Profile {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    pub user_id: Uuid,
    pub username: String,
    pub enable_ghost: bool,
    pub notify_kinds: Vec<String>,
    pub notify_cooldown_mins: i32,
}

#[derive(Clone, Debug)]
pub struct ProfileParams {
    pub user_id: Uuid,
    pub username: String,
    pub enable_ghost: bool,
    pub notify_kinds: Vec<String>,
    pub notify_cooldown_mins: i32,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            id: Uuid::nil(),
            created: Utc::now(),
            updated: Utc::now(),
            user_id: Uuid::nil(),
            username: String::new(),
            enable_ghost: true,
            notify_kinds: Vec::new(),
            notify_cooldown_mins: 0,
        }
    }
}

impl Profile {
    pub async fn find_or_create_by_user(client: &Client, user_id: Uuid) -> Result<Self> {
        let user = User::get(client, user_id).await?.ok_or_else(|| anyhow::anyhow!("User not found"))?;
        Ok(Self::from_user(user))
    }

    pub async fn update_by_user_id(
        client: &Client,
        user_id: Uuid,
        id: Uuid,
        params: ProfileParams,
    ) -> Result<Self> {
        if params.user_id != user_id || id != user_id {
            bail!("Profile not found");
        }

        let existing = User::get(client, user_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("User not found"))?;
        let mut settings = existing.settings.clone();
        set_enable_ghost(&mut settings, params.enable_ghost);
        set_notify_kinds(&mut settings, &params.notify_kinds);
        set_notify_cooldown_mins(&mut settings, params.notify_cooldown_mins);

        let row = client
            .query_opt(
                "UPDATE users
                 SET username = $1,
                     settings = $2,
                     updated = current_timestamp
                 WHERE id = $3 AND id = $4
                 RETURNING *",
                &[&params.username, &settings, &user_id, &id],
            )
            .await?;
        let Some(row) = row else {
            bail!("Profile not found");
        };
        Ok(Self::from_user(User::from(row)))
    }

    fn from_user(user: User) -> Self {
        Self {
            id: user.id,
            created: user.created,
            updated: user.updated,
            user_id: user.id,
            username: user.username.clone(),
            enable_ghost: extract_enable_ghost(&user.settings),
            notify_kinds: extract_notify_kinds(&user.settings),
            notify_cooldown_mins: extract_notify_cooldown_mins(&user.settings),
        }
    }
}

/// Look up a user's display name by user_id. Returns "someone" on failure.
pub async fn fetch_username(client: &Client, user_id: Uuid) -> String {
    client
        .query_opt("SELECT username FROM users WHERE id = $1", &[&user_id])
        .await
        .ok()
        .flatten()
        .map(|row| row.get::<_, String>("username"))
        .filter(|username| !username.trim().is_empty())
        .unwrap_or_else(|| "someone".to_string())
}

pub fn sanitize_username_input(username: &str) -> String {
    let trimmed = username.trim();
    if trimmed.is_empty() {
        return "user".to_string();
    }

    let mut normalized = String::with_capacity(trimmed.len());
    let mut previous_was_separator = false;

    for ch in trimmed.chars() {
        if ch == '@' {
            continue;
        }
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
            normalized.push(ch);
            previous_was_separator = false;
        } else if !previous_was_separator {
            normalized.push('_');
            previous_was_separator = true;
        }
    }

    let normalized = normalized.trim_matches('_');
    if normalized.is_empty() {
        return "user".to_string();
    }

    let truncated = truncate_to_boundary(normalized, USERNAME_MAX_LEN);
    let truncated = truncated.trim_matches('_');
    if truncated.is_empty() {
        "user".to_string()
    } else {
        truncated.to_string()
    }
}

fn truncate_to_boundary(value: &str, max_len: usize) -> String {
    value.chars().take(max_len).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_username_input_trims_and_falls_back() {
        assert_eq!(sanitize_username_input("  night-owl  "), "night-owl");
        assert_eq!(sanitize_username_input("   "), "user");
    }

    #[test]
    fn sanitize_username_input_replaces_spaces_and_invalid_chars() {
        assert_eq!(sanitize_username_input("  night owl  "), "night_owl");
        assert_eq!(sanitize_username_input("alice!!!bob"), "alice_bob");
        assert_eq!(sanitize_username_input("@alice"), "alice");
        assert_eq!(sanitize_username_input("a@b"), "ab");
        assert_eq!(sanitize_username_input("...alice..."), "...alice...");
    }

    #[test]
    fn sanitize_username_input_collapses_repeated_separators() {
        assert_eq!(sanitize_username_input("a   b\t\tc"), "a_b_c");
        assert_eq!(sanitize_username_input("a@@@b###c"), "ab_c");
    }

    #[test]
    fn truncate_to_boundary_respects_char_boundaries() {
        assert_eq!(truncate_to_boundary("abcdef", 4), "abcd");
        assert_eq!(truncate_to_boundary("żółw", 3), "żół");
    }
}
