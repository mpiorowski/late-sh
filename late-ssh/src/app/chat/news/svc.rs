use crate::app::{ai::svc::AiService, chat::svc::ChatService};
use anyhow::{Context, Result};
use late_core::models::article::{ArticleEvent, ArticleFeedItem, ArticleSnapshot, NEWS_MARKER};
use late_core::{
    db::Db,
    models::{
        article::{Article, ArticleParams},
        article_feed_read::ArticleFeedRead,
        chat_message::ChatMessage,
        chat_room::ChatRoom,
        user::User,
    },
    telemetry::TracedExt,
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tokio::sync::{broadcast, watch};
use tracing::{Instrument, info_span};
use uuid::Uuid;

const NEWS_SEPARATOR: &str = " || ";
const ASCII_WIDTH: u32 = 12;
const ASCII_HEIGHT: u32 = 6;
const PROCESS_URL_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Clone)]
pub struct ArticleService {
    db: Db,
    ai_service: AiService,
    chat_service: ChatService,
    http_client: reqwest::Client,
    snapshot_tx: watch::Sender<ArticleSnapshot>,
    snapshot_rx: watch::Receiver<ArticleSnapshot>,
    evt_tx: broadcast::Sender<ArticleEvent>,
}

impl ArticleService {
    pub fn new(db: Db, ai_service: AiService, chat_service: ChatService) -> Self {
        let (snapshot_tx, snapshot_rx) = watch::channel(ArticleSnapshot::default());
        let (evt_tx, _) = broadcast::channel(512);

        Self {
            db,
            ai_service,
            chat_service,
            http_client: reqwest::Client::new(),
            snapshot_tx,
            snapshot_rx,
            evt_tx,
        }
    }
    pub fn subscribe_snapshot(&self) -> watch::Receiver<ArticleSnapshot> {
        self.snapshot_rx.clone()
    }
    pub fn subscribe_events(&self) -> broadcast::Receiver<ArticleEvent> {
        self.evt_tx.subscribe()
    }

    fn publish_snapshot(&self, snapshot: ArticleSnapshot) -> Result<()> {
        self.snapshot_tx.send(snapshot)?;
        Ok(())
    }
    fn publish_event(&self, event: ArticleEvent) {
        if let Err(e) = self.evt_tx.send(event) {
            tracing::error!(%e, "failed to send article event");
        }
    }

    pub fn refresh_unread_count_task(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(async move {
            if let Err(e) = service.publish_unread_count(user_id).await {
                late_core::error_span!(
                    "article_unread_refresh_failed",
                    error = ?e,
                    user_id = %user_id,
                    "failed to refresh article unread count"
                );
            }
        });
    }

    pub fn mark_read_task(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(async move {
            if let Err(e) = service.mark_read_and_publish(user_id).await {
                late_core::error_span!(
                    "article_mark_read_failed",
                    error = ?e,
                    user_id = %user_id,
                    "failed to mark article feed read"
                );
            }
        });
    }

    pub fn list_articles_task(&self) {
        let service = self.clone();
        tokio::spawn(async move {
            if let Err(e) = service.do_list_articles().await {
                late_core::error_span!(
                    "article_list_failed",
                    error = ?e,
                    "failed to list articles"
                );
            }
        });
    }

    #[tracing::instrument(skip(self))]
    async fn do_list_articles(&self) -> Result<()> {
        let db_client = self.db.get().await?;
        let articles = Article::list_recent(&db_client, 20).await?;
        let user_ids = articles
            .iter()
            .map(|article| article.user_id)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let usernames = User::list_usernames_by_ids(&db_client, &user_ids).await?;
        let articles = articles
            .into_iter()
            .map(|article| ArticleFeedItem {
                author_username: display_author(&usernames, article.user_id),
                article,
            })
            .collect();

        self.publish_snapshot(ArticleSnapshot {
            user_id: None, // Global feed
            articles,
        })?;

        Ok(())
    }

    async fn publish_unread_count(&self, user_id: Uuid) -> Result<()> {
        let db_client = self.db.get().await?;
        let unread_count = ArticleFeedRead::unread_count_for_user(&db_client, user_id).await?;
        let last_read_at = ArticleFeedRead::last_read_at(&db_client, user_id).await?;
        self.publish_event(ArticleEvent::UnreadCountUpdated {
            user_id,
            unread_count,
            last_read_at,
        });
        Ok(())
    }

