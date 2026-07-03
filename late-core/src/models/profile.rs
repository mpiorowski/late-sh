use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::{BTreeSet, HashMap};
use tokio_postgres::Client;
use uuid::Uuid;

use super::chips::INITIAL_CHIP_BALANCE;
use super::user::{
    RightSidebarComponentSetting, RightSidebarMode, User, extract_bio, extract_birthday,
    extract_country, extract_enable_background_color, extract_favorite_room_ids, extract_ide,
    extract_keep_composer_focused, extract_langs, extract_notify_bell,
    extract_notify_cooldown_mins, extract_notify_format, extract_notify_kinds, extract_os,
    extract_right_sidebar_components, extract_right_sidebar_mode, extract_show_dashboard_header,
    extract_show_flag_fallback, extract_show_right_sidebar, extract_show_room_list_sidebar,
    extract_start_with_music_muted, extract_terminal,
    extract_text_brightness_adjustment, extract_theme_id, extract_timezone,
    normalize_right_sidebar_components, normalize_text_brightness_adjustment,
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
    pub text_brightness_adjustment: i32,
    /// Controls the lounge top info boxes.
    pub show_dashboard_header: bool,
    pub show_right_sidebar: bool,
    pub right_sidebar_mode: RightSidebarMode,
    /// Ordered list of sidebar panels with their on/off state. List order is
    /// the render order (top to bottom); the clock is pinned above it.
    pub right_sidebar_components: Vec<RightSidebarComponentSetting>,
    pub show_room_list_sidebar: bool,
    /// Tweak: pressing Enter in the chat composer sends without closing it.
    /// While on, the Alt+S shortcut becomes a no-op.
    pub keep_composer_focused: bool,
    /// Tweak: silently mute the first paired audio client on each new SSH
    /// session so music does not auto-play.
    pub start_with_music_muted: bool,
    /// Tweak: show text labels instead of flag emoji in the shop Flags tab.
    pub show_flag_fallback: bool,
    /// Ordered list of room ids pinned to the dashboard quick-switch strip.
    pub favorite_room_ids: Vec<Uuid>,
    /// Year-less `MM-DD` birthday, or `None` if unset.
    pub birthday: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ProfileWithChipBalance {
    pub profile: Profile,
    pub chip_balance: i64,
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
            text_brightness_adjustment: 0,
            show_dashboard_header: true,
            show_right_sidebar: true,
            right_sidebar_mode: RightSidebarMode::On,
            right_sidebar_components: super::user::default_right_sidebar_components(),
            show_room_list_sidebar: true,
            keep_composer_focused: false,
            start_with_music_muted: false,
            show_flag_fallback: false,
            favorite_room_ids: Vec::new(),
            birthday: None,
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
    pub text_brightness_adjustment: i32,
    pub show_dashboard_header: bool,
    pub show_right_sidebar: bool,
    pub right_sidebar_mode: RightSidebarMode,
    pub right_sidebar_components: Vec<RightSidebarComponentSetting>,
    pub show_room_list_sidebar: bool,
    pub keep_composer_focused: bool,
    pub start_with_music_muted: bool,
    pub show_flag_fallback: bool,
    pub favorite_room_ids: Vec<Uuid>,
    /// Year-less `MM-DD` birthday, normalised on write. Empty/invalid clears it.
    pub birthday: Option<String>,
}

impl Profile {
    pub async fn load(client: &Client, user_id: Uuid) -> Result<Self> {
        let user = User::get(client, user_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("user not found"))?;
        Ok(Self::from_user(&user))
    }

    pub async fn list_by_user_ids(
        client: &Client,
        user_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Self>> {
        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = client
            .query("SELECT * FROM users WHERE id = ANY($1)", &[&user_ids])
            .await?;
        let mut profiles = HashMap::with_capacity(rows.len());
        for row in rows {
            let user = User::from(row);
            profiles.insert(user.id, Self::from_user(&user));
        }
        Ok(profiles)
    }

    pub async fn load_with_chip_balance(
        client: &Client,
        user_id: Uuid,
    ) -> Result<ProfileWithChipBalance> {
        let row = client
            .query_opt(
                "SELECT u.*,
                        COALESCE(c.balance, $2) AS chip_balance
                 FROM users u
                 LEFT JOIN user_chips c ON c.user_id = u.id
                 WHERE u.id = $1",
                &[&user_id, &INITIAL_CHIP_BALANCE],
            )
            .await?
            .ok_or_else(|| anyhow::anyhow!("user not found"))?;
        let chip_balance = row.get("chip_balance");
        let user = User::from(row);
        Ok(ProfileWithChipBalance {
            profile: Self::from_user(&user),
            chip_balance,
        })
    }

    /// Atomic partial update — merges
    /// bio/country/timezone/theme_id/notify_kinds/notify_bell/notify_cooldown_mins/
    /// enable_background_color/text_brightness_adjustment/
    /// show_dashboard_header/show_right_sidebar/
    /// right_sidebar_mode/right_sidebar_components/
    /// show_room_list_sidebar/keep_composer_focused/
    /// start_with_music_muted/show_flag_fallback into settings via
    /// `settings || jsonb_build_object(...)`, so concurrent writes to
    /// unrelated keys (ignored_user_ids) are preserved.
    pub async fn update(client: &Client, user_id: Uuid, params: ProfileParams) -> Result<Self> {
        let kinds_json = serde_json::to_value(&params.notify_kinds)?;
        let favorite_room_ids_json = serde_json::to_value(
            params
                .favorite_room_ids
                .iter()
                .map(Uuid::to_string)
                .collect::<Vec<_>>(),
        )?;
        let right_sidebar_components_json = serde_json::to_value(
            normalize_right_sidebar_components(&params.right_sidebar_components)
                .into_iter()
                .map(|setting| {
                    serde_json::json!({
                        "key": setting.component.as_str(),
                        "enabled": setting.enabled,
                    })
                })
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
        let birthday = params
            .birthday
            .as_deref()
            .and_then(crate::models::birthday::normalize_birthday);
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
                         'text_brightness_adjustment', $10::int,
                         'notify_format', $11::text,
                         'show_dashboard_header', $12::bool,
                         'show_right_sidebar', $13::bool,
                         'right_sidebar_mode', $14::text,
                         'right_sidebar_components', $15::jsonb,
                         'show_room_list_sidebar', $16::bool,
                         'favorite_room_ids', $17::jsonb,
                         'ide', $18::text,
                         'terminal', $19::text,
                         'os', $20::text,
                         'langs', $21::jsonb,
                         'birthday', $22::text,
                         'keep_composer_focused', $23::bool,
                         'start_with_music_muted', $24::bool,
                         'show_flag_fallback', $25::bool
                     ),
                     updated = current_timestamp
                 WHERE id = $26
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
                    &normalize_text_brightness_adjustment(params.text_brightness_adjustment),
                    &notify_format,
                    &params.show_dashboard_header,
                    &params.show_right_sidebar,
                    &params.right_sidebar_mode.as_str(),
                    &right_sidebar_components_json,
                    &params.show_room_list_sidebar,
                    &favorite_room_ids_json,
                    &ide,
                    &terminal,
                    &os,
                    &langs_json,
                    &birthday,
                    &params.keep_composer_focused,
                    &params.start_with_music_muted,
                    &params.show_flag_fallback,
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
            text_brightness_adjustment: extract_text_brightness_adjustment(&user.settings),
            show_dashboard_header: extract_show_dashboard_header(&user.settings),
            show_right_sidebar: extract_show_right_sidebar(&user.settings),
            right_sidebar_mode: extract_right_sidebar_mode(&user.settings),
            right_sidebar_components: extract_right_sidebar_components(&user.settings),
            show_room_list_sidebar: extract_show_room_list_sidebar(&user.settings),
            keep_composer_focused: extract_keep_composer_focused(&user.settings),
            start_with_music_muted: extract_start_with_music_muted(&user.settings),
            show_flag_fallback: extract_show_flag_fallback(&user.settings),
            favorite_room_ids: extract_favorite_room_ids(&user.settings),
            birthday: extract_birthday(&user.settings),
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
