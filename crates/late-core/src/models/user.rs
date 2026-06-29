use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use deadpool_postgres::GenericClient;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use tokio_postgres::Client;
use uuid::Uuid;

use super::marketplace::{
    BONSAI_VARIANT_SLOT, CHAT_BADGE_SLOT, CHAT_FLAG_SLOT, DYNAMIC_BONSAI_SKU,
};
use super::profile_award::PROFILE_AWARD_RANK_LIMIT;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioSource {
    #[default]
    Icecast,
    Youtube,
    Radio,
}

impl AudioSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Icecast => "icecast",
            Self::Youtube => "youtube",
            Self::Radio => "radio",
        }
    }

    pub fn from_settings_str(value: &str) -> Self {
        match value {
            "youtube" => Self::Youtube,
            "radio" => Self::Radio,
            _ => Self::Icecast,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IcecastStream {
    #[default]
    Chill,
    Classical,
}

impl IcecastStream {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chill => "chill",
            Self::Classical => "classical",
        }
    }

    pub fn from_settings_str(value: &str) -> Self {
        match value {
            "classical" => Self::Classical,
            _ => Self::Chill,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RadioStation {
    #[default]
    Chillsynth,
    Nightride,
    Datawave,
    Spacesynth,
}

impl RadioStation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chillsynth => "chillsynth",
            Self::Nightride => "nightride",
            Self::Datawave => "datawave",
            Self::Spacesynth => "spacesynth",
        }
    }

    pub fn from_settings_str(value: &str) -> Self {
        match value {
            "nightride" => Self::Nightride,
            "datawave" => Self::Datawave,
            "spacesynth" => Self::Spacesynth,
            _ => Self::Chillsynth,
        }
    }
}

crate::model! {
    table = "users";
    params = UserParams;
    struct User {
        @generated
        pub last_seen: DateTime<Utc>,
        pub is_admin: bool,
        pub is_moderator: bool;

        @data
        pub fingerprint: String,
        pub username: String,
        pub settings: serde_json::Value,
    }
}

pub const USERNAME_MAX_LEN: usize = 32;

/// Master on/off for the global right sidebar. The sidebar only appears on the
/// first three top-level screens (Home, Arcade, Rooms); which panels show and
/// in what order is governed by the component list, not by this mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RightSidebarMode {
    On,
    Off,
}

impl RightSidebarMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
        }
    }

    pub fn cycle(self, _forward: bool) -> Self {
        match self {
            Self::On => Self::Off,
            Self::Off => Self::On,
        }
    }
}

/// Number of reorderable/toggleable panels in the right sidebar (the clock is
/// always pinned at the top and is not part of this list).
pub const RIGHT_SIDEBAR_COMPONENT_COUNT: usize = 4;

/// A right-sidebar panel the user can reorder and toggle. The clock is not
/// listed here — it is always pinned at the top of the sidebar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RightSidebarComponent {
    Visualizer,
    Music,
    Pet,
    Bonsai,
}

impl RightSidebarComponent {
    /// Default order, top to bottom. Used when a user has no stored list and
    /// to backfill any panels missing from a stored list.
    pub const ALL: [RightSidebarComponent; RIGHT_SIDEBAR_COMPONENT_COUNT] =
        [Self::Visualizer, Self::Music, Self::Pet, Self::Bonsai];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Visualizer => "visualizer",
            Self::Music => "music",
            Self::Pet => "pet",
            Self::Bonsai => "bonsai",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim() {
            "visualizer" => Some(Self::Visualizer),
            "music" => Some(Self::Music),
            "pet" => Some(Self::Pet),
            "bonsai" => Some(Self::Bonsai),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Visualizer => "Visualizer",
            Self::Music => "Audio playback",
            Self::Pet => "Pet companion",
            Self::Bonsai => "Bonsai",
        }
    }
}

/// One entry in the ordered right-sidebar component list: a panel plus whether
/// it is currently shown. List order is the render order, top to bottom.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RightSidebarComponentSetting {
    pub component: RightSidebarComponent,
    pub enabled: bool,
}

/// Default component list: every panel, in default order, all enabled.
pub fn default_right_sidebar_components() -> Vec<RightSidebarComponentSetting> {
    RightSidebarComponent::ALL
        .into_iter()
        .map(|component| RightSidebarComponentSetting {
            component,
            enabled: true,
        })
        .collect()
}

/// Drop duplicates and backfill any missing panels (enabled) at the end so the
/// list always covers every component exactly once, preserving stored order.
pub fn normalize_right_sidebar_components(
    components: &[RightSidebarComponentSetting],
) -> Vec<RightSidebarComponentSetting> {
    let mut result: Vec<RightSidebarComponentSetting> = Vec::new();
    for setting in components {
        if result.iter().any(|s| s.component == setting.component) {
            continue;
        }
        result.push(*setting);
    }
    for component in RightSidebarComponent::ALL {
        if !result.iter().any(|s| s.component == component) {
            result.push(RightSidebarComponentSetting {
                component,
                enabled: true,
            });
        }
    }
    result
}

