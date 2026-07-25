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
        @data
        pub user_id: Uuid,
        pub fingerprint: String,
        pub label: Option<String>,
        pub settings: Value,
    }
}

const ROOM_LIST_MODE_KEY: &str = "room_list_mode";
const RIGHT_SIDEBAR_MODE_KEY: &str = "right_sidebar_mode";

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

    /// Load just this device's rail layout, or `None` when the key has no
    /// stored layout (or no row yet) and should follow the account default.
    pub async fn layout_for(
        client: &Client,
        user_id: Uuid,
        fingerprint: &str,
    ) -> Result<Option<KeyLayout>> {
        let key = Self::find_by_fingerprint(client, user_id, fingerprint).await?;
        Ok(key.and_then(|key| extract_key_layout(&key.settings)))
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
