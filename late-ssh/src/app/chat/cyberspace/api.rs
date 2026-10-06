//! Typed client for the cyberspace.online v1 API.
//!
//! late.sh acts as a personal client the linked human drives: every call runs
//! under that user's own bearer token, one call per user action. Their API
//! terms ban bots, scraping, and feeding content to AI systems, so nothing
//! fetched here may be cached server-side, shown to other users, or routed
//! into any AI pipeline. Errors never carry credentials or tokens.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::time::Duration;

pub const BASE_URL: &str = "https://api.cyberspace.online";
/// Their website, which is where a shared link has to point: the API host
/// serves JSON, not pages. Entries live at `/{username}/{slug}`, the deep
/// link their notification metadata is documented against.
pub const WEB_URL: &str = "https://cyberspace.online";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// How much room and conversation history one page carries, their documented
/// cap for both. A mention jumped to from a notification names no message, so
/// the deeper the page a room opens with, the likelier the line that pulled
/// the user in is already on it; `before` pages further back.
const CIRC_HISTORY_LIMIT: u8 = 100;
/// The live window their realtime database replays when a stream opens. Their
/// hard ceiling is 100 and unbounded reads are rejected outright.
const CIRC_STREAM_WINDOW: u8 = 50;
const FEED_PAGE_LIMIT: u8 = 30;
/// How many entries the recurring unread probe pulls. The feed is newest-first
/// and this page only feeds a badge, so a small one is enough however long the
/// user has been away, and it keeps the poll cheap against a third party.
pub(crate) const UNREAD_PROBE_LIMIT: u8 = 10;
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
    /// Where their Realtime Database lives, which is where live cIRC messages
    /// come from. Both login and refresh answer with it, so a reopened stream
    /// after a token refresh never has to remember the old one.
    #[serde(default)]
    pub rtdb_url: Option<String>,
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
    /// Everything the fields above do not name. Their docs type `targetType`
    /// as `post | reply` and call `metadata` open-ended, so a `chat_mention`
    /// or a `dm_message` can carry its room or conversation anywhere in the
    /// payload; keeping the rest is what lets `shape` report it.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
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

    /// The reply a `reply` notification is about, which `target_id` does not
    /// name (that is the post). This is the finest grain the payload offers
    /// for telling two notifications apart, so it is what the notifications
    /// view dedupes on.
    pub fn reply_id(&self) -> Option<&str> {
        self.metadata
            .get("replyId")
            .and_then(|value| value.as_str())
    }

    /// The chat room a `chat_mention` happened in. Their payload names it
    /// twice, as `targetId` and as `metadata.roomSlug`, and carries no
    /// message id or message timestamp at all, so the room is the finest a
    /// jump can land. Other kinds never name a room.
    pub fn room_slug(&self) -> Option<&str> {
        if self.kind != "chat_mention" {
            return None;
        }
        self.metadata
            .get("roomSlug")
            .and_then(|value| value.as_str())
            .or(self.target_id.as_deref())
    }

    /// A content-free, one-line description of what this notification
    /// carries: its type, its target, and every other key with an id-shaped
    /// value. Diagnostic, for answering what a `chat_mention` and a
    /// `dm_message` actually point at, which their docs leave open-ended.
    ///
    /// Their terms keep their content out of any AI pipeline, and a log line
    /// is exactly where it would otherwise leak, so a value prints only where
    /// it cannot be prose: content keys are dropped by name (exact, or as a
    /// fragment of a longer non-id key), and any other string carrying
    /// whitespace or longer than `SHAPE_VALUE_MAX_CHARS` prints as its length
    /// instead of its text.
    pub fn shape(&self) -> String {
        let mut out = format!(
            "type={} targetType={} targetId={}",
            self.kind,
            self.target_type.as_deref().unwrap_or("-"),
            self.target_id.as_deref().unwrap_or("-"),
        );
        if let Some(metadata) = self.metadata.as_object() {
            for (key, value) in metadata {
                out.push_str(&format!(" metadata.{key}={}", shape_value(key, value, 0)));
            }
        }
        for (key, value) in &self.extra {
            out.push_str(&format!(" {key}={}", shape_value(key, value, 0)));
        }
        out
    }
}

