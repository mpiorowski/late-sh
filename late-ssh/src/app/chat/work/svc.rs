use anyhow::Result;
use chrono::{DateTime, Utc};
use late_core::{
    db::Db,
    models::{
        user::User,
        work_feed_read::WorkFeedRead,
        work_profile::{WorkProfile, WorkProfileParams},
    },
};
use std::collections::{HashMap, HashSet};
use tokio::sync::{broadcast, watch};
use tracing::{Instrument, info_span};
use uuid::Uuid;

const LIST_LIMIT: i64 = 100;

#[derive(Clone, Default)]
pub struct WorkSnapshot {
    pub items: Vec<WorkFeedItem>,
}

#[derive(Clone)]
pub struct WorkFeedItem {
    pub profile: WorkProfile,
    pub author_username: String,
}

#[derive(Clone, Debug)]
pub enum WorkEvent {
    Created {
        user_id: Uuid,
    },
    Updated {
        user_id: Uuid,
    },
    Deleted {
        user_id: Uuid,
    },
    Failed {
        user_id: Uuid,
        error: String,
    },
    UnreadCountUpdated {
        user_id: Uuid,
        unread_count: i64,
        last_read_at: Option<DateTime<Utc>>,
    },
    NewWorkProfilesAvailable {
        user_id: Uuid,
        unread_count: i64,
    },
}

#[derive(Clone)]
pub struct WorkService {
    db: Db,
    snapshot_tx: watch::Sender<WorkSnapshot>,
    snapshot_rx: watch::Receiver<WorkSnapshot>,
    evt_tx: broadcast::Sender<WorkEvent>,
}

impl WorkService {
    pub fn new(db: Db) -> Self {
        let (snapshot_tx, snapshot_rx) = watch::channel(WorkSnapshot::default());
        let (evt_tx, _) = broadcast::channel(256);
        Self {
            db,
            snapshot_tx,
            snapshot_rx,
            evt_tx,
        }
    }

