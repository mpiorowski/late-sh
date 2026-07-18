// The arcade handle: one immutable public name per account, shared by door
// games whose upstream binaries key saves and public score files by player
// name (DCSS today; NetHack may adopt it later). Validation and uniqueness
// live in late-core (`models::arcade_handle`); this is the session-side
// accessor, cloned into each connection like the other door services.

use anyhow::Result;
use late_core::db::Db;
use late_core::models::arcade_handle::{ArcadeHandle, ClaimOutcome};
use uuid::Uuid;

/// Thin async accessor for the account's arcade handle.
#[derive(Clone)]
pub struct ArcadeHandleService {
    db: Db,
}

impl ArcadeHandleService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// The account's claimed handle, if any.
    pub async fn get(&self, user_id: Uuid) -> Result<Option<String>> {
        let client = self.db.get().await?;
        ArcadeHandle::find_by_user_id(&client, user_id).await
    }

    /// Claim a handle for the account (first claim wins; immutable after).
    /// The caller pre-validates shape and reserved names.
    pub async fn claim(&self, user_id: Uuid, handle: &str) -> Result<ClaimOutcome> {
        let client = self.db.get().await?;
        ArcadeHandle::claim(&client, user_id, handle).await
    }
}
