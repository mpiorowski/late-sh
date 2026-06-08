use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use late_core::models::account_link;
use late_core::models::bonsai::{BonsaiV2Tree, Tree};
use late_core::models::marketplace;
use late_core::models::profile::{Profile, ProfileParams};
use late_core::models::profile_award::{ProfileAward, list_profile_awards_for_user};
use late_core::models::user::{User, sanitize_username_input};
use tokio_postgres::error::SqlState;
use uuid::Uuid;

use late_core::MutexRecover;
use late_core::db::Db;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, watch};
use tracing::{Instrument, info_span};

use crate::session::{SessionMessage, SessionRegistry};
use crate::state::ActiveUsers;
use crate::usernames::{self, UsernameDirectory};

#[derive(Clone)]
pub struct ProfileService {
    db: Db,
    snapshot_txs: Arc<Mutex<HashMap<Uuid, watch::Sender<ProfileSnapshot>>>>,
    evt_tx: broadcast::Sender<ProfileEvent>,
    active_users: ActiveUsers,
    username_directory: Option<UsernameDirectory>,
    session_registry: Option<SessionRegistry>,
}

#[derive(Clone, Default)]
pub struct ProfileSnapshot {
    pub user_id: Option<Uuid>,
    pub profile: Option<Profile>,
    pub chip_balance: Option<i64>,
    pub bonsai: Option<Tree>,
    pub bonsai_v2: Option<BonsaiV2Tree>,
    pub dynamic_bonsai_selected: bool,
    pub aquarium_fish: Vec<(String, usize)>,
    pub profile_awards: Vec<ProfileAward>,
}

#[derive(Clone, Debug)]
pub enum ProfileEvent {
    Saved {
        user_id: Uuid,
    },
    AccountLinkCodeCreated {
        user_id: Uuid,
        code: String,
        expires_at: DateTime<Utc>,
    },
    AccountLinkPeerLoaded {
        user_id: Uuid,
        peer_user_id: Uuid,
        peer_username: String,
        peer_created: DateTime<Utc>,
    },
    AccountLinked {
        kept_user_id: Uuid,
        abandoned_user_id: Uuid,
        kept_username: String,
        abandoned_username: String,
    },
    Error {
        user_id: Uuid,
        message: String,
    },
    /// Connect-time summary of friends whose birthday is today or within the
    /// next week. Surfaced as an in-app banner.
    BirthdayAlert {
        user_id: Uuid,
        message: String,
    },
}

