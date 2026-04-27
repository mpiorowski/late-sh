use anyhow::Result;
use chrono::{DateTime, Utc};
use late_core::{
    db::Db,
    models::{
        showcase::{Showcase, ShowcaseParams},
        showcase_feed_read::ShowcaseFeedRead,
        user::User,
    },
};
use std::collections::{HashMap, HashSet};
use tokio::sync::{broadcast, watch};
use tracing::{Instrument, info_span};
use uuid::Uuid;

const LIST_LIMIT: i64 = 100;

#[derive(Clone, Default)]
pub struct ShowcaseSnapshot {
    pub items: Vec<ShowcaseFeedItem>,
}

#[derive(Clone)]
pub struct ShowcaseFeedItem {
    pub showcase: Showcase,
    pub author_username: String,
}

#[derive(Clone, Debug)]
pub enum ShowcaseEvent {
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
    NewShowcasesAvailable {
        user_id: Uuid,
        unread_count: i64,
    },
}

#[derive(Clone)]
pub struct ShowcaseService {
    db: Db,
    snapshot_tx: watch::Sender<ShowcaseSnapshot>,
    snapshot_rx: watch::Receiver<ShowcaseSnapshot>,
    evt_tx: broadcast::Sender<ShowcaseEvent>,
}

impl ShowcaseService {
    pub fn new(db: Db) -> Self {
        let (snapshot_tx, snapshot_rx) = watch::channel(ShowcaseSnapshot::default());
        let (evt_tx, _) = broadcast::channel(256);
        Self {
            db,
            snapshot_tx,
            snapshot_rx,
            evt_tx,
        }
    }

