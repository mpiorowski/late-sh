use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::app::common::primitives::Banner;
use late_core::models::article::{ArticleEvent, ArticleFeedItem, ArticleSnapshot};

use super::svc::ArticleService;

pub struct State {
    article_service: ArticleService,
    user_id: Uuid,
    is_admin: bool,
    articles: Vec<ArticleFeedItem>,
    selected: usize,
    snapshot_rx: watch::Receiver<ArticleSnapshot>,
    event_rx: broadcast::Receiver<ArticleEvent>,
    unread_count: i64,
    composing: bool,
    composer: String,
    processing: bool,
    current_task: Option<tokio::task::AbortHandle>,
}

impl State {
    pub fn new(article_service: ArticleService, user_id: Uuid, is_admin: bool) -> Self {
        let snapshot_rx = article_service.subscribe_snapshot();
        let event_rx = article_service.subscribe_events();
        article_service.refresh_unread_count_task(user_id);
        Self {
            article_service,
            user_id,
            is_admin,
            articles: Vec::new(),
            selected: 0,
            snapshot_rx,
            event_rx,
            unread_count: 0,
            composing: false,
            composer: String::new(),
            processing: false,
            current_task: None,
        }
    }

    pub fn all_articles(&self) -> &[ArticleFeedItem] {
        &self.articles
    }

    pub fn list_articles(&self) {
        self.article_service.list_articles_task();
        self.article_service.refresh_unread_count_task(self.user_id);
    }

    pub fn selected_index(&self) -> usize {
        clamp_index(self.selected, self.articles.len())
    }

    pub fn move_selection(&mut self, delta: isize) {
        self.selected = move_index(self.selected_index(), delta, self.articles.len());
    }

    pub fn selected_url(&self) -> Option<&str> {
        self.articles
            .get(self.selected_index())
            .map(|item| item.article.url.as_str())
    }

    pub fn unread_count(&self) -> i64 {
        self.unread_count
    }

    pub fn composing(&self) -> bool {
        self.composing
    }

    pub fn composer(&self) -> &str {
        &self.composer
    }

    pub fn processing(&self) -> bool {
        self.processing
    }

    pub fn start_composing(&mut self) {
        self.composing = true;
        self.processing = false;
    }

    pub fn stop_composing(&mut self) {
        if let Some(task) = self.current_task.take() {
            task.abort();
        }
        self.composing = false;
        self.composer.clear();
        self.processing = false;
    }

    pub fn mark_read(&mut self) {
        self.unread_count = 0;
        self.article_service.mark_read_task(self.user_id);
    }

    pub fn composer_push(&mut self, ch: char) {
        if !self.processing {
            self.composer.push(ch);
        }
    }

    pub fn composer_clear(&mut self) {
        if !self.processing {
            self.composer.clear();
        }
    }
    pub fn composer_pop(&mut self) {
        if !self.processing {
            self.composer.pop();
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
        if self.processing || self.composer.trim().is_empty() {
            return;
        }
        self.processing = true;
        self.current_task = Some(
            self.article_service
                .process_url(self.user_id, self.composer.trim()),
        );
    }

    pub fn tick(&mut self) -> Option<Banner> {
        self.drain_snapshot();
        self.drain_events()
    }

    fn drain_snapshot(&mut self) {
        if let Ok(true) = self.snapshot_rx.has_changed() {
            let snapshot = self.snapshot_rx.borrow_and_update().clone();
            self.articles = snapshot.articles;
            self.selected = clamp_index(self.selected, self.articles.len());
        }
    }

    fn drain_events(&mut self) -> Option<Banner> {
        let mut banner = None;
        loop {
            match self.event_rx.try_recv() {
                Ok(event) => match event {
                    ArticleEvent::Created { user_id } if self.user_id == user_id => {
                        self.current_task = None;
                        self.composing = false;
                        self.processing = false;
                        self.composer.clear();
                        banner = Some(Banner::success("Article shared!"));
                    }
                    ArticleEvent::Failed { user_id, error } if self.user_id == user_id => {
                        self.current_task = None;
                        self.processing = false;
                        banner = Some(Banner::error(&format!("Failed: {}", error)));
                    }
                    ArticleEvent::Deleted { user_id } if self.user_id == user_id => {
                        banner = Some(Banner::success("Article deleted."));
                    }
                    ArticleEvent::UnreadCountUpdated {
                        user_id,
                        unread_count,
                    } if self.user_id == user_id => {
                        self.unread_count = unread_count;
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