/// Build a one-line alert from tracked `(username, MM-DD)` pairs: anyone whose
/// birthday is today, then anyone within the next 7 days. `None` if nobody
/// qualifies. Pure — `today` is injected so it is unit-testable.
pub(crate) fn build_birthday_alert(
    birthdays: &[(String, String)],
    today: NaiveDate,
) -> Option<String> {
    use late_core::models::birthday::{days_until, is_today};
    let mut today_names = Vec::new();
    let mut soon = Vec::new();
    for (name, mmdd) in birthdays {
        if is_today(mmdd, today) {
            today_names.push(name.clone());
        } else if let Some(d) = days_until(mmdd, today)
            && (1..=7).contains(&d)
        {
            soon.push((d, name.clone()));
        }
    }
    let mut parts = Vec::new();
    if !today_names.is_empty() {
        parts.push(format!("{} — birthday today!", today_names.join(", ")));
    }
    soon.sort();
    for (d, name) in soon {
        let when = if d == 1 {
            "tomorrow".to_string()
        } else {
            format!("in {d} days")
        };
        parts.push(format!("{name}'s birthday {when}"));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn date_for_timezone(now: DateTime<Utc>, timezone: Option<&str>) -> NaiveDate {
    let Some(timezone) = timezone.map(str::trim).filter(|value| !value.is_empty()) else {
        return now.date_naive();
    };
    timezone
        .parse::<chrono_tz::Tz>()
        .map(|tz| now.with_timezone(&tz).date_naive())
        .unwrap_or_else(|_| now.date_naive())
}

impl ProfileService {
    pub fn new(db: Db, active_users: ActiveUsers) -> Self {
        let (evt_tx, _) = broadcast::channel(512);

        Self {
            db,
            snapshot_txs: Arc::new(Mutex::new(HashMap::new())),
            evt_tx,
            active_users,
            username_directory: None,
            session_registry: None,
        }
    }

    pub fn with_username_directory(mut self, username_directory: UsernameDirectory) -> Self {
        self.username_directory = Some(username_directory);
        self
    }

    pub fn with_session_registry(mut self, session_registry: SessionRegistry) -> Self {
        self.session_registry = Some(session_registry);
        self
    }

    // Snapshot
    pub fn subscribe_snapshot(&self, user_id: Uuid) -> watch::Receiver<ProfileSnapshot> {
        self.snapshot_sender(user_id).subscribe()
    }
    fn snapshot_sender(&self, user_id: Uuid) -> watch::Sender<ProfileSnapshot> {
        let mut channels = self.snapshot_txs.lock_recover();
        let make = || watch::channel(ProfileSnapshot::default()).0;
        let sender = channels.entry(user_id).or_insert_with(&make);
        if sender.is_closed() {
            *sender = make();
        }
        sender.clone()
    }
    fn publish_snapshot(&self, user_id: Uuid, snapshot: ProfileSnapshot) -> Result<()> {
        self.snapshot_sender(user_id).send(snapshot)?;
        Ok(())
    }

    // Events
    pub fn subscribe_events(&self) -> broadcast::Receiver<ProfileEvent> {
        self.evt_tx.subscribe()
    }
    fn publish_event(&self, event: ProfileEvent) {
        if let Err(e) = self.evt_tx.send(event) {
            tracing::error!(%e, "failed to send profile event");
        }
    }

    // Prune
    pub fn prune_user_snapshot_channel(&self, user_id: Uuid) {
        let mut channels = self.snapshot_txs.lock_recover();
        // Called from ProfileState::drop while that state's receiver still exists.
        // Remove when there are no receivers, or only the dropping receiver remains.
        let should_remove = channels
            .get(&user_id)
            .is_some_and(should_prune_snapshot_sender);
        if should_remove {
            channels.remove(&user_id);
        }
    }

    // Actions
    pub fn find_profile(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service.do_find_profile(user_id).await {
                    late_core::error_span!(
                        "profile_find_failed",
                        error = ?e,
                        "failed to find profile"
                    );
                }
            }
            .instrument(info_span!("profile.find_task", user_id = %user_id)),
        );
    }

    #[tracing::instrument(skip(self), fields(user_id = %user_id))]
    async fn do_find_profile(&self, user_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        let profile = Profile::load_with_chip_balance(&client, user_id).await?;
        let bonsai = Tree::find_by_user_id(&client, user_id).await?;
        let bonsai_v2 = BonsaiV2Tree::find_by_user_id(&client, user_id).await?;
        let dynamic_bonsai_selected =
            marketplace::is_dynamic_bonsai_selected(&client, user_id).await?;
        let aquarium_fish = marketplace::active_aquarium_fish_for_user(&client, user_id).await?;
        let profile_awards = list_profile_awards_for_user(&client, user_id).await?;
        self.publish_snapshot(
            user_id,
            ProfileSnapshot {
                user_id: Some(user_id),
                profile: Some(profile.profile),
                chip_balance: Some(profile.chip_balance),
                bonsai,
                bonsai_v2,
                dynamic_bonsai_selected,
                aquarium_fish,
                profile_awards,
            },
        )?;
        Ok(())
    }

    /// Fire-and-forget: on connect, surface a single banner for friends whose
    /// birthday is today or within the next week.
    pub fn check_birthdays_task(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service.do_check_birthdays(user_id).await {
                    late_core::error_span!(
                        "birthday_alert_failed",
                        error = ?e,
                        user_id = %user_id,
                        "failed to compute birthday alert"
                    );
                }
            }
            .instrument(info_span!("profile.check_birthdays", user_id = %user_id)),
        );
    }

    async fn do_check_birthdays(&self, user_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        let profile = Profile::load(&client, user_id).await?;
        let birthdays = User::friend_birthdays(&client, user_id).await?;
        let today = date_for_timezone(Utc::now(), profile.timezone.as_deref());
        if let Some(message) = build_birthday_alert(&birthdays, today) {
            self.publish_event(ProfileEvent::BirthdayAlert { user_id, message });
        }
        Ok(())
    }

    pub fn edit_profile(&self, user_id: Uuid, params: ProfileParams) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service.do_edit_profile(user_id, params).await {
                    late_core::error_span!(
                        "profile_edit_failed",
                        error = ?e,
                        "failed to edit profile"
                    );
                    service.publish_event(ProfileEvent::Error {
                        user_id,
                        message: profile_error_message(&e).to_string(),
                    });
                }
            }
            .instrument(info_span!("profile.edit_task", user_id = %user_id)),
        );
    }

    #[tracing::instrument(skip(self, params), fields(user_id = %user_id))]
    async fn do_edit_profile(&self, user_id: Uuid, mut params: ProfileParams) -> Result<()> {
        let client = self.db.get().await?;
        params.username = sanitize_username_input(&params.username);
        let _ = Profile::update(&client, user_id, params).await?;

        if let Ok(mut username_map) = User::list_usernames_by_ids(&client, &[user_id]).await
            && let Some(username) = username_map.remove(&user_id)
        {
            if let Some(directory) = &self.username_directory {
                usernames::upsert(directory, user_id, username.clone());
            }
            if let Ok(mut users) = self.active_users.lock()
                && let Some(user) = users.get_mut(&user_id)
            {
                user.username = username;
            }
        }

        self.find_profile(user_id);
        self.publish_event(ProfileEvent::Saved { user_id });
        Ok(())
    }

    pub fn set_theme_id(&self, user_id: Uuid, theme_id: String) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service.do_set_theme_id(user_id, &theme_id).await {
                    late_core::error_span!(
                        "profile_theme_edit_failed",
                        error = ?e,
                        "failed to edit profile theme"
                    );
                    service.publish_event(ProfileEvent::Error {
                        user_id,
                        message: "Could not save theme. Please try again.".to_string(),
                    });
                }
            }
            .instrument(info_span!("profile.theme_task", user_id = %user_id)),
        );
    }

    #[tracing::instrument(skip(self, theme_id), fields(user_id = %user_id))]
    async fn do_set_theme_id(&self, user_id: Uuid, theme_id: &str) -> Result<()> {
        let client = self.db.get().await?;
        User::set_theme_id(&client, user_id, theme_id).await?;
        self.find_profile(user_id);
        self.publish_event(ProfileEvent::Saved { user_id });
        Ok(())
    }

    pub fn delete_account(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service.do_delete_account(user_id).await {
                    late_core::error_span!(
                        "account_delete_failed",
                        error = ?e,
                        "failed to delete account"
                    );
                    service.publish_event(ProfileEvent::Error {
                        user_id,
                        message: "Could not delete account. Please try again.".to_string(),
                    });
                }
            }
            .instrument(info_span!("profile.delete_account_task", user_id = %user_id)),
        );
    }

    #[tracing::instrument(skip(self), fields(user_id = %user_id))]
    async fn do_delete_account(&self, user_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        let deleted = User::delete_by_id(&client, user_id).await?;
        if deleted == 0 {
            anyhow::bail!("user not found");
        }

        self.terminate_active_sessions(user_id, "account deleted")
            .await;
        if let Ok(mut users) = self.active_users.lock() {
            users.remove(&user_id);
        }
        if let Some(directory) = &self.username_directory {
            usernames::remove(directory, user_id);
        }
        Ok(())
    }

    pub fn create_account_link_code(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service.do_create_account_link_code(user_id).await {
                    late_core::error_span!(
                        "account_link_code_create_failed",
                        error = ?e,
                        user_id = %user_id,
                        "failed to create account link code"
                    );
                    service.publish_event(ProfileEvent::Error {
                        user_id,
                        message: account_link_error_message(&e),
                    });
                }
            }
            .instrument(info_span!("profile.account_link_code_task", user_id = %user_id)),
        );
    }

    #[tracing::instrument(skip(self), fields(user_id = %user_id))]
    async fn do_create_account_link_code(&self, user_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        let (code, expires_at) = account_link::create_code(&client, user_id).await?;
        self.publish_event(ProfileEvent::AccountLinkCodeCreated {
            user_id,
            code,
            expires_at,
        });
        Ok(())
    }

    pub fn preview_account_link_code(&self, user_id: Uuid, code: String) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service.do_preview_account_link_code(user_id, &code).await {
                    late_core::error_span!(
                        "account_link_preview_failed",
                        error = ?e,
                        user_id = %user_id,
                        "failed to preview account link code"
                    );
                    service.publish_event(ProfileEvent::Error {
                        user_id,
                        message: account_link_error_message(&e),
                    });
                }
            }
            .instrument(info_span!("profile.account_link_preview_task", user_id = %user_id)),
        );
    }

    #[tracing::instrument(skip(self, code), fields(user_id = %user_id))]
    async fn do_preview_account_link_code(&self, user_id: Uuid, code: &str) -> Result<()> {
        let client = self.db.get().await?;
        let peer = account_link::peer_for_code(&client, user_id, code).await?;
        self.publish_event(ProfileEvent::AccountLinkPeerLoaded {
            user_id,
            peer_user_id: peer.user_id,
            peer_username: peer.username,
            peer_created: peer.created,
        });
        Ok(())
    }

    pub fn complete_account_link(
        &self,
        current_user_id: Uuid,
        peer_user_id: Uuid,
        code: String,
        kept_user_id: Uuid,
    ) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service
                    .do_complete_account_link(current_user_id, peer_user_id, &code, kept_user_id)
                    .await
                {
                    late_core::error_span!(
                        "account_link_complete_failed",
                        error = ?e,
                        current_user_id = %current_user_id,
                        peer_user_id = %peer_user_id,
                        kept_user_id = %kept_user_id,
                        "failed to complete account link"
                    );
                    service.publish_event(ProfileEvent::Error {
                        user_id: current_user_id,
                        message: account_link_error_message(&e),
                    });
                }
            }
            .instrument(info_span!(
                "profile.account_link_complete_task",
                current_user_id = %current_user_id,
                peer_user_id = %peer_user_id,
                kept_user_id = %kept_user_id
            )),
        );
    }

    #[tracing::instrument(skip(self, code), fields(current_user_id = %current_user_id, peer_user_id = %peer_user_id, kept_user_id = %kept_user_id))]
    async fn do_complete_account_link(
        &self,
        current_user_id: Uuid,
        peer_user_id: Uuid,
        code: &str,
        kept_user_id: Uuid,
    ) -> Result<()> {
        let mut client = self.db.get().await?;
        let result = account_link::complete(
            &mut client,
            current_user_id,
            peer_user_id,
            code,
            kept_user_id,
        )
        .await?;

        self.find_profile(result.kept_user_id);
        self.publish_event(ProfileEvent::AccountLinked {
            kept_user_id: result.kept_user_id,
            abandoned_user_id: result.abandoned_user_id,
            kept_username: result.kept_username,
            abandoned_username: result.abandoned_username,
        });
        self.terminate_active_sessions(result.abandoned_user_id, "account linked")
            .await;
        if let Ok(mut users) = self.active_users.lock() {
            users.remove(&result.abandoned_user_id);
        }
        if let Some(directory) = &self.username_directory {
            usernames::remove(directory, result.abandoned_user_id);
        }
        Ok(())
    }

    async fn terminate_active_sessions(&self, user_id: Uuid, reason: &str) {
        let Some(registry) = self.session_registry.clone() else {
            return;
        };
        let tokens = self
            .active_users
            .lock()
            .ok()
            .and_then(|users| users.get(&user_id).cloned())
            .map(|user| {
                user.sessions
                    .into_iter()
                    .map(|session| session.token)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for token in tokens {
            let _ = registry
                .send_message(
                    &token,
                    SessionMessage::Terminate {
                        reason: reason.to_string(),
                    },
                )
                .await;
        }
    }
}

fn should_prune_snapshot_sender(sender: &watch::Sender<ProfileSnapshot>) -> bool {
    sender.is_closed() || sender.receiver_count() <= 1
}

fn profile_error_message(error: &anyhow::Error) -> &'static str {
    let Some(db_error) = error.downcast_ref::<tokio_postgres::Error>() else {
        return "Could not save profile. Please try again.";
    };
    let Some(sql_state) = db_error.code() else {
        return "Could not save profile. Please try again.";
    };

    match *sql_state {
        SqlState::UNIQUE_VIOLATION => "That username is already taken.",
        SqlState::CHECK_VIOLATION => "Username must be between 1 and 32 characters.",
        _ => "Could not save profile. Please try again.",
    }
}

