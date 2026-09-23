use chrono::{DateTime, Utc};
use ratatui_textarea::{TextArea, WrapMode};
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::app::common::{composer, primitives::Banner};
use late_core::models::article::{
    ArticleEvent, ArticleFeedItem, ArticleSnapshot, NEWS_FEED_LIMIT, NEWS_SHARE_MAX_PAID_PER_DAY,
    NEWS_SHARE_REWARD_CHIPS, NewsShareReward,
};

/// The success banner for a share, from what it actually minted. Both share
/// surfaces (the News composer and an RSS `s`) read this, so neither can
/// claim chips the ledger never wrote.
pub fn news_share_banner(lead: &str, reward: NewsShareReward) -> Banner {
    match reward {
        NewsShareReward::Paid => {
            Banner::success(&format!("{lead} +{NEWS_SHARE_REWARD_CHIPS} chips"))
        }
        NewsShareReward::RepeatUrl => Banner::success(&format!(
            "{lead} Already paid for this link, no chips this time."
        )),
        NewsShareReward::DailyCapReached => Banner::success(&format!(
            "{lead} Today's {NEWS_SHARE_MAX_PAID_PER_DAY} paid shares are used up, no chips this time."
        )),
    }
}

use super::svc::ArticleService;

/// This session's news read cursor. Until the first load answers, the badge
/// stays empty rather than guessing: a missing cursor row (`Loaded(None)`)
/// means everything is unread, which is not the same as not knowing yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReadCursor {
    Loading,
    Loaded(Option<DateTime<Utc>>),
}

/// The news badge: articles in the shared snapshot newer than the reader's
/// cursor, no cursor row meaning all of them. The snapshot holds the newest
/// [`NEWS_FEED_LIMIT`] articles, so the count saturates there.
pub(crate) fn unread_in_snapshot(
    articles: &[ArticleFeedItem],
    last_read_at: Option<DateTime<Utc>>,
) -> i64 {
    let unread = articles
        .iter()
        .filter(|item| is_unread(item, last_read_at))
        .count();
    unread as i64
}

/// Badge text for a news unread count: a full snapshot of unread articles
/// reads as a floor ("20+"), since older unread ones sit past the snapshot.
pub(crate) fn news_unread_label(unread: i64) -> String {
    if unread >= NEWS_FEED_LIMIT {
        format!("{NEWS_FEED_LIMIT}+")
    } else {
        unread.to_string()
    }
}

/// Whether a refreshed snapshot brought this reader something to announce:
/// an article the previous snapshot did not hold, still unread, shared by
/// someone else. The first snapshot a session sees is its starting state,
/// not news, so it never announces.
pub(crate) fn has_fresh_unread_from_others(
    previous: &[ArticleFeedItem],
    next: &[ArticleFeedItem],
    last_read_at: Option<DateTime<Utc>>,
    reader: Uuid,
) -> bool {
    if previous.is_empty() {
        return false;
    }
    next.iter().any(|item| {
        item.article.user_id != reader
            && is_unread(item, last_read_at)
            && !previous
                .iter()
                .any(|seen| seen.article.id == item.article.id)
    })
}

fn is_unread(item: &ArticleFeedItem, last_read_at: Option<DateTime<Utc>>) -> bool {
    match last_read_at {
        Some(last_read_at) => item.article.created > last_read_at,
        None => true,
    }
}

/// Outcome of one tab tick: the banner to surface plus whether a drained
/// snapshot or event may have changed the rendered tab (badge counts, article list).
pub struct NewsTick {
    pub banner: Option<Banner>,
    pub changed: bool,
}