    pub fn subscribe_snapshot(&self) -> watch::Receiver<ShowcaseSnapshot> {
        self.snapshot_rx.clone()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<ShowcaseEvent> {
        self.evt_tx.subscribe()
    }

    fn publish_event(&self, event: ShowcaseEvent) {
        if let Err(e) = self.evt_tx.send(event) {
            tracing::debug!(%e, "no showcase event subscribers");
        }
    }

    pub fn list_task(&self) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service.do_list().await {
                    late_core::error_span!(
                        "showcase_list_failed",
                        error = ?e,
                        "failed to list showcases"
                    );
                }
            }
            .instrument(info_span!("showcase.list")),
        );
    }

    pub fn refresh_unread_count_task(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(async move {
            if let Err(e) = service.publish_unread_count(user_id).await {
                late_core::error_span!(
                    "showcase_unread_refresh_failed",
                    error = ?e,
                    user_id = %user_id,
                    "failed to refresh showcase unread count"
                );
            }
        });
    }

    pub fn mark_read_task(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(async move {
            if let Err(e) = service.mark_read_and_publish(user_id).await {
                late_core::error_span!(
                    "showcase_mark_read_failed",
                    error = ?e,
                    user_id = %user_id,
                    "failed to mark showcase feed read"
                );
            }
        });
    }

    pub fn create_task(&self, user_id: Uuid, params: ShowcaseParams) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result = async {
                    let client = service.db.get().await?;
                    Showcase::create_by_user_id(&client, user_id, params).await?;
                    service.do_list().await?;
                    Ok::<_, anyhow::Error>(())
                }
                .await;

                match result {
                    Ok(()) => {
                        service.publish_event(ShowcaseEvent::Created { user_id });
                        if let Err(e) = service
                            .publish_unread_updates_for_all(true, Some(user_id))
                            .await
                        {
                            late_core::error_span!(
                                "showcase_unread_broadcast_failed",
                                error = ?e,
                                "failed to publish showcase unread updates after create"
                            );
                        }
                    }
                    Err(e) => {
                        late_core::error_span!(
                            "showcase_create_failed",
                            error = ?e,
                            "failed to create showcase"
                        );
                        service.publish_event(ShowcaseEvent::Failed {
                            user_id,
                            error: e.to_string(),
                        });
                    }
                }
            }
            .instrument(info_span!("showcase.create", user_id = %user_id)),
        );
    }

    pub fn update_task(
        &self,
        user_id: Uuid,
        showcase_id: Uuid,
        params: ShowcaseParams,
        is_admin: bool,
    ) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result = async {
                    let client = service.db.get().await?;
                    let Some(existing) = Showcase::get(&client, showcase_id).await? else {
                        anyhow::bail!("showcase not found");
                    };
                    if !is_admin && existing.user_id != user_id {
                        anyhow::bail!("not your showcase");
                    }
                    let owner_id = existing.user_id;
                    if Showcase::update_by_user_id(&client, owner_id, showcase_id, params)
                        .await?
                        .is_none()
                    {
                        anyhow::bail!("showcase update missed");
                    }
                    service.do_list().await?;
                    Ok::<_, anyhow::Error>(())
                }
                .await;

                match result {
                    Ok(()) => {
                        service.publish_event(ShowcaseEvent::Updated { user_id });
                        if let Err(e) = service.publish_unread_updates_for_all(false, None).await {
                            late_core::error_span!(
                                "showcase_unread_broadcast_failed",
                                error = ?e,
                                "failed to publish showcase unread updates after update"
                            );
                        }
                    }
                    Err(e) => {
                        late_core::error_span!(
                            "showcase_update_failed",
                            error = ?e,
                            "failed to update showcase"
                        );
                        service.publish_event(ShowcaseEvent::Failed {
                            user_id,
                            error: e.to_string(),
                        });
                    }
                }
            }
            .instrument(info_span!(
                "showcase.update",
                user_id = %user_id,
                showcase_id = %showcase_id
            )),
        );
    }

    pub fn delete_task(&self, user_id: Uuid, showcase_id: Uuid, is_admin: bool) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result = async {
                    let client = service.db.get().await?;
                    let Some(existing) = Showcase::get(&client, showcase_id).await? else {
                        anyhow::bail!("showcase not found");
                    };
                    if !is_admin && existing.user_id != user_id {
                        anyhow::bail!("not your showcase");
                    }
                    let count = Showcase::delete(&client, showcase_id).await?;
                    if count == 0 {
                        anyhow::bail!("showcase already deleted");
                    }
                    service.do_list().await?;
                    Ok::<_, anyhow::Error>(())
                }
                .await;

                match result {
                    Ok(()) => {
                        service.publish_event(ShowcaseEvent::Deleted { user_id });
                        if let Err(e) = service.publish_unread_updates_for_all(false, None).await {
                            late_core::error_span!(
                                "showcase_unread_broadcast_failed",
                                error = ?e,
                                "failed to publish showcase unread updates after delete"
                            );
                        }
                    }
                    Err(e) => {
                        late_core::error_span!(
                            "showcase_delete_failed",
                            error = ?e,
                            "failed to delete showcase"
                        );
                        service.publish_event(ShowcaseEvent::Failed {
                            user_id,
                            error: e.to_string(),
                        });
                    }
                }
            }
            .instrument(info_span!(
                "showcase.delete",
                user_id = %user_id,
                showcase_id = %showcase_id
            )),
        );
    }

    #[tracing::instrument(skip(self))]
    async fn do_list(&self) -> Result<()> {
        let client = self.db.get().await?;
        let items = Showcase::list_recent(&client, LIST_LIMIT).await?;
        let user_ids: Vec<Uuid> = items
            .iter()
            .map(|s| s.user_id)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let usernames = User::list_usernames_by_ids(&client, &user_ids).await?;
        let items = items
            .into_iter()
            .map(|showcase| ShowcaseFeedItem {
                author_username: display_author(&usernames, showcase.user_id),
                showcase,
            })
            .collect();

        if let Err(e) = self.snapshot_tx.send(ShowcaseSnapshot { items }) {
            tracing::debug!(%e, "no showcase snapshot subscribers");
        }
        Ok(())
    }

    async fn publish_unread_count(&self, user_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        let unread_count = ShowcaseFeedRead::unread_count_for_user(&client, user_id).await?;
        let last_read_at = ShowcaseFeedRead::last_read_at(&client, user_id).await?;
        self.publish_event(ShowcaseEvent::UnreadCountUpdated {
            user_id,
            unread_count,
            last_read_at,
        });
        Ok(())
    }

    async fn mark_read_and_publish(&self, user_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        ShowcaseFeedRead::mark_read_now(&client, user_id).await?;
        let last_read_at = ShowcaseFeedRead::last_read_at(&client, user_id).await?;
        self.publish_event(ShowcaseEvent::UnreadCountUpdated {
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
        let rows = client.query("SELECT id FROM users", &[]).await?;
        for row in rows {
            let user_id: Uuid = row.get("id");
            let unread_count = ShowcaseFeedRead::unread_count_for_user(&client, user_id).await?;
            let last_read_at = ShowcaseFeedRead::last_read_at(&client, user_id).await?;
            self.publish_event(ShowcaseEvent::UnreadCountUpdated {
                user_id,
                unread_count,
                last_read_at,
            });
            if announce_new && Some(user_id) != actor_user_id && unread_count > 0 {
                self.publish_event(ShowcaseEvent::NewShowcasesAvailable {
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

pub fn parse_tags(input: &str) -> Vec<String> {
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
        if out.len() >= 8 {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{display_author, parse_tags};
    use std::collections::HashMap;
    use uuid::Uuid;

    #[test]
    fn parse_tags_normalizes_and_dedupes() {
        let tags = parse_tags("Rust, CLI rust, web-dev");
        assert_eq!(tags, vec!["rust", "cli", "web-dev"]);
    }

    #[test]
    fn parse_tags_strips_hash_and_filters_invalid() {
        let tags = parse_tags("#rust, !!!, ok");
        assert_eq!(tags, vec!["rust", "ok"]);
    }

    #[test]
    fn parse_tags_caps_count() {
        let raw = (0..20)
            .map(|i| format!("tag{i}"))
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(parse_tags(&raw).len(), 8);
    }

    #[test]
    fn parse_tags_empty_input() {
        assert!(parse_tags("").is_empty());
        assert!(parse_tags("   ,  ").is_empty());
    }

    #[test]
    fn display_author_prefers_username() {
        let id = Uuid::now_v7();
        let mut map = HashMap::new();
        map.insert(id, "alice".to_string());
        assert_eq!(display_author(&map, id), "alice");
    }

    #[test]
    fn display_author_falls_back_to_short_id() {
        let id = Uuid::now_v7();
        let map = HashMap::new();
        assert_eq!(display_author(&map, id), id.to_string()[..8]);
    }
}
