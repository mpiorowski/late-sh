// Every SSH key that has ever authenticated as this account, one row per key.
// A key is created on first sight and its `user_id` follows account links, so
// after linking a phone to a desktop both keys point at the same account.
//
// The row is also where *per-device* preferences live. A session knows exactly
// which key it authenticated with, which is the only device identity late.sh
// has, so the home rail layout is stored here rather than on the account: a
// 50-column phone and a 200-column desktop on one account no longer overwrite
// each other's sidebars. `settings` is empty for a key that has never been
// configured, and an empty override means "inherit the account default".
//
// A key is not perfectly a device (one key copied to two machines, or a
// forwarded agent, reads as one identity). `RoomListMode::Auto` /
// `RightSidebarMode::Auto` cover that case by resolving from the live terminal
// width instead; see `late-ssh/src/app/render.rs`.

use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use deadpool_postgres::GenericClient;
use serde_json::{Value, json};
use tokio_postgres::Client;
use uuid::Uuid;

use super::user::{RightSidebarMode, RoomListMode};

crate::user_scoped_model! {
    table = "user_ssh_keys";
    user_field = user_id;
    params = UserSshKeyParams;
    struct UserSshKey {
        @generated
        pub last_seen: DateTime<Utc>;

        // `label` is a human name for the device, `None` until the user sets
        // one; `settings` holds this device's overrides (empty = inherit).
        // `left_at` is when a session on this device last ended, stamped at
        // the moment its keyboard went quiet (see migration 166); `None` until
        // the first one does.
        @data
        pub user_id: Uuid,
        pub fingerprint: String,
        pub label: Option<String>,
        pub settings: Value,
        pub left_at: Option<DateTime<Utc>>,
    }
}

const ROOM_LIST_MODE_KEY: &str = "room_list_mode";
const RIGHT_SIDEBAR_MODE_KEY: &str = "right_sidebar_mode";
const AUDIO_MUTED_KEY: &str = "audio_muted";
const AUDIO_VOLUME_KEY: &str = "audio_volume_percent";

/// One device's music mute and volume, and the only source of truth for
/// either. Both live on the key rather than the account because they belong
/// to the machine with the speakers: muting on a laptop must not silence the
/// desktop. The server writes this from what the paired CLI reports after
/// applying a control (the webview helper's reports are never persisted; the
/// CLI is the surface of record), so `m`, `+`/`-`, a media key, and `/brb`'s
/// auto-mute all land here through one path, and a session resumes exactly
/// where the last one left off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyAudio {
    pub muted: bool,
    pub volume_percent: u8,
}

impl KeyAudio {
    pub fn to_value(self) -> Value {
        json!({
            AUDIO_MUTED_KEY: self.muted,
            AUDIO_VOLUME_KEY: self.volume_percent,
        })
    }
}

/// This device's stored audio, or `None` when the key has never reported any
/// and the caller should fall back to its own default. Written as a pair and
/// read as a pair: a blob missing either half reads as `None` rather than
/// half-applying, matching [`extract_key_layout`].
pub fn extract_key_audio(settings: &Value) -> Option<KeyAudio> {
    let muted = settings.get(AUDIO_MUTED_KEY).and_then(Value::as_bool)?;
    let volume_percent = settings
        .get(AUDIO_VOLUME_KEY)
        .and_then(Value::as_u64)
        .and_then(|v| u8::try_from(v).ok())
        .filter(|v| *v <= 100)?;
    Some(KeyAudio {
        muted,
        volume_percent,
    })
}

/// One device's home rail layout. Stored complete or not at all: both rails are
/// always written together, so there is no half-configured state to reason
/// about, and a key with no stored layout inherits the account default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyLayout {
    pub room_list_mode: RoomListMode,
    pub right_sidebar_mode: RightSidebarMode,
}

impl KeyLayout {
    pub fn to_value(self) -> Value {
        json!({
            ROOM_LIST_MODE_KEY: self.room_list_mode.as_str(),
            RIGHT_SIDEBAR_MODE_KEY: self.right_sidebar_mode.as_str(),
        })
    }
}

/// The device layout stored on a key, or `None` when this key has never been
/// configured and should follow the account default. A blob missing or
/// mangling either mode reads as `None` rather than half-applying: the pair is
/// written atomically, so a partial one can only come from hand-editing.
pub fn extract_key_layout(settings: &Value) -> Option<KeyLayout> {
    let room_list_mode = settings
        .get(ROOM_LIST_MODE_KEY)
        .and_then(Value::as_str)
        .and_then(RoomListMode::from_key)?;
    let right_sidebar_mode = settings
        .get(RIGHT_SIDEBAR_MODE_KEY)
        .and_then(Value::as_str)
        .and_then(RightSidebarMode::from_key)?;
    Some(KeyLayout {
        room_list_mode,
        right_sidebar_mode,
    })
}

