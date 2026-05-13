use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::BTreeSet;
use tokio_postgres::Client;
use uuid::Uuid;

use super::user::{
    User, extract_bio, extract_country, extract_enable_background_color, extract_favorite_room_ids,
    extract_ide, extract_langs, extract_notify_bell, extract_notify_cooldown_mins,
    extract_notify_format, extract_notify_kinds, extract_os, extract_show_dashboard_header,
    extract_show_right_sidebar, extract_show_room_list_sidebar, extract_show_settings_on_connect,
    extract_terminal, extract_theme_id, extract_timezone,
};

#[derive(Clone, Debug)]
pub struct Profile {
    pub created_at: Option<DateTime<Utc>>,
    pub username: String,
    pub bio: String,
    pub country: Option<String>,
    pub timezone: Option<String>,
    pub ide: Option<String>,
    pub terminal: Option<String>,
    pub os: Option<String>,
    pub langs: Vec<String>,
    pub notify_kinds: Vec<String>,
    pub notify_bell: bool,
    pub notify_cooldown_mins: i32,
    /// One of `"both"`, `"osc777"`, `"osc9"`. `None` falls back to `"both"`.
    pub notify_format: Option<String>,
    pub theme_id: Option<String>,
    pub enable_background_color: bool,
    /// Controls the general-room lounge chrome: top info boxes plus wire strip.
    pub show_dashboard_header: bool,
    pub show_right_sidebar: bool,
    pub show_room_list_sidebar: bool,
    /// When false, the settings modal is not auto-opened on connect.
    pub show_settings_on_connect: bool,
    /// Ordered list of room ids pinned to the dashboard quick-switch strip.
    pub favorite_room_ids: Vec<Uuid>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            created_at: None,
            username: String::new(),
            bio: String::new(),
            country: None,
            timezone: None,
            ide: None,
            terminal: None,
            os: None,
            langs: Vec::new(),
            notify_kinds: Vec::new(),
            notify_bell: false,
            notify_cooldown_mins: 0,
            notify_format: None,
            theme_id: None,
            enable_background_color: true,
            show_dashboard_header: true,
            show_right_sidebar: true,
            show_room_list_sidebar: true,
            show_settings_on_connect: true,
            favorite_room_ids: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProfileParams {
    pub username: String,
    pub bio: String,
    pub country: Option<String>,
    pub timezone: Option<String>,
    pub ide: Option<String>,
    pub terminal: Option<String>,
    pub os: Option<String>,
    pub langs: Vec<String>,
    pub notify_kinds: Vec<String>,
    pub notify_bell: bool,
    pub notify_cooldown_mins: i32,
    pub notify_format: Option<String>,
    pub theme_id: Option<String>,
    pub enable_background_color: bool,
    pub show_dashboard_header: bool,
    pub show_right_sidebar: bool,
    pub show_room_list_sidebar: bool,
    pub show_settings_on_connect: bool,
    pub favorite_room_ids: Vec<Uuid>,
}

impl Profile {
    pub async fn load(client: &Client, user_id: Uuid) -> Result<Self> {
        let user = User::get(client, user_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("user not found"))?;
        Ok(Self::from_user(&user))
    }

    /// Atomic partial update — merges
    /// bio/country/timezone/theme_id/notify_kinds/notify_bell/notify_cooldown_mins/
    /// enable_background_color/show_dashboard_header/show_right_sidebar/
    /// show_room_list_sidebar/show_settings_on_connect into settings via
    /// `settings || jsonb_build_object(...)`, so concurrent writes to unrelated keys
    /// (ignored_user_ids) are preserved.
    pub async fn update(client: &Client, user_id: Uuid, params: ProfileParams) -> Result<Self> {
        let kinds_json = serde_json::to_value(&params.notify_kinds)?;
        let favorite_room_ids_json = serde_json::to_value(
            params
                .favorite_room_ids
                .iter()
                .map(Uuid::to_string)
                .collect::<Vec<_>>(),
        )?;
        let cooldown = params.notify_cooldown_mins.max(0);
        let bio = params.bio.trim().to_string();
        let country = params
            .country
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_uppercase());
        let timezone = params
            .timezone
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        let ide = normalize_profile_text(params.ide.as_deref());
        let terminal = normalize_profile_text(params.terminal.as_deref());
        let os = normalize_profile_text(params.os.as_deref());
        let langs = normalize_profile_tags(params.langs.iter().map(String::as_str));
        let langs_json = serde_json::to_value(&langs)?;
        let current_user = User::get(client, user_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("user not found"))?;
        let theme_id = params
            .theme_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .or_else(|| extract_theme_id(&current_user.settings))
            .unwrap_or_else(|| "contrast".to_string());
        let notify_format = params
            .notify_format
            .as_deref()
            .map(str::trim)
            .filter(|value| matches!(*value, "both" | "osc777" | "osc9"))
            .map(ToString::to_string)
            .or_else(|| extract_notify_format(&current_user.settings))
            .unwrap_or_else(|| "both".to_string());