const IGNORED_USER_IDS_KEY: &str = "ignored_user_ids";
const FRIEND_USER_IDS_KEY: &str = "friend_user_ids";
const THEME_ID_KEY: &str = "theme_id";
const AUDIO_SOURCE_KEY: &str = "audio_source";
const ICECAST_STREAM_KEY: &str = "icecast_stream";
const RADIO_STATION_KEY: &str = "radio_station";
const NOTIFY_KINDS_KEY: &str = "notify_kinds";
const NOTIFY_BELL_KEY: &str = "notify_bell";
const NOTIFY_COOLDOWN_MINS_KEY: &str = "notify_cooldown_mins";
const NOTIFY_FORMAT_KEY: &str = "notify_format";
const ENABLE_BACKGROUND_COLOR_KEY: &str = "enable_background_color";
const TEXT_BRIGHTNESS_ADJUSTMENT_KEY: &str = "text_brightness_adjustment";
const SHOW_DASHBOARD_HEADER_KEY: &str = "show_dashboard_header";
const SHOW_RIGHT_SIDEBAR_KEY: &str = "show_right_sidebar";
const RIGHT_SIDEBAR_MODE_KEY: &str = "right_sidebar_mode";
const RIGHT_SIDEBAR_COMPONENTS_KEY: &str = "right_sidebar_components";
const SHOW_ROOM_LIST_SIDEBAR_KEY: &str = "show_room_list_sidebar";
const SHOW_SETTINGS_ON_CONNECT_KEY: &str = "show_settings_on_connect";
const KEEP_COMPOSER_FOCUSED_KEY: &str = "keep_composer_focused";
const START_WITH_MUSIC_MUTED_KEY: &str = "start_with_music_muted";
const SHOW_FLAG_FALLBACK_KEY: &str = "show_flag_fallback";
const FAVORITE_ROOM_IDS_KEY: &str = "favorite_room_ids";
const BIO_KEY: &str = "bio";
const COUNTRY_KEY: &str = "country";
const TIMEZONE_KEY: &str = "timezone";
const IDE_KEY: &str = "ide";
const TERMINAL_KEY: &str = "terminal";
const OS_KEY: &str = "os";
const LANGS_KEY: &str = "langs";
const BIRTHDAY_KEY: &str = "birthday";

