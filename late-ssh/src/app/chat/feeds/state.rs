use late_core::models::{article::ArticleEvent, rss_entry::RssEntryView, rss_feed::RssFeed};
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::app::{chat::news::svc::ArticleService, common::primitives::Banner};

use super::svc::{FeedEvent, FeedService, FeedSnapshot};

pub struct State {
    service: FeedService,
    article_service: ArticleService,
    user_id: Uuid,
    feeds: Vec<RssFeed>,
    entries: Vec<RssEntryView>,
    selected: usize,
    snapshot_rx: watch::Receiver<FeedSnapshot>,
    event_rx: broadcast::Receiver<FeedEvent>,
    article_event_rx: broadcast::Receiver<ArticleEvent>,
    pending_share: Option<(Uuid, String)>,
    unread_count: i64,
}

impl State {
    pub fn new(service: FeedService, article_service: ArticleService, user_id: Uuid) -> Self {
        let snapshot_rx = service.subscribe_snapshot();
        let event_rx = service.subscribe_events();
        let article_event_rx = article_service.subscribe_events();
        service.list_task(user_id);
        Self {
            service,
            article_service,
            user_id,
            feeds: Vec::new(),
            entries: Vec::new(),
            selected: 0,
            snapshot_rx,
            event_rx,
            article_event_rx,
            pending_share: None,
            unread_count: 0,
        }
    }

    pub fn all_entries(&self) -> &[RssEntryView] {
        &self.entries
    }

    pub fn has_feeds(&self) -> bool {
        !self.feeds.is_empty()
    }

    pub fn selected_index(&self) -> usize {
        clamp_index(self.selected, self.entries.len())
    }

    pub fn unread_count(&self) -> i64 {
        self.unread_count
    }

    pub fn list(&self) {
        self.service.list_task(self.user_id);
    }

    pub fn poll_now(&self) {
        self.service.poll_once_task();
        self.service.list_task(self.user_id);
    }

    pub fn move_selection(&mut self, delta: isize) {
        self.selected = move_index(self.selected_index(), delta, self.entries.len());
    }

    pub fn selected_url(&self) -> Option<&str> {
        self.entries
            .get(self.selected_index())
            .map(|item| item.entry.url.as_str())
    }

    pub fn share_selected(&mut self) -> Option<Banner> {
        let item = self.entries.get(self.selected_index())?;
        self.pending_share = Some((item.entry.id, item.entry.url.clone()));
        self.article_service
            .process_url(self.user_id, item.entry.url.as_str());
        Some(Banner::success("Sharing feed entry..."))
    }

    pub fn dismiss_selected(&mut self) -> Option<Banner> {
        let item = self.entries.get(self.selected_index())?;
        self.service.dismiss_entry_task(self.user_id, item.entry.id);
        Some(Banner::success("Feed entry dismissed."))
    }

    pub fn tick(&mut self) -> Option<Banner> {
        self.drain_snapshot();
        let feed_banner = self.drain_events();
        let article_banner = self.drain_article_events();
        feed_banner.or(article_banner)
    }

    fn drain_snapshot(&mut self) {
        if let Ok(true) = self.snapshot_rx.has_changed() {
            let snapshot = self.snapshot_rx.borrow_and_update().clone();
            if snapshot.user_id == Some(self.user_id) {
                self.feeds = snapshot.feeds;
                self.entries = snapshot.entries;
                self.selected = clamp_index(self.selected, self.entries.len());
                self.unread_count = self.entries.len() as i64;
            }
        }
    }

    fn drain_events(&mut self) -> Option<Banner> {
        let mut banner = None;
        loop {
            match self.event_rx.try_recv() {
                Ok(FeedEvent::FeedAdded { user_id }) if user_id == self.user_id => {
                    banner = Some(Banner::success("Feed connected."));
                }
                Ok(FeedEvent::FeedDeleted { user_id }) if user_id == self.user_id => {
                    banner = Some(Banner::success("Feed removed."));
                }
                Ok(FeedEvent::FeedFailed { user_id, error }) if user_id == self.user_id => {
                    banner = Some(Banner::error(&format!("Feed failed: {error}")));
                }
                Ok(FeedEvent::EntriesUpdated {
                    user_id,
                    unread_count,
                }) if user_id == self.user_id => {
                    self.unread_count = unread_count;
                    if unread_count > 0 {
                        banner = Some(Banner::success(&format!(
                            "{unread_count} feed entries ready"
                        )));
                    }
                }
                Ok(FeedEvent::EntryDismissed { user_id }) if user_id == self.user_id => {
                    banner = Some(Banner::success("Feed entry dismissed."));
                }
                Ok(FeedEvent::EntryShared { user_id }) if user_id == self.user_id => {
                    banner = Some(Banner::success("Feed entry shared."));
                }
                Ok(_) => {}
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(e) => {
                    tracing::error!(%e, "failed to receive feed event");
                    break;
                }
            }
        }
        banner
    }

    fn drain_article_events(&mut self) -> Option<Banner> {
        let mut banner = None;
        loop {
            match self.article_event_rx.try_recv() {
                Ok(ArticleEvent::Created { user_id, url })
                    if user_id == self.user_id
                        && self
                            .pending_share
                            .as_ref()
                            .is_some_and(|(_, pending_url)| pending_url == &url) =>
                {
                    if let Some((entry_id, _)) = self.pending_share.take() {
                        self.service.mark_shared_task(self.user_id, entry_id);
                    }
                }
                Ok(ArticleEvent::Failed {
                    user_id,
                    error,
                    url: Some(url),
                }) if user_id == self.user_id
                    && self.pending_share.is_some()
                    && self
                        .pending_share
                        .as_ref()
                        .is_some_and(|(_, pending_url)| pending_url == &url)
                    && error.to_ascii_lowercase().contains("exists") =>
                {
                    if let Some((entry_id, _)) = self.pending_share.take() {
                        self.service.mark_shared_task(self.user_id, entry_id);
                        banner = Some(Banner::success("Already shared."));
                    }
                }
                Ok(ArticleEvent::Failed {
                    user_id,
                    error,
                    url: Some(url),
                }) if user_id == self.user_id && self.pending_share.is_some() => {
                    if self
                        .pending_share
                        .as_ref()
                        .is_some_and(|(_, pending_url)| pending_url == &url)
                    {
                        self.pending_share = None;
                        banner = Some(Banner::error(&format!("Share failed: {error}")));
                    }
                }
                Ok(_) => {}
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(e) => {
                    tracing::error!(%e, "failed to receive article event in feeds state");
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
