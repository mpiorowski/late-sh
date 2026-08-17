//! Savable dated chat logs (the "save daily chat logs" tweak, see
//! `settings_modal`). A user who opts in gets a plain-text transcript of
//! their chat (every room they're in, plus DMs) for the current local day,
//! written to the same R2 bucket `app::files::image_upload` already uses for
//! chat images, and rebuilt at session end.
//!
//! The bucket is CDN-public by design (`image_upload::public_url`), an
//! accepted trust model for chat images (unguessable UUID path) but not one
//! this feature can reuse unmodified: a DM's other participant never
//! consented to their words being written to a URL-fetchable object under
//! someone else's account. The mitigation is `log_object_key`: the object
//! key is an HMAC of the user id and date, deterministic (so re-saving the
//! same user+day overwrites cleanly) but infeasible to guess or enumerate
//! without the server-held `chat_log_secret` - never derived from the user's
//! id in cleartext, and never surfaced to a player as a raw URL. Retrieval
//! always happens server-side (`fetch_daily_log`), rendered inside the app.

use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use late_core::db::Db;
use late_core::models::chat_message::{ChatLogRow, ChatMessage};
use late_core::models::user::User;
use uuid::Uuid;

use super::files::image_upload::{USER_AGENT, hmac_sha256, public_url, put_object};
use super::profile::svc::parse_account_tz;
use crate::config::FilesConfig;

/// Bounded well past what a real day of chat could ever reach, so a
/// malformed/oversized object can't be read into memory unbounded.
const MAX_LOG_TEXT_BYTES: usize = 4 * 1024 * 1024;

/// Deterministic, per-user-per-day object key. See the module doc comment
/// for why this needs to be unguessable rather than a plain `user_id/date`
/// path.
fn log_object_key(secret: &str, user_id: Uuid, date: NaiveDate) -> String {
    let mac = hmac_sha256(secret.as_bytes(), format!("{user_id}|{date}").as_bytes());
    format!("logs/{}.txt", hex::encode(mac))
}

/// The user's local calendar day, as `[start, end)` UTC bounds plus the
/// local date itself (used to name the log). Falls back to UTC when the
/// timezone is unset or unparseable, same tolerance as
/// `common::time::timezone_current_time`.
pub(crate) fn day_range_for_user(
    now: DateTime<Utc>,
    timezone: Option<&str>,
) -> (DateTime<Utc>, DateTime<Utc>, NaiveDate) {
    match parse_account_tz(timezone) {
        Some(tz) => {
            let date = now.with_timezone(&tz).date_naive();
            let start = local_midnight_utc(tz, date);
            let end = local_midnight_utc(tz, date.succ_opt().unwrap_or(date));
            (start, end, date)
        }
        None => {
            let date = now.date_naive();
            let start = date.and_hms_opt(0, 0, 0).unwrap_or_default().and_utc();
            let end = date
                .succ_opt()
                .unwrap_or(date)
                .and_hms_opt(0, 0, 0)
                .unwrap_or_default()
                .and_utc();
            (start, end, date)
        }
    }
}

/// Midnight on `date` in `tz`, converted to UTC. A DST gap landing exactly
/// on midnight (rare) falls back to treating the naive time as if it were
/// already UTC - harmless for a day-boundary query, never worth failing the
/// whole save over.
fn local_midnight_utc(tz: chrono_tz::Tz, date: NaiveDate) -> DateTime<Utc> {
    let naive = date.and_hms_opt(0, 0, 0).unwrap_or_default();
    match tz.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) => dt.with_timezone(&Utc),
        chrono::LocalResult::Ambiguous(dt, _) => dt.with_timezone(&Utc),
        chrono::LocalResult::None => naive.and_utc(),
    }
}

