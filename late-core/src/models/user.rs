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
use super::profile_award::{
    MILESTONE_AWARD_CATEGORIES, PROFILE_AWARD_RANK_LIMIT, top_badge_per_game,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioSource {
    Icecast,
    Youtube,
    /// Nightride FM direct streams. The default for users who never picked
    /// a source, so fresh `late` sessions land on the radio.
    #[default]
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
            "icecast" => Self::Icecast,
            _ => Self::Radio,
        }
    }
}

/// How a session is driven. Chosen on first entry (see the onboarding prompt),
/// then editable in settings. The key behavioural lever is whether the terminal
/// mouse reporting is turned on: off in `Keyboard` so native selection/copy keep
/// working; on in `Mouse` and `Hybrid`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionMode {
    /// Keyboard only, the classic terminal/programmer experience; mouse
    /// reporting stays off so the terminal's own text selection works.
    Keyboard,
    /// Mouse-first, Discord-like: everything is clickable, mouse reporting on.
    Mouse,
    /// Both keyboard shortcuts and the mouse work. The safe default.
    #[default]
    Hybrid,
}

impl InteractionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Keyboard => "keyboard",
            Self::Mouse => "mouse",
            Self::Hybrid => "hybrid",
        }
    }

    pub fn from_settings_str(value: &str) -> Self {
        match value {
            "keyboard" => Self::Keyboard,
            "mouse" => Self::Mouse,
            _ => Self::Hybrid,
        }
    }

    /// Whether the terminal's mouse reporting should be enabled in this mode.
    pub fn mouse_enabled(self) -> bool {
        matches!(self, Self::Mouse | Self::Hybrid)
    }

    /// Whether keyboard shortcuts are the primary/expected input (for which set
    /// of on-screen hints to show). Both keyboard-only and hybrid say yes.
    pub fn keyboard_primary(self) -> bool {
        matches!(self, Self::Keyboard | Self::Hybrid)
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
    Ambient,
}

impl RadioStation {
    /// Settings/persistence key, also used to look up live now-playing
    /// metadata in the Nightride `/meta` feed. The feed keys stations by
    /// their stream filename, so `Ambient` must key on `"rektify"` (its
    /// `rektify.mp3` stream) even though its display label is `"ambient"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chillsynth => "chillsynth",
            Self::Nightride => "nightride",
            Self::Datawave => "datawave",
            Self::Spacesynth => "spacesynth",
            Self::Ambient => "rektify",
        }
    }

    pub fn from_settings_str(value: &str) -> Self {
        match value {
            "nightride" => Self::Nightride,
            "datawave" => Self::Datawave,
            "spacesynth" => Self::Spacesynth,
            "rektify" => Self::Ambient,
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
///
/// `Auto` hands the decision to the terminal: the sidebar shows only when the
/// session is wide enough to spare the columns, so one account works on both a
/// phone and a desktop. The width thresholds live in `late-ssh`'s render layer,
/// which is the only place that knows the live terminal size.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RightSidebarMode {
    On,
    Off,
    Auto,
}

impl RightSidebarMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
            Self::Auto => "auto",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim() {
            "on" => Some(Self::On),
            "off" => Some(Self::Off),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }

    pub fn cycle(self, _forward: bool) -> Self {
        match self {
            Self::On => Self::Off,
            Self::Off => Self::Auto,
            Self::Auto => Self::On,
        }
    }
}

/// Master on/off for the Home room-list rail, the left column. Mirrors
/// [`RightSidebarMode`], including `Auto`: the rail folds away on terminals too
/// narrow to carry three columns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoomListMode {
    On,
    Off,
    Auto,
}

impl RoomListMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
            Self::Auto => "auto",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim() {
            "on" => Some(Self::On),
            "off" => Some(Self::Off),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }

    pub fn cycle(self, _forward: bool) -> Self {
        match self {
            Self::On => Self::Off,
            Self::Off => Self::Auto,
            Self::Auto => Self::On,
        }
    }
}

/// Number of reorderable/toggleable panels in the right sidebar (the clock is
/// always pinned at the top and is not part of this list).
pub const RIGHT_SIDEBAR_COMPONENT_COUNT: usize = 3;

