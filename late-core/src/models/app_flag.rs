//! Process-wide switches, stored as rows so every replica reads the same
//! answer and a restart changes nothing (root CONTEXT.md, multi-replica
//! rule). This module owns every read and write of `app_flags`.
//!
//! The roster is a closed enum: adding a switch means adding a variant, a
//! seed row in a migration, and a field on [`AppFlags`], and the exhaustive
//! matches below break the build until all three exist. A row missing for a
//! known variant is a load error, never a default.

use anyhow::{Context, Result, bail};
use tokio_postgres::Client;

/// Cross-process refresh channel. Any insert or update on `app_flags` fires
/// it (migration 171 trigger); a listener re-reads the whole table rather
/// than trusting the payload, which only names the key for logs.
pub const APP_FLAG_CHANGED_CHANNEL: &str = "app_flag_changed";

/// Every switch the code knows about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppFlag {
    /// First contact's kill switch (`/haunt on|off`): while off no haunting
    /// stage arms and a live one drops mid-scene.
    HauntEnabled,
    /// First contact's fuse (`/haunt live on|off`): while unlit only staff
    /// (admins and moderators) are haunted; lit, stage 1 fires for everyone
    /// and the eligibility gate decides who goes further.
    HauntLive,
    /// The daily paper's kill switch (`/paper off`): while off the sweeper
    /// prints nothing and `/paper` answers unavailable.
    PaperEnabled,
    /// The paper's "Outside" page (`/paper outside on|off`): the grounded
    /// look at the world beyond late.sh. On from the start; the switch is
    /// there for the day it reads like slop.
    PaperOutsideEnabled,
    /// The Artboard gallery's kill switch (`/gallery on|off`): while off
    /// nothing can be hung or applauded and the rail hides the gallery.
    ArtboardGalleryEnabled,
}

impl AppFlag {
    pub fn key(self) -> &'static str {
        match self {
            Self::HauntEnabled => "haunt_enabled",
            Self::HauntLive => "haunt_live",
            Self::PaperEnabled => "paper_enabled",
            Self::PaperOutsideEnabled => "paper_outside_enabled",
            Self::ArtboardGalleryEnabled => "artboard_gallery_enabled",
        }
    }
}

/// The whole table, read in one query. Sessions hold this behind a `watch`
/// and read it on the tick path, so it stays a plain copyable struct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppFlags {
    pub haunt_enabled: bool,
    pub haunt_live: bool,
    pub paper_enabled: bool,
    pub paper_outside_enabled: bool,
    pub artboard_gallery_enabled: bool,
}

impl AppFlags {
    /// Read every known switch. Bails when a known switch has no row: the
    /// migration that adds a variant must seed it, and a silent default
    /// here would hide a forgotten seed until it mattered.
    pub async fn load(client: &Client) -> Result<Self> {
        let rows = client
            .query("SELECT key, enabled FROM app_flags", &[])
            .await
            .context("loading app flags")?;
        let lookup = |flag: AppFlag| -> Result<bool> {
            let key = flag.key();
            match rows.iter().find(|row| row.get::<_, String>("key") == key) {
                Some(row) => Ok(row.get("enabled")),
                None => bail!("app flag {key} has no row; seed it in a migration"),
            }
        };
        Ok(Self {
            haunt_enabled: lookup(AppFlag::HauntEnabled)?,
            haunt_live: lookup(AppFlag::HauntLive)?,
            paper_enabled: lookup(AppFlag::PaperEnabled)?,
            paper_outside_enabled: lookup(AppFlag::PaperOutsideEnabled)?,
            artboard_gallery_enabled: lookup(AppFlag::ArtboardGalleryEnabled)?,
        })
    }

    /// Flip one switch. The trigger tells every replica, including the one
    /// that wrote it. Bails when the row is missing, same reasoning as
    /// [`AppFlags::load`].
    pub async fn set(
        client: &impl deadpool_postgres::GenericClient,
        flag: AppFlag,
        enabled: bool,
    ) -> Result<()> {
        let updated = client
            .execute(
                "UPDATE app_flags SET enabled = $2, updated = current_timestamp WHERE key = $1",
                &[&flag.key(), &enabled],
            )
            .await
            .context("setting app flag")?;
        if updated == 0 {
            bail!("app flag {} has no row; seed it in a migration", flag.key());
        }
        Ok(())
    }
}

pub async fn listen_for_app_flag_changes(client: &Client) -> Result<()> {
    client
        .batch_execute(&format!("LISTEN {APP_FLAG_CHANGED_CHANNEL};"))
        .await?;
    Ok(())
}

#[cfg(test)]
#[path = "app_flag_test.rs"]
mod app_flag_test;