fn account_link_error_message(error: &anyhow::Error) -> String {
    if error.downcast_ref::<tokio_postgres::Error>().is_some() {
        return "Could not link accounts. Please try again.".to_string();
    }
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_snapshot_default_is_empty() {
        let snapshot = ProfileSnapshot::default();
        assert_eq!(snapshot.user_id, None);
        assert!(snapshot.profile.is_none());
        assert!(snapshot.bonsai.is_none());
    }

    #[test]
    fn should_prune_when_only_one_receiver_remains() {
        let (tx, _rx) = watch::channel(ProfileSnapshot::default());
        assert!(should_prune_snapshot_sender(&tx));
    }

    #[test]
    fn should_not_prune_when_multiple_receivers_exist() {
        let (tx, _rx1) = watch::channel(ProfileSnapshot::default());
        let _rx2 = tx.subscribe();
        assert!(!should_prune_snapshot_sender(&tx));
    }

    #[test]
    fn should_prune_when_channel_is_closed() {
        let (tx, rx) = watch::channel(ProfileSnapshot::default());
        drop(rx);
        assert!(should_prune_snapshot_sender(&tx));
    }

    fn day(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn no_friend_birthdays_yields_no_alert() {
        assert_eq!(build_birthday_alert(&[], day(2026, 5, 20)), None);
        let none_soon = vec![("zoe".to_string(), "11-30".to_string())];
        assert_eq!(build_birthday_alert(&none_soon, day(2026, 5, 20)), None);
    }

    #[test]
    fn today_birthday_is_called_out_first() {
        let b = vec![
            ("ada".to_string(), "05-20".to_string()),
            ("bo".to_string(), "05-23".to_string()),
        ];
        let msg = build_birthday_alert(&b, day(2026, 5, 20)).unwrap();
        assert!(msg.starts_with("ada — birthday today!"), "{msg}");
        assert!(msg.contains("bo's birthday in 3 days"), "{msg}");
    }

    #[test]
    fn tomorrow_is_phrased_specially_and_sorted_by_proximity() {
        let b = vec![
            ("far".to_string(), "05-27".to_string()),
            ("near".to_string(), "05-21".to_string()),
        ];
        let msg = build_birthday_alert(&b, day(2026, 5, 20)).unwrap();
        assert_eq!(msg, "near's birthday tomorrow · far's birthday in 7 days");
    }

    #[test]
    fn eight_days_out_is_outside_the_window() {
        let b = vec![("late".to_string(), "05-28".to_string())];
        assert_eq!(build_birthday_alert(&b, day(2026, 5, 20)), None);
    }
}