        let row = client
            .query_opt(
                "UPDATE users
                 SET username = $1,
                     settings = settings || jsonb_build_object(
                         'bio', $2::text,
                         'country', $3::text,
                         'timezone', $4::text,
                         'notify_kinds', $5::jsonb,
                         'notify_bell', $6::bool,
                         'notify_cooldown_mins', $7::int,
                         'theme_id', $8::text,
                         'enable_background_color', $9::bool,
                         'notify_format', $10::text,
                         'show_dashboard_header', $11::bool,
                         'show_right_sidebar', $12::bool,
                         'show_room_list_sidebar', $13::bool,
                         'show_settings_on_connect', $14::bool,
                         'favorite_room_ids', $15::jsonb,
                         'ide', $16::text,
                         'terminal', $17::text,
                         'os', $18::text,
                         'langs', $19::jsonb
                     ),
                     updated = current_timestamp
                 WHERE id = $20
                 RETURNING *",
                &[
                    &params.username,
                    &bio,
                    &country,
                    &timezone,
                    &kinds_json,
                    &params.notify_bell,
                    &cooldown,
                    &theme_id,
                    &params.enable_background_color,
                    &notify_format,
                    &params.show_dashboard_header,
                    &params.show_right_sidebar,
                    &params.show_room_list_sidebar,
                    &params.show_settings_on_connect,
                    &favorite_room_ids_json,
                    &ide,
                    &terminal,
                    &os,
                    &langs_json,
                    &user_id,
                ],
            )
            .await?;
        let row = row.ok_or_else(|| anyhow::anyhow!("user not found"))?;
        Ok(Self::from_user(&User::from(row)))
    }

    fn from_user(user: &User) -> Self {
        Self {
            created_at: Some(user.created),
            username: user.username.clone(),
            bio: extract_bio(&user.settings),
            country: extract_country(&user.settings),
            timezone: extract_timezone(&user.settings),
            ide: extract_ide(&user.settings),
            terminal: extract_terminal(&user.settings),
            os: extract_os(&user.settings),
            langs: extract_langs(&user.settings),
            notify_kinds: extract_notify_kinds(&user.settings),
            notify_bell: extract_notify_bell(&user.settings),
            notify_cooldown_mins: extract_notify_cooldown_mins(&user.settings),
            notify_format: extract_notify_format(&user.settings),
            theme_id: extract_theme_id(&user.settings),
            enable_background_color: extract_enable_background_color(&user.settings),
            show_dashboard_header: extract_show_dashboard_header(&user.settings),
            show_right_sidebar: extract_show_right_sidebar(&user.settings),
            show_room_list_sidebar: extract_show_room_list_sidebar(&user.settings),
            show_settings_on_connect: extract_show_settings_on_connect(&user.settings),
            favorite_room_ids: extract_favorite_room_ids(&user.settings),
        }
    }
}

fn normalize_profile_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub fn normalize_profile_tags<'a>(values: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for value in values {
        for raw in value.split(|c: char| c == ',' || c.is_whitespace()) {
            let tag: String = raw
                .trim()
                .trim_matches('#')
                .to_ascii_lowercase()
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || matches!(*c, '-' | '_' | '.'))
                .collect();
            if tag.is_empty() || tag.len() > 24 || !seen.insert(tag.clone()) {
                continue;
            }
            out.push(tag);
            if out.len() >= 8 {
                return out;
            }
        }
    }
    out
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
