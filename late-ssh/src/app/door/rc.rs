// Per-account door rc files (.nethackrc / DCSS init.txt): the session-side
// accessor plus the client half of the push contract. The DB row is the source
// of truth; at launch the door proxy sends the content to the game host as one
// SSH env request before requesting the shell, and the host materializes it as
// an ephemeral per-player file for the child.

use anyhow::Result;
use late_core::db::Db;
use late_core::models::door_rc::{DoorRc, DoorRcGame};
use uuid::Uuid;

/// SSH env variable carrying the base64-encoded rc to a door host. Sent on
/// EVERY launch, deliberately including the empty value for an account with no
/// stored rc: the empty push is what deletes the host's per-player file after
/// a clear, so "optimizing" it away would resurrect stale configs. The host's
/// no-push branch exists only for version skew (an older client that never
/// sends the request leaves the host file alone). The name is duplicated in
/// `late-nethack` and `late-dcss` (like the doors' identity derivations); keep
/// the copies in sync.
pub const RC_ENV_VAR: &str = "LATE_DOOR_RC_B64";

/// Normalize pasted rc content for storage: CRLF and bare CR become LF, and
/// control characters are dropped except the newlines and tabs a real config
/// file legitimately contains. Escape sequences lose their ESC byte here, so
/// nothing a paste smuggles in can reach the game's terminal.
pub fn sanitize_rc_paste(pasted: &str) -> String {
    let unix = pasted.replace("\r\n", "\n").replace('\r', "\n");
    unix.chars()
        .filter(|&ch| ch == '\n' || ch == '\t' || (!ch.is_control() && ch != '\u{7f}'))
        .collect()
}

/// Thin async accessor for the account's door rc files.
#[derive(Clone)]
pub struct DoorRcService {
    db: Db,
}

impl DoorRcService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Every configured rc for the account, for session-init preloading.
    pub async fn list(&self, user_id: Uuid) -> Result<Vec<(DoorRcGame, String)>> {
        let client = self.db.get().await?;
        DoorRc::list_for_user(&client, user_id).await
    }

    /// Fire-and-forget save. Logs its own failure; the App's in-memory copy is
    /// already updated by the caller, so a lost write surfaces as a stale rc
    /// next session, not a broken modal.
    pub fn save_task(&self, user_id: Uuid, game: DoorRcGame, content: String) {
        let db = self.db.clone();
        tokio::spawn(async move {
            let result = async {
                let client = db.get().await?;
                DoorRc::upsert(&client, user_id, game, &content).await
            }
            .await;
            if let Err(e) = result {
                tracing::error!(error = ?e, %user_id, game = game.as_key(), "failed to save door rc");
            }
        });
    }

    /// Fire-and-forget clear (back to upstream defaults). Same logging rule.
    pub fn clear_task(&self, user_id: Uuid, game: DoorRcGame) {
        let db = self.db.clone();
        tokio::spawn(async move {
            let result = async {
                let client = db.get().await?;
                DoorRc::clear(&client, user_id, game).await
            }
            .await;
            if let Err(e) = result {
                tracing::error!(error = ?e, %user_id, game = game.as_key(), "failed to clear door rc");
            }
        });
    }
}

#[cfg(test)]
#[path = "rc_test.rs"]
mod rc_test;
