use chrono::{DateTime, Utc};
use ratatui_textarea::{TextArea, WrapMode};
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::app::common::{composer, primitives::Banner};
use late_core::models::article::{ArticleEvent, ArticleFeedItem, ArticleSnapshot};

use super::svc::ArticleService;

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
    unread_count: i64,
    last_read_at: Option<DateTime<Utc>>,
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
        article_service.refresh_unread_count_task(user_id);
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
            unread_count: 0,
            last_read_at: None,
            marker_read_at: None,
            preserve_marker_read_at: false,
            composing: false,
            composer: new_news_textarea(),
            processing: false,
            current_task: None,
        }
    }

    /// All articles known to the client, ignoring any mine-only filter.
    /// Used by surfaces that should not be affected by chat-page filtering
    /// (e.g. the dashboard wire).
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

        // The article exists but is hidden by the mine-only filter (e.g.
        // dashboard wire jumped to someone else's article). Drop the filter
        // so the article becomes visible and selectable.
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

    pub fn unread_count(&self) -> i64 {
        self.unread_count
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
        self.marker_read_at = self.last_read_at;
        self.preserve_marker_read_at = true;
        self.unread_count = 0;
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

    pub fn tick(&mut self) -> Option<Banner> {
        self.drain_snapshot();
        self.drain_events()
    }

    fn drain_snapshot(&mut self) {
        if let Ok(true) = self.snapshot_rx.has_changed() {
            let snapshot = self.snapshot_rx.borrow_and_update().clone();
            self.source_articles = snapshot.articles;
            self.rebuild_display();
        }
    }

    fn drain_events(&mut self) -> Option<Banner> {
        let mut banner = None;
        loop {
            match self.event_rx.try_recv() {
                Ok(event) => match event {
                    ArticleEvent::Created { user_id, .. } if self.user_id == user_id => {
                        self.current_task = None;
                        self.composing = false;
                        self.processing = false;
                        self.composer = new_news_textarea();
                        banner = Some(Banner::success("Article shared!"));
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
                    ArticleEvent::UnreadCountUpdated {
                        user_id,
                        unread_count,
                        last_read_at,
                    } if self.user_id == user_id => {
                        self.unread_count = unread_count;
                        self.last_read_at = last_read_at;
                        if unread_count == 0 && !self.preserve_marker_read_at {
                            self.marker_read_at = last_read_at;
                        }
                    }
                    ArticleEvent::NewArticlesAvailable {
                        user_id,
                        unread_count,
                    } if self.user_id == user_id => {
                        let increased = unread_count > self.unread_count;
                        self.unread_count = unread_count;
                        if increased {
                            let noun = if unread_count == 1 {
                                "article"
                            } else {
                                "articles"
                            };
                            banner = Some(Banner::success(&format!(
                                "{unread_count} new {noun} in news"
                            )));
                        }
                    }
                    _ => (),
                },
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(e) => {
                    tracing::error!(%e, "failed to receive article event");
                    break;
                }
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
mod tests {
    use super::{clamp_index, move_index};

    #[test]
    fn clamp_index_handles_empty_list() {
        assert_eq!(clamp_index(4, 0), 0);
    }

    #[test]
    fn clamp_index_caps_to_last_item() {
        assert_eq!(clamp_index(9, 3), 2);
    }

    #[test]
    fn move_index_moves_within_bounds() {
        assert_eq!(move_index(2, -1, 5), 1);
        assert_eq!(move_index(2, 2, 5), 4);
    }

    #[test]
    fn move_index_clamps_at_edges() {
        assert_eq!(move_index(0, -1, 5), 0);
        assert_eq!(move_index(4, 1, 5), 4);
    }

    #[test]
    fn move_index_returns_zero_for_empty_list() {
        assert_eq!(move_index(0, 1, 0), 0);
        assert_eq!(move_index(3, -1, 0), 0);
    }

    #[test]
    fn clamp_index_passes_through_when_within_bounds() {
        assert_eq!(clamp_index(1, 5), 1);
    }
}