impl UserSshKey {
    /// Record a key as belonging to `user_id`, creating it on first sight and
    /// re-pointing it (plus bumping `last_seen`) on every later connect. Takes
    /// a `GenericClient` so account linking can run it inside its transaction.
    pub async fn ensure(
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

    pub async fn touch(client: &Client, fingerprint: &str) -> Result<()> {
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

    pub async fn find_by_fingerprint(
        client: &Client,
        user_id: Uuid,
        fingerprint: &str,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM user_ssh_keys WHERE fingerprint = $1 AND user_id = $2",
                &[&fingerprint, &user_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// Store this device's rail layout. Scoped by owner as well as fingerprint
    /// so a stale fingerprint can never write onto another account's key.
    pub async fn set_layout(
        client: &Client,
        user_id: Uuid,
        fingerprint: &str,
        layout: KeyLayout,
    ) -> Result<()> {
        let updated = client
            .execute(
                "UPDATE user_ssh_keys
                 SET settings = settings || $1::jsonb,
                     updated = current_timestamp
                 WHERE fingerprint = $2 AND user_id = $3",
                &[&layout.to_value(), &fingerprint, &user_id],
            )
            .await?;
        if updated == 0 {
            bail!("ssh key not found");
        }
        Ok(())
    }

    /// Load just this device's stored mute/volume, or `None` when the key has
    /// never reported any (or has no row yet).
    pub async fn audio_for(
        client: &Client,
        user_id: Uuid,
        fingerprint: &str,
    ) -> Result<Option<KeyAudio>> {
        let key = Self::find_by_fingerprint(client, user_id, fingerprint).await?;
        Ok(key.and_then(|key| extract_key_audio(&key.settings)))
    }

    /// Store this device's mute/volume. Scoped by owner as well as fingerprint
    /// so a stale fingerprint can never write onto another account's key.
    pub async fn set_audio(
        client: &Client,
        user_id: Uuid,
        fingerprint: &str,
        audio: KeyAudio,
    ) -> Result<()> {
        let updated = client
            .execute(
                "UPDATE user_ssh_keys
                 SET settings = settings || $1::jsonb,
                     updated = current_timestamp
                 WHERE fingerprint = $2 AND user_id = $3",
                &[&audio.to_value(), &fingerprint, &user_id],
            )
            .await?;
        if updated == 0 {
            bail!("ssh key not found");
        }
        Ok(())
    }

    /// Take this device's mark: when a session on it last left the app, or
    /// `None` when the key has never ended one (or has no row yet). The read
    /// clears the column, so a mark is served to exactly one session. That
    /// is what makes a lost [`set_left_at`](Self::set_left_at) safe: the
    /// next session falls back to the default `/summary` window rather than
    /// inheriting a leave from before the session that failed to write,
    /// which would summarize messages already read.
    pub async fn take_left_at(
        client: &Client,
        user_id: Uuid,
        fingerprint: &str,
    ) -> Result<Option<DateTime<Utc>>> {
        let row = client
            .query_opt(
                "UPDATE user_ssh_keys
                 SET left_at = NULL, updated = current_timestamp
                 WHERE fingerprint = $1 AND user_id = $2
                 RETURNING (SELECT left_at FROM user_ssh_keys
                            WHERE fingerprint = $1 AND user_id = $2) AS left_at",
                &[&fingerprint, &user_id],
            )
            .await?;
        Ok(row.and_then(|row| row.get("left_at")))
    }

    /// Record that a session on this device ended, with `left_at` the moment
    /// its keyboard went quiet rather than the moment it disconnected.
    /// Scoped by owner as well as fingerprint so a stale fingerprint can
    /// never write onto another account's key. Last write wins: two live
    /// sessions on one key are one device as far as late.sh can tell.
    pub async fn set_left_at(
        client: &Client,
        user_id: Uuid,
        fingerprint: &str,
        left_at: DateTime<Utc>,
    ) -> Result<()> {
        let updated = client
            .execute(
                "UPDATE user_ssh_keys
                 SET left_at = $1, updated = current_timestamp
                 WHERE fingerprint = $2 AND user_id = $3",
                &[&left_at, &fingerprint, &user_id],
            )
            .await?;
        if updated == 0 {
            bail!("ssh key not found");
        }
        Ok(())
    }

    /// Re-point every key of one account at another, used when linking two
    /// accounts together. Bound on `tokio_postgres`'s client trait rather than
    /// the pool's, because linking runs this inside its own transaction.
    pub async fn move_to_user(
        client: &impl tokio_postgres::GenericClient,
        from_user_id: Uuid,
        to_user_id: Uuid,
    ) -> Result<()> {
        client
            .execute(
                "UPDATE user_ssh_keys
                 SET user_id = $1, updated = current_timestamp
                 WHERE user_id = $2",
                &[&to_user_id, &from_user_id],
            )
            .await?;
        Ok(())
    }
}
