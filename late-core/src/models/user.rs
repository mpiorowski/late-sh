use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use tokio_postgres::Client;
use uuid::Uuid;

crate::model! {
    table = "users";
    params = UserParams;
    struct User {
        @generated
        pub last_seen: DateTime<Utc>,
        pub is_admin: bool;

        @data
        pub fingerprint: String,
        pub username: String,
        pub settings: serde_json::Value,
    }
}

pub const USERNAME_MAX_LEN: usize = 32;

const IGNORED_USER_IDS_KEY: &str = "ignored_user_ids";
const THEME_ID_KEY: &str = "theme_id";
const NOTIFY_KINDS_KEY: &str = "notify_kinds";
const NOTIFY_BELL_KEY: &str = "notify_bell";
const NOTIFY_COOLDOWN_MINS_KEY: &str = "notify_cooldown_mins";
const ENABLE_BACKGROUND_COLOR_KEY: &str = "enable_background_color";

impl User {
    pub async fn find_by_fingerprint(client: &Client, fingerprint: &str) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM users WHERE fingerprint = $1",
                &[&fingerprint],
            )
            .await?;
        Ok(row.map(Self::from))
    }
    pub async fn update_last_seen(&mut self, client: &Client) -> Result<()> {
        self.last_seen = Utc::now();
        client
            .execute(
                &format!("UPDATE {} SET last_seen = $1 WHERE id = $2", Self::TABLE),
                &[&self.last_seen, &self.id],
            )
            .await?;
        Ok(())
    }

    pub async fn list_usernames_by_ids(
        client: &Client,
        user_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, String>> {
        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = client
            .query(
                "SELECT id, username
                 FROM users
                 WHERE id = ANY($1) AND username <> ''",
                &[&user_ids],
            )
            .await?;

        let mut usernames = HashMap::with_capacity(rows.len());
        for row in rows {
            usernames.insert(row.get("id"), row.get("username"));
        }
        Ok(usernames)
    }

    pub async fn list_all_usernames(client: &Client) -> Result<Vec<String>> {
        let rows = client
            .query(
                "SELECT username FROM users
                 WHERE username <> ''
                 ORDER BY username",
                &[],
            )
            .await?;
        Ok(rows.iter().map(|r| r.get("username")).collect())
    }

    pub async fn list_all_username_map(client: &Client) -> Result<HashMap<Uuid, String>> {
        let rows = client
            .query(
                "SELECT id, username
                 FROM users
                 WHERE username <> ''",
                &[],
            )
            .await?;
        let mut map = HashMap::with_capacity(rows.len());
        for row in rows {
            map.insert(row.get("id"), row.get("username"));
        }
        Ok(map)
    }

    pub async fn find_by_username(client: &Client, username: &str) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM users WHERE LOWER(username) = LOWER($1)",
                &[&username],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn next_available_username(client: &Client, desired: &str) -> Result<String> {
        let base_username = sanitize_username_input(desired);
        let mut candidate = base_username.clone();
        let mut suffix = 2usize;

        loop {
            let row = client
                .query_opt(
                    "SELECT 1 FROM users WHERE LOWER(username) = LOWER($1)",
                    &[&candidate],
                )
                .await?;
            if row.is_none() {
                return Ok(candidate);
            }

            let suffix_text = format!("-{suffix}");
            let max_base_len = USERNAME_MAX_LEN.saturating_sub(suffix_text.len());
            candidate = format!(
                "{}{}",
                truncate_to_boundary(&base_username, max_base_len),
                suffix_text
            );
            suffix += 1;
        }
    }

    pub async fn ignored_user_ids(client: &Client, user_id: Uuid) -> Result<Vec<Uuid>> {
        let settings = Self::settings_for_user(client, user_id).await?;
        Ok(extract_ignored_user_ids(&settings))
    }

    pub async fn theme_id(client: &Client, user_id: Uuid) -> Result<Option<String>> {
        let settings = Self::settings_for_user(client, user_id).await?;
        Ok(extract_theme_id(&settings))
    }

    /// Adds `target_id` to the ignore list. Returns `(changed, ids)` —
    /// `changed` is false if the id was already present.
    pub async fn add_ignored_user_id(
        client: &Client,
        user_id: Uuid,
        target_id: Uuid,
    ) -> Result<(bool, Vec<Uuid>)> {
        let mut settings = Self::settings_for_user(client, user_id).await?;
        let mut ids = extract_ignored_user_ids(&settings);

        if ids.contains(&target_id) {
            return Ok((false, ids));
        }

        ids.push(target_id);
        ids.sort();
        set_ignored_user_ids(&mut settings, &ids);
        Self::update_settings(client, user_id, &settings).await?;
        Ok((true, ids))
    }

    /// Removes `target_id` from the ignore list. Returns `(changed, ids)` —
    /// `changed` is false if the id was not present.
    pub async fn remove_ignored_user_id(
        client: &Client,
        user_id: Uuid,
        target_id: Uuid,
    ) -> Result<(bool, Vec<Uuid>)> {
        let mut settings = Self::settings_for_user(client, user_id).await?;
        let mut ids = extract_ignored_user_ids(&settings);

        if !ids.contains(&target_id) {
            return Ok((false, ids));
        }

        ids.retain(|entry| entry != &target_id);
        set_ignored_user_ids(&mut settings, &ids);
        Self::update_settings(client, user_id, &settings).await?;
        Ok((true, ids))
    }

    /// Atomically merge `theme_id` into `settings` without clobbering other keys.
    pub async fn set_theme_id(client: &Client, user_id: Uuid, theme_id: &str) -> Result<()> {
        let updated = client
            .execute(
                "UPDATE users
                 SET settings = settings || jsonb_build_object($1::text, $2::text),
                     updated = current_timestamp
                 WHERE id = $3",
                &[&THEME_ID_KEY, &theme_id, &user_id],
            )
            .await?;
        if updated == 0 {
            bail!("user not found");
        }
        Ok(())
    }

    async fn settings_for_user(client: &Client, user_id: Uuid) -> Result<Value> {
        let row = client
            .query_opt("SELECT settings FROM users WHERE id = $1", &[&user_id])
            .await?;
        let Some(row) = row else {
            bail!("user not found");
        };
        Ok(row.get("settings"))
    }

    async fn update_settings(client: &Client, user_id: Uuid, settings: &Value) -> Result<()> {
        let updated = client
            .execute(
                "UPDATE users
                 SET settings = $1, updated = current_timestamp
                 WHERE id = $2",
                &[settings, &user_id],
            )
            .await?;
        if updated == 0 {
            bail!("user not found");
        }
        Ok(())
    }
}