impl User {
    pub async fn find_by_fingerprint(client: &Client, fingerprint: &str) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT u.*
                 FROM user_ssh_keys k
                 JOIN users u ON u.id = k.user_id
                 WHERE k.fingerprint = $1",
                &[&fingerprint],
            )
            .await?;
        if let Some(row) = row {
            return Ok(Some(Self::from(row)));
        }

        let row = client
            .query_opt(
                "SELECT * FROM users WHERE fingerprint = $1",
                &[&fingerprint],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn ensure_ssh_key(
        client: &impl GenericClient,
        user_id: Uuid,
        fingerprint: &str,
    ) -> Result<()> {
        client
            .execute(
                "INSERT INTO user_ssh_keys (user_id, fingerprint)
                 VALUES ($1, $2)
                 ON CONFLICT (fingerprint) DO UPDATE
                 SET user_id = EXCLUDED.user_id,
                     last_seen = current_timestamp,
                     updated = current_timestamp",
                &[&user_id, &fingerprint],
            )
            .await?;
        Ok(())
    }

    pub async fn touch_ssh_key(client: &Client, fingerprint: &str) -> Result<()> {
        client
            .execute(
                "UPDATE user_ssh_keys
                 SET last_seen = current_timestamp, updated = current_timestamp
                 WHERE fingerprint = $1",
                &[&fingerprint],
            )
            .await?;
        Ok(())
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

    /// Staff (admin/moderator) flags for the given users. Users with neither
    /// flag are omitted; values are `(is_admin, is_moderator)`.
    pub async fn staff_flags_by_ids(
        client: &Client,
        user_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, (bool, bool)>> {
        if user_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows = client
            .query(
                "SELECT id, is_admin, is_moderator
                 FROM users
                 WHERE id = ANY($1) AND (is_admin OR is_moderator)",
                &[&user_ids],
            )
            .await?;

        let mut flags = HashMap::with_capacity(rows.len());
        for row in rows {
            flags.insert(
                row.get("id"),
                (row.get("is_admin"), row.get("is_moderator")),
            );
        }
        Ok(flags)
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

    pub async fn list_ids(client: &Client) -> Result<Vec<Uuid>> {
        let rows = client.query("SELECT id FROM users", &[]).await?;
        Ok(rows.into_iter().map(|row| row.get("id")).collect())
    }

    pub async fn list_spotlight_candidates(client: &Client) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT *
                 FROM users
                 WHERE username <> ''
                   AND settings ? 'bio'
                   AND btrim(settings->>'bio') <> ''
                   AND COALESCE(settings->'bot', 'false'::jsonb) <> 'true'::jsonb
                 ORDER BY last_seen DESC, created DESC, id DESC",
                &[],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn delete_by_id(client: &Client, user_id: Uuid) -> Result<u64> {
        let deleted = client
            .execute("DELETE FROM users WHERE id = $1", &[&user_id])
            .await?;
        Ok(deleted)
    }

    pub async fn list_chat_author_metadata(
        client: &Client,
        user_ids: &[Uuid],
    ) -> Result<Vec<ChatAuthorMetadata>> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows = client
            .query(
                "SELECT u.id,
                        u.username,
                        u.is_admin,
                        u.is_moderator,
                        t.is_alive,
                        t.growth_points,
                        v2.badge_glyph AS bonsai_v2_badge_glyph,
                        EXISTS (
                            SELECT 1
                            FROM user_purchases dynamic_up
                            JOIN marketplace_items dynamic_bonsai
                              ON dynamic_bonsai.id = dynamic_up.item_id
                            WHERE dynamic_up.user_id = u.id
                              AND dynamic_up.equipped_slot = $3
                              AND dynamic_bonsai.sku = $4
                        ) AS dynamic_bonsai_selected,
                        flag.payload->>'emoji' AS chat_flag,
                        badge.payload->>'emoji' AS chat_badge,
                        award.badges AS profile_award_badges
                 FROM users u
                 LEFT JOIN bonsai_trees t ON t.user_id = u.id
                 LEFT JOIN bonsai_v2_trees v2 ON v2.user_id = u.id
                 LEFT JOIN user_purchases up
                   ON up.user_id = u.id
                  AND up.equipped_slot = $2
                 LEFT JOIN marketplace_items badge
                   ON badge.id = up.item_id
                 LEFT JOIN user_purchases flag_up
                   ON flag_up.user_id = u.id
                  AND flag_up.equipped_slot = $5
                 LEFT JOIN marketplace_items flag
                   ON flag.id = flag_up.item_id
                 LEFT JOIN LATERAL (
                    SELECT string_agg(
                        CASE category
                          WHEN 'lateania_archdemon' THEN 'LAD'
                          WHEN 'lateania_frontier_king' THEN 'LFK'
                          WHEN 'nethack_amulet' THEN 'NHA'
                          WHEN 'nethack_ascension' THEN 'NHY'
                          ELSE (
                            CASE category
                              WHEN 'top_chips' THEN 'CHIP'
                              WHEN 'arcade_wins' THEN 'AW'
                              WHEN 'tetris' THEN 'LA'
                              WHEN 'twenty_forty_eight' THEN '24#'
                              WHEN 'snake' THEN 'SN'
                              ELSE 'LB'
                            END
                          ) || rank::text
                        END,
                        ' '
                        ORDER BY rank ASC,
                                 CASE category
                                   WHEN 'arcade_wins' THEN 0
                                   WHEN 'top_chips' THEN 1
                                   WHEN 'tetris' THEN 2
                                   WHEN 'twenty_forty_eight' THEN 3
                                   WHEN 'snake' THEN 4
                                   WHEN 'lateania_archdemon' THEN 10
                                   WHEN 'lateania_frontier_king' THEN 11
                                   WHEN 'nethack_amulet' THEN 12
                                   WHEN 'nethack_ascension' THEN 13
                                   ELSE 99
                                 END
                    ) AS badges
                    FROM profile_awards pa
                    WHERE pa.user_id = u.id
                      AND pa.rank <= $6
                      AND (
                        pa.period_month = (date_trunc('month', now() AT TIME ZONE 'UTC')::date - INTERVAL '1 month')::date
                        OR pa.category IN ('lateania_archdemon', 'lateania_frontier_king', 'nethack_amulet', 'nethack_ascension')
                      )
                 ) award ON true
                 WHERE u.id = ANY($1)",
                &[
                    &user_ids,
                    &CHAT_BADGE_SLOT,
                    &BONSAI_VARIANT_SLOT,
                    &DYNAMIC_BONSAI_SKU,
                    &CHAT_FLAG_SLOT,
                    &PROFILE_AWARD_RANK_LIMIT,
                ],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let profile_award_badges: Option<String> = row.get("profile_award_badges");
                ChatAuthorMetadata {
                    user_id: row.get("id"),
                    username: row.get("username"),
                    is_admin: row.get("is_admin"),
                    is_moderator: row.get("is_moderator"),
                    bonsai_is_alive: row.get("is_alive"),
                    bonsai_growth_points: row.get("growth_points"),
                    bonsai_v2_badge_glyph: row.get("bonsai_v2_badge_glyph"),
                    dynamic_bonsai_selected: row.get("dynamic_bonsai_selected"),
                    chat_flag: row.get("chat_flag"),
                    chat_badge: row.get("chat_badge"),
                    profile_award_badges: chat_profile_award_badges(profile_award_badges),
                }
            })
            .collect())
    }

    pub async fn list_all_country_map(client: &Client) -> Result<HashMap<Uuid, String>> {
        let rows = client
            .query(
                "SELECT id, settings
                 FROM users
                 WHERE settings ? $1",
                &[&COUNTRY_KEY],
            )
            .await?;
        let mut map = HashMap::with_capacity(rows.len());
        for row in rows {
            let settings: Value = row.get("settings");
            if let Some(country) = extract_country(&settings) {
                map.insert(row.get("id"), country);
            }
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
        Ok(extract_uuid_ids(&settings, IGNORED_USER_IDS_KEY))
    }

    pub async fn friend_user_ids(client: &Client, user_id: Uuid) -> Result<Vec<Uuid>> {
        let settings = Self::settings_for_user(client, user_id).await?;
        Ok(extract_uuid_ids(&settings, FRIEND_USER_IDS_KEY))
    }

    pub async fn favorite_room_ids(client: &Client, user_id: Uuid) -> Result<Vec<Uuid>> {
        let settings = Self::settings_for_user(client, user_id).await?;
        Ok(extract_favorite_room_ids(&settings))
    }

    pub async fn theme_id(client: &Client, user_id: Uuid) -> Result<Option<String>> {
        let settings = Self::settings_for_user(client, user_id).await?;
        Ok(extract_theme_id(&settings))
    }

    pub async fn audio_source(client: &Client, user_id: Uuid) -> Result<AudioSource> {
        let settings = Self::settings_for_user(client, user_id).await?;
        Ok(extract_audio_source(&settings))
    }

    pub async fn icecast_stream(client: &Client, user_id: Uuid) -> Result<IcecastStream> {
        let settings = Self::settings_for_user(client, user_id).await?;
        Ok(extract_icecast_stream(&settings))
    }

    pub async fn radio_station(client: &Client, user_id: Uuid) -> Result<RadioStation> {
        let settings = Self::settings_for_user(client, user_id).await?;
        Ok(extract_radio_station(&settings))
    }

    pub async fn start_with_music_muted(client: &Client, user_id: Uuid) -> Result<bool> {
        let settings = Self::settings_for_user(client, user_id).await?;
        Ok(extract_start_with_music_muted(&settings))
    }

    /// Atomically merge `audio_source` into `settings` without clobbering other keys.
    pub async fn set_audio_source(
        client: &Client,
        user_id: Uuid,
        source: AudioSource,
    ) -> Result<()> {
        let value = source.as_str();
        let updated = client
            .execute(
                "UPDATE users
                 SET settings = settings || jsonb_build_object($1::text, $2::text),
                     updated = current_timestamp
                 WHERE id = $3",
                &[&AUDIO_SOURCE_KEY, &value, &user_id],
            )
            .await?;
        if updated == 0 {
            bail!("user not found");
        }
        Ok(())
    }

    pub async fn set_icecast_stream(
        client: &Client,
        user_id: Uuid,
        stream: IcecastStream,
    ) -> Result<()> {
        let value = stream.as_str();
        let updated = client
            .execute(
                "UPDATE users
                 SET settings = settings || jsonb_build_object($1::text, $2::text),
                     updated = current_timestamp
                 WHERE id = $3",
                &[&ICECAST_STREAM_KEY, &value, &user_id],
            )
            .await?;
        if updated == 0 {
            bail!("user not found");
        }
        Ok(())
    }

    pub async fn set_radio_station(
        client: &Client,
        user_id: Uuid,
        station: RadioStation,
    ) -> Result<()> {
        let value = station.as_str();
        let updated = client
            .execute(
                "UPDATE users
                 SET settings = settings || jsonb_build_object($1::text, $2::text),
                     updated = current_timestamp
                 WHERE id = $3",
                &[&RADIO_STATION_KEY, &value, &user_id],
            )
            .await?;
        if updated == 0 {
            bail!("user not found");
        }
        Ok(())
    }

    /// Adds `target_id` to the ignore list. Returns `(changed, ids)` —
    /// `changed` is false if the id was already present.
    pub async fn add_ignored_user_id(
        client: &Client,
        user_id: Uuid,
        target_id: Uuid,
    ) -> Result<(bool, Vec<Uuid>)> {
        Self::add_uuid_setting_id(client, user_id, target_id, IGNORED_USER_IDS_KEY).await
    }

    /// Removes `target_id` from the ignore list. Returns `(changed, ids)` —
    /// `changed` is false if the id was not present.
    pub async fn remove_ignored_user_id(
        client: &Client,
        user_id: Uuid,
        target_id: Uuid,
    ) -> Result<(bool, Vec<Uuid>)> {
        Self::remove_uuid_setting_id(client, user_id, target_id, IGNORED_USER_IDS_KEY).await
    }

    pub async fn add_friend_user_id(
        client: &Client,
        user_id: Uuid,
        target_id: Uuid,
    ) -> Result<(bool, Vec<Uuid>)> {
        Self::add_uuid_setting_id(client, user_id, target_id, FRIEND_USER_IDS_KEY).await
    }

    pub async fn remove_friend_user_id(
        client: &Client,
        user_id: Uuid,
        target_id: Uuid,
    ) -> Result<(bool, Vec<Uuid>)> {
        Self::remove_uuid_setting_id(client, user_id, target_id, FRIEND_USER_IDS_KEY).await
    }

    async fn add_uuid_setting_id(
        client: &Client,
        user_id: Uuid,
        target_id: Uuid,
        key: &str,
    ) -> Result<(bool, Vec<Uuid>)> {
        let mut settings = Self::settings_for_user(client, user_id).await?;
        let mut ids = extract_uuid_ids(&settings, key);

        if ids.contains(&target_id) {
            return Ok((false, ids));
        }

        ids.push(target_id);
        ids.sort();
        set_uuid_ids(&mut settings, key, &ids);
        Self::update_settings(client, user_id, &settings).await?;
        Ok((true, ids))
    }

    async fn remove_uuid_setting_id(
        client: &Client,
        user_id: Uuid,
        target_id: Uuid,
        key: &str,
    ) -> Result<(bool, Vec<Uuid>)> {
        let mut settings = Self::settings_for_user(client, user_id).await?;
        let mut ids = extract_uuid_ids(&settings, key);

        if !ids.contains(&target_id) {
            return Ok((false, ids));
        }

        ids.retain(|entry| entry != &target_id);
        set_uuid_ids(&mut settings, key, &ids);
        Self::update_settings(client, user_id, &settings).await?;
        Ok((true, ids))
    }

    /// `(username, birthday MM-DD)` for every friend that has set a birthday.
    /// Used to build connect-time birthday alerts.
    pub async fn friend_birthdays(client: &Client, user_id: Uuid) -> Result<Vec<(String, String)>> {
        let ids = Self::friend_user_ids(client, user_id).await?;
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = client
            .query(
                "SELECT username, settings FROM users WHERE id = ANY($1)",
                &[&ids],
            )
            .await?;
        let mut out = Vec::new();
        for row in &rows {
            let username: String = row.get("username");
            let settings: Value = row.get("settings");
            if let Some(birthday) = extract_birthday(&settings) {
                out.push((username, birthday));
            }
        }
        out.sort();
        Ok(out)
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

    pub async fn set_moderator(
        client: &impl GenericClient,
        user_id: Uuid,
        is_moderator: bool,
    ) -> Result<()> {
        let updated = client
            .execute(
                "UPDATE users
                 SET is_moderator = $1, updated = current_timestamp
                 WHERE id = $2",
                &[&is_moderator, &user_id],
            )
            .await?;
        if updated == 0 {
            bail!("user not found");
        }
        Ok(())
    }

    pub async fn rename(
        client: &impl GenericClient,
        user_id: Uuid,
        username: &str,
    ) -> Result<Self> {
        let username = sanitize_username_input(username);
        let row = client
            .query_one(
                "UPDATE users
                 SET username = $1, updated = current_timestamp
                 WHERE id = $2
                 RETURNING *",
                &[&username, &user_id],
            )
            .await?;
        Ok(Self::from(row))
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

    pub async fn update_settings(client: &Client, user_id: Uuid, settings: &Value) -> Result<()> {
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

#[derive(Clone, Debug)]
pub struct ChatAuthorMetadata {
    pub user_id: Uuid,
    pub username: String,
    pub is_admin: bool,
    pub is_moderator: bool,
    pub bonsai_is_alive: Option<bool>,
    pub bonsai_growth_points: Option<i32>,
    pub bonsai_v2_badge_glyph: Option<String>,
    pub dynamic_bonsai_selected: bool,
    pub chat_flag: Option<String>,
    pub chat_badge: Option<String>,
    pub profile_award_badges: Option<String>,
}

fn chat_profile_award_badges(raw: Option<String>) -> Option<String> {
    let raw = raw?;
    // Collapse the lesser milestone when its superseding one is present: the
    // Frontier King implies the Archdemon, and an Ascension implies the Amulet.
    // Profile views still show both; chat author labels show only the higher.
    let has_frontier_king = raw.split_whitespace().any(|badge| badge == "LFK");
    let has_ascension = raw.split_whitespace().any(|badge| badge == "NHY");
    let badges = raw
        .split_whitespace()
        .filter(|badge| !(has_frontier_king && *badge == "LAD"))
        .filter(|badge| !(has_ascension && *badge == "NHA"))
        .collect::<Vec<_>>()
        .join(" ");
    (!badges.is_empty()).then_some(badges)
}

fn extract_uuid_ids(settings: &Value, key: &str) -> Vec<Uuid> {
    let Some(entries) = settings.get(key).and_then(Value::as_array) else {
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

fn set_uuid_ids(settings: &mut Value, key: &str, ids: &[Uuid]) {
    if !settings.is_object() {
        *settings = json!({});
    }
    settings[key] = json!(ids.iter().map(Uuid::to_string).collect::<Vec<_>>());
}

pub fn extract_birthday(settings: &Value) -> Option<String> {
    settings
        .get(BIRTHDAY_KEY)
        .and_then(Value::as_str)
        .and_then(crate::models::birthday::normalize_birthday)
}

pub fn extract_theme_id(settings: &Value) -> Option<String> {
    settings
        .get(THEME_ID_KEY)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub fn extract_audio_source(settings: &Value) -> AudioSource {
    settings
        .get(AUDIO_SOURCE_KEY)
        .and_then(Value::as_str)
        .map(AudioSource::from_settings_str)
        .unwrap_or_default()
}

pub fn extract_icecast_stream(settings: &Value) -> IcecastStream {
    settings
        .get(ICECAST_STREAM_KEY)
        .and_then(Value::as_str)
        .map(IcecastStream::from_settings_str)
        .unwrap_or_default()
}

pub fn extract_radio_station(settings: &Value) -> RadioStation {
    settings
        .get(RADIO_STATION_KEY)
        .and_then(Value::as_str)
        .map(RadioStation::from_settings_str)
        .unwrap_or_default()
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

/// Valid values: `"both"` (default), `"osc777"`, `"osc9"`. Returns `None`
/// for missing, empty, or unrecognized values so the caller can fall back
/// to the default.
pub fn extract_notify_format(settings: &Value) -> Option<String> {
    let raw = settings.get(NOTIFY_FORMAT_KEY).and_then(Value::as_str)?;
    match raw.trim() {
        "both" | "osc777" | "osc9" => Some(raw.trim().to_string()),
        _ => None,
    }
}

pub fn extract_enable_background_color(settings: &Value) -> bool {
    settings
        .get(ENABLE_BACKGROUND_COLOR_KEY)
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

pub fn normalize_text_brightness_adjustment(value: i32) -> i32 {
    value.clamp(-5, 5)
}

pub fn extract_text_brightness_adjustment(settings: &Value) -> i32 {
    settings
        .get(TEXT_BRIGHTNESS_ADJUSTMENT_KEY)
        .and_then(Value::as_i64)
        .map(|value| normalize_text_brightness_adjustment(value as i32))
        .unwrap_or(0)
}

pub fn extract_show_dashboard_header(settings: &Value) -> bool {
    settings
        .get(SHOW_DASHBOARD_HEADER_KEY)
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

pub fn extract_show_right_sidebar(settings: &Value) -> bool {
    // Legacy `"custom"` predates the global component list and meant "shown";
    // treat it as on.
    match settings
        .get(RIGHT_SIDEBAR_MODE_KEY)
        .and_then(Value::as_str)
        .map(str::trim)
    {
        Some("on" | "custom") => return true,
        Some("off") => return false,
        _ => {}
    }

    settings
        .get(SHOW_RIGHT_SIDEBAR_KEY)
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

pub fn extract_right_sidebar_mode(settings: &Value) -> RightSidebarMode {
    match settings
        .get(RIGHT_SIDEBAR_MODE_KEY)
        .and_then(Value::as_str)
        .map(str::trim)
    {
        Some("off") => RightSidebarMode::Off,
        // Legacy per-screen `"custom"` collapses to On now that visibility is
        // governed by the global component list.
        Some("on" | "custom") => RightSidebarMode::On,
        _ if settings
            .get(SHOW_RIGHT_SIDEBAR_KEY)
            .and_then(Value::as_bool)
            .unwrap_or(true) =>
        {
            RightSidebarMode::On
        }
        _ => RightSidebarMode::Off,
    }
}

pub fn extract_right_sidebar_components(settings: &Value) -> Vec<RightSidebarComponentSetting> {
    let Some(values) = settings
        .get(RIGHT_SIDEBAR_COMPONENTS_KEY)
        .and_then(Value::as_array)
    else {
        return default_right_sidebar_components();
    };

    let mut parsed: Vec<RightSidebarComponentSetting> = Vec::new();
    for value in values {
        let Some(component) = value
            .get("key")
            .and_then(Value::as_str)
            .and_then(RightSidebarComponent::from_key)
        else {
            continue;
        };
        let enabled = value
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        parsed.push(RightSidebarComponentSetting { component, enabled });
    }

    normalize_right_sidebar_components(&parsed)
}

pub fn extract_show_room_list_sidebar(settings: &Value) -> bool {
    settings
        .get(SHOW_ROOM_LIST_SIDEBAR_KEY)
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

pub fn extract_show_settings_on_connect(settings: &Value) -> bool {
    settings
        .get(SHOW_SETTINGS_ON_CONNECT_KEY)
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// Tweak: when true, pressing Enter in the chat composer sends the message
/// but keeps the composer focused (same behavior as Alt+S, which becomes a
/// no-op while the tweak is on). Opt-in; defaults to false so existing
/// muscle memory is preserved.
pub fn extract_keep_composer_focused(settings: &Value) -> bool {
    settings
        .get(KEEP_COMPOSER_FOCUSED_KEY)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Tweak: when true, the first paired audio client for a new SSH session is
/// silently muted as soon as it reports `muted: false`. Opt-in; defaults to
/// false so audio plays on connect like today.
pub fn extract_start_with_music_muted(settings: &Value) -> bool {
    settings
        .get(START_WITH_MUSIC_MUTED_KEY)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Tweak: show text labels instead of flag emoji in the shop Flags tab for
/// terminal/font stacks that render regional-indicator flags as letters.
pub fn extract_show_flag_fallback(settings: &Value) -> bool {
    settings
        .get(SHOW_FLAG_FALLBACK_KEY)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Ordered list of room ids the user has pinned as favorites. Insertion
/// order is preserved (user-chosen ordering); missing/invalid entries are
/// dropped silently. Duplicates are collapsed while keeping the first
/// occurrence so cycling on the dashboard doesn't flicker.
pub fn extract_favorite_room_ids(settings: &Value) -> Vec<Uuid> {
    let Some(entries) = settings
        .get(FAVORITE_ROOM_IDS_KEY)
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };

    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(id) = entry.as_str().and_then(|s| Uuid::parse_str(s.trim()).ok()) else {
            continue;
        };
        if seen.insert(id) {
            out.push(id);
        }
    }
    out
}

pub fn extract_bio(settings: &Value) -> String {
    settings
        .get(BIO_KEY)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_default()
}

pub fn extract_country(settings: &Value) -> Option<String> {
    settings
        .get(COUNTRY_KEY)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_uppercase())
}

pub fn extract_timezone(settings: &Value) -> Option<String> {
    settings
        .get(TIMEZONE_KEY)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub fn extract_ide(settings: &Value) -> Option<String> {
    extract_trimmed_profile_text(settings, IDE_KEY)
}

pub fn extract_terminal(settings: &Value) -> Option<String> {
    extract_trimmed_profile_text(settings, TERMINAL_KEY)
}

pub fn extract_os(settings: &Value) -> Option<String> {
    extract_trimmed_profile_text(settings, OS_KEY)
}

pub fn extract_langs(settings: &Value) -> Vec<String> {
    let Some(value) = settings.get(LANGS_KEY) else {
        return Vec::new();
    };

    let raw_tags: Vec<String> = if let Some(entries) = value.as_array() {
        entries
            .iter()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect()
    } else if let Some(text) = value.as_str() {
        vec![text.to_string()]
    } else {
        Vec::new()
    };

    normalize_profile_tags(raw_tags.iter().map(String::as_str))
}

fn extract_trimmed_profile_text(settings: &Value, key: &str) -> Option<String> {
    settings
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn normalize_profile_tags<'a>(values: impl IntoIterator<Item = &'a str>) -> Vec<String> {
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
    fn chat_profile_award_badges_prefer_frontier_king_over_archdemon() {
        assert_eq!(
            chat_profile_award_badges(Some("LAD LFK".to_string())).as_deref(),
            Some("LFK")
        );
        assert_eq!(
            chat_profile_award_badges(Some("AW1 LAD LFK CHIP2".to_string())).as_deref(),
            Some("AW1 LFK CHIP2")
        );
    }

    #[test]
    fn chat_profile_award_badges_keep_archdemon_when_it_is_the_best_lateania_badge() {
        assert_eq!(
            chat_profile_award_badges(Some("AW1 LAD CHIP2".to_string())).as_deref(),
            Some("AW1 LAD CHIP2")
        );
        assert_eq!(
            chat_profile_award_badges(Some("LAD".to_string())).as_deref(),
            Some("LAD")
        );
    }

    #[test]
    fn chat_profile_award_badges_prefer_ascension_over_amulet() {
        // Ascension implies the Amulet, so the chat label collapses NHA into NHY.
        assert_eq!(
            chat_profile_award_badges(Some("NHA NHY".to_string())).as_deref(),
            Some("NHY")
        );
        // The Amulet alone stands on its own.
        assert_eq!(
            chat_profile_award_badges(Some("AW1 NHA".to_string())).as_deref(),
            Some("AW1 NHA")
        );
    }

    #[test]
    fn extract_bio_missing_returns_empty() {
        let settings = json!({});
        assert_eq!(extract_bio(&settings), "");
    }

    #[test]
    fn extract_show_right_sidebar_defaults_to_true() {
        let settings = json!({});
        assert!(extract_show_right_sidebar(&settings));
    }

    #[test]
    fn extract_show_dashboard_header_defaults_to_true() {
        let settings = json!({});
        assert!(extract_show_dashboard_header(&settings));
    }

    #[test]
    fn extract_enable_background_color_defaults_to_true() {
        let settings = json!({});
        assert!(extract_enable_background_color(&settings));
    }

    #[test]
    fn extract_text_brightness_adjustment_defaults_to_zero_and_clamps() {
        assert_eq!(extract_text_brightness_adjustment(&json!({})), 0);
        assert_eq!(
            extract_text_brightness_adjustment(&json!({ "text_brightness_adjustment": 2 })),
            2
        );
        assert_eq!(
            extract_text_brightness_adjustment(&json!({ "text_brightness_adjustment": 9 })),
            5
        );
        assert_eq!(
            extract_text_brightness_adjustment(&json!({ "text_brightness_adjustment": -9 })),
            -5
        );
    }

    #[test]
    fn extract_enable_background_color_reads_explicit_false() {
        let settings = json!({ "enable_background_color": false });
        assert!(!extract_enable_background_color(&settings));
    }

    #[test]
    fn extract_show_dashboard_header_reads_explicit_false() {
        let settings = json!({ "show_dashboard_header": false });
        assert!(!extract_show_dashboard_header(&settings));
    }

    #[test]
    fn extract_show_right_sidebar_reads_explicit_false() {
        let settings = json!({ "show_right_sidebar": false });
        assert!(!extract_show_right_sidebar(&settings));
    }

    #[test]
    fn extract_show_right_sidebar_prefers_new_mode() {
        let settings = json!({
            "show_right_sidebar": true,
            "right_sidebar_mode": "off",
        });
        assert!(!extract_show_right_sidebar(&settings));
    }

    #[test]
    fn extract_right_sidebar_mode_collapses_legacy_custom_to_on() {
        let settings = json!({ "right_sidebar_mode": "custom" });
        assert_eq!(extract_right_sidebar_mode(&settings), RightSidebarMode::On);
    }

    #[test]
    fn extract_right_sidebar_mode_falls_back_to_legacy_bool() {
        let settings = json!({ "show_right_sidebar": false });
        assert_eq!(extract_right_sidebar_mode(&settings), RightSidebarMode::Off);
    }

    #[test]
    fn extract_right_sidebar_components_defaults_to_all_enabled() {
        let settings = json!({});
        assert_eq!(
            extract_right_sidebar_components(&settings),
            default_right_sidebar_components()
        );
    }

    #[test]
    fn extract_right_sidebar_components_preserves_order_and_backfills() {
        let settings = json!({
            "right_sidebar_components": [
                { "key": "bonsai", "enabled": false },
                { "key": "music", "enabled": true },
                { "key": "bogus", "enabled": true },
            ]
        });
        let components = extract_right_sidebar_components(&settings);
        // Stored order kept for known entries, unknown dropped, missing
        // (visualizer, pet) backfilled enabled at the end.
        assert_eq!(
            components,
            vec![
                RightSidebarComponentSetting {
                    component: RightSidebarComponent::Bonsai,
                    enabled: false,
                },
                RightSidebarComponentSetting {
                    component: RightSidebarComponent::Music,
                    enabled: true,
                },
                RightSidebarComponentSetting {
                    component: RightSidebarComponent::Visualizer,
                    enabled: true,
                },
                RightSidebarComponentSetting {
                    component: RightSidebarComponent::Pet,
                    enabled: true,
                },
            ]
        );
    }

    #[test]
    fn extract_show_room_list_sidebar_defaults_to_true() {
        let settings = json!({});
        assert!(extract_show_room_list_sidebar(&settings));
    }

    #[test]
    fn extract_show_room_list_sidebar_reads_explicit_false() {
        let settings = json!({ "show_room_list_sidebar": false });
        assert!(!extract_show_room_list_sidebar(&settings));
    }

    #[test]
    fn extract_country_normalizes_uppercase() {
        let settings = json!({ "country": " pl " });
        assert_eq!(extract_country(&settings).as_deref(), Some("PL"));
    }

    #[test]
    fn extract_timezone_reads_trimmed_value() {
        let settings = json!({ "timezone": " Europe/Warsaw " });
        assert_eq!(
            extract_timezone(&settings).as_deref(),
            Some("Europe/Warsaw")
        );
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
