//! The #lounge system feed: one background task drains the global activity
//! broadcast, keeps the events `filter::lounge_includes` approves, and posts
//! each as a persisted chat message authored by the `system` bot user. The
//! chat renderer shows these as authorless single rows (see
//! `chat/ui_text.rs::parse_system_line`); IRC clients see ordinary PRIVMSGs
//! from the `system` nick.
//!
//! Bodies never contain `@`, so the mention pipeline stays quiet, and the
//! system author is excluded from unread counts at the SQL layer
//! (`ChatRoomMember::unread_counts_for_user`).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::Result;
use late_core::db::Db;
use late_core::models::{
    chat_room_member::ChatRoomMember,
    user::{User, UserParams},
};
use serde_json::json;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use crate::app::chat::svc::{ChatService, SendLoungeMessageTask};
use crate::usernames::UsernameDirectory;

use super::channel::ActivityReceiver;
use super::event::{ActivityEvent, ActivityKind};
use super::filter::lounge_includes;

/// The system feed's author row. Same lazily-ensured bot pattern as the
/// ghost users; `settings.system` additionally marks it for the unread-count
/// exclusion in `ChatRoomMember::unread_counts_for_user`.
///
/// Prod note: `system` was squatted by a real zero-message account until
/// 2026-07-12, when the owner renamed it to `system-9` to free the nick.
/// First deploy creates this row and the case-insensitive unique username
/// index reserves it from then on.
pub const SYSTEM_FINGERPRINT: &str = "system-fp-000";
pub const SYSTEM_USERNAME: &str = "system";

/// Body prefix marking a system line. Deliberately readable: IRC clients see
/// it verbatim (`<system> · mira joined`). The TUI only styles a message as
/// a system row when the body carries this prefix AND the author is the
/// feed bot, so neither a human squatting the nick nor a pasted `· ` can
/// spoof the authorless style.
pub const SYSTEM_LINE_PREFIX: &str = "· ";

/// Same user re-sitting at the same table (or re-triggering any one event
/// shape) within this window is dropped: seat toggling must not fill the
/// lounge. Everything else in the feed is naturally rare.
const REPEAT_WINDOW: Duration = Duration::from_secs(10 * 60);

/// Spawn the forwarder. Failure to ensure the system user disables the feed
/// for this process (logged); it does not take the server down.
pub fn start_lounge_feed_task(
    db: Db,
    chat: ChatService,
    username_directory: UsernameDirectory,
    mut rx: ActivityReceiver,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let system_user_id = match ensure_system_user(&db, &username_directory).await {
            Ok(id) => id,
            Err(error) => {
                tracing::warn!(?error, "lounge system feed disabled: no system user");
                return;
            }
        };
        let mut recent: HashMap<String, Instant> = HashMap::new();
        loop {
            let event = match rx.recv().await {
                Ok(event) => event,
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "lounge system feed lagged; events dropped");
                    continue;
                }
                Err(RecvError::Closed) => break,
            };
            if !lounge_includes(&event) {
                continue;
            }
            if is_repeat(&mut recent, &event) {
                continue;
            }
            chat.send_lounge_message_task(SendLoungeMessageTask {
                user_id: system_user_id,
                body: format!("{SYSTEM_LINE_PREFIX}{} {}", event.username, event.action),
                request_id: None,
                join_if_needed: true,
                failure_log: "failed to post lounge system line",
            });
        }
    })
}

/// Drop a second identical (user, event-shape) line inside `REPEAT_WINDOW`.
/// Keyed on the kind's discriminant plus its game, not the full action text,
/// so "sat down at poker" twice in a row is a repeat even across re-seats.
fn is_repeat(recent: &mut HashMap<String, Instant>, event: &ActivityEvent) -> bool {
    let key = repeat_key(event);
    let now = Instant::now();
    recent.retain(|_, at| now.duration_since(*at) < REPEAT_WINDOW);
    match recent.get(&key) {
        Some(_) => true,
        None => {
            recent.insert(key, now);
            false
        }
    }
}

fn repeat_key(event: &ActivityEvent) -> String {
    let user = event
        .user_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| event.username.clone());
    let shape = match &event.kind {
        ActivityKind::UserJoined => "joined".to_string(),
        ActivityKind::GameStarted { game } => format!("started:{}", game.key()),
        ActivityKind::BossSlain { game, boss } => format!("boss:{}:{boss}", game.key()),
        ActivityKind::SatDown { game } => format!("sat:{}", game.key()),
        ActivityKind::DailyResult { game } => format!("daily:{game}"),
        ActivityKind::GameWon { game, .. } => format!("won:{}", game.key()),
        ActivityKind::GameEvent { game, detail } => format!("event:{}:{detail}", game.key()),
        ActivityKind::GamePlayed { game, .. } => format!("played:{}", game.key()),
        ActivityKind::GameScored { game, .. } => format!("scored:{}", game.key()),
        ActivityKind::BonsaiWatered => "bonsai-watered".to_string(),
        ActivityKind::BonsaiLost { .. } => "bonsai-lost".to_string(),
    };
    format!("{user}:{shape}")
}

/// Mirror of the ghost-user ensure flow (`app/ai/ghost.rs::ensure_user`) with
/// the extra `system` settings flag. Idempotent per process start.
async fn ensure_system_user(db: &Db, username_directory: &UsernameDirectory) -> Result<Uuid> {
    let client = db.get().await?;
    let settings = json!({ "bot": true, "system": true });

    let user =
        if let Some(existing) = User::find_by_fingerprint(&client, SYSTEM_FINGERPRINT).await? {
            User::update_settings(&client, existing.id, &settings).await?;
            User::ensure_ssh_key(&client, existing.id, SYSTEM_FINGERPRINT).await?;
            existing
        } else {
            let created = User::create(
                &client,
                UserParams {
                    fingerprint: SYSTEM_FINGERPRINT.to_string(),
                    username: SYSTEM_USERNAME.to_string(),
                    settings,
                },
            )
            .await?;
            User::ensure_ssh_key(&client, created.id, SYSTEM_FINGERPRINT).await?;
            created
        };

    ChatRoomMember::auto_join_public_rooms(&client, user.id).await?;
    crate::usernames::upsert(username_directory, user.id, SYSTEM_USERNAME);
    Ok(user.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_window_drops_same_shape_and_keeps_distinct() {
        let mut recent = HashMap::new();
        let sit = ActivityEvent::sat_down(
            Uuid::nil(),
            "mira",
            crate::app::activity::event::ActivityGame::Poker,
        );
        assert!(!is_repeat(&mut recent, &sit));
        assert!(is_repeat(&mut recent, &sit));

        let other_game = ActivityEvent::sat_down(
            Uuid::nil(),
            "mira",
            crate::app::activity::event::ActivityGame::Chess,
        );
        assert!(!is_repeat(&mut recent, &other_game));

        let other_user = ActivityEvent::sat_down(
            Uuid::now_v7(),
            "someone-else",
            crate::app::activity::event::ActivityGame::Poker,
        );
        assert!(!is_repeat(&mut recent, &other_user));
    }
}