    pub fn subscribe_snapshot(&self) -> watch::Receiver<WorkSnapshot> {
        self.snapshot_rx.clone()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<WorkEvent> {
        self.evt_tx.subscribe()
    }

    fn publish_event(&self, event: WorkEvent) {
        if let Err(e) = self.evt_tx.send(event) {
            tracing::debug!(%e, "no work event subscribers");
        }
    }

    pub fn list_task(&self) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service.do_list().await {
                    late_core::error_span!(
                        "work_list_failed",
                        error = ?e,
                        "failed to list work profiles"
                    );
                }
            }
            .instrument(info_span!("work.list")),
        );
    }

    pub fn refresh_unread_count_task(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(async move {
            if let Err(e) = service.publish_unread_count(user_id).await {
                late_core::error_span!(
                    "work_unread_refresh_failed",
                    error = ?e,
                    user_id = %user_id,
                    "failed to refresh work unread count"
                );
            }
        });
    }

    pub fn mark_read_task(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(async move {
            if let Err(e) = service.mark_read_and_publish(user_id).await {
                late_core::error_span!(
                    "work_mark_read_failed",
                    error = ?e,
                    user_id = %user_id,
                    "failed to mark work feed read"
                );
            }
        });
    }

    pub fn create_task(&self, user_id: Uuid, params: WorkProfileParams) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result = async {
                    let client = service.db.get().await?;
                    let existing = WorkProfile::find_by_user_id(&client, user_id).await?;
                    if let Some(existing) = existing {
                        let mut params = params;
                        params.slug = existing.slug;
                        WorkProfile::update_by_user_id(&client, user_id, existing.id, params)
                            .await?
                            .ok_or_else(|| anyhow::anyhow!("work profile update missed"))?;
                        service.do_list().await?;
                        Ok::<_, anyhow::Error>(WorkEvent::Updated { user_id })
                    } else {
                        WorkProfile::create_by_user_id(&client, user_id, params).await?;
                        service.do_list().await?;
                        Ok::<_, anyhow::Error>(WorkEvent::Created { user_id })
                    }
                }
                .await;

                match result {
                    Ok(event) => {
                        let announce_new = matches!(event, WorkEvent::Created { .. });
                        service.publish_event(event);
                        if let Err(e) = service
                            .publish_unread_updates_for_all(announce_new, Some(user_id))
                            .await
                        {
                            late_core::error_span!(
                                "work_unread_broadcast_failed",
                                error = ?e,
                                "failed to publish work unread updates after save"
                            );
                        }
                    }
                    Err(e) => {
                        late_core::error_span!(
                            "work_create_failed",
                            error = ?e,
                            "failed to save work profile"
                        );
                        service.publish_event(WorkEvent::Failed {
                            user_id,
                            error: e.to_string(),
                        });
                    }
                }
            }
            .instrument(info_span!("work.create", user_id = %user_id)),
        );
    }

    pub fn update_task(
        &self,
        user_id: Uuid,
        profile_id: Uuid,
        params: WorkProfileParams,
        is_admin: bool,
    ) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result = async {
                    let client = service.db.get().await?;
                    let Some(existing) = WorkProfile::get(&client, profile_id).await? else {
                        anyhow::bail!("work profile not found");
                    };
                    if !is_admin && existing.user_id != user_id {
                        anyhow::bail!("not your work profile");
                    }
                    let owner_id = existing.user_id;
                    let mut params = params;
                    params.slug = existing.slug;
                    if WorkProfile::update_by_user_id(&client, owner_id, profile_id, params)
                        .await?
                        .is_none()
                    {
                        anyhow::bail!("work profile update missed");
                    }
                    service.do_list().await?;
                    Ok::<_, anyhow::Error>(())
                }
                .await;

                match result {
                    Ok(()) => {
                        service.publish_event(WorkEvent::Updated { user_id });
                        if let Err(e) = service.publish_unread_updates_for_all(false, None).await {
                            late_core::error_span!(
                                "work_unread_broadcast_failed",
                                error = ?e,
                                "failed to publish work unread updates after update"
                            );
                        }
                    }
                    Err(e) => {
                        late_core::error_span!(
                            "work_update_failed",
                            error = ?e,
                            "failed to update work profile"
                        );
                        service.publish_event(WorkEvent::Failed {
                            user_id,
                            error: e.to_string(),
                        });
                    }
                }
            }
            .instrument(info_span!(
                "work.update",
                user_id = %user_id,
                profile_id = %profile_id
            )),
        );
    }

    pub fn delete_task(&self, user_id: Uuid, profile_id: Uuid, is_admin: bool) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result = async {
                    let client = service.db.get().await?;
                    let Some(existing) = WorkProfile::get(&client, profile_id).await? else {
                        anyhow::bail!("work profile not found");
                    };
                    if !is_admin && existing.user_id != user_id {
                        anyhow::bail!("not your work profile");
                    }
                    let count = WorkProfile::delete(&client, profile_id).await?;
                    if count == 0 {
                        anyhow::bail!("work profile already deleted");
                    }
                    service.do_list().await?;
                    Ok::<_, anyhow::Error>(())
                }
                .await;

                match result {
                    Ok(()) => {
                        service.publish_event(WorkEvent::Deleted { user_id });
                        if let Err(e) = service.publish_unread_updates_for_all(false, None).await {
                            late_core::error_span!(
                                "work_unread_broadcast_failed",
                                error = ?e,
                                "failed to publish work unread updates after delete"
                            );
                        }
                    }
                    Err(e) => {
                        late_core::error_span!(
                            "work_delete_failed",
                            error = ?e,
                            "failed to delete work profile"
                        );
                        service.publish_event(WorkEvent::Failed {
                            user_id,
                            error: e.to_string(),
                        });
                    }
                }
            }
            .instrument(info_span!(
                "work.delete",
                user_id = %user_id,
                profile_id = %profile_id
            )),
        );
    }

    #[tracing::instrument(skip(self))]
    async fn do_list(&self) -> Result<()> {
        let client = self.db.get().await?;
        let items = WorkProfile::list_recent(&client, LIST_LIMIT).await?;
        let user_ids: Vec<Uuid> = items
            .iter()
            .map(|profile| profile.user_id)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let usernames = User::list_usernames_by_ids(&client, &user_ids).await?;
        let items = items
            .into_iter()
            .map(|profile| WorkFeedItem {
                author_username: display_author(&usernames, profile.user_id),
                profile,
            })
            .collect();

        if let Err(e) = self.snapshot_tx.send(WorkSnapshot { items }) {
            tracing::debug!(%e, "no work snapshot subscribers");
        }
        Ok(())
    }

    async fn publish_unread_count(&self, user_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        let unread_count = WorkFeedRead::unread_count_for_user(&client, user_id).await?;
        let last_read_at = WorkFeedRead::last_read_at(&client, user_id).await?;
        self.publish_event(WorkEvent::UnreadCountUpdated {
            user_id,
            unread_count,
            last_read_at,
        });
        Ok(())
    }

    async fn mark_read_and_publish(&self, user_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        WorkFeedRead::mark_read_now(&client, user_id).await?;
        let last_read_at = WorkFeedRead::last_read_at(&client, user_id).await?;
        self.publish_event(WorkEvent::UnreadCountUpdated {
            user_id,
            unread_count: 0,
            last_read_at,
        });
        Ok(())
    }

    async fn publish_unread_updates_for_all(
        &self,
        announce_new: bool,
        actor_user_id: Option<Uuid>,
    ) -> Result<()> {
        let client = self.db.get().await?;
        for user_id in User::list_ids(&client).await? {
            let unread_count = WorkFeedRead::unread_count_for_user(&client, user_id).await?;
            let last_read_at = WorkFeedRead::last_read_at(&client, user_id).await?;
            self.publish_event(WorkEvent::UnreadCountUpdated {
                user_id,
                unread_count,
                last_read_at,
            });
            if announce_new && Some(user_id) != actor_user_id && unread_count > 0 {
                self.publish_event(WorkEvent::NewWorkProfilesAvailable {
                    user_id,
                    unread_count,
                });
            }
        }
        Ok(())
    }
}