    async fn mark_read_and_publish(&self, user_id: Uuid) -> Result<()> {
        let db_client = self.db.get().await?;
        ArticleFeedRead::mark_read_now(&db_client, user_id).await?;
        let last_read_at = ArticleFeedRead::last_read_at(&db_client, user_id).await?;
        self.publish_event(ArticleEvent::UnreadCountUpdated {
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
        let db_client = self.db.get().await?;
        let rows = db_client.query("SELECT id FROM users", &[]).await?;
        for row in rows {
            let user_id: Uuid = row.get("id");
            let unread_count = ArticleFeedRead::unread_count_for_user(&db_client, user_id).await?;
            let last_read_at = ArticleFeedRead::last_read_at(&db_client, user_id).await?;
            self.publish_event(ArticleEvent::UnreadCountUpdated {
                user_id,
                unread_count,
                last_read_at,
            });
            if announce_new && Some(user_id) != actor_user_id && unread_count > 0 {
                self.publish_event(ArticleEvent::NewArticlesAvailable {
                    user_id,
                    unread_count,
                });
            }
        }
        Ok(())
    }

    pub fn delete_article(&self, user_id: Uuid, article_id: Uuid, is_admin: bool) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result = async {
                    let client = service.db.get().await?;

                    // Fetch the article so we can (a) enforce ownership, (b)
                    // clean up the chat announcement afterwards.
                    let Some(article) = Article::get(&client, article_id).await? else {
                        anyhow::bail!("Article not found");
                    };
                    if !is_admin && article.user_id != user_id {
                        anyhow::bail!("Article not owned by you");
                    }

                    let count = Article::delete(&client, article_id).await?;
                    if count == 0 {
                        anyhow::bail!("Article already deleted");
                    }

                    // Delete the news announcement from general chat
                    if let Err(e) = ChatMessage::delete_news_by_user_and_url(
                        &client,
                        article.user_id,
                        NEWS_MARKER,
                        &article.url,
                    )
                    .await
                    {
                        tracing::warn!(
                            error = ?e,
                            url = %article.url,
                            "failed to delete news chat announcement"
                        );
                    }

                    service.do_list_articles().await?;
                    service.publish_unread_updates_for_all(false, None).await?;
                    Ok::<_, anyhow::Error>(())
                }
                .await;

                match result {
                    Ok(()) => service.publish_event(ArticleEvent::Deleted { user_id }),
                    Err(e) => {
                        late_core::error_span!(
                            "article_delete_failed",
                            error = ?e,
                            "failed to delete article"
                        );
                        service.publish_event(ArticleEvent::Failed {
                            user_id,
                            error: e.to_string(),
                        });
                    }
                }
            }
            .instrument(info_span!(
                "article.delete",
                user_id = %user_id,
                article_id = %article_id
            )),
        );
    }

    /// Spawns a background task to process the URL asynchronously
    pub fn process_url(&self, user_id: Uuid, url: &str) -> tokio::task::AbortHandle {
        let service = self.clone();
        let target_url = url.to_string();
        let span_url = target_url.clone();

        let handle = tokio::spawn(
            async move {
                let processing = tokio::time::timeout(
                    PROCESS_URL_TIMEOUT,
                    service.do_process_url(user_id, &target_url),
                )
                .await;

                if let Err(e) = match processing {
                    Ok(result) => result,
                    Err(_) => Err(anyhow::anyhow!(
                        "article processing timed out after {} seconds",
                        PROCESS_URL_TIMEOUT.as_secs()
                    )),
                } {
                    late_core::error_span!(
                        "article_process_failed",
                        error = ?e,
                        url = %target_url,
                        "failed to process article url"
                    );
                    service.publish_event(ArticleEvent::Failed {
                        user_id,
                        error: e.to_string(),
                    });
                }
            }
            .instrument(info_span!(
                "article.process_url_task",
                user_id = %user_id,
                url = %span_url
            )),
        );

        handle.abort_handle()
    }

    #[tracing::instrument(skip(self), fields(user_id = %user_id, url = %url))]
    async fn do_process_url(&self, user_id: Uuid, url: &str) -> Result<()> {
        // 1. Quick existence check — acquire and release before the slow AI work
        tracing::info!(%url, "checking article url");
        {
            let client = self.db.get().await?;
            if Article::find_by_url(&client, url).await?.is_some() {
                tracing::info!(%url, "article already exists, skipping");
                anyhow::bail!("Article exists");
            }
        }

        // YouTube gets its own path: oEmbed pins identity, AI writes the
        // summary. Everything else goes through the legacy AI-first flow.
        let extraction = if is_youtube_url(url) {
            self.extract_youtube(url).await?
        } else {
            self.extract_via_ai(url).await?
        };

        // 3. Fetch og:image and convert to ASCII — still no DB client
        tracing::info!(%url, "fetching og:image and generating ASCII art");
        let ascii_art = if let Some(img_url) = extraction
            .image_url
            .filter(|s| !s.trim().is_empty() && s != "null")
        {
            // Safely resolve relative URLs against the base URL
            let parsed_base = reqwest::Url::parse(url)?;
            let parsed_img = parsed_base.join(&img_url)?;

            match self.http_client.get(parsed_img).send_traced().await {
                Ok(res) => {
                    let bytes = res.bytes().await?;
                    late_core::ascii::bytes_to_ascii(&bytes, ASCII_WIDTH, ASCII_HEIGHT)
                        .unwrap_or_else(|_| "Image Convert Error".to_string())
                }
                Err(_) => "Image Fetch Failed".to_string(),
            }
        } else {
            // Generate a stable procedural ASCII pattern based on the URL hash.
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            url.hash(&mut hasher);
            let mut seed = hasher.finish();

            let chars = b" .:-=+*#%@";
            let width = ASCII_WIDTH as usize;
            let height = ASCII_HEIGHT as usize;
            let mut art = String::with_capacity(width * height + height);
            for y in 0..height {
                for _x in 0..width {
                    // Simple linear congruential generator step
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    // Use top bits for better pseudo-random distribution
                    let idx = (seed >> 59) as usize % chars.len();
                    art.push(chars[idx] as char);
                }
                if y < height - 1 {
                    art.push('\n');
                }
            }
            art
        };

        let announcement =
            build_news_chat_announcement(&extraction.title, &extraction.summary, url, &ascii_art);

        // 4. Save to database — scoped so the client is dropped before helper calls
        tracing::info!(%url, "saving article to database");
        {
            let db_client = self.db.get().await?;
            Article::create_by_user_id(
                &db_client,
                user_id,
                ArticleParams {
                    user_id,
                    url: url.to_string(),
                    title: extraction.title.trim().to_string(),
                    summary: extraction.summary.trim().to_string(),
                    ascii_art,
                },
            )
            .await?;
        }

        // Post the announcement into #general via the same send path as any
        // other message. No special-case task needed — resolve the room id
        // here and call send_message_task like a normal composer submit.
        let general_room_id = {
            let client = self.db.get().await?;
            ChatRoom::find_general(&client).await?.map(|room| room.id)
        };
        if let Some(room_id) = general_room_id {
            self.chat_service.send_message_task(
                user_id,
                room_id,
                Some("general".to_string()),
                announcement,
                Uuid::now_v7(),
                false,
            );
        }

        // Refresh the shared feed snapshot immediately so clients see the new item
        // without waiting for the periodic poll tick.
        if let Err(e) = self.do_list_articles().await {
            late_core::error_span!(
                "article_refresh_failed",
                error = ?e,
                "failed to refresh article snapshot after create"
            );
        }

        if let Err(e) = self
            .publish_unread_updates_for_all(true, Some(user_id))
            .await
        {
            late_core::error_span!(
                "article_unread_broadcast_failed",
                error = ?e,
                "failed to publish article unread updates after create"
            );
        }

        // 5. Publish Event
        tracing::info!(%url, "publishing ArticleEvent::Created");
        self.publish_event(ArticleEvent::Created { user_id });

        Ok(())
    }

    /// AI-first extraction for non-YouTube URLs, with a Twitter/X oEmbed
    /// fallback when the AI rejects or misidentifies the link.
    #[tracing::instrument(skip(self), fields(url = %url))]
    async fn extract_via_ai(&self, url: &str) -> Result<ArticleExtraction> {
        tracing::info!(%url, "researching article via AI search");
        let system_prompt = "You are a helpful assistant. Research the provided URL using Google Search. First, check if this is a valid article, news site, blog, or youtube video. If the link is explicitly NSFW (like PornHub), malicious, spam, or otherwise invalid, you MUST return strictly: {\"title\": \"INVALID_OR_NSFW\", \"image_url\": null, \"summary\": \"Rejected\"}. Otherwise, extract its actual title, the main image/thumbnail URL (often from og:image), and a 3 short bullet point summary of its content. Return the result strictly as a JSON object with keys: 'title', 'image_url' (null if none found), and 'summary'.";

        let json_str = self
            .ai_service
            .generate_json_with_search(system_prompt, url)
            .await?
            .context("AI failed to return extraction")?;

        let mut extraction: ArticleExtraction =
            serde_json::from_str(&json_str).context("failed to parse AI json extraction")?;

        if extraction.title == "INVALID_OR_NSFW" || extraction_looks_not_found(&extraction) {
            if is_twitter_url(url) {
                tracing::warn!(%url, "AI extraction for Twitter/X looked invalid; trying oEmbed fallback");
                extraction = self
                    .fetch_twitter_oembed_extraction(url)
                    .await?
                    .context("Twitter/X oEmbed fallback failed")?;
            } else if extraction.title == "INVALID_OR_NSFW" {
                tracing::warn!(%url, "AI rejected URL as invalid or NSFW");
                anyhow::bail!(
                    "Link was rejected due to content policy violations or being invalid."
                );
            }
        }

        Ok(extraction)
    }

    /// YouTube fast path. oEmbed gives us authoritative title/author/thumbnail
    /// pinned to the exact URL; the AI is then asked only for a summary with
    /// that identity injected as context. Worst case on AI failure is the
    /// generic `youtube_fallback_summary`.
    #[tracing::instrument(skip(self), fields(url = %url))]
    async fn extract_youtube(&self, url: &str) -> Result<ArticleExtraction> {
        let identity = self
            .fetch_youtube_identity(url)
            .await?
            .context("YouTube oEmbed could not resolve this video")?;

        let ai_summary = match self
            .fetch_youtube_ai_summary(&identity.title, &identity.author, url)
            .await
        {
            Ok(Some(summary)) if !summary.trim().is_empty() => Some(summary),
            Ok(_) => None,
            Err(e) => {
                tracing::warn!(error = ?e, "YouTube AI summary failed, using fallback");
                None
            }
        };

        let summary = ai_summary.unwrap_or_else(|| youtube_fallback_summary(&identity.author));

        Ok(ArticleExtraction {
            title: identity.title,
            image_url: identity.thumbnail_url,
            summary,
        })
    }

    /// Hit YouTube's oEmbed endpoint and return the video's canonical
    /// identity. `None` on any non-success or empty-title response.
    #[tracing::instrument(skip(self), fields(url = %url))]
    async fn fetch_youtube_identity(&self, url: &str) -> Result<Option<YoutubeIdentity>> {
        if !is_youtube_url(url) {
            return Ok(None);
        }

        let endpoint = reqwest::Url::parse_with_params(
            "https://www.youtube.com/oembed",
            &[("url", url), ("format", "json")],
        )?;

        let res = self.http_client.get(endpoint).send_traced().await?;
        if !res.status().is_success() {
            return Ok(None);
        }

        let payload: YoutubeOEmbedResponse = res.json().await?;
        let title = payload.title.trim().to_string();
        if title.is_empty() {
            return Ok(None);
        }
        let author = payload.author_name.trim().to_string();
        let thumbnail_url = if payload.thumbnail_url.trim().is_empty() {
            None
        } else {
            Some(payload.thumbnail_url)
        };

        Ok(Some(YoutubeIdentity {
            title,
            author,
            thumbnail_url,
        }))
    }

    /// Ask the AI for a summary of a known video. The user message must be
    /// just the raw URL — Gemini only invokes its Search tool when the
    /// prompt looks like a research task, not a formatting task. Verified
    /// title and channel go in the system prompt as context.
    #[tracing::instrument(skip(self), fields(url = %url, title = %title, author = %author))]
    async fn fetch_youtube_ai_summary(
        &self,
        title: &str,
        author: &str,
        url: &str,
    ) -> Result<Option<String>> {
        let system_prompt = format!(
            "You are a helpful assistant. Research the provided YouTube URL using Google Search and describe the video's content. For context, the video's verified title is \"{title}\" and the channel is \"{author}\" — use this to make sure Search results refer to the correct video. Write a concise 3 bullet point summary of what the video is about. If Search results are thin, use the title and channel to infer what the video covers. Return the result strictly as a JSON object with a single key 'summary' containing either an array of 3 short bullet point strings or a single string with the bullets separated by newlines. Do not wrap the JSON in markdown."
        );

        let Some(json_str) = self
            .ai_service
            .generate_json_with_search(&system_prompt, url)
            .await?
        else {
            return Ok(None);
        };

        tracing::debug!(%url, raw_response = %json_str, "YouTube AI summary response");

        match serde_json::from_str::<AiSummaryOnly>(&json_str) {
            Ok(parsed) => Ok(Some(parsed.summary)),
            Err(e) => {
                tracing::warn!(error = ?e, json = %json_str, "failed to parse YouTube AI summary JSON");
                Ok(None)
            }
        }
    }

    #[tracing::instrument(skip(self), fields(url = %url))]
    async fn fetch_twitter_oembed_extraction(
        &self,
        url: &str,
    ) -> Result<Option<ArticleExtraction>> {
        if !is_twitter_url(url) {
            return Ok(None);
        }

        let endpoint =
            reqwest::Url::parse_with_params("https://publish.twitter.com/oembed", &[("url", url)])?;

        let res = self.http_client.get(endpoint).send_traced().await?;
        if !res.status().is_success() {
            return Ok(None);
        }

        let payload: TwitterOEmbedResponse = res.json().await?;
        let author = payload.author_name.trim();
        if author.is_empty() {
            return Ok(None);
        }

        let title = format!("Post by {author}");
        let summary = format!(
            "• Post by {author} on X/Twitter.\n• Open the link to view the full post.\n• Metadata fetched from Twitter oEmbed."
        );

        Ok(Some(ArticleExtraction {
            title,
            image_url: None,
            summary,
        }))
    }
}