fn room_label(row: &ChatLogRow, viewer_user_id: Uuid, dm_peer_usernames: &HashMap<Uuid, String>) -> String {
    if row.room_kind == "dm" {
        let peer_id = if row.dm_user_a == Some(viewer_user_id) {
            row.dm_user_b
        } else {
            row.dm_user_a
        };
        let peer = peer_id
            .and_then(|id| dm_peer_usernames.get(&id).cloned())
            .unwrap_or_else(|| "DM".to_string());
        return format!("@{peer}");
    }
    if let Some(slug) = row.room_slug.as_deref().filter(|s| !s.is_empty()) {
        return format!("#{slug}");
    }
    if let Some(code) = row.room_language_code.as_deref().filter(|c| !c.is_empty()) {
        return format!("#lang-{code}");
    }
    format!("#{}", row.room_kind)
}

/// Format the day's rows into a plain-text transcript, oldest first, one
/// line per message: `[14:32] #lounge <alice>: hello` / `[14:33] @bob: hey`
/// for DMs. `None` if there's nothing to say - callers should skip the
/// upload entirely rather than write an empty object.
fn format_log_text(
    rows: &[ChatLogRow],
    viewer_user_id: Uuid,
    dm_peer_usernames: &HashMap<Uuid, String>,
) -> Option<String> {
    if rows.is_empty() {
        return None;
    }
    let mut out = String::new();
    for row in rows {
        let label = room_label(row, viewer_user_id, dm_peer_usernames);
        let time = row.created.format("%H:%M");
        out.push_str(&format!(
            "[{time}] {label} <{}>: {}\n",
            row.author_username, row.body
        ));
    }
    Some(out)
}

/// Query, format, and upload today's chat log for `user_id`. A no-op
/// (skips the upload) when there's nothing to say. Fire-and-forget from the
/// caller's perspective - see `ssh.rs`'s session-end hook.
pub(crate) async fn save_daily_log(
    db: &Db,
    files: &FilesConfig,
    secret: &str,
    user_id: Uuid,
    timezone: Option<&str>,
) -> Result<()> {
    let client = db.get().await?;
    let (start, end, date) = day_range_for_user(Utc::now(), timezone);
    let rows = ChatMessage::list_for_daily_log(&client, user_id, start, end).await?;

    let dm_peer_ids: Vec<Uuid> = rows
        .iter()
        .filter(|r| r.room_kind == "dm")
        .filter_map(|r| {
            let peer = if r.dm_user_a == Some(user_id) {
                r.dm_user_b
            } else {
                r.dm_user_a
            };
            peer.filter(|id| !rows.iter().any(|row| row.author_id == *id))
        })
        .collect();
    let dm_peer_usernames = User::list_usernames_by_ids(&client, &dm_peer_ids).await?;

    let Some(text) = format_log_text(&rows, user_id, &dm_peer_usernames) else {
        return Ok(());
    };

    let key = log_object_key(secret, user_id, date);
    put_object(files, &key, text.into_bytes(), "text/plain; charset=utf-8", Utc::now()).await
}

/// Fetch today's already-saved log for display, server-side only - never
/// surfaced to the user as a URL. `Ok(None)` covers both "nothing saved
/// today" (404) and any fetch failure; the settings viewer renders both
/// identically ("No messages logged today."), matching this feature's
/// best-effort tone rather than surfacing a background-op error.
pub(crate) async fn fetch_daily_log(
    files: &FilesConfig,
    secret: &str,
    user_id: Uuid,
    timezone: Option<&str>,
) -> Result<Option<String>> {
    let (_, _, date) = day_range_for_user(Utc::now(), timezone);
    let key = log_object_key(secret, user_id, date);
    let url = public_url(files, &key);

    let resp = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent(USER_AGENT)
        .build()?
        .get(&url)
        .send()
        .await?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !resp.status().is_success() {
        anyhow::bail!("chat log fetch failed: http {}", resp.status());
    }
    let bytes = resp.bytes().await?;
    if bytes.len() > MAX_LOG_TEXT_BYTES {
        anyhow::bail!("chat log unexpectedly large");
    }
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}