/// Keys that carry their prose rather than an identifier. Matched
/// case-insensitively: exactly for any value, and as fragments of longer
/// keys for string values, since the payload is open-ended and only the
/// documented keys are known.
const CONTENT_KEYS: &[&str] = &[
    "content",
    "postcontent",
    "replycontent",
    "messagecontent",
    "text",
    "body",
    "message",
    "preview",
    "snippet",
    "excerpt",
    "title",
    "reason",
    "bio",
];
/// The longest string `shape` prints outright. An id, a slug, or a username
/// fits; a sentence does not.
const SHAPE_VALUE_MAX_CHARS: usize = 64;
/// How far `shape` walks into nested objects before reporting their key count
/// instead. Two levels is enough to see a `{ room: { id, slug } }` without
/// walking a whole payload.
const SHAPE_MAX_DEPTH: usize = 2;

/// Whether a string under `key` (already lowercased) is their prose by
/// name: any content word inside the key marks it, so an undocumented
/// `messagePreview` or `lastMessage` cannot slip past the exact list. Keys
/// naming an id are exempt: `messageId` is a pointer, not a sentence, and
/// pointers are what the log is for.
fn is_content_string_key(key: &str) -> bool {
    if key.ends_with("id") {
        return false;
    }
    CONTENT_KEYS.iter().any(|word| key.contains(word))
}

fn shape_value(key: &str, value: &serde_json::Value, depth: usize) -> String {
    let key = key.to_ascii_lowercase();
    if CONTENT_KEYS.contains(&key.as_str()) {
        return "<content>".to_string();
    }
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(flag) => flag.to_string(),
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::String(_) if is_content_string_key(&key) => "<content>".to_string(),
        serde_json::Value::String(text)
            if text.chars().count() <= SHAPE_VALUE_MAX_CHARS
                && !text.chars().any(char::is_whitespace) =>
        {
            text.clone()
        }
        serde_json::Value::String(text) => format!("<str:{}>", text.chars().count()),
        serde_json::Value::Array(items) => format!("<array:{}>", items.len()),
        serde_json::Value::Object(map) if depth >= SHAPE_MAX_DEPTH => {
            format!("<object:{}>", map.len())
        }
        serde_json::Value::Object(map) => {
            let inner: Vec<String> = map
                .iter()
                .map(|(key, value)| format!("{key}={}", shape_value(key, value, depth + 1)))
                .collect();
            format!("{{{}}}", inner.join(","))
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

/// One of their chat rooms. There is no join or leave over there: this roster
/// is simply what the account may read, and pinning one into our rail is our
/// own bookmark.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CircRoom {
    pub id: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub name: String,
    /// Milliseconds since the epoch, on their clock.
    #[serde(default)]
    pub last_message_at: Option<i64>,
    #[serde(default)]
    pub online_count: i64,
}

impl CircRoom {
    /// What the rail and room list call it. Their `slug` is what a user types
    /// and what we pin, so it wins over the display name.
    pub fn key(&self) -> &str {
        match self.slug.is_empty() {
            true => &self.id,
            false => &self.slug,
        }
    }
}

/// One message in a cIRC room or a C-Mail conversation. Their two chat
/// surfaces carry the same fields under two names for the author (`userId` /
/// `username` in a room, `senderId` / `senderUsername` in a conversation), so
/// the aliases collapse them here and everything downstream sees one message.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CircMessage {
    /// Absent in realtime-database payloads, where the message id is the key
    /// its object hangs off rather than a field inside it.
    #[serde(default)]
    pub id: String,
    #[serde(default, alias = "senderId")]
    pub user_id: String,
    #[serde(default, alias = "senderUsername")]
    pub username: String,
    #[serde(default)]
    pub is_chat_admin: bool,
    /// May be empty: an attachment can be the whole message.
    #[serde(default)]
    pub content: String,
    /// Milliseconds since the epoch, on their clock.
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub deleted: bool,
    /// `/me` and the emotes, rendered `* username content`.
    #[serde(default)]
    pub is_action: bool,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub gif_url: Option<String>,
    /// One style name or several; parsed into the same shape either way.
    /// Their wire name is singular whatever the shape.
    #[serde(default, rename = "style", deserialize_with = "deserialize_styles")]
    pub styles: Vec<String>,
}