#[derive(Deserialize)]
struct ArticleExtraction {
    title: String,
    image_url: Option<String>,
    #[serde(with = "summary_parser")]
    summary: String,
}

#[derive(Deserialize)]
struct YoutubeOEmbedResponse {
    title: String,
    #[serde(default)]
    author_name: String,
    #[serde(default)]
    thumbnail_url: String,
}

#[derive(Deserialize)]
struct TwitterOEmbedResponse {
    #[serde(default)]
    author_name: String,
}

/// Verified YouTube video identity pulled from oEmbed.
struct YoutubeIdentity {
    title: String,
    author: String,
    thumbnail_url: Option<String>,
}

/// Slim JSON shape for the summary-only AI call in the YouTube path.
#[derive(Deserialize)]
struct AiSummaryOnly {
    #[serde(with = "summary_parser")]
    summary: String,
}

mod summary_parser {
    use serde::{Deserialize, Deserializer};
    pub fn deserialize<'de, D>(deserializer: D) -> Result<String, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Summary {
            String(String),
            Array(Vec<String>),
        }
        match Summary::deserialize(deserializer)? {
            Summary::String(s) => Ok(s),
            Summary::Array(arr) => {
                let bulleted = arr
                    .into_iter()
                    .map(|s| format!("• {}", s))
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(bulleted)
            }
        }
    }
}