pub struct State {
    article_service: ArticleService,
    user_id: Uuid,
    is_admin: bool,
    source_articles: Vec<ArticleFeedItem>,
    articles: Vec<ArticleFeedItem>,
    mine_only: bool,
    selected: usize,
    snapshot_rx: watch::Receiver<ArticleSnapshot>,
    event_rx: broadcast::Receiver<ArticleEvent>,
    read_cursor: ReadCursor,
    marker_read_at: Option<DateTime<Utc>>,
    preserve_marker_read_at: bool,
    composing: bool,
    composer: TextArea<'static>,
    processing: bool,
    current_task: Option<tokio::task::AbortHandle>,
}

impl State {
    pub fn new(article_service: ArticleService, user_id: Uuid, is_admin: bool) -> Self {
        let snapshot_rx = article_service.subscribe_snapshot();
        let event_rx = article_service.subscribe_events();
        article_service.list_articles_task();
        article_service.load_read_cursor_task(user_id);
        Self {
            article_service,
            user_id,
            is_admin,
            source_articles: Vec::new(),
            articles: Vec::new(),
            mine_only: false,
            selected: 0,
            snapshot_rx,
            event_rx,
            read_cursor: ReadCursor::Loading,
            marker_read_at: None,
            preserve_marker_read_at: false,
            composing: false,
            composer: new_news_textarea(),
            processing: false,
            current_task: None,
        }
    }

    /// All articles known to the client, ignoring any mine-only filter.
    /// Used by surfaces that should not be affected by chat-page filtering.
    pub fn all_articles(&self) -> &[ArticleFeedItem] {
        &self.source_articles
    }

    /// Articles in current display order, with the mine-only filter applied
    /// when active. This is what the chat news view renders and what the
    /// j/k/d/Enter selection operates on.
    pub fn displayed_articles(&self) -> &[ArticleFeedItem] {
        &self.articles
    }

    pub fn set_is_admin(&mut self, is_admin: bool) {
        self.is_admin = is_admin;
    }

    pub fn list_articles(&self) {
        self.article_service.list_articles_task();
    }

    pub fn mine_only(&self) -> bool {
        self.mine_only
    }

    pub fn toggle_mine_only(&mut self) {
        self.mine_only = !self.mine_only;
        self.rebuild_display();
    }

    fn rebuild_display(&mut self) {
        let prev_id = self
            .articles
            .get(self.selected.min(self.articles.len().saturating_sub(1)))
            .map(|item| item.article.id);

        let mut next: Vec<ArticleFeedItem> = self.source_articles.clone();
        if self.mine_only {
            next.retain(|item| item.article.user_id == self.user_id);
        }

        self.articles = next;
        if let Some(id) = prev_id
            && let Some(idx) = self.articles.iter().position(|item| item.article.id == id)
        {
            self.selected = idx;
        } else {
            self.selected = clamp_index(self.selected, self.articles.len());
        }
    }

    pub fn selected_index(&self) -> usize {
        clamp_index(self.selected, self.articles.len())
    }

    pub fn select_article_by_id(&mut self, article_id: Uuid) {
        if let Some(index) = self
            .articles
            .iter()
            .position(|item| item.article.id == article_id)
        {
            self.selected = index;
            return;
        }

        // The article exists but is hidden by the mine-only filter. Drop the
        // filter so the article becomes visible and selectable.
        if self.mine_only
            && self
                .source_articles
                .iter()
                .any(|item| item.article.id == article_id)
        {
            self.mine_only = false;
            self.rebuild_display();
            if let Some(index) = self
                .articles
                .iter()
                .position(|item| item.article.id == article_id)
            {
                self.selected = index;
            }
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        self.selected = move_index(self.selected_index(), delta, self.articles.len());
    }

    pub fn selected_url(&self) -> Option<&str> {
        self.articles
            .get(self.selected_index())
            .map(|item| item.article.url.as_str())
    }

    pub fn selected_item(&self) -> Option<&ArticleFeedItem> {
        self.articles.get(self.selected_index())
    }

    pub fn article_id_by_url(&self, url: &str) -> Option<Uuid> {
        let url = url.trim();
        if url.is_empty() {
            return None;
        }
        self.source_articles
            .iter()
            .find(|item| item.article.url.trim() == url)
            .map(|item| item.article.id)
    }

    pub fn unread_count(&self) -> i64 {
        match self.read_cursor {
            ReadCursor::Loading => 0,
            ReadCursor::Loaded(last_read_at) => {
                unread_in_snapshot(&self.source_articles, last_read_at)
            }
        }
    }

    pub fn marker_read_at(&self) -> Option<DateTime<Utc>> {
        self.marker_read_at
    }

    pub fn composing(&self) -> bool {
        self.composing
    }

    pub fn composer(&self) -> &TextArea<'static> {
        &self.composer
    }

