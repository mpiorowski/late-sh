use anyhow::Result;
use late_core::models::bonsai::Tree;
use late_core::models::profile::{Profile, ProfileParams};
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
use crate::state::{AccountDeletionGate, ActiveUsers};

#[derive(Clone)]
pub struct ProfileService {
    db: Db,
    snapshot_txs: Arc<Mutex<HashMap<Uuid, watch::Sender<ProfileSnapshot>>>>,
    evt_tx: broadcast::Sender<ProfileEvent>,
    active_users: ActiveUsers,
    session_registry: Option<SessionRegistry>,
    account_deletions: AccountDeletionGate,
}

#[derive(Clone, Default)]
pub struct ProfileSnapshot {
    pub user_id: Option<Uuid>,
    pub profile: Option<Profile>,
    pub bonsai: Option<Tree>,
}

#[derive(Clone, Debug)]
pub enum ProfileEvent {
    Saved { user_id: Uuid },
    Error { user_id: Uuid, message: String },
    AccountDeleted { user_id: Uuid },
}

impl ProfileService {
    pub fn new(db: Db, active_users: ActiveUsers) -> Self {
        let (evt_tx, _) = broadcast::channel(512);

        Self {
            db,
            snapshot_txs: Arc::new(Mutex::new(HashMap::new())),
            evt_tx,
            active_users,
            session_registry: None,
            account_deletions: AccountDeletionGate::new(),
        }
    }

    pub fn with_session_registry(mut self, session_registry: SessionRegistry) -> Self {
        self.session_registry = Some(session_registry);
        self
    }

    pub fn with_account_deletion_gate(mut self, account_deletions: AccountDeletionGate) -> Self {
        self.account_deletions = account_deletions;
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
        let profile = Profile::load(&client, user_id).await?;
        let bonsai = Tree::find_by_user_id(&client, user_id).await?;
        self.publish_snapshot(
            user_id,
            ProfileSnapshot {
                user_id: Some(user_id),
                profile: Some(profile),
                bonsai,
            },
        )?;
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

        if let Ok(mut usernames) = User::list_usernames_by_ids(&client, &[user_id]).await
            && let Some(username) = usernames.remove(&user_id)
            && let Ok(mut users) = self.active_users.lock()
            && let Some(user) = users.get_mut(&user_id)
        {
            user.username = username;
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
        let active_fingerprint = service.active_fingerprint(user_id);
        service
            .account_deletions
            .mark(user_id, active_fingerprint.as_deref());
        tokio::spawn(
            async move {
                match service.do_delete_account(user_id, active_fingerprint).await {
                    Ok(()) => service.publish_event(ProfileEvent::AccountDeleted { user_id }),
                    Err(e) => {
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
            }
            .instrument(info_span!("profile.delete_account_task", user_id = %user_id)),
        );
    }

    #[tracing::instrument(skip(self, active_fingerprint), fields(user_id = %user_id))]
    async fn do_delete_account(
        &self,
        user_id: Uuid,
        active_fingerprint: Option<String>,
    ) -> Result<()> {
        let mut fingerprint = active_fingerprint;
        let result = async {
            let mut client = self.db.get().await?;
            let user = User::get(&client, user_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("user not found"))?;
            if fingerprint.is_none() {
                fingerprint = Some(user.fingerprint.clone());
            }
            self.account_deletions.mark(user_id, fingerprint.as_deref());
            let tx = client.transaction().await?;
            let deleted = tx
                .execute("DELETE FROM users WHERE id = $1", &[&user_id])
                .await?;
            if deleted == 0 {
                anyhow::bail!("user not found");
            }
            tx.commit().await?;
            Ok::<(), anyhow::Error>(())
        }
        .await;

        if let Err(e) = result {
            self.account_deletions
                .clear(user_id, fingerprint.as_deref());
            return Err(e);
        }

        self.terminate_active_sessions(user_id).await;
        if let Ok(mut users) = self.active_users.lock() {
            users.remove(&user_id);
        }
        self.account_deletions
            .clear(user_id, fingerprint.as_deref());
        Ok(())
    }

    fn active_fingerprint(&self, user_id: Uuid) -> Option<String> {
        self.active_users.lock().ok().and_then(|users| {
            users
                .get(&user_id)
                .and_then(|user| user.fingerprint.clone())
        })
    }

    async fn terminate_active_sessions(&self, user_id: Uuid) {
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
                        reason: "account deleted".to_string(),
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
}