/// Their `style` field is a name, a list of names, or absent. Parsing all
/// three into one list at the boundary means nothing downstream has to ask
/// which shape arrived.
fn deserialize_styles<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Styles {
        One(String),
        Many(Vec<String>),
        Absent,
    }
    match Styles::deserialize(deserializer)? {
        Styles::One(style) => Ok(vec![style]),
        Styles::Many(styles) => Ok(styles),
        Styles::Absent => Ok(Vec::new()),
    }
}

impl CircMessage {
    pub fn at(&self) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp_millis(self.timestamp)
    }

    /// The text to render. Three of their conventions collapse here so no
    /// caller has to know them: a deleted message is a tombstone whatever it
    /// used to say, `style: "art"` means the content is base64-encoded ASCII
    /// art rather than readable text, and a website post whose caption is just
    /// its own attachment URL would otherwise print the link twice.
    pub fn display_text(&self) -> String {
        if self.deleted {
            return "[deleted]".to_string();
        }
        if self.styles.iter().any(|style| style == "art") {
            return match BASE64.decode(self.content.trim()) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                // Undecodable art is not text either: printing the base64 at
                // the user is worse than saying nothing.
                Err(_) => String::new(),
            };
        }
        let attachment = self.image_url.as_deref().or(self.gif_url.as_deref());
        match attachment {
            Some(url) if url == self.content.trim() => String::new(),
            _ => self.content.clone(),
        }
    }

    /// The one-word tag for an attachment, so a message that is nothing but
    /// an image still renders as something.
    pub fn attachment_label(&self) -> Option<&'static str> {
        match (&self.image_url, &self.gif_url) {
            _ if self.deleted => None,
            (Some(_), _) => Some("[image]"),
            (None, Some(_)) => Some("[gif]"),
            (None, None) => None,
        }
    }
}

/// A page of room history plus the cursor for the page before it.
#[derive(Clone, Debug)]
pub struct CircHistory {
    pub messages: Vec<CircMessage>,
    pub cursor: Option<i64>,
}

/// Their history endpoints are documented by their fields rather than their
/// envelope shape, so accept both the bare list and the paged object. Shared
/// by rooms and C-Mail, which return the same two shapes.
#[derive(Deserialize)]
#[serde(untagged)]
enum HistoryBody {
    Paged {
        messages: Vec<CircMessage>,
        #[serde(default)]
        cursor: Option<i64>,
    },
    List(Vec<CircMessage>),
}

impl HistoryBody {
    fn into_history(self) -> CircHistory {
        match self {
            HistoryBody::Paged { messages, cursor } => CircHistory { messages, cursor },
            HistoryBody::List(messages) => CircHistory {
                cursor: messages.first().map(|message| message.timestamp),
                messages,
            },
        }
    }
}

/// Which realtime-database collection a live stream reads from. Their two
/// chat surfaces are the same mechanism over two nodes, so the opener takes
/// this instead of growing a second copy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamNode {
    /// cIRC rooms, keyed by room id.
    ChatMessages,
    /// C-Mail conversations, keyed by conversation id.
    DmMessages,
}

impl StreamNode {
    fn path(self) -> &'static str {
        match self {
            StreamNode::ChatMessages => "chat_messages",
            StreamNode::DmMessages => "dm_messages",
        }
    }
}

/// One C-Mail conversation as their list reports it. `unread_count` is theirs,
/// not ours: they read it back, so the rail badge is a real number instead of
/// the dot a cIRC room has to settle for.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CmailConversation {
    pub conversation_id: String,
    #[serde(default)]
    pub other_user: CmailUser,
    #[serde(default)]
    pub last_message: Option<String>,
    /// Milliseconds since the epoch, on their clock.
    #[serde(default)]
    pub last_message_at: Option<i64>,
    #[serde(default)]
    pub unread_count: i64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CmailUser {
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub username: String,
}

