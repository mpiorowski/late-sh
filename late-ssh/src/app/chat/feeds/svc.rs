use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use late_core::{
    db::Db,
    models::{
        rss_entry::{RssEntry, RssEntryParams, RssEntryView},
        rss_feed::RssFeed,
    },
    telemetry::TracedExt,
};
use std::time::Duration;
use tokio::sync::{broadcast, watch};
use tracing::{Instrument, info_span};
use uuid::Uuid;

const ENTRY_LIMIT: i64 = 100;
const POLL_LIMIT: i64 = 64;
const POLL_INTERVAL: Duration = Duration::from_secs(30 * 60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
const FEED_MAX_BYTES: u64 = 1_000_000;
const MAX_ENTRIES_PER_FETCH: usize = 20;

#[derive(Clone, Default)]
pub struct FeedSnapshot {
    pub user_id: Option<Uuid>,
    pub feeds: Vec<RssFeed>,
    pub entries: Vec<RssEntryView>,
}

#[derive(Clone, Debug)]
pub enum FeedEvent {
    FeedAdded { user_id: Uuid },
    FeedDeleted { user_id: Uuid },
    FeedFailed { user_id: Uuid, error: String },
    EntriesUpdated { user_id: Uuid, unread_count: i64 },
    EntryDismissed { user_id: Uuid },
    EntryShared { user_id: Uuid },
}

#[derive(Clone)]
pub struct FeedService {
    db: Db,
    http_client: reqwest::Client,
    snapshot_tx: watch::Sender<FeedSnapshot>,
    snapshot_rx: watch::Receiver<FeedSnapshot>,
    evt_tx: broadcast::Sender<FeedEvent>,
}

impl FeedService {
    pub fn new(db: Db) -> Self {
        let (snapshot_tx, snapshot_rx) = watch::channel(FeedSnapshot::default());
        let (evt_tx, _) = broadcast::channel(256);
        Self {
            db,
            http_client: reqwest::Client::new(),
            snapshot_tx,
            snapshot_rx,
            evt_tx,
        }
    }

    pub fn subscribe_snapshot(&self) -> watch::Receiver<FeedSnapshot> {
        self.snapshot_rx.clone()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<FeedEvent> {
        self.evt_tx.subscribe()
    }

    fn publish_event(&self, event: FeedEvent) {
        if let Err(e) = self.evt_tx.send(event) {
            tracing::debug!(%e, "no feed event subscribers");
        }
    }

    pub fn list_task(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service.do_list(user_id).await {
                    late_core::error_span!(
                        "feed_list_failed",
                        error = ?e,
                        user_id = %user_id,
                        "failed to list feeds"
                    );
                }
            }
            .instrument(info_span!("feed.list", user_id = %user_id)),
        );
    }

    async fn do_list(&self, user_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        let feeds = RssFeed::list_for_user(&client, user_id).await?;
        let entries = RssEntry::list_pending_for_user(&client, user_id, ENTRY_LIMIT).await?;
        self.snapshot_tx.send(FeedSnapshot {
            user_id: Some(user_id),
            feeds,
            entries,
        })?;
        Ok(())
    }

    pub fn add_feed_task(&self, user_id: Uuid, url: String) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result = async {
                    let url = normalize_feed_url(&url)?;
                    let client = service.db.get().await?;
                    let feed = RssFeed::create_for_user(&client, user_id, &url).await?;
                    drop(client);
                    service.fetch_feed(feed).await?;
                    service.do_list(user_id).await?;
                    Ok::<_, anyhow::Error>(())
                }
                .await;

                match result {
                    Ok(()) => service.publish_event(FeedEvent::FeedAdded { user_id }),
                    Err(e) => {
                        late_core::error_span!(
                            "feed_add_failed",
                            error = ?e,
                            user_id = %user_id,
                            "failed to add feed"
                        );
                        service.publish_event(FeedEvent::FeedFailed {
                            user_id,
                            error: e.to_string(),
                        });
                    }
                }
            }
            .instrument(info_span!("feed.add", user_id = %user_id)),
        );
    }

    pub fn delete_feed_task(&self, user_id: Uuid, feed_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let result = async {
                    let client = service.db.get().await?;
                    RssFeed::delete_for_user(&client, user_id, feed_id).await?;
                    drop(client);
                    service.do_list(user_id).await?;
                    Ok::<_, anyhow::Error>(())
                }
                .await;

                match result {
                    Ok(()) => service.publish_event(FeedEvent::FeedDeleted { user_id }),
                    Err(e) => service.publish_event(FeedEvent::FeedFailed {
                        user_id,
                        error: e.to_string(),
                    }),
                }
            }
            .instrument(info_span!(
                "feed.delete",
                user_id = %user_id,
                feed_id = %feed_id
            )),
        );
    }

    pub fn dismiss_entry_task(&self, user_id: Uuid, entry_id: Uuid) {
        let service = self.clone();
        tokio::spawn(async move {
            if let Err(e) = service.do_dismiss_entry(user_id, entry_id).await {
                late_core::error_span!(
                    "feed_entry_dismiss_failed",
                    error = ?e,
                    user_id = %user_id,
                    entry_id = %entry_id,
                    "failed to dismiss feed entry"
                );
            }
        });
    }

    async fn do_dismiss_entry(&self, user_id: Uuid, entry_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        RssEntry::dismiss(&client, user_id, entry_id).await?;
        drop(client);
        self.do_list(user_id).await?;
        self.publish_event(FeedEvent::EntryDismissed { user_id });
        Ok(())
    }

    pub fn mark_shared_task(&self, user_id: Uuid, entry_id: Uuid) {
        let service = self.clone();
        tokio::spawn(async move {
            if let Err(e) = service.do_mark_shared(user_id, entry_id).await {
                late_core::error_span!(
                    "feed_entry_shared_failed",
                    error = ?e,
                    user_id = %user_id,
                    entry_id = %entry_id,
                    "failed to mark feed entry shared"
                );
            }
        });
    }

    async fn do_mark_shared(&self, user_id: Uuid, entry_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        RssEntry::mark_shared(&client, user_id, entry_id).await?;
        drop(client);
        self.do_list(user_id).await?;
        self.publish_event(FeedEvent::EntryShared { user_id });
        Ok(())
    }

    pub fn poll_once_task(&self) {
        let service = self.clone();
        tokio::spawn(async move {
            if let Err(e) = service.poll_once().await {
                late_core::error_span!("feed_poll_failed", error = ?e, "failed to poll feeds");
            }
        });
    }

    pub fn start_poll_task(&self) {
        let service = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(POLL_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            interval.tick().await;
            loop {
                interval.tick().await;
                if let Err(e) = service.poll_once().await {
                    late_core::error_span!("feed_poll_failed", error = ?e, "failed to poll feeds");
                }
            }
        });
    }

    async fn poll_once(&self) -> Result<()> {
        let feeds = {
            let client = self.db.get().await?;
            RssFeed::list_active(&client, POLL_LIMIT).await?
        };
        for feed in feeds {
            let user_id = feed.user_id;
            match self.fetch_feed(feed.clone()).await {
                Ok(new_count) if new_count > 0 => {
                    self.do_list(user_id).await?;
                    let unread_count = {
                        let client = self.db.get().await?;
                        RssEntry::unread_count_for_user(&client, user_id).await?
                    };
                    self.publish_event(FeedEvent::EntriesUpdated {
                        user_id,
                        unread_count,
                    });
                }
                Ok(_) => {}
                Err(e) => {
                    let client = self.db.get().await?;
                    RssFeed::record_failure(&client, feed.id, &e.to_string()).await?;
                    self.publish_event(FeedEvent::FeedFailed {
                        user_id,
                        error: e.to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    async fn fetch_feed(&self, feed: RssFeed) -> Result<usize> {
        let xml = self.fetch_feed_body(&feed.url).await?;
        let parsed = parse_feed(&feed.url, &xml)?;
        let title = if parsed.title.trim().is_empty() {
            feed.url.clone()
        } else {
            parsed.title
        };

        let client = self.db.get().await?;
        let mut inserted = 0;
        for entry in parsed.entries.into_iter().take(MAX_ENTRIES_PER_FETCH) {
            if RssEntry::upsert_for_feed(
                &client,
                RssEntryParams {
                    feed_id: feed.id,
                    user_id: feed.user_id,
                    guid: truncate(entry.guid.trim(), 2000),
                    url: truncate(entry.url.trim(), 2000),
                    title: truncate(non_empty(&entry.title, "Untitled"), 500),
                    summary: truncate(entry.summary.trim(), 2000),
                    published_at: entry.published_at,
                    shared_at: None,
                    dismissed_at: None,
                },
            )
            .await?
            .is_some()
            {
                inserted += 1;
            }
        }
        RssFeed::record_success(&client, feed.id, &truncate(title.trim(), 500)).await?;
        Ok(inserted)
    }

    async fn fetch_feed_body(&self, url: &str) -> Result<String> {
        let response = tokio::time::timeout(FETCH_TIMEOUT, self.http_client.get(url).send_traced())
            .await
            .context("feed fetch timed out")??;
        if !response.status().is_success() {
            anyhow::bail!("feed returned HTTP {}", response.status());
        }
        if let Some(len) = response.content_length()
            && len > FEED_MAX_BYTES
        {
            anyhow::bail!("feed body exceeds {} bytes", FEED_MAX_BYTES);
        }

        let bytes = response.bytes().await?;
        if bytes.len() as u64 > FEED_MAX_BYTES {
            anyhow::bail!("feed body exceeds {} bytes", FEED_MAX_BYTES);
        }
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }
}

#[derive(Debug)]
struct ParsedFeed {
    title: String,
    entries: Vec<ParsedEntry>,
}

#[derive(Debug)]
struct ParsedEntry {
    guid: String,
    url: String,
    title: String,
    summary: String,
    published_at: Option<DateTime<Utc>>,
}

fn normalize_feed_url(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    let parsed =
        reqwest::Url::parse(trimmed).context("feed URL must include http:// or https://")?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed.to_string()),
        _ => anyhow::bail!("feed URL must use http:// or https://"),
    }
}

fn parse_feed(base_url: &str, xml: &str) -> Result<ParsedFeed> {
    let title = extract_first_tag(xml, "title").unwrap_or_default();
    let mut entries = Vec::new();

    for item in split_elements(xml, "item") {
        if let Some(entry) = parse_rss_item(base_url, item) {
            entries.push(entry);
        }
    }
    for item in split_elements(xml, "entry") {
        if let Some(entry) = parse_atom_entry(base_url, item) {
            entries.push(entry);
        }
    }

    if entries.is_empty() {
        anyhow::bail!("no RSS/Atom entries found");
    }

    Ok(ParsedFeed { title, entries })
}

fn parse_rss_item(base_url: &str, item: &str) -> Option<ParsedEntry> {
    let url = extract_first_tag(item, "link")
        .and_then(|link| resolve_url(base_url, &link))
        .or_else(|| {
            extract_first_tag(item, "guid").and_then(|guid| resolve_url(base_url, &guid))
        })?;
    let guid = extract_first_tag(item, "guid").unwrap_or_else(|| url.clone());
    let title = extract_first_tag(item, "title").unwrap_or_else(|| url.clone());
    let summary = extract_first_tag(item, "description")
        .or_else(|| extract_first_tag(item, "content:encoded"))
        .unwrap_or_default();
    let published_at = extract_first_tag(item, "pubDate")
        .or_else(|| extract_first_tag(item, "dc:date"))
        .and_then(|value| parse_feed_date(&value));
    Some(ParsedEntry {
        guid,
        url,
        title,
        summary,
        published_at,
    })
}

fn parse_atom_entry(base_url: &str, item: &str) -> Option<ParsedEntry> {
    let url = extract_atom_link(item)
        .and_then(|link| resolve_url(base_url, &link))
        .or_else(|| extract_first_tag(item, "id").and_then(|id| resolve_url(base_url, &id)))?;
    let guid = extract_first_tag(item, "id").unwrap_or_else(|| url.clone());
    let title = extract_first_tag(item, "title").unwrap_or_else(|| url.clone());
    let summary = extract_first_tag(item, "summary")
        .or_else(|| extract_first_tag(item, "content"))
        .unwrap_or_default();
    let published_at = extract_first_tag(item, "published")
        .or_else(|| extract_first_tag(item, "updated"))
        .and_then(|value| parse_feed_date(&value));
    Some(ParsedEntry {
        guid,
        url,
        title,
        summary,
        published_at,
    })
}

fn split_elements<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut rest = xml;
    let close = format!("</{tag}>");
    loop {
        let Some(start) = find_open_tag(rest, tag) else {
            break;
        };
        let after_start = &rest[start..];
        let Some(open_end) = after_start.find('>') else {
            break;
        };
        let content_start = start + open_end + 1;
        let Some(close_start_rel) = rest[content_start..].find(&close) else {
            break;
        };
        let close_start = content_start + close_start_rel;
        out.push(&rest[content_start..close_start]);
        rest = &rest[close_start + close.len()..];
    }
    out
}

fn extract_first_tag(xml: &str, tag: &str) -> Option<String> {
    let start = find_open_tag(xml, tag)?;
    let after_start = &xml[start..];
    let open_end = after_start.find('>')?;
    let content_start = start + open_end + 1;
    let close = format!("</{tag}>");
    let close_start = xml[content_start..].find(&close)? + content_start;
    Some(clean_text(&xml[content_start..close_start]))
}

fn find_open_tag(xml: &str, tag: &str) -> Option<usize> {
    let open_plain = format!("<{tag}>");
    let open_attrs = format!("<{tag} ");
    let plain = xml.find(&open_plain);
    let attrs = xml.find(&open_attrs);
    match (plain, attrs) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn extract_atom_link(item: &str) -> Option<String> {
    let mut rest = item;
    while let Some(idx) = rest.find("<link") {
        rest = &rest[idx..];
        let end = rest.find('>')?;
        let tag = &rest[..=end];
        let rel = attr_value(tag, "rel").unwrap_or_else(|| "alternate".to_string());
        if rel == "alternate" || rel.is_empty() {
            if let Some(href) = attr_value(tag, "href") {
                return Some(clean_text(&href));
            }
        }
        rest = &rest[end + 1..];
    }
    None
}

fn attr_value(tag: &str, name: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let pattern = format!("{name}={quote}");
        if let Some(start) = tag.find(&pattern) {
            let value_start = start + pattern.len();
            let value_end = tag[value_start..].find(quote)? + value_start;
            return Some(tag[value_start..value_end].to_string());
        }
    }
    None
}

fn clean_text(input: &str) -> String {
    let no_cdata = input
        .trim()
        .strip_prefix("<![CDATA[")
        .and_then(|s| s.strip_suffix("]]>"))
        .unwrap_or(input)
        .trim();
    decode_entities(&strip_tags(no_cdata))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

fn decode_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

fn resolve_url(base_url: &str, value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(url) = reqwest::Url::parse(value) {
        return matches!(url.scheme(), "http" | "https").then(|| url.to_string());
    }
    reqwest::Url::parse(base_url)
        .ok()
        .and_then(|base| base.join(value).ok())
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .map(|url| url.to_string())
}

fn parse_feed_date(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc2822(value)
        .or_else(|_| DateTime::parse_from_rfc3339(value))
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}

fn truncate(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    input.chars().take(max_chars).collect()
}

fn non_empty<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::parse_feed;

    #[test]
    fn parse_feed_reads_rss_items() {
        let xml = r#"
            <rss><channel><title>Blog</title>
            <item><title>Hello</title><link>/hello</link><guid>1</guid><description><![CDATA[<p>Hi</p>]]></description></item>
            </channel></rss>
        "#;
        let feed = parse_feed("https://example.com/feed.xml", xml).expect("feed");
        assert_eq!(feed.title, "Blog");
        assert_eq!(feed.entries[0].url, "https://example.com/hello");
        assert_eq!(feed.entries[0].summary, "Hi");
    }

    #[test]
    fn parse_feed_reads_atom_entries() {
        let xml = r#"
            <feed><title>Atom</title>
            <entry><title>Post</title><id>tag:post</id><link href="https://example.com/post" /></entry>
            </feed>
        "#;
        let feed = parse_feed("https://example.com/feed", xml).expect("feed");
        assert_eq!(feed.entries[0].url, "https://example.com/post");
    }
}