/// A right-sidebar panel the user can reorder and toggle. The clock is not
/// listed here — it is always pinned at the top of the sidebar. The
/// visualizer is not a panel of its own: it renders inline at the top of
/// `Music`, see `common/sidebar.rs`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RightSidebarComponent {
    Music,
    Bonsai,
    Daily,
}

impl RightSidebarComponent {
    /// Default order, top to bottom. Used when a user has no stored list and
    /// to backfill any panels missing from a stored list. Space cuts by
    /// shrink priority; Bonsai is the one flexible panel and absorbs leftover
    /// rows. Stale stored keys (e.g. the retired "activity", "visualizer" and
    /// "pot" panels) are dropped on read by `from_key`.
    pub const ALL: [RightSidebarComponent; RIGHT_SIDEBAR_COMPONENT_COUNT] =
        [Self::Daily, Self::Music, Self::Bonsai];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Music => "music",
            Self::Bonsai => "bonsai",
            Self::Daily => "daily",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim() {
            "music" => Some(Self::Music),
            "bonsai" => Some(Self::Bonsai),
            "daily" => Some(Self::Daily),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Music => "Audio playback",
            Self::Bonsai => "Bonsai",
            Self::Daily => "Lobby",
        }
    }

    /// Whether the panel starts enabled for users without a stored setting.
    pub fn default_enabled(self) -> bool {
        true
    }
}

/// One entry in the ordered right-sidebar component list: a panel plus whether
/// it is currently shown. List order is the render order, top to bottom.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RightSidebarComponentSetting {
    pub component: RightSidebarComponent,
    pub enabled: bool,
}

/// Default component list: every panel, in default order, at its default
/// on/off state.
pub fn default_right_sidebar_components() -> Vec<RightSidebarComponentSetting> {
    RightSidebarComponent::ALL
        .into_iter()
        .map(|component| RightSidebarComponentSetting {
            component,
            enabled: component.default_enabled(),
        })
        .collect()
}

/// Drop duplicates and backfill any missing panels at the end so the list
/// always covers every component exactly once, preserving stored order.
///
/// Missing panels are backfilled **enabled**, not at `default_enabled()`: a
/// user with a stored list is an existing user whose effective state must not
/// silently change when a new panel ships. `default_enabled()` applies only to
/// the no-stored-list path in `default_right_sidebar_components`, i.e.
/// genuinely new users.
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
const INTERACTION_MODE_KEY: &str = "interaction_mode";
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
const SHOW_RIGHT_SIDEBAR_KEY: &str = "show_right_sidebar";
const RIGHT_SIDEBAR_MODE_KEY: &str = "right_sidebar_mode";
const RIGHT_SIDEBAR_COMPONENTS_KEY: &str = "right_sidebar_components";
const SHOW_AQUARIUM_TRAY_KEY: &str = "show_aquarium_tray";
const SHOW_PET_STRIP_KEY: &str = "show_pet_strip";
const SHOW_ROOM_LIST_SIDEBAR_KEY: &str = "show_room_list_sidebar";
const ROOM_LIST_MODE_KEY: &str = "room_list_mode";
const KEEP_COMPOSER_FOCUSED_KEY: &str = "keep_composer_focused";
const START_WITH_MUSIC_MUTED_KEY: &str = "start_with_music_muted";
const LAND_ON_HOME_KEY: &str = "land_on_home";
const TRANSLATE_TO_KEY: &str = "translate_to";
const AUTO_TRANSLATE_KEY: &str = "auto_translate";
const TRANSLATE_MINE_TO_EN_KEY: &str = "translate_mine_to_en";
const SHOW_FLAG_FALLBACK_KEY: &str = "show_flag_fallback";
const CLUBHOUSE_TUTORIAL_DONE_KEY: &str = "clubhouse_tutorial_done";
const FAVORITE_ROOM_IDS_KEY: &str = "favorite_room_ids";
const FAVORITE_THEME_IDS_KEY: &str = "favorite_theme_ids";
const BIO_KEY: &str = "bio";
const COUNTRY_KEY: &str = "country";
const TIMEZONE_KEY: &str = "timezone";
const IDE_KEY: &str = "ide";
const TERMINAL_KEY: &str = "terminal";
const OS_KEY: &str = "os";
const LANGS_KEY: &str = "langs";