/// Placeholder summary for when the AI summary call returns nothing —
/// usually because the API key is unset or Gemini came back empty.
fn youtube_fallback_summary(author: &str) -> String {
    let who = author.trim();
    let who = if who.is_empty() {
        "unknown channel"
    } else {
        who
    };
    format!("• YouTube video by {who}.\n• Open the link to watch on YouTube.")
}

fn display_author(usernames: &HashMap<Uuid, String>, user_id: Uuid) -> String {
    usernames
        .get(&user_id)
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| user_id.to_string()[..8].to_string())
}

fn is_youtube_url(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    host == "youtu.be"
        || host.ends_with(".youtu.be")
        || host == "youtube.com"
        || host.ends_with(".youtube.com")
        || host == "youtube-nocookie.com"
        || host.ends_with(".youtube-nocookie.com")
}

fn is_twitter_url(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    host == "twitter.com"
        || host.ends_with(".twitter.com")
        || host == "x.com"
        || host.ends_with(".x.com")
}

fn extraction_looks_not_found(extraction: &ArticleExtraction) -> bool {
    let title = extraction.title.trim().to_ascii_lowercase();
    let summary = extraction.summary.trim().to_ascii_lowercase();
    if title.is_empty() || summary.is_empty() {
        return true;
    }

    let markers = [
        "video not found",
        "unknown video",
        "could not be found",
        "does not return any public search results",
        "no content details are available",
        "non-existent",
        "private",
        "unlisted",
        "deleted",
    ];
    markers
        .iter()
        .any(|marker| title.contains(marker) || summary.contains(marker))
}