/// The answer to starting a conversation: their id for it, and who it is with.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CmailStarted {
    pub conversation_id: String,
    #[serde(default)]
    pub other_user: CmailUser,
}

/// The floor under the presence cadence. Their response names the interval
/// and is normally far above this; a zero or near-zero value from a
/// misbehaving response would otherwise turn the presence loop into a hot
/// cycle of authenticated POSTs.
pub const CIRC_PRESENCE_MIN_HEARTBEAT_MS: u64 = 5_000;

/// How often to heartbeat presence, read off their answer rather than
/// hard-coded: they publish the cadence precisely so clients do not guess it.
/// Floored at parse time, so a `CircPresence` can never carry a hot value.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CircPresence {
    #[serde(deserialize_with = "floored_heartbeat")]
    pub heartbeat_ms: u64,
    #[serde(default)]
    pub idle_after_ms: u64,
}

fn floored_heartbeat<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let value = u64::deserialize(deserializer)?;
    Ok(value.max(CIRC_PRESENCE_MIN_HEARTBEAT_MS))
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
        self.feed_page(id_token, FEED_PAGE_LIMIT).await
    }

    /// The newest handful of entries, for counting what is unread without
    /// pulling a whole page every time the badge refreshes.
    pub async fn list_recent_entries(&self, id_token: &str) -> Result<Vec<CsPost>, CsApiError> {
        self.feed_page(id_token, UNREAD_PROBE_LIMIT).await
    }

    async fn feed_page(&self, id_token: &str, limit: u8) -> Result<Vec<CsPost>, CsApiError> {
        self.get_json(&format!("/v1/posts?limit={limit}"), id_token)
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

    pub async fn list_circ_rooms(&self, id_token: &str) -> Result<Vec<CircRoom>, CsApiError> {
        self.get_json("/v1/circ", id_token).await
    }

    /// A page of room history, oldest-first. `before` is the cursor from the
    /// previous page, so paging walks backwards through scrollback.
    pub async fn read_circ_room(
        &self,
        id_token: &str,
        room_id: &str,
        before: Option<i64>,
    ) -> Result<CircHistory, CsApiError> {
        let path = match before {
            Some(before) => {
                format!("/v1/circ/{room_id}?limit={CIRC_HISTORY_LIMIT}&before={before}")
            }
            None => format!("/v1/circ/{room_id}?limit={CIRC_HISTORY_LIMIT}"),
        };
        let body: HistoryBody = self.get_json(&path, id_token).await?;
        Ok(body.into_history())
    }

    pub async fn send_circ_message(
        &self,
        id_token: &str,
        room_id: &str,
        content: &str,
    ) -> Result<(), CsApiError> {
        self.post_void(
            &format!("/v1/circ/{room_id}"),
            Some(id_token),
            &serde_json::json!({ "content": content }),
        )
        .await
    }

    pub async fn mark_circ_room_read(
        &self,
        id_token: &str,
        room_id: &str,
    ) -> Result<(), CsApiError> {
        self.post_void(
            &format!("/v1/circ/{room_id}/read"),
            Some(id_token),
            &serde_json::json!({}),
        )
        .await
    }

    /// Announce (or re-announce) the user in a room. This is what puts them in
    /// the room's user list, including for people reading on the website.
    pub async fn circ_presence(
        &self,
        id_token: &str,
        room_id: &str,
        last_activity_ms: i64,
    ) -> Result<CircPresence, CsApiError> {
        self.post_json(
            &format!("/v1/circ/{room_id}/presence"),
            Some(id_token),
            &serde_json::json!({ "lastActivity": last_activity_ms }),
        )
        .await
    }

    /// Drop out of the room's user list now rather than when the heartbeat
    /// goes stale. Politeness toward everyone else reading the room.
    pub async fn leave_circ_room(&self, id_token: &str, room_id: &str) -> Result<(), CsApiError> {
        let response = self
            .http
            .delete(format!("{}/v1/circ/{room_id}/presence", self.base_url))
            .bearer_auth(id_token)
            .send()
            .await
            .map_err(transport)?;
        let status = response.status().as_u16();
        let body = response.text().await.map_err(transport)?;
        parse_void(status, &body)
    }

    /// Their conversation list, newest activity first, with the unread count
    /// per conversation. This is the C-Mail badge: unlike the cIRC roster it
    /// reports read state back, so no cursor of ours is involved.
    pub async fn list_cmail(&self, id_token: &str) -> Result<Vec<CmailConversation>, CsApiError> {
        self.get_json("/v1/cmail", id_token).await
    }

    /// Start (or find) the conversation with a username. Idempotent on their
    /// side: an existing conversation comes back rather than a second one.
    pub async fn start_cmail(
        &self,
        id_token: &str,
        recipient_username: &str,
    ) -> Result<CmailStarted, CsApiError> {
        self.post_json(
            "/v1/cmail",
            Some(id_token),
            &serde_json::json!({ "recipientUsername": recipient_username }),
        )
        .await
    }

    /// A page of conversation history, oldest-first. Same two shapes as the
    /// room history endpoint.
    pub async fn read_cmail(
        &self,
        id_token: &str,
        conversation_id: &str,
        before: Option<i64>,
    ) -> Result<CircHistory, CsApiError> {
        let path = match before {
            Some(before) => {
                format!("/v1/cmail/{conversation_id}?limit={CIRC_HISTORY_LIMIT}&before={before}")
            }
            None => format!("/v1/cmail/{conversation_id}?limit={CIRC_HISTORY_LIMIT}"),
        };
        let body: HistoryBody = self.get_json(&path, id_token).await?;
        Ok(body.into_history())
    }

    pub async fn send_cmail(
        &self,
        id_token: &str,
        conversation_id: &str,
        content: &str,
    ) -> Result<(), CsApiError> {
        self.post_void(
            &format!("/v1/cmail/{conversation_id}"),
            Some(id_token),
            &serde_json::json!({ "content": content }),
        )
        .await
    }

    /// Zero the unread count on their side. Unlike the cIRC equivalent this
    /// one is readable back (their conversation list carries `unreadCount`),
    /// which is why C-Mail needs no read cursor of ours.
    pub async fn mark_cmail_read(
        &self,
        id_token: &str,
        conversation_id: &str,
    ) -> Result<(), CsApiError> {
        self.post_void(
            &format!("/v1/cmail/{conversation_id}/read"),
            Some(id_token),
            &serde_json::json!({}),
        )
        .await
    }

    /// Open the live message stream for a room: their realtime database over
    /// Server-Sent Events, under the user's own id token. The bounds are not
    /// optional politeness, unbounded reads are rejected: always ordered by
    /// timestamp and always windowed.
    ///
    /// The stream ends when the id token expires (~60 minutes), which the
    /// caller answers by minting a fresh one and opening a new stream.
    ///
    /// `node` is the realtime-database collection: `chat_messages` for a cIRC
    /// room, `dm_messages` for a C-Mail conversation. Their docs describe the
    /// two as the same mechanism, and the frames are identical, so one opener
    /// and one parser serve both.
    pub async fn open_message_stream(
        &self,
        rtdb_url: &str,
        node: StreamNode,
        id: &str,
        id_token: &str,
    ) -> Result<reqwest::Response, CsApiError> {
        let node = node.path();
        let url = format!(
            "{}/{node}/{id}.json?auth={id_token}&orderBy=%22timestamp%22&limitToLast={CIRC_STREAM_WINDOW}",
            rtdb_url.trim_end_matches('/')
        );
        let response = self
            .http
            .get(url)
            .header(reqwest::header::ACCEPT, "text/event-stream")
            // The stream is meant to stay open; the client-wide request
            // timeout would otherwise cut it every 15 seconds.
            .timeout(Duration::from_secs(60 * 60))
            .send()
            .await
            .map_err(transport)?;
        match response.status().is_success() {
            true => Ok(response),
            false => Err(CsApiError::Transport(format!(
                "chat stream refused (HTTP {})",
                response.status().as_u16()
            ))),
        }
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

/// What one realtime-database frame means for the room on screen. Their stream
/// carries edits as well as arrivals: a delete is a `patch` that rewrites a
/// message already held, so a client that only listens for new messages never
/// sees it.
#[derive(Clone, Debug)]
pub enum CircStreamEvent {
    /// The opening frame: the whole live window in one payload.
    Window(Vec<CircMessage>),
    Upsert(Box<CircMessage>),
    /// A field-level change to a message already on screen. Deletion is the
    /// one their docs promise; anything else is applied the same way.
    Patch {
        id: String,
        content: Option<String>,
        deleted: bool,
    },
    Removed(String),
}

/// Reassembles the stream's blank-line-separated frames from raw network
/// chunks. Chunk boundaries fall wherever TCP cuts them, including inside a
/// multi-byte character, so bytes stay bytes until a frame is complete and
/// each frame is decoded exactly once.
#[derive(Default)]
pub struct CircStreamBuffer {
    bytes: Vec<u8>,
}

impl CircStreamBuffer {
    /// Append a network chunk and drain every frame it completed, in order.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.bytes.extend_from_slice(chunk);
        let mut frames = Vec::new();
        while let Some(split) = self.bytes.windows(2).position(|window| window == b"\n\n") {
            let frame: Vec<u8> = self.bytes.drain(..split + 2).collect();
            frames.push(String::from_utf8_lossy(&frame).into_owned());
        }
        frames
    }

    /// Bytes still waiting for their frame to complete, for the caller's
    /// oversized-frame cap.
    pub fn pending_len(&self) -> usize {
        self.bytes.len()
    }
}

/// Parse one Server-Sent Events frame. `None` covers every frame that says
/// nothing about the room's messages: keep-alives, revocations, and the
/// deeper paths their database can emit but this view does not model.
pub fn parse_circ_stream_frame(frame: &str) -> Option<CircStreamEvent> {
    let mut event = "";
    let mut data = String::new();
    for line in frame.lines() {
        match line.split_once(':') {
            Some(("event", value)) => event = value.trim(),
            // A payload can be split across several `data:` lines.
            Some(("data", value)) => {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(value.trim_start());
            }
            _ => {}
        }
    }
    if !matches!(event, "put" | "patch") {
        return None;
    }

    #[derive(Deserialize)]
    struct Frame {
        path: String,
        data: serde_json::Value,
    }
    let frame: Frame = serde_json::from_str(&data).ok()?;
    let path = frame.path.trim_matches('/');

    if path.is_empty() {
        // The whole window. A `null` here is an empty room, not a failure.
        let entries = match frame.data {
            serde_json::Value::Object(entries) => entries,
            _ => return Some(CircStreamEvent::Window(Vec::new())),
        };
        let mut messages: Vec<CircMessage> = entries
            .into_iter()
            .filter_map(|(id, value)| message_from_entry(&id, value))
            .collect();
        messages.sort_by_key(|message| message.timestamp);
        return Some(CircStreamEvent::Window(messages));
    }

    // Only message-level paths are modelled; a deeper one (a single field)
    // arrives rarely and the next room load settles it.
    if path.contains('/') {
        return None;
    }
    let id = path.to_string();
    match (event, frame.data) {
        (_, serde_json::Value::Null) => Some(CircStreamEvent::Removed(id)),
        ("put", value) => {
            message_from_entry(&id, value).map(|m| CircStreamEvent::Upsert(Box::new(m)))
        }
        ("patch", value) => Some(CircStreamEvent::Patch {
            content: value
                .get("content")
                .and_then(|content| content.as_str())
                .map(str::to_string),
            deleted: value
                .get("deleted")
                .and_then(|deleted| deleted.as_bool())
                .unwrap_or_default(),
            id,
        }),
        _ => None,
    }
}

/// One entry of a realtime-database object, whose key is the message id.
fn message_from_entry(id: &str, value: serde_json::Value) -> Option<CircMessage> {
    let mut message: CircMessage = serde_json::from_value(value).ok()?;
    if message.id.is_empty() {
        message.id = id.to_string();
    }
    Some(message)
}

#[cfg(test)]
#[path = "api_test.rs"]
mod api_test;