    pub fn refresh_composer_theme(&mut self) {
        composer::apply_themed_textarea_style(&mut self.composer, self.composing);
    }

    pub fn processing(&self) -> bool {
        self.processing
    }

    pub fn start_composing(&mut self) {
        self.composing = true;
        self.processing = false;
        composer::set_themed_textarea_cursor_visible(&mut self.composer, true);
    }

    pub fn stop_composing(&mut self) {
        if let Some(task) = self.current_task.take() {
            task.abort();
        }
        self.composing = false;
        self.composer = new_news_textarea();
        self.processing = false;
    }

    pub fn mark_read(&mut self) {
        self.marker_read_at = match self.read_cursor {
            ReadCursor::Loading => None,
            ReadCursor::Loaded(last_read_at) => last_read_at,
        };
        self.preserve_marker_read_at = true;
        // Clear the badge now; the stored cursor comes back as
        // `ReadCursorLoaded` once the write lands.
        self.read_cursor = ReadCursor::Loaded(Some(Utc::now()));
        self.article_service.mark_read_task(self.user_id);
    }

    pub fn composer_push(&mut self, ch: char) {
        if !self.processing {
            self.composer.insert_char(ch);
        }
    }

    pub fn composer_clear(&mut self) {
        if !self.processing {
            self.composer = new_news_textarea();
            composer::set_themed_textarea_cursor_visible(&mut self.composer, self.composing);
        }
    }
    pub fn composer_pop(&mut self) {
        if !self.processing {
            self.composer.delete_char();
        }
    }

    pub fn composer_paste(&mut self) {
        if !self.processing {
            self.composer.paste();
        }
    }

    pub fn composer_undo(&mut self) {
        if !self.processing {
            self.composer.undo();
        }
    }

    pub fn composer_delete_right(&mut self) {
        if !self.processing {
            self.composer.delete_next_char();
        }
    }

    pub fn composer_delete_word_left(&mut self) {
        if !self.processing {
            self.composer.delete_word();
        }
    }

    pub fn composer_delete_word_right(&mut self) {
        if !self.processing {
            self.composer.delete_next_word();
        }
    }

    pub fn composer_cursor_left(&mut self) {
        if !self.processing {
            self.composer
                .move_cursor(ratatui_textarea::CursorMove::Back);
        }
    }

    pub fn composer_cursor_right(&mut self) {
        if !self.processing {
            self.composer
                .move_cursor(ratatui_textarea::CursorMove::Forward);
        }
    }

    pub fn composer_cursor_word_left(&mut self) {
        if !self.processing {
            self.composer
                .move_cursor(ratatui_textarea::CursorMove::WordBack);
        }
    }

    pub fn composer_cursor_word_right(&mut self) {
        if !self.processing {
            self.composer
                .move_cursor(ratatui_textarea::CursorMove::WordForward);
        }
    }

    pub fn composer_cursor_home(&mut self) {
        if !self.processing {
            self.composer
                .move_cursor(ratatui_textarea::CursorMove::Head);
        }
    }

    pub fn composer_cursor_end(&mut self) {
        if !self.processing {
            self.composer.move_cursor(ratatui_textarea::CursorMove::End);
        }
    }

