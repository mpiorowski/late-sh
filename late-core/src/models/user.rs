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

const IGNORED_USERNAMES_KEY: &str = "ignored_usernames";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IgnoreListMutation {
    Added {
        username: String,
        ignored_usernames: Vec<String>,
    },
    AlreadyPresent {
        username: String,
        ignored_usernames: Vec<String>,
    },
    Removed {
        username: String,
        ignored_usernames: Vec<String>,
    },
    Missing {
        username: String,
        ignored_usernames: Vec<String>,
    },
}

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

    pub async fn ignored_usernames(client: &Client, user_id: Uuid) -> Result<Vec<String>> {
        let settings = Self::settings_for_user(client, user_id).await?;
        Ok(extract_ignored_usernames(&settings))
    }

    pub async fn add_ignored_username(
        client: &Client,
        user_id: Uuid,
        username: &str,
    ) -> Result<IgnoreListMutation> {
        let username = normalize_ignored_username(username)?;
        let mut settings = Self::settings_for_user(client, user_id).await?;
        let mut ignored_usernames = extract_ignored_usernames(&settings);

        if ignored_usernames.contains(&username) {
            return Ok(IgnoreListMutation::AlreadyPresent {
                username,
                ignored_usernames,
            });
        }

        ignored_usernames.push(username.clone());
        ignored_usernames.sort();
        set_ignored_usernames(&mut settings, &ignored_usernames);
        Self::update_settings(client, user_id, &settings).await?;

        Ok(IgnoreListMutation::Added {
            username,
            ignored_usernames,
        })
    }

    pub async fn remove_ignored_username(
        client: &Client,
        user_id: Uuid,
        username: &str,
    ) -> Result<IgnoreListMutation> {
        let username = normalize_ignored_username(username)?;
        let mut settings = Self::settings_for_user(client, user_id).await?;
        let mut ignored_usernames = extract_ignored_usernames(&settings);

        if !ignored_usernames.contains(&username) {
            return Ok(IgnoreListMutation::Missing {
                username,
                ignored_usernames,
            });
        }

        ignored_usernames.retain(|entry| entry != &username);
        set_ignored_usernames(&mut settings, &ignored_usernames);
        Self::update_settings(client, user_id, &settings).await?;

        Ok(IgnoreListMutation::Removed {
            username,
            ignored_usernames,
        })
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

fn normalize_ignored_username(username: &str) -> Result<String> {
    let username = username.trim().trim_start_matches('@').trim();
    if username.is_empty() {
        bail!("Username cannot be empty");
    }
    Ok(username.to_ascii_lowercase())
}

fn extract_ignored_usernames(settings: &Value) -> Vec<String> {
    let Some(entries) = settings
        .get(IGNORED_USERNAMES_KEY)
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };

    let mut normalized = BTreeSet::new();
    for entry in entries {
        if let Some(username) = entry.as_str() {
            let trimmed = username.trim();
            if !trimmed.is_empty() {
                normalized.insert(trimmed.to_ascii_lowercase());
            }
        }
    }

    normalized.into_iter().collect()
}

fn set_ignored_usernames(settings: &mut Value, ignored_usernames: &[String]) {
    if !settings.is_object() {
        *settings = json!({});
    }

    let ignored = ignored_usernames
        .iter()
        .map(|username| Value::String(username.clone()))
        .collect();
    settings[IGNORED_USERNAMES_KEY] = Value::Array(ignored);
}