fn display_author(usernames: &HashMap<Uuid, String>, user_id: Uuid) -> String {
    usernames
        .get(&user_id)
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| user_id.to_string()[..8].to_string())
}

pub fn parse_words(input: &str, limit: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for raw in input.split(|c: char| c == ',' || c.is_whitespace()) {
        let tag: String = raw
            .trim()
            .trim_matches('#')
            .to_ascii_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
            .collect();
        if tag.is_empty() || tag.len() > 24 {
            continue;
        }
        if seen.insert(tag.clone()) {
            out.push(tag);
        }
        if out.len() >= limit {
            break;
        }
    }
    out
}

pub fn parse_links(input: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for raw in input.split(|c: char| c == ',' || c == '\n' || c == '\r') {
        let link = raw.trim().trim_matches(['<', '>']).to_string();
        if !looks_like_url(&link) || link.len() > 2000 {
            continue;
        }
        if seen.insert(link.clone()) {
            out.push(link);
        }
        if out.len() >= 6 {
            break;
        }
    }
    out
}

pub fn looks_like_url(s: &str) -> bool {
    let s = s.trim();
    s.starts_with("http://") || s.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::{parse_links, parse_words};

    #[test]
    fn parse_words_normalizes_and_caps() {
        let raw = "Rust, CLI rust, web-dev extra one two three four five six seven";
        assert_eq!(parse_words(raw, 3), vec!["rust", "cli", "web-dev"]);
    }

    #[test]
    fn parse_links_keeps_http_urls_only() {
        let links = parse_links("late.sh, https://late.sh, http://x.test\nftp://no");
        assert_eq!(links, vec!["https://late.sh", "http://x.test"]);
    }
}