fn build_news_chat_announcement(title: &str, summary: &str, url: &str, ascii_art: &str) -> String {
    let title = truncate_for_chat(&sanitize_payload_field(title.trim()), 90);
    let summary = truncate_for_chat(&encode_summary_bullets(summary), 400);
    let url = sanitize_payload_field(url.trim());
    let ascii = encode_ascii_payload(ascii_art);
    let payload = format!(
        "{NEWS_MARKER} {title}{NEWS_SEPARATOR}{summary}{NEWS_SEPARATOR}{url}{NEWS_SEPARATOR}{ascii}"
    );
    truncate_for_chat(&payload, 1800)
}

fn truncate_for_chat(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }

    let mut out = String::new();
    for ch in input.chars().take(max_chars.saturating_sub(3)) {
        out.push(ch);
    }
    out.push_str("...");
    out
}

fn sanitize_payload_field(input: &str) -> String {
    input
        .replace(NEWS_SEPARATOR, " | ")
        .replace(['\n', '\r'], " ")
}

fn encode_summary_bullets(summary: &str) -> String {
    summary
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| {
            !line
                .to_ascii_lowercase()
                .starts_with("• no content details")
        })
        .take(3)
        .map(|line| {
            truncate_for_chat(
                &sanitize_payload_field(
                    line.trim_start_matches('•').trim_start_matches('-').trim(),
                ),
                120,
            )
        })
        .collect::<Vec<_>>()
        .join("\\n")
}

