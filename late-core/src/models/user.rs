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

const IGNORED_USER_IDS_KEY: &str = "ignored_user_ids";
const THEME_ID_KEY: &str = "theme_id";

impl User {
    pub async fn find_by_fingerprint(client: &Client, fingerprint: &str) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT u.id, u.created, u.updated, u.last_seen, u.is_admin, u.fingerprint, COALESCE(p.username, '') AS username, u.settings
                 FROM users u
                 LEFT JOIN profiles p ON u.id = p.user_id
                 WHERE u.fingerprint = $1",
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
                "SELECT p.user_id AS id, p.username
                 FROM profiles p
                 WHERE p.user_id = ANY($1)",
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
                "SELECT p.username FROM profiles p
                 WHERE p.username IS NOT NULL AND p.username != ''
                 ORDER BY p.username",
                &[],
            )
            .await?;
        Ok(rows.iter().map(|r| r.get("username")).collect())
    }

    pub async fn list_all_username_map(client: &Client) -> Result<HashMap<Uuid, String>> {
        let rows = client
            .query(
                "SELECT p.user_id AS id, p.username
                 FROM profiles p
                 WHERE p.username IS NOT NULL AND p.username != ''",
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
                "SELECT u.id, u.created, u.updated, u.last_seen, u.is_admin, u.fingerprint,
                        p.username AS username, u.settings
                 FROM users u
                 JOIN profiles p ON u.id = p.user_id
                 WHERE LOWER(p.username) = LOWER($1)",
                &[&username],
            )
            .await?;
        Ok(row.map(Self::from))
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

    pub async fn set_theme_id(client: &Client, user_id: Uuid, theme_id: &str) -> Result<()> {
        let mut settings = Self::settings_for_user(client, user_id).await?;
        set_theme_id(&mut settings, theme_id);
        Self::update_settings(client, user_id, &settings).await
    }

    async fn settings_for_user(client: &Client, user_id: Uuid) -> Result<Value> {
        let row = client
            .query_opt("SELECT settings FROM users WHERE id = $1", &[&user_id])
            .await?;
        let Some(row) = row else {
            bail!("User not found");
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
            bail!("User not found");
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

fn extract_theme_id(settings: &Value) -> Option<String> {
    settings
        .get(THEME_ID_KEY)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn set_theme_id(settings: &mut Value, theme_id: &str) {
    if !settings.is_object() {
        *settings = json!({});
    }
    settings[THEME_ID_KEY] = json!(theme_id);
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
    fn set_theme_id_creates_settings_object() {
        let mut settings = Value::Null;
        set_theme_id(&mut settings, "contrast");
        assert_eq!(extract_theme_id(&settings).as_deref(), Some("contrast"));
    }
}
