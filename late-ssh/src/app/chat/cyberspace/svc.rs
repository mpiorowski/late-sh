//! Orchestration for the cyberspace pane: fire-and-forget tasks that call the
//! API as the linked user and publish results as broadcast events. The service
//! keeps no per-user content (their terms ban caching for redistribution);
//! everything fetched lives only in the requesting session's UI state. The
//! one thing held here is the in-memory id-token cache, so a browsing session
//! doesn't re-refresh on every keypress.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use late_core::{db::Db, models::cyberspace_account::CyberspaceAccount};
use tokio::sync::broadcast;
use tracing::{Instrument, info_span};
use uuid::Uuid;

use crate::app::activity::publisher::ActivityPublisher;

use super::api::{CsApi, CsApiError, CsNotification, CsPost, CsReply, NewPost};

/// Firebase id tokens live ~60 minutes; refresh with slack so a token handed
/// out by the cache is never about to expire mid-request.
const TOKEN_CACHE_TTL: Duration = Duration::from_secs(50 * 60);

#[derive(Clone, Debug)]
pub struct CsThread {
    pub post: CsPost,
    pub replies: Vec<CsReply>,
}

/// Events carry their data instead of going through a shared snapshot:
/// cyberspace content is per-user by their API terms, so there is nothing
/// shared to snapshot. Sessions filter on `user_id`.
#[derive(Clone, Debug)]
pub enum CsEvent {
    /// Session-init answer: whether this user has a linked account.
    LinkStatus {
        user_id: Uuid,
        username: Option<String>,
    },
    LinkSucceeded {
        user_id: Uuid,
        username: String,
    },
    LinkFailed {
        user_id: Uuid,
        error: String,
    },
    Unlinked {
        user_id: Uuid,
    },
    FeedLoaded {
        user_id: Uuid,
        posts: Vec<CsPost>,
    },
    ThreadLoaded {
        user_id: Uuid,
        thread: CsThread,
    },
    NotificationsLoaded {
        user_id: Uuid,
        notifications: Vec<CsNotification>,
    },
    UnreadCount {
        user_id: Uuid,
        count: i64,
    },
    PostCreated {
        user_id: Uuid,
        title: Option<String>,
    },
    ReplyPosted {
        user_id: Uuid,
        post_id: String,
    },
    /// Any task failure the user should see, rendered as a banner.
    ActionFailed {
        user_id: Uuid,
        error: String,
    },
}

struct CachedToken {
    id_token: String,
    fetched_at: Instant,
}

/// Why a usable id token could not be produced. `NotLinked` renders as the
/// login form; `Broken` means the stored refresh token was rejected (password
/// change, revocation) and the user must re-link.
enum TokenError {
    NotLinked,
    Broken(String),
    Transport(String),
}

impl TokenError {
    fn user_message(&self) -> String {
        match self {
            Self::NotLinked => "cyberspace account not linked. /cs link".to_string(),
            Self::Broken(error) => format!("cyberspace link broken ({error}). /cs link again"),
            Self::Transport(error) => format!("cyberspace unreachable: {error}"),
        }
    }
}

#[derive(Clone)]
pub struct CyberspaceService {
    db: Db,
    api: CsApi,
    tokens: Arc<Mutex<HashMap<Uuid, CachedToken>>>,
    evt_tx: broadcast::Sender<CsEvent>,
    activity: Option<ActivityPublisher>,
}

impl CyberspaceService {
    pub fn new(db: Db, base_url: String) -> Self {
        let (evt_tx, _) = broadcast::channel(256);
        Self {
            db,
            api: CsApi::new(base_url),
            tokens: Arc::new(Mutex::new(HashMap::new())),
            evt_tx,
            activity: None,
        }
    }