fn encode_ascii_payload(ascii_art: &str) -> String {
    ascii_art.replace('\\', "\\\\").replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::{
        ArticleExtraction, display_author, encode_ascii_payload, extraction_looks_not_found,
        is_twitter_url, is_youtube_url, sanitize_payload_field, truncate_for_chat,
    };
    use std::collections::HashMap;
    use uuid::Uuid;

    #[test]
    fn youtube_url_detection_covers_common_hosts() {
        assert!(is_youtube_url("https://www.youtube.com/watch?v=abc"));
        assert!(is_youtube_url("https://youtu.be/abc"));
        assert!(is_youtube_url("https://m.youtube.com/watch?v=abc"));
        assert!(!is_youtube_url("https://vimeo.com/123"));
    }

    #[test]
    fn not_found_detection_flags_low_confidence_ai_output() {
        let extraction = ArticleExtraction {
            title: "Video Not Found".to_string(),
            image_url: None,
            summary: "• No content details are available to generate a summary.".to_string(),
        };
        assert!(extraction_looks_not_found(&extraction));
    }

    #[test]
    fn not_found_detection_allows_normal_extractions() {
        let extraction = ArticleExtraction {
            title: "Never Run claude /init".to_string(),
            image_url: Some("https://i.ytimg.com/vi/abc/default.jpg".to_string()),
            summary: "• Explains tradeoffs of generated context files.".to_string(),
        };
        assert!(!extraction_looks_not_found(&extraction));
    }

    #[test]
    fn display_author_prefers_username() {
        let user_id = Uuid::now_v7();
        let mut usernames = HashMap::new();
        usernames.insert(user_id, "mat".to_string());
        assert_eq!(display_author(&usernames, user_id), "mat");
    }

    #[test]
    fn display_author_falls_back_to_short_id() {
        let user_id = Uuid::now_v7();
        let usernames = HashMap::new();
        assert_eq!(
            display_author(&usernames, user_id),
            user_id.to_string()[..8]
        );
    }

    #[test]
    fn encode_summary_bullets_preserves_all_bullets() {
        let summary = "• first point\n• second point\n• third point";
        assert_eq!(
            super::encode_summary_bullets(summary),
            "first point\\nsecond point\\nthird point"
        );
    }

    #[test]
    fn encode_summary_bullets_empty_input() {
        assert_eq!(super::encode_summary_bullets(""), "");
    }

    #[test]
    fn encode_summary_bullets_skips_no_content_lines() {
        let summary = "• No content details are available.\n• Actual point";
        assert_eq!(super::encode_summary_bullets(summary), "Actual point");
    }

    // --- truncate_for_chat ---

    #[test]
    fn truncate_for_chat_returns_short_string_unchanged() {
        assert_eq!(truncate_for_chat("hello", 10), "hello");
    }

    #[test]
    fn truncate_for_chat_at_exact_limit() {
        assert_eq!(truncate_for_chat("abcde", 5), "abcde");
    }

    #[test]
    fn truncate_for_chat_adds_ellipsis_when_over_limit() {
        assert_eq!(truncate_for_chat("abcdefghij", 7), "abcd...");
    }

    // --- sanitize_payload_field ---

    #[test]
    fn sanitize_payload_field_replaces_separator() {
        let input = format!("before{}after", super::NEWS_SEPARATOR);
        assert_eq!(sanitize_payload_field(&input), "before | after");
    }

    #[test]
    fn sanitize_payload_field_replaces_newlines() {
        assert_eq!(sanitize_payload_field("a\nb\rc"), "a b c");
    }

    // --- encode_ascii_payload ---

    #[test]
    fn encode_ascii_payload_encodes_newlines() {
        assert_eq!(encode_ascii_payload("a\nb"), "a\\nb");
    }

    #[test]
    fn encode_ascii_payload_escapes_backslashes() {
        assert_eq!(encode_ascii_payload("a\\b"), "a\\\\b");
    }

    #[test]
    fn encode_ascii_payload_handles_both() {
        assert_eq!(encode_ascii_payload("a\\b\nc"), "a\\\\b\\nc");
    }

    // --- edge cases for existing functions ---

    #[test]
    fn display_author_ignores_whitespace_only_username() {
        let user_id = Uuid::now_v7();
        let mut usernames = HashMap::new();
        usernames.insert(user_id, "   ".to_string());
        assert_eq!(
            display_author(&usernames, user_id),
            user_id.to_string()[..8]
        );
    }

    #[test]
    fn is_youtube_url_detects_nocookie_domain() {
        assert!(is_youtube_url("https://www.youtube-nocookie.com/embed/abc"));
    }

    #[test]
    fn is_youtube_url_rejects_invalid_url() {
        assert!(!is_youtube_url("not a url at all"));
    }

    #[test]
    fn twitter_url_detection_covers_common_hosts() {
        assert!(is_twitter_url("https://twitter.com/user/status/123"));
        assert!(is_twitter_url("https://x.com/user/status/123"));
        assert!(is_twitter_url("https://mobile.twitter.com/user/status/123"));
        assert!(!is_twitter_url("https://youtube.com/watch?v=abc"));
        assert!(!is_twitter_url("not a url at all"));
    }

    #[test]
    fn build_news_chat_announcement_is_compact_and_branded() {
        let msg = super::build_news_chat_announcement(
            "A very cool post title",
            "• one interesting summary point\n• another point",
            "https://example.com/article",
            ".:-\n+*#",
        );
        assert!(msg.starts_with(super::NEWS_MARKER));
        assert!(msg.contains(super::NEWS_SEPARATOR));
        assert!(msg.contains("A very cool post title"));
        assert!(msg.contains("one interesting summary point"));
        assert!(msg.contains("\\n"));
    }
}