    pub fn delete_selected(&mut self) {
        if let Some(item) = self.articles.get(self.selected_index()) {
            let is_owner = item.article.user_id == self.user_id;
            if !is_owner && !self.is_admin {
                return;
            }
            self.article_service
                .delete_article(self.user_id, item.article.id, self.is_admin);
        }
    }

    pub fn submit_composer(&mut self) {
        let url = self.composer.lines().join("");
        if self.processing || url.trim().is_empty() {
            return;
        }
        self.processing = true;
        composer::set_themed_textarea_cursor_visible(&mut self.composer, false);
        self.current_task = Some(self.article_service.process_url(self.user_id, url.trim()));
    }

    pub fn tick(&mut self) -> NewsTick {
        // Peek before draining: anything queued may change the rendered tab
        // (badge counts, article list), so it counts as changed.
        let changed = self.snapshot_rx.has_changed().unwrap_or(false) || !self.event_rx.is_empty();
        let snapshot_banner = self.drain_snapshot();
        let event_banner = self.drain_events();
        NewsTick {
            banner: event_banner.or(snapshot_banner),
            changed,
        }
    }

    fn drain_snapshot(&mut self) -> Option<Banner> {
        let Ok(true) = self.snapshot_rx.has_changed() else {
            return None;
        };
        let snapshot = self.snapshot_rx.borrow_and_update().clone();
        let announce = match self.read_cursor {
            ReadCursor::Loading => false,
            ReadCursor::Loaded(last_read_at) => has_fresh_unread_from_others(
                &self.source_articles,
                &snapshot.articles,
                last_read_at,
                self.user_id,
            ),
        };
        self.source_articles = snapshot.articles;
        self.rebuild_display();
        if !announce {
            return None;
        }
        let unread = self.unread_count();
        let noun = if unread == 1 { "article" } else { "articles" };
        Some(Banner::success(&format!(
            "{} new {noun} in news",
            news_unread_label(unread)
        )))
    }

    fn drain_events(&mut self) -> Option<Banner> {
        let mut banner = None;
        loop {
            match self.event_rx.try_recv() {
                Ok(event) => match event {
                    ArticleEvent::Created {
                        user_id, reward, ..
                    } if self.user_id == user_id => {
                        self.current_task = None;
                        self.composing = false;
                        self.processing = false;
                        self.composer = new_news_textarea();
                        banner = Some(news_share_banner("Article shared!", reward));
                    }
                    ArticleEvent::Failed { user_id, error, .. } if self.user_id == user_id => {
                        self.current_task = None;
                        self.processing = false;
                        composer::set_themed_textarea_cursor_visible(
                            &mut self.composer,
                            self.composing,
                        );
                        banner = Some(Banner::error(&format!("Failed: {}", error)));
                    }
                    ArticleEvent::Deleted { user_id } if self.user_id == user_id => {
                        banner = Some(Banner::success("Article deleted."));
                    }
                    ArticleEvent::ReadCursorLoaded {
                        user_id,
                        last_read_at,
                    } if self.user_id == user_id => {
                        self.read_cursor = ReadCursor::Loaded(last_read_at);
                        if self.unread_count() == 0 && !self.preserve_marker_read_at {
                            self.marker_read_at = last_read_at;
                        }
                    }
                    _ => (),
                },
                Err(broadcast::error::TryRecvError::Empty) => break,
                // Skipped events are gone; the receiver resumes at the oldest
                // one still buffered, so keep draining.
                Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "article event receiver lagged");
                }
                Err(broadcast::error::TryRecvError::Closed) => break,
            }
        }
        banner
    }
}

fn clamp_index(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { index.min(len - 1) }
}

fn move_index(current: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }

    (current as isize + delta).clamp(0, len as isize - 1) as usize
}

fn new_news_textarea() -> TextArea<'static> {
    composer::new_themed_textarea("", WrapMode::Glyph, false)
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