    pub fn with_activity(mut self, activity: ActivityPublisher) -> Self {
        self.activity = Some(activity);
        self
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<CsEvent> {
        self.evt_tx.subscribe()
    }

    fn publish(&self, event: CsEvent) {
        if let Err(e) = self.evt_tx.send(event) {
            tracing::debug!(%e, "no cyberspace event subscribers");
        }
    }

    fn fail(&self, user_id: Uuid, error: String) {
        self.publish(CsEvent::ActionFailed { user_id, error });
    }

    /// Session init: answer whether the user is linked, and if so fetch the
    /// unread notification badge. Later refreshes come from the session tick
    /// (`State::poll_unread_if_due`) or a user action in the pane.
    pub fn session_init_task(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let username = match service.linked_username(user_id).await {
                    Ok(username) => username,
                    Err(e) => {
                        late_core::error_span!(
                            "cyberspace_link_status_failed",
                            error = ?e,
                            user_id = %user_id,
                            "failed to load cyberspace link status"
                        );
                        return;
                    }
                };
                let linked = username.is_some();
                service.publish(CsEvent::LinkStatus { user_id, username });
                if linked {
                    service.refresh_unread(user_id).await;
                }
            }
            .instrument(info_span!("cyberspace.session_init", user_id = %user_id)),
        );
    }

    pub fn link_task(&self, user_id: Uuid, email: String, password: String) {
        let service = self.clone();
        tokio::spawn(
            async move {
                match service.do_link(user_id, email, password).await {
                    Ok(username) => {
                        service.publish(CsEvent::LinkSucceeded { user_id, username });
                        service.load_feed(user_id).await;
                        service.refresh_unread(user_id).await;
                    }
                    Err(error) => service.publish(CsEvent::LinkFailed { user_id, error }),
                }
            }
            .instrument(info_span!("cyberspace.link", user_id = %user_id)),
        );
    }

    pub fn unlink_task(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result = async {
                    let client = service.db.get().await?;
                    CyberspaceAccount::delete_for_user(&client, user_id).await
                }
                .await;
                match result {
                    Ok(_) => {
                        service
                            .tokens
                            .lock()
                            .expect("token cache lock")
                            .remove(&user_id);
                        service.publish(CsEvent::Unlinked { user_id });
                    }
                    Err(e) => {
                        late_core::error_span!(
                            "cyberspace_unlink_failed",
                            error = ?e,
                            user_id = %user_id,
                            "failed to unlink cyberspace account"
                        );
                        service.fail(user_id, "unlink failed, try again".to_string());
                    }
                }
            }
            .instrument(info_span!("cyberspace.unlink", user_id = %user_id)),
        );
    }

    pub fn load_feed_task(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move { service.load_feed(user_id).await }
                .instrument(info_span!("cyberspace.feed", user_id = %user_id)),
        );
    }

    pub fn load_thread_task(&self, user_id: Uuid, post: CsPost) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let token = match service.id_token(user_id).await {
                    Ok(token) => token,
                    Err(e) => return service.fail(user_id, e.user_message()),
                };
                match service.api.list_replies(&token, &post.post_id).await {
                    Ok(replies) => service.publish(CsEvent::ThreadLoaded {
                        user_id,
                        thread: CsThread { post, replies },
                    }),
                    Err(e) => service.fail(user_id, format!("loading replies failed: {e}")),
                }
            }
            .instrument(info_span!("cyberspace.thread", user_id = %user_id)),
        );
    }

    /// Open a thread from a post id alone, which is all a notification gives
    /// us. The post itself is fetched first: the entry a notification is
    /// about is often older than the feed page in memory, so it cannot be
    /// looked up locally.
    pub fn load_thread_by_id_task(&self, user_id: Uuid, post_id: String) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let token = match service.id_token(user_id).await {
                    Ok(token) => token,
                    Err(e) => return service.fail(user_id, e.user_message()),
                };
                let post = match service.api.get_post(&token, &post_id).await {
                    Ok(post) => post,
                    Err(e) => return service.fail(user_id, format!("opening the entry failed: {e}")),
                };
                match service.api.list_replies(&token, &post.post_id).await {
                    Ok(replies) => service.publish(CsEvent::ThreadLoaded {
                        user_id,
                        thread: CsThread { post, replies },
                    }),
                    Err(e) => service.fail(user_id, format!("loading replies failed: {e}")),
                }
            }
            .instrument(info_span!("cyberspace.thread_by_id", user_id = %user_id)),
        );
    }

    pub fn post_task(&self, user_id: Uuid, post: NewPost) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let token = match service.id_token(user_id).await {
                    Ok(token) => token,
                    Err(e) => return service.fail(user_id, e.user_message()),
                };
                match service.api.create_post(&token, &post).await {
                    Ok(created) => {
                        if let Some(activity) = &service.activity {
                            activity.cyberspace_posted_task(user_id, created.title.clone());
                        }
                        service.publish(CsEvent::PostCreated {
                            user_id,
                            title: created.title,
                        });
                        service.load_feed(user_id).await;
                    }
                    Err(e) => service.fail(user_id, format!("post failed: {e}")),
                }
            }
            .instrument(info_span!("cyberspace.post", user_id = %user_id)),
        );
    }

    pub fn reply_task(&self, user_id: Uuid, post: CsPost, content: String) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let token = match service.id_token(user_id).await {
                    Ok(token) => token,
                    Err(e) => return service.fail(user_id, e.user_message()),
                };
                let post_id = post.post_id.clone();
                match service.api.create_reply(&token, &post_id, &content).await {
                    Ok(()) => {
                        service.publish(CsEvent::ReplyPosted {
                            user_id,
                            post_id: post_id.clone(),
                        });
                        // Reload the thread so the fresh reply shows up in place.
                        match service.api.list_replies(&token, &post_id).await {
                            Ok(replies) => service.publish(CsEvent::ThreadLoaded {
                                user_id,
                                thread: CsThread { post, replies },
                            }),
                            Err(e) => {
                                tracing::debug!(%e, "reply landed but thread reload failed");
                            }
                        }
                    }
                    Err(e) => service.fail(user_id, format!("reply failed: {e}")),
                }
            }
            .instrument(info_span!("cyberspace.reply", user_id = %user_id)),
        );
    }

    /// Load notifications, then mark them read server-side (opening the view
    /// is reading them, same contract as the RSS inbox).
    pub fn load_notifications_task(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let token = match service.id_token(user_id).await {
                    Ok(token) => token,
                    Err(e) => return service.fail(user_id, e.user_message()),
                };
                match service.api.list_notifications(&token).await {
                    Ok(notifications) => {
                        service.publish(CsEvent::NotificationsLoaded {
                            user_id,
                            notifications,
                        });
                        if let Err(e) = service.api.mark_all_notifications_read(&token).await {
                            tracing::debug!(%e, "marking cyberspace notifications read failed");
                        }
                        service.publish(CsEvent::UnreadCount { user_id, count: 0 });
                    }
                    Err(e) => service.fail(user_id, format!("loading notifications failed: {e}")),
                }
            }
            .instrument(info_span!("cyberspace.notifications", user_id = %user_id)),
        );
    }

    async fn do_link(
        &self,
        user_id: Uuid,
        email: String,
        password: String,
    ) -> Result<String, String> {
        let tokens = match self.api.login(&email, &password).await {
            Ok(tokens) => tokens,
            Err(e) => return Err(format!("login failed: {e}")),
        };
        let Some(refresh_token) = tokens.refresh_token.clone() else {
            return Err("login response missing refresh token".to_string());
        };
        let identity = match self.api.me(&tokens.id_token).await {
            Ok(identity) => identity,
            Err(e) => return Err(format!("loading your cyberspace profile failed: {e}")),
        };
        let result = async {
            let client = self.db.get().await?;
            CyberspaceAccount::upsert_for_user(
                &client,
                user_id,
                &identity.cs_user_id,
                &identity.cs_username,
                &refresh_token,
            )
            .await
        }
        .await;
        match result {
            Ok(_) => {
                self.cache_token(user_id, tokens.id_token);
                Ok(identity.cs_username)
            }
            Err(e) => {
                late_core::error_span!(
                    "cyberspace_link_store_failed",
                    error = ?e,
                    user_id = %user_id,
                    "failed to store cyberspace link"
                );
                Err("storing the link failed, try again".to_string())
            }
        }
    }

    async fn load_feed(&self, user_id: Uuid) {
        let token = match self.id_token(user_id).await {
            Ok(token) => token,
            Err(e) => return self.fail(user_id, e.user_message()),
        };
        match self.api.list_feed(&token).await {
            Ok(posts) => self.publish(CsEvent::FeedLoaded { user_id, posts }),
            Err(e) => self.fail(user_id, format!("loading the feed failed: {e}")),
        }
    }

    /// Fire-and-forget badge refresh, driven by the session tick. Failures
    /// are logged inside `refresh_unread`: nobody upstream is waiting on it,
    /// and a stale badge is not worth a banner.
    pub fn refresh_unread_task(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move { service.refresh_unread(user_id).await }
                .instrument(info_span!("cyberspace.unread", user_id = %user_id)),
        );
    }

    async fn refresh_unread(&self, user_id: Uuid) {
        let token = match self.id_token(user_id).await {
            Ok(token) => token,
            Err(_) => return,
        };
        match self.api.unread_count(&token).await {
            Ok(count) => self.publish(CsEvent::UnreadCount { user_id, count }),
            Err(e) => tracing::debug!(%e, "cyberspace unread count failed"),
        }
    }

    async fn linked_username(&self, user_id: Uuid) -> anyhow::Result<Option<String>> {
        let client = self.db.get().await?;
        let account = CyberspaceAccount::find_by_user_id(&client, user_id).await?;
        Ok(account.map(|account| account.cs_username))
    }

    /// Caching a token also drops every expired one. These are live bearer
    /// tokens for a third-party account: without the sweep the map keeps one
    /// resident for every user who has ever linked, for the life of the
    /// process, long after their session ended and the token went stale.
    fn cache_token(&self, user_id: Uuid, id_token: String) {
        let mut tokens = self.tokens.lock().expect("token cache lock");
        tokens.retain(|_, cached| cached.fetched_at.elapsed() < TOKEN_CACHE_TTL);
        tokens.insert(
            user_id,
            CachedToken {
                id_token,
                fetched_at: Instant::now(),
            },
        );
    }

    /// A usable id token for the user: cache hit inside the TTL, otherwise a
    /// refresh with the stored refresh token. A rejected refresh token means
    /// the link is broken (password change, revocation) and needs a re-link.
    async fn id_token(&self, user_id: Uuid) -> Result<String, TokenError> {
        {
            let tokens = self.tokens.lock().expect("token cache lock");
            if let Some(cached) = tokens.get(&user_id)
                && cached.fetched_at.elapsed() < TOKEN_CACHE_TTL
            {
                return Ok(cached.id_token.clone());
            }
        }
        let refresh_token = {
            let client = self
                .db
                .get()
                .await
                .map_err(|e| TokenError::Transport(e.to_string()))?;
            match CyberspaceAccount::find_by_user_id(&client, user_id).await {
                Ok(Some(account)) => account.refresh_token,
                Ok(None) => return Err(TokenError::NotLinked),
                Err(e) => return Err(TokenError::Transport(e.to_string())),
            }
        };
        match self.api.refresh(&refresh_token).await {
            Ok(tokens) => {
                self.cache_token(user_id, tokens.id_token.clone());
                Ok(tokens.id_token)
            }
            Err(CsApiError::Api { code, .. }) => Err(TokenError::Broken(code)),
            Err(CsApiError::Transport(message)) => Err(TokenError::Transport(message)),
        }
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