impl User {
    /// Whether this account is one of the app's own actors (the ghost bots,
    /// the `system` feed author). Set in `settings.bot` when the row is
    /// ensured. Callers use it to keep player-to-player mechanics between
    /// players: nobody tips the house.
    pub fn is_bot(&self) -> bool {
        self.settings
            .get("bot")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    }

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

    /// Seconds since the account was created, or `None` if the user is unknown.
    /// Used by the chat link rate-limiter to pick a cooldown tier by account age.
    pub async fn account_age_seconds(client: &Client, user_id: Uuid) -> Result<Option<i64>> {
        let row = client
            .query_opt(
                "SELECT EXTRACT(EPOCH FROM (now() - created))::bigint AS age FROM users WHERE id = $1",
                &[&user_id],
            )
            .await?;
        Ok(row.map(|r| r.get::<_, i64>("age")))
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

        // The rankless milestone badges show whatever month they were earned,
        // unlike the ranked boards. Bound as a parameter so the set lives in
        // `profile_award` alone rather than being respelled in SQL.
        let milestone_categories: Vec<&str> = MILESTONE_AWARD_CATEGORIES.to_vec();
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
                        flag_rental.payload->>'emoji' AS chat_flag,
                        badge_rental.payload->>'emoji' AS chat_badge,
                        award.badges AS profile_award_badges
                 FROM users u
                 LEFT JOIN bonsai_trees t ON t.user_id = u.id
                 LEFT JOIN bonsai_v2_trees v2 ON v2.user_id = u.id
                 -- A rental is the only thing that fills these two slots.
                 -- Expiry is read-time: once `ends_at` passes the label goes
                 -- bare, with no background job to run. Migration 165 cleared
                 -- the last permanent equips, so `equipped_slot` no longer
                 -- carries a badge or a flag; $2 and $5 are effect kinds here,
                 -- and `bonsai_variant` above is the only equip slot left.
                 LEFT JOIN LATERAL (
                    SELECT e.payload
                    FROM shop_consumable_effects e
                    WHERE e.user_id = u.id
                      AND e.room_id IS NULL
                      AND e.effect_kind = $2
                      AND e.active = true
                      AND e.ends_at > current_timestamp
                    ORDER BY e.ends_at DESC
                    LIMIT 1
                 ) badge_rental ON true
                 LEFT JOIN LATERAL (
                    SELECT e.payload
                    FROM shop_consumable_effects e
                    WHERE e.user_id = u.id
                      AND e.room_id IS NULL
                      AND e.effect_kind = $5
                      AND e.active = true
                      AND e.ends_at > current_timestamp
                    ORDER BY e.ends_at DESC
                    LIMIT 1
                 ) flag_rental ON true
                 LEFT JOIN LATERAL (
                    SELECT string_agg(
                        CASE category
                          WHEN 'lateania_archdemon' THEN 'LMG'
                          WHEN 'lateania_frontier_king' THEN 'LKN'
                          WHEN 'lateania_sundering_deep' THEN 'LYS'
                          WHEN 'lateania_kaethyr_ascendant' THEN 'LKA'
                          WHEN 'nethack_amulet' THEN 'NHA'
                          WHEN 'nethack_ascension' THEN 'NHY'
                          WHEN 'dcss_orb' THEN 'DCO'
                          WHEN 'dcss_win' THEN 'DCW'
                          WHEN 'brogue_escape' THEN 'BRE'
                          WHEN 'brogue_mastery' THEN 'BRM'
                          WHEN 'greendragon_dragon' THEN 'GDS'
                          WHEN 'darkroom_escape' THEN 'ADE'
                          WHEN 'darkroom_beacon' THEN 'ADB'
                          -- Monthly like the boards below, rankless like the
                          -- milestones above: one holder, so no rank digit
                          -- (`profile_award::is_rankless_award`).
                          WHEN 'crown' THEN 'CRWN'
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
                                   WHEN 'crown' THEN 5
                                   WHEN 'tetris' THEN 2
                                   WHEN 'twenty_forty_eight' THEN 3
                                   WHEN 'snake' THEN 4
                                   WHEN 'lateania_archdemon' THEN 10
                                   WHEN 'lateania_frontier_king' THEN 11
                                   WHEN 'lateania_sundering_deep' THEN 12
                                   WHEN 'lateania_kaethyr_ascendant' THEN 13
                                   WHEN 'nethack_amulet' THEN 14
                                   WHEN 'nethack_ascension' THEN 15
                                   WHEN 'greendragon_dragon' THEN 16
                                   WHEN 'dcss_orb' THEN 17
                                   WHEN 'dcss_win' THEN 18
                                   WHEN 'brogue_escape' THEN 19
                                   WHEN 'brogue_mastery' THEN 20
                                   WHEN 'darkroom_escape' THEN 21
                                   WHEN 'darkroom_beacon' THEN 22
                                   ELSE 99
                                 END
                    ) AS badges
                    FROM profile_awards pa
                    WHERE pa.user_id = u.id
                      AND pa.rank <= $6
                      AND (
                        pa.period_month = (date_trunc('month', now() AT TIME ZONE 'UTC')::date - INTERVAL '1 month')::date
                        OR pa.category = ANY($7)
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
                    &milestone_categories,
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

    /// Friends and ignores from one read. Both lists live in the same
    /// `users.settings` document and the chat snapshot needs both on every
    /// pass, so calling the two single-list helpers fetched the identical row
    /// twice: 11.1M `SELECT settings` calls in an 18-day window, exactly 2.006
    /// per snapshot.
    pub async fn friend_and_ignored_user_ids(
        client: &Client,
        user_id: Uuid,
    ) -> Result<(Vec<Uuid>, Vec<Uuid>)> {
        let settings = Self::settings_for_user(client, user_id).await?;
        Ok((
            extract_uuid_ids(&settings, FRIEND_USER_IDS_KEY),
            extract_uuid_ids(&settings, IGNORED_USER_IDS_KEY),
        ))
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

    pub async fn translate_mine_to_en(client: &Client, user_id: Uuid) -> Result<bool> {
        let settings = Self::settings_for_user(client, user_id).await?;
        Ok(extract_translate_mine_to_en(&settings))
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

    /// Persist the chosen interaction mode (keyboard / mouse / hybrid).
    pub async fn set_interaction_mode(
        client: &Client,
        user_id: Uuid,
        mode: InteractionMode,
    ) -> Result<()> {
        let value = mode.as_str();
        let updated = client
            .execute(
                "UPDATE users
                 SET settings = settings || jsonb_build_object($1::text, $2::text),
                     updated = current_timestamp
                 WHERE id = $3",
                &[&INTERACTION_MODE_KEY, &value, &user_id],
            )
            .await?;
        if updated == 0 {
            bail!("user not found");
        }
        Ok(())
    }

    /// Persist whether the aquarium tray is open so it survives reconnects.
    pub async fn set_show_aquarium_tray(client: &Client, user_id: Uuid, shown: bool) -> Result<()> {
        let updated = client
            .execute(
                "UPDATE users
                 SET settings = settings || jsonb_build_object($1::text, $2::bool),
                     updated = current_timestamp
                 WHERE id = $3",
                &[&SHOW_AQUARIUM_TRAY_KEY, &shown, &user_id],
            )
            .await?;
        if updated == 0 {
            bail!("user not found");
        }
        Ok(())
    }

    /// Atomically mark the clubhouse first-visit tutorial as completed.
    pub async fn set_clubhouse_tutorial_done(client: &Client, user_id: Uuid) -> Result<()> {
        let updated = client
            .execute(
                "UPDATE users
                 SET settings = settings || jsonb_build_object($1::text, true),
                     updated = current_timestamp
                 WHERE id = $2",
                &[&CLUBHOUSE_TUTORIAL_DONE_KEY, &user_id],
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

    pub async fn set_admin(
        client: &impl GenericClient,
        user_id: Uuid,
        is_admin: bool,
    ) -> Result<()> {
        let updated = client
            .execute(
                "UPDATE users
                 SET is_admin = $1, updated = current_timestamp
                 WHERE id = $2",
                &[&is_admin, &user_id],
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
    // One badge per game: the lesser milestone drops out whenever the player
    // also holds a higher one on that game's ladder (see `BADGE_LADDERS`).
    // Profile views still list every award; chat author labels show only the
    // top of each ladder, so a shelf of crowns cannot crowd out the message.
    let badges = top_badge_per_game(raw.split_whitespace()).join(" ");
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

/// The chosen interaction mode, or `None` if the user has never picked one -
/// which is the signal to show the first-run onboarding prompt.
pub fn extract_interaction_mode(settings: &Value) -> Option<InteractionMode> {
    settings
        .get(INTERACTION_MODE_KEY)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(InteractionMode::from_settings_str)
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
        Some("auto") => RightSidebarMode::Auto,
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

/// The account default for the room-list rail. Mirrors
/// `extract_right_sidebar_mode`, including its legacy bool fallback: accounts
/// that predate the mode key only stored `show_room_list_sidebar`, and the bool
/// is still written alongside the mode so a rollback keeps working.
pub fn extract_room_list_mode(settings: &Value) -> RoomListMode {
    match settings
        .get(ROOM_LIST_MODE_KEY)
        .and_then(Value::as_str)
        .map(str::trim)
    {
        Some("off") => RoomListMode::Off,
        Some("auto") => RoomListMode::Auto,
        Some("on") => RoomListMode::On,
        _ if extract_show_room_list_sidebar(settings) => RoomListMode::On,
        _ => RoomListMode::Off,
    }
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

/// The language chat translations render into for this user. Defaults to
/// English: inert for the English-reading majority (their messages are
/// already in it), one settings flip for everyone else.
pub fn extract_translate_to(settings: &Value) -> crate::models::message_translation::TranslateLang {
    settings
        .get(TRANSLATE_TO_KEY)
        .and_then(Value::as_str)
        .and_then(crate::models::message_translation::TranslateLang::from_key)
        .unwrap_or(crate::models::message_translation::TranslateLang::En)
}

/// Tweak: auto-translate foreign-script messages arriving in the room being
/// viewed (plus anything already cached). Opt-in; defaults to false so
/// translation stays on-demand (`t`) until the user asks for more.
pub fn extract_auto_translate(settings: &Value) -> bool {
    settings
        .get(AUTO_TRANSLATE_KEY)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Tweak: pre-translate this author's outgoing messages to English at send
/// time, warming the shared cache so English readers see them without
/// asking. Opt-in; defaults to false since it spends an API call per
/// message the author writes.
pub fn extract_translate_mine_to_en(settings: &Value) -> bool {
    settings
        .get(TRANSLATE_MINE_TO_EN_KEY)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Tweak: land on Home (Dashboard, page 1) instead of the Clubhouse (page 0)
/// when a session starts. Opt-in; defaults to false so sessions land in the
/// clubhouse tavern like today.
pub fn extract_land_on_home(settings: &Value) -> bool {
    settings
        .get(LAND_ON_HOME_KEY)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Whether the aquarium tray was open when the user last toggled it; defaults
/// to true so the tray appears as soon as the Aquarium is unlocked, the same
/// way `show_pet_strip` reveals the companion. Rendering is gated on the
/// entitlement, so this stays inert for everyone who does not own one.
pub fn extract_show_aquarium_tray(settings: &Value) -> bool {
    settings
        .get(SHOW_AQUARIUM_TRAY_KEY)
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// Tweak: show the pet strip above the chat composer (pet owners only);
/// defaults to true so the companion appears as soon as it is unlocked.
pub fn extract_show_pet_strip(settings: &Value) -> bool {
    settings
        .get(SHOW_PET_STRIP_KEY)
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// True once the user has finished (or skipped) the clubhouse first-visit
/// tutorial; defaults to false so brand-new users get the walkthrough.
pub fn extract_clubhouse_tutorial_done(settings: &Value) -> bool {
    settings
        .get(CLUBHOUSE_TUTORIAL_DONE_KEY)
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

/// Theme ids the user has starred, in the order they starred them. Ids are
/// opaque strings rather than uuids and are not validated against the theme
/// table here: a theme that gets renamed or retired simply stops matching, and
/// the stale entry is inert until the user unstars it.
pub fn extract_favorite_theme_ids(settings: &Value) -> Vec<String> {
    let Some(entries) = settings
        .get(FAVORITE_THEME_IDS_KEY)
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };

    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(id) = entry.as_str().map(str::trim).filter(|id| !id.is_empty()) else {
            continue;
        };
        if seen.insert(id.to_string()) {
            out.push(id.to_string());
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
#[path = "user_internal_test.rs"]
mod user_internal_test;
