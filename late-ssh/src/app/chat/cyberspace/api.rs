//! Typed client for the cyberspace.online v1 API.
//!
//! late.sh acts as a personal client the linked human drives: every call runs
//! under that user's own bearer token, one call per user action. Their API
//! terms ban bots, scraping, and feeding content to AI systems, so nothing
//! fetched here may be cached server-side, shown to other users, or routed
//! into any AI pipeline. Errors never carry credentials or tokens.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::time::Duration;

pub const BASE_URL: &str = "https://api.cyberspace.online";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const FEED_PAGE_LIMIT: u8 = 30;
const REPLIES_PAGE_LIMIT: u8 = 50;
const NOTIFICATIONS_PAGE_LIMIT: u8 = 20;

/// One arm per failure the UI tells apart: a server-reported API error
/// (rendered by code + message) versus a transport/decode failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CsApiError {
    Api { code: String, message: String },
    Transport(String),
}

impl std::fmt::Display for CsApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Api { code, message } => write!(f, "{code}: {message}"),
            Self::Transport(message) => write!(f, "network error: {message}"),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginTokens {
    pub id_token: String,
    // Absent on refresh responses: the stored refresh token stays valid.
    #[serde(default)]
    pub refresh_token: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsPost {
    pub post_id: String,
    #[serde(default)]
    pub author_username: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub replies_count: i64,
    #[serde(default)]
    pub is_nsfw: bool,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsReply {
    pub reply_id: String,
    #[serde(default)]
    pub author_username: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsNotification {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub actor_username: Option<String>,
    /// What the notification is about. For `post` and `reply` targets this is
    /// the **post** id either way: a reply notification names the post that
    /// was replied to, and puts the reply's own id in `metadata.replyId`.
    #[serde(default)]
    pub target_id: Option<String>,
    #[serde(default)]
    pub target_type: Option<String>,
    #[serde(default)]
    pub read: bool,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl CsNotification {
    /// The post this notification can open, when it has one. Follows and
    /// pokes target a user rather than a post, so they open nothing.
    pub fn post_id(&self) -> Option<&str> {
        match self.target_type.as_deref() {
            Some("post" | "reply") => self.target_id.as_deref(),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedPost {
    pub post_id: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CsIdentity {
    pub cs_user_id: String,
    pub cs_username: String,
}

#[derive(Clone, Debug)]
pub struct NewPost {
    pub content: String,
    pub title: Option<String>,
    pub topics: Vec<String>,
}

#[derive(Clone)]
pub struct CsApi {
    http: reqwest::Client,
    base_url: String,
}

impl CsApi {
    pub fn new(base_url: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent("late.sh (personal client; https://late.sh)")
            .build()
            .expect("reqwest client with static config always builds");
        Self { http, base_url }
    }

    pub async fn login(&self, email: &str, password: &str) -> Result<LoginTokens, CsApiError> {
        self.post_json(
            "/v1/auth/login",
            None,
            &serde_json::json!({ "email": email, "password": password }),
        )
        .await
    }

    pub async fn refresh(&self, refresh_token: &str) -> Result<LoginTokens, CsApiError> {
        self.post_json(
            "/v1/auth/refresh",
            None,
            &serde_json::json!({ "refreshToken": refresh_token }),
        )
        .await
    }

    /// Own profile, parsed leniently: the docs pin down the endpoint but not
    /// the exact field names, so accept the obvious spellings for the id.
    pub async fn me(&self, id_token: &str) -> Result<CsIdentity, CsApiError> {
        let data: serde_json::Value = self.get_json("/v1/users/me", id_token).await?;
        let cs_username = data
            .get("username")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        let cs_user_id = ["userId", "uid", "id"]
            .iter()
            .find_map(|key| data.get(*key).and_then(|value| value.as_str()))
            .unwrap_or_default()
            .to_string();
        if cs_username.is_empty() || cs_user_id.is_empty() {
            return Err(CsApiError::Transport(
                "profile response missing username or id".to_string(),
            ));
        }
        Ok(CsIdentity {
            cs_user_id,
            cs_username,
        })
    }

    pub async fn list_feed(&self, id_token: &str) -> Result<Vec<CsPost>, CsApiError> {
        self.get_json(&format!("/v1/posts?limit={FEED_PAGE_LIMIT}"), id_token)
            .await
    }

    pub async fn create_post(
        &self,
        id_token: &str,
        post: &NewPost,
    ) -> Result<CreatedPost, CsApiError> {
        let mut body = serde_json::json!({ "content": post.content });
        if let Some(title) = &post.title {
            body["title"] = serde_json::json!(title);
        }
        if !post.topics.is_empty() {
            body["topics"] = serde_json::json!(post.topics);
        }
        self.post_json("/v1/posts", Some(id_token), &body).await
    }

    pub async fn list_replies(
        &self,
        id_token: &str,
        post_id: &str,
    ) -> Result<Vec<CsReply>, CsApiError> {
        self.get_json(
            &format!("/v1/posts/{post_id}/replies?limit={REPLIES_PAGE_LIMIT}"),
            id_token,
        )
        .await
    }

    pub async fn create_reply(
        &self,
        id_token: &str,
        post_id: &str,
        content: &str,
    ) -> Result<(), CsApiError> {
        self.post_void(
            "/v1/replies",
            Some(id_token),
            &serde_json::json!({ "postId": post_id, "content": content }),
        )
        .await
    }

    pub async fn list_notifications(
        &self,
        id_token: &str,
    ) -> Result<Vec<CsNotification>, CsApiError> {
        self.get_json(
            &format!("/v1/notifications?limit={NOTIFICATIONS_PAGE_LIMIT}"),
            id_token,
        )
        .await
    }

    /// One post by id, for opening a thread the feed page does not hold (a
    /// notification about an older entry). Ids only: slugs 404 here.
    pub async fn get_post(&self, id_token: &str, post_id: &str) -> Result<CsPost, CsApiError> {
        self.get_json(&format!("/v1/posts/{post_id}"), id_token)
            .await
    }

    pub async fn unread_count(&self, id_token: &str) -> Result<i64, CsApiError> {
        #[derive(Deserialize)]
        struct Count {
            count: i64,
        }
        let count: Count = self
            .get_json("/v1/notifications/unread-count", id_token)
            .await?;
        Ok(count.count)
    }

    pub async fn mark_all_notifications_read(&self, id_token: &str) -> Result<(), CsApiError> {
        self.post_void(
            "/v1/notifications/read-all",
            Some(id_token),
            &serde_json::json!({}),
        )
        .await
    }

    async fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        id_token: &str,
    ) -> Result<T, CsApiError> {
        let response = self
            .http
            .get(format!("{}{path}", self.base_url))
            .bearer_auth(id_token)
            .send()
            .await
            .map_err(transport)?;
        decode_envelope(response).await
    }

    async fn post_json<T: DeserializeOwned>(
        &self,
        path: &str,
        id_token: Option<&str>,
        body: &serde_json::Value,
    ) -> Result<T, CsApiError> {
        let mut request = self
            .http
            .post(format!("{}{path}", self.base_url))
            .json(body);
        if let Some(token) = id_token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.map_err(transport)?;
        decode_envelope(response).await
    }

    /// POST to an endpoint whose payload we do not read. See [`parse_void`].
    async fn post_void(
        &self,
        path: &str,
        id_token: Option<&str>,
        body: &serde_json::Value,
    ) -> Result<(), CsApiError> {
        let mut request = self
            .http
            .post(format!("{}{path}", self.base_url))
            .json(body);
        if let Some(token) = id_token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.map_err(transport)?;
        let status = response.status().as_u16();
        let body = response.text().await.map_err(transport)?;
        parse_void(status, &body)
    }
}

fn transport(error: reqwest::Error) -> CsApiError {
    // reqwest errors embed the URL but never the request body or headers,
    // so no credentials can leak through this string.
    CsApiError::Transport(error.without_url().to_string())
}

async fn decode_envelope<T: DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, CsApiError> {
    let status = response.status();
    let body = response.text().await.map_err(transport)?;
    parse_envelope(status.as_u16(), &body)
}

#[derive(Deserialize)]
struct ErrorBody {
    code: String,
    #[serde(default)]
    message: String,
}

/// Every response is `{ "data": ... }` or `{ "error": { code, message } }`.
/// The error branch wins whenever present, since 4xx/5xx bodies carry it.
fn parse_envelope<T: DeserializeOwned>(status: u16, body: &str) -> Result<T, CsApiError> {
    #[derive(Deserialize)]
    struct Envelope<T> {
        data: Option<T>,
        error: Option<ErrorBody>,
    }

    let envelope: Envelope<T> = match serde_json::from_str(body) {
        Ok(envelope) => envelope,
        Err(error) => {
            return Err(CsApiError::Transport(format!(
                "unexpected response (HTTP {status}): {error}"
            )));
        }
    };
    match (envelope.data, envelope.error) {
        (_, Some(error)) => Err(CsApiError::Api {
            code: error.code,
            message: error.message,
        }),
        (Some(data), None) => Ok(data),
        (None, None) => Err(CsApiError::Transport(format!(
            "response carried neither data nor error (HTTP {status})"
        ))),
    }
}

/// Same envelope, for the endpoints whose payload we deliberately ignore.
/// Only the error branch matters: a 2xx carrying `{"data": null}`, or no body
/// at all, means the call landed. Routing those through `parse_envelope` with
/// `T = Value` reports a successful write as a transport failure, which on the
/// reply path means a landed reply looks failed and the user sends it twice.
fn parse_void(status: u16, body: &str) -> Result<(), CsApiError> {
    #[derive(Deserialize)]
    struct Envelope {
        error: Option<ErrorBody>,
    }

    // An explicit error wins whatever the status says, same as parse_envelope.
    if let Ok(Envelope { error: Some(error) }) = serde_json::from_str::<Envelope>(body) {
        return Err(CsApiError::Api {
            code: error.code,
            message: error.message,
        });
    }
    match (200..300).contains(&status) {
        true => Ok(()),
        false => Err(CsApiError::Transport(format!(
            "unexpected response (HTTP {status})"
        ))),
    }
}

#[cfg(test)]
#[path = "api_test.rs"]
mod api_test;
