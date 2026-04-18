use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

use super::user::{
    User, extract_enable_background_color, extract_notify_bell, extract_notify_cooldown_mins,
    extract_notify_kinds, extract_theme_id,
};

#[derive(Clone, Debug, Default)]
pub struct Profile {
    pub username: String,
    pub notify_kinds: Vec<String>,
    pub notify_bell: bool,
    pub notify_cooldown_mins: i32,
    pub theme_id: Option<String>,
    pub enable_background_color: bool,
}

#[derive(Clone, Debug)]
pub struct ProfileParams {
    pub username: String,
    pub notify_kinds: Vec<String>,
    pub notify_bell: bool,
    pub notify_cooldown_mins: i32,
    pub enable_background_color: bool,
}

impl Profile {
    pub async fn load(client: &Client, user_id: Uuid) -> Result<Self> {
        let user = User::get(client, user_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("user not found"))?;
        Ok(Self::from_user(&user))
    }

    /// Atomic partial update — merges
    /// notify_kinds/notify_bell/notify_cooldown_mins/enable_background_color into settings via
    /// `settings || jsonb_build_object(...)`, so concurrent writes to unrelated keys (theme_id,
    /// ignored_user_ids) are preserved.
    pub async fn update(client: &Client, user_id: Uuid, params: ProfileParams) -> Result<Self> {
        let kinds_json = serde_json::to_value(&params.notify_kinds)?;
        let cooldown = params.notify_cooldown_mins.max(0);

        let row = client
            .query_opt(
                "UPDATE users
                 SET username = $1,
                     settings = settings || jsonb_build_object(
                         'notify_kinds', $2::jsonb,
                         'notify_bell', $3::bool,
                         'notify_cooldown_mins', $4::int,
                         'enable_background_color', $5::bool
                     ),
                     updated = current_timestamp
                 WHERE id = $6
                 RETURNING *",
                &[
                    &params.username,
                    &kinds_json,
                    &params.notify_bell,
                    &cooldown,
                    &params.enable_background_color,
                    &user_id,
                ],
            )
            .await?;
        let row = row.ok_or_else(|| anyhow::anyhow!("user not found"))?;
        Ok(Self::from_user(&User::from(row)))
    }

    fn from_user(user: &User) -> Self {
        Self {
            username: user.username.clone(),
            notify_kinds: extract_notify_kinds(&user.settings),
            notify_bell: extract_notify_bell(&user.settings),
            notify_cooldown_mins: extract_notify_cooldown_mins(&user.settings),
            theme_id: extract_theme_id(&user.settings),
            enable_background_color: extract_enable_background_color(&user.settings),
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