fn extract_ignored_user_ids(settings: &Value) -> Vec<Uuid> {
    let Some(entries) = settings.get(IGNORED_USER_IDS_KEY).and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut deduped = BTreeSet::new();
    for entry in entries {
        if let Some(id) = entry.as_str().and_then(|s| Uuid::parse_str(s.trim()).ok()) {
            deduped.insert(id);
        }
    }
    deduped.into_iter().collect()
}

fn set_ignored_user_ids(settings: &mut Value, ids: &[Uuid]) {
    if !settings.is_object() {
        *settings = json!({});
    }
    settings[IGNORED_USER_IDS_KEY] = json!(ids.iter().map(Uuid::to_string).collect::<Vec<_>>());
}

pub fn extract_theme_id(settings: &Value) -> Option<String> {
    settings
        .get(THEME_ID_KEY)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub fn extract_notify_kinds(settings: &Value) -> Vec<String> {
    settings
        .get(NOTIFY_KINDS_KEY)
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub fn extract_notify_bell(settings: &Value) -> bool {
    settings
        .get(NOTIFY_BELL_KEY)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

pub fn extract_notify_cooldown_mins(settings: &Value) -> i32 {
    settings
        .get(NOTIFY_COOLDOWN_MINS_KEY)
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(0) as i32
}

pub fn extract_enable_background_color(settings: &Value) -> bool {
    settings
        .get(ENABLE_BACKGROUND_COLOR_KEY)
        .and_then(Value::as_bool)
        .unwrap_or(false)
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
    fn extract_theme_id_reads_trimmed_string() {
        let settings = json!({ "theme_id": " purple " });
        assert_eq!(extract_theme_id(&settings).as_deref(), Some("purple"));
    }

    #[test]
    fn extract_theme_id_missing_returns_none() {
        let settings = json!({});
        assert_eq!(extract_theme_id(&settings), None);
    }

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
