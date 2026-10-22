use std::collections::{HashMap, HashSet};
use std::time::Instant;

use ratatui_textarea::{TextArea, WrapMode};
use tokio::sync::{broadcast, oneshot, watch};
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::app::common::{composer, primitives::Banner};
use late_core::models::article::{
    ArticleEvent, ArticleFeedItem, ArticleSnapshot, NEWS_FEED_LIMIT, NEWS_SHARE_MAX_PAID_PER_DAY,
    NEWS_SHARE_REWARD_CHIPS, NewsShareReward,
};
use late_core::models::article_body::BodyStatus;
use late_core::models::article_read::WatchPosition;
use late_core::models::article_save::NEWS_SAVED_LIMIT;

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

use super::reader::state::{ReaderBody, ReaderModal};
use super::svc::{ArticleService, BodyLoad, BodyReply, NewsSaveOutcome, ShelfStep, is_youtube_url};
use super::watch::state::{LivePlay, Player, SetupBox, SetupInputs, WatchMemory, WatchModal};
use crate::paired_clients::PairControlMessage;

/// This session's news reads: the ids of the snapshot's articles the
/// reader has read (`article_reads`), a visit to the News room reading
/// every one of them and opening the reader reading one. Reads only grow,
/// so a load merges into what is held. Until the first load answers, the
/// badge stays empty rather than guessing: no reads means everything is
/// unread, which is not the same as not knowing yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Reads {
    Loading,
    Loaded(HashSet<Uuid>),
}

pub(crate) fn is_unread(item: &ArticleFeedItem, read_ids: &HashSet<Uuid>) -> bool {
    !read_ids.contains(&item.article.id)
}

/// Whether an article is unread against the reads, for a mark that must
/// not guess: nothing is unread until the reads have loaded.
pub(crate) fn is_unread_at(item: &ArticleFeedItem, reads: &Reads) -> bool {
    match reads {
        Reads::Loading => false,
        Reads::Loaded(read_ids) => is_unread(item, read_ids),
    }
}

/// The news badge: articles in the shared snapshot the reader has not read.
/// The snapshot holds the newest [`NEWS_FEED_LIMIT`] articles, so the
/// count saturates there.
pub(crate) fn unread_in_snapshot(articles: &[ArticleFeedItem], read_ids: &HashSet<Uuid>) -> i64 {
    let unread = articles
        .iter()
        .filter(|item| is_unread(item, read_ids))
        .count();
    unread as i64
}

/// Badge text for a news unread count: a full snapshot of unread articles
/// reads as a floor ("40+"), since older unread ones sit past the snapshot.
pub(crate) fn news_unread_label(unread: i64) -> String {
    if unread >= NEWS_FEED_LIMIT {
        format!("{NEWS_FEED_LIMIT}+")
    } else {
        unread.to_string()
    }
}

/// Whether a refreshed snapshot brought this reader something to announce:
/// an article newer than anything the previous snapshot held, still
/// unread, shared by someone else. Newer by `created`, not by id: the
/// snapshot is capped, so a delete backfills an older article whose id was
/// never seen, and that is not news. The first snapshot a session sees is
/// its starting state, not news, so it never announces.
pub(crate) fn has_fresh_unread_from_others(
    previous: &[ArticleFeedItem],
    next: &[ArticleFeedItem],
    read_ids: &HashSet<Uuid>,
    reader: Uuid,
) -> bool {
    let Some(newest_seen) = previous.iter().map(|item| item.article.created).max() else {
        return false;
    };
    next.iter().any(|item| {
        item.article.created > newest_seen
            && item.article.user_id != reader
            && is_unread(item, read_ids)
    })
}

/// Outcome of one tab tick: the banner to surface plus whether a drained
/// snapshot or event may have changed the rendered tab (badge counts, article list).
pub struct NewsTick {
    pub banner: Option<Banner>,
    pub changed: bool,
}

/// The banner for an `s`, from what it did to the shelf. Both `s` surfaces
/// (the News room and the reader) and a save the service refused read this.
pub(crate) fn save_banner(outcome: NewsSaveOutcome) -> Banner {
    match outcome {
        NewsSaveOutcome::Saved => Banner::success("Saved for later."),
        NewsSaveOutcome::Unsaved => Banner::success("Taken off saved."),
        NewsSaveOutcome::Full => Banner::error(&format!(
            "Saved is full: {NEWS_SAVED_LIMIT} stories. Press s on one of them to make room."
        )),
    }
}

/// The News list in display order: the reader's shelf first, newest save
/// first, then the feed's stories that are not on it, newest first. A
/// saved story appears once, on the shelf, whatever the feed holds; the
/// feed's copy of it is the one shown, since the snapshot carries the
/// freshest body status. The mine-only filter applies to both parts.
pub(crate) fn display_order(
    saved: &[ArticleFeedItem],
    feed: &[ArticleFeedItem],
    mine_only: Option<Uuid>,
) -> (Vec<ArticleFeedItem>, usize) {
    let shown = |item: &ArticleFeedItem| match mine_only {
        Some(user_id) => item.article.user_id == user_id,
        None => true,
    };
    let saved_ids: HashSet<Uuid> = saved.iter().map(|item| item.article.id).collect();
    let mut rows: Vec<ArticleFeedItem> = saved
        .iter()
        .map(|item| {
            feed.iter()
                .find(|fresh| fresh.article.id == item.article.id)
                .unwrap_or(item)
        })
        .filter(|item| shown(item))
        .cloned()
        .collect();
    let saved_len = rows.len();
    rows.extend(
        feed.iter()
            .filter(|item| !saved_ids.contains(&item.article.id) && shown(item))
            .cloned(),
    );
    (rows, saved_len)
}

pub struct State {
    article_service: ArticleService,
    user_id: Uuid,
    is_admin: bool,
    source_articles: Vec<ArticleFeedItem>,
    /// The reader's shelf (`article_saves`), newest save first, at most
    /// `NEWS_SAVED_LIMIT`. Replaced by every load; a save or an unsave
    /// moves it at once, ahead of the write, so the story changes section
    /// under the key.
    saved_articles: Vec<ArticleFeedItem>,
    /// The last shelf step this session asked for (`ArticleService::shelf_task`).
    /// The next one waits on it, so the session's saves, unsaves and loads
    /// land in the order they were asked for.
    shelf_step: Option<JoinHandle<()>>,
    articles: Vec<ArticleFeedItem>,
    /// How many leading rows of `articles` are the shelf.
    saved_len: usize,
    mine_only: bool,
    selected: usize,
    snapshot_rx: watch::Receiver<ArticleSnapshot>,
    event_rx: broadcast::Receiver<ArticleEvent>,
    reads: Reads,
    /// Where this reader left each video they are partway through, by
    /// article: the resume point for the player and the card's progress.
    /// Replaced by every load, kept in step by the session's own saves.
    watch_positions: HashMap<Uuid, WatchPosition>,
    marker: HashSet<Uuid>,
    preserve_marker: bool,
    composing: bool,
    composer: TextArea<'static>,
    processing: bool,
    current_task: Option<tokio::task::AbortHandle>,
    reader: Option<OpenReader>,
    /// Never up at the same time as the reader.
    watch: Option<WatchModal>,
    /// What the paired CLI was last told to play, kept past the modal so
    /// closing it still sends the stop (`watch::state::sync`).
    live_play: Option<LivePlay>,
    /// The last `play_id` handed out this session.
    last_play_id: u64,
}

/// The reader on screen, the story it is on, and its outstanding body
/// request. They travel together: closing or reopening the reader drops
/// the request with it, so an answer only ever lands on the reader that
/// asked.
struct OpenReader {
    modal: ReaderModal,
    /// Kept so the reader's keys still find the story once it is in
    /// neither the snapshot nor the shelf (`find_article`).
    item: ArticleFeedItem,
    body_request: Option<oneshot::Receiver<BodyReply>>,
}

impl State {
    pub fn new(article_service: ArticleService, user_id: Uuid, is_admin: bool) -> Self {
        let snapshot_rx = article_service.subscribe_snapshot();
        let event_rx = article_service.subscribe_events();
        article_service.list_articles_task();
        article_service.load_reads_task(user_id);
        let first_shelf_load = article_service.shelf_task(user_id, ShelfStep::Load, None);
        Self {
            article_service,
            user_id,
            is_admin,
            source_articles: Vec::new(),
            saved_articles: Vec::new(),
            shelf_step: Some(first_shelf_load),
            articles: Vec::new(),
            saved_len: 0,
            mine_only: false,
            selected: 0,
            snapshot_rx,
            event_rx,
            reads: Reads::Loading,
            watch_positions: HashMap::new(),
            marker: HashSet::new(),
            preserve_marker: false,
            composing: false,
            composer: new_news_textarea(),
            processing: false,
            current_task: None,
            reader: None,
            watch: None,
            live_play: None,
            last_play_id: 0,
        }
    }

    /// The shared snapshot, ignoring the shelf and any mine-only filter:
    /// what the live strip, the Zen headlines and the paper read. The shelf
    /// is the reader's own and never rides these.
    pub fn all_articles(&self) -> &[ArticleFeedItem] {
        &self.source_articles
    }

    /// Articles in current display order (`display_order`): the shelf, then
    /// the feed, with the mine-only filter applied when active. This is
    /// what the chat news view renders and what the j/k/s/d/Enter
    /// selection operates on.
    pub fn displayed_articles(&self) -> &[ArticleFeedItem] {
        &self.articles
    }

    /// How many leading rows of `displayed_articles` are the shelf.
    pub fn saved_len(&self) -> usize {
        self.saved_len
    }

    /// How many stories the shelf holds, filter or no filter.
    pub fn saved_count(&self) -> usize {
        self.saved_articles.len()
    }

    fn is_saved(&self, article_id: Uuid) -> bool {
        self.saved_articles
            .iter()
            .any(|item| item.article.id == article_id)
    }

    /// A story this session knows: in the snapshot, on the shelf once it
    /// has left the snapshot, or open in the reader once it has left both
    /// (unsaved there). Every action on a story by id (the reader, the
    /// watch, a reply, a delete) resolves it here.
    pub(crate) fn find_article(&self, article_id: Uuid) -> Option<&ArticleFeedItem> {
        self.source_articles
            .iter()
            .chain(self.saved_articles.iter())
            .chain(self.reader.iter().map(|open| &open.item))
            .find(|item| item.article.id == article_id)
    }

    pub fn set_is_admin(&mut self, is_admin: bool) {
        self.is_admin = is_admin;
    }

    /// A visit to the News room: the feed and the shelf are re-read, the
    /// shelf being replica-ready by refreshing when its page opens (a
    /// delete by an admin on another replica shows up here).
    pub fn list_articles(&mut self) {
        self.article_service.list_articles_task();
        self.queue_shelf_step(ShelfStep::Load);
    }

    /// Run a step on the shelf after the session's last one.
    fn queue_shelf_step(&mut self, step: ShelfStep) {
        let after = self.shelf_step.take();
        self.shelf_step = Some(self.article_service.shelf_task(self.user_id, step, after));
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

        let mine_only = self.mine_only.then_some(self.user_id);
        let (next, saved_len) =
            display_order(&self.saved_articles, &self.source_articles, mine_only);

        self.articles = next;
        self.saved_len = saved_len;
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
        if self.mine_only && self.find_article(article_id).is_some() {
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
        match &self.reads {
            Reads::Loading => 0,
            Reads::Loaded(read_ids) => unread_in_snapshot(&self.source_articles, read_ids),
        }
    }

    /// The reads the News list marks its dots against: held at what they
    /// were when the room was entered, so the visit keeps showing what was new.
    pub(crate) fn marker(&self) -> &HashSet<Uuid> {
        &self.marker
    }

    pub(crate) fn reads(&self) -> &Reads {
        &self.reads
    }

    pub(crate) fn watch_positions(&self) -> &HashMap<Uuid, WatchPosition> {
        &self.watch_positions
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

    /// A visit to the News room: every article in the snapshot is read.
    /// The badge clears now; the stored reads come back as `ReadsLoaded`
    /// once the write lands and merge into these.
    pub fn mark_read(&mut self) {
        self.marker = match &self.reads {
            Reads::Loading => HashSet::new(),
            Reads::Loaded(read_ids) => read_ids.clone(),
        };
        self.preserve_marker = true;
        let snapshot_ids = self.source_articles.iter().map(|item| item.article.id);
        match &mut self.reads {
            Reads::Loading => self.reads = Reads::Loaded(snapshot_ids.collect()),
            Reads::Loaded(read_ids) => read_ids.extend(snapshot_ids),
        }
        self.article_service.mark_read_task(self.user_id);
    }

    /// Mark the one article the reader opened on as read, and no other. Its
    /// mark clears now, and the write always goes: it is a no-op on a row
    /// that exists, and a reopen is the retry of a write that failed.
    pub fn mark_article_read(&mut self, article_id: Uuid) {
        if let Reads::Loaded(read_ids) = &mut self.reads {
            read_ids.insert(article_id);
        }
        self.article_service
            .mark_article_read_task(self.user_id, article_id);
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
        if let Some(article_id) = self.selected_item().map(|item| item.article.id) {
            self.delete_article(article_id);
        }
    }

    /// `s` on the selected story: onto the shelf, or off it. The cursor
    /// stays where it is in the list rather than following the story to
    /// its new section: the story that came next is what is under it now,
    /// so saving reads as "keep this, move on". With nothing after it, the
    /// cursor follows the story; a refused save moves nothing.
    pub(crate) fn toggle_save_selected(&mut self) -> Option<NewsSaveOutcome> {
        let index = self.selected_index();
        let article_id = self.articles.get(index)?.article.id;
        let follower = self.articles.get(index + 1).map(|item| item.article.id);
        let outcome = self.toggle_save(article_id)?;
        match outcome {
            NewsSaveOutcome::Saved | NewsSaveOutcome::Unsaved => {
                if let Some(follower) = follower {
                    self.select_article_by_id(follower);
                }
            }
            NewsSaveOutcome::Full => {}
        }
        Some(outcome)
    }

    /// Put a story on the shelf, or take it off: the session's own list
    /// moves now, so the story changes section under the key, and the
    /// write goes out as a shelf step whose load brings every session of
    /// the reader's in line. A full shelf refuses here, before any write;
    /// one another session filled meanwhile refuses in the service and
    /// comes back as `SaveRefused`. None when the story is not known.
    pub(crate) fn toggle_save(&mut self, article_id: Uuid) -> Option<NewsSaveOutcome> {
        let item = self.find_article(article_id)?.clone();
        let outcome = if self.is_saved(article_id) {
            self.saved_articles
                .retain(|saved| saved.article.id != article_id);
            self.queue_shelf_step(ShelfStep::Unsave(article_id));
            NewsSaveOutcome::Unsaved
        } else if self.saved_articles.len() >= NEWS_SAVED_LIMIT as usize {
            self.article_service.record_refused_save();
            return Some(NewsSaveOutcome::Full);
        } else {
            self.saved_articles.insert(0, item);
            self.queue_shelf_step(ShelfStep::Save(article_id));
            NewsSaveOutcome::Saved
        };
        self.rebuild_display();
        Some(outcome)
    }

    /// Delete a story by id: its sharer's own, or any for an admin.
    pub fn delete_article(&mut self, article_id: Uuid) {
        let Some(item) = self.find_article(article_id) else {
            return;
        };
        if item.article.user_id != self.user_id && !self.is_admin {
            return;
        }
        self.article_service
            .delete_article(self.user_id, article_id, self.is_admin);
    }

    /// Open the reader on a known story (the snapshot's or the shelf's),
    /// marking it read and asking for its body. False when the story is
    /// gone (deleted).
    pub(crate) fn open_reader(&mut self, article_id: Uuid) -> bool {
        let Some(item) = self.find_article(article_id).cloned() else {
            return false;
        };
        let modal = ReaderModal::open(&item);
        self.close_watch();
        self.mark_article_read(article_id);
        let body_request =
            self.article_service
                .load_body_task(self.user_id, article_id, BodyLoad::Open);
        self.reader = Some(OpenReader {
            modal,
            item,
            body_request: Some(body_request),
        });
        true
    }

    pub(crate) fn reader(&self) -> Option<&ReaderModal> {
        self.reader.as_ref().map(|open| &open.modal)
    }

    pub(crate) fn close_reader(&mut self) -> Option<ReaderModal> {
        self.reader.take().map(|open| open.modal)
    }

    /// Whether the selected card is a video: the one case `w` in the News
    /// room is "watch" and not the global Bonsai shortcut.
    pub(crate) fn selected_card_is_watchable(&self) -> bool {
        self.selected_item()
            .is_some_and(|item| is_youtube_url(&item.article.url))
    }

    /// Whether a snapshot story is a video the watch modal can play.
    pub(crate) fn is_watchable(&self, article_id: Uuid) -> bool {
        self.watchable_item(article_id).is_some()
    }

    /// Open the player on a snapshot video, a fresh `play_id` for it, and
    /// mark it read. A video the reader left partway through resumes
    /// there. False when it left the snapshot or is not a video.
    pub(crate) fn open_watch_player(&mut self, article_id: Uuid, now: Instant) -> bool {
        let Some(item) = self.watchable_item(article_id).cloned() else {
            return false;
        };
        self.close_watch();
        self.last_play_id += 1;
        self.reader = None;
        let resume_from = self.watch_positions.get(&article_id).copied();
        self.watch = Some(WatchModal::Player(Box::new(Player::new(
            &item,
            self.last_play_id,
            now,
            resume_from,
        ))));
        self.mark_article_read(article_id);
        true
    }

    /// Open the setup box for a snapshot video the user's setup cannot
    /// play. False when it left the snapshot or is not a video.
    pub(crate) fn open_watch_setup(&mut self, article_id: Uuid, inputs: &SetupInputs) -> bool {
        let Some(item) = self.watchable_item(article_id) else {
            return false;
        };
        let setup = SetupBox::decide(&item.article.url, inputs);
        self.close_watch();
        self.reader = None;
        self.watch = Some(WatchModal::Setup(setup));
        true
    }

    fn watchable_item(&self, article_id: Uuid) -> Option<&ArticleFeedItem> {
        self.find_article(article_id)
            .filter(|item| is_youtube_url(&item.article.url))
    }

    pub(crate) fn watch(&self) -> Option<&WatchModal> {
        self.watch.as_ref()
    }

    pub(crate) fn player_mut(&mut self) -> Option<&mut Player> {
        match &mut self.watch {
            Some(WatchModal::Player(player)) => Some(player),
            Some(WatchModal::Setup(_)) | None => None,
        }
    }

    /// Close the watch modal. A closing player leaves its position behind
    /// for the next watch (`Player::memory`): the session's own map changes
    /// now, so the card shows the progress at once, and the write goes out
    /// as a task whose load brings every session of the reader's in line.
    pub(crate) fn close_watch(&mut self) {
        let Some(WatchModal::Player(player)) = self.watch.take() else {
            return;
        };
        let article_id = player.article_id();
        match player.memory() {
            WatchMemory::Save(position) => {
                self.watch_positions.insert(article_id, position);
                self.article_service
                    .save_watch_position_task(self.user_id, article_id, position);
            }
            WatchMemory::Clear => {
                self.watch_positions.remove(&article_id);
                self.article_service
                    .clear_watch_position_task(self.user_id, article_id);
            }
            WatchMemory::Nothing => {}
        }
    }

    /// After a `video_state`: save the playing position every
    /// `WATCH_SAVE_EVERY` (`Player::checkpoint`), so a session that drops
    /// mid-video still resumes close to where it was.
    pub(crate) fn checkpoint_watch(&mut self, now: Instant) {
        let Some(player) = self.player_mut() else {
            return;
        };
        let article_id = player.article_id();
        let Some(position) = player.checkpoint(now) else {
            return;
        };
        self.watch_positions.insert(article_id, position);
        self.article_service
            .save_watch_position_task(self.user_id, article_id, position);
    }

    /// The pair messages that bring the CLI in line with the open player:
    /// a play, a move, a stop. Called after every draw, which is what
    /// records the player's body.
    pub(crate) fn video_sync(&mut self) -> Vec<PairControlMessage> {
        let wanted = match &self.watch {
            Some(WatchModal::Player(player)) => player.wanted(),
            Some(WatchModal::Setup(_)) | None => None,
        };
        super::watch::state::sync(&mut self.live_play, wanted)
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
        let body = self.drain_reader_body();
        NewsTick {
            banner: event_banner.or(body.banner).or(snapshot_banner),
            changed: changed || body.changed,
        }
    }

    fn drain_snapshot(&mut self) -> Option<Banner> {
        let Ok(true) = self.snapshot_rx.has_changed() else {
            return None;
        };
        let snapshot = self.snapshot_rx.borrow_and_update().clone();
        let announce = match &self.reads {
            Reads::Loading => false,
            Reads::Loaded(read_ids) => has_fresh_unread_from_others(
                &self.source_articles,
                &snapshot.articles,
                read_ids,
                self.user_id,
            ),
        };
        self.source_articles = snapshot.articles;
        self.rebuild_display();
        self.reload_pending_reader();
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

    /// A reader left waiting on a fetch asks again once the snapshot says
    /// the body landed, so it turns into the article without a reopen. One
    /// request at a time: a snapshot landing while one is out asks nothing.
    fn reload_pending_reader(&mut self) {
        let Some(open) = &mut self.reader else {
            return;
        };
        if open.body_request.is_some() || *open.modal.body() != ReaderBody::Pending {
            return;
        }
        let article_id = open.modal.article_id();
        let landed = self
            .source_articles
            .iter()
            .any(|item| item.article.id == article_id && item.body != BodyStatus::Pending);
        if landed {
            open.body_request = Some(self.article_service.load_body_task(
                self.user_id,
                article_id,
                BodyLoad::Landed,
            ));
        }
    }

    /// The open reader's body answer, once it lands: the reader shows the
    /// text, or falls back to the summary with a banner.
    fn drain_reader_body(&mut self) -> NewsTick {
        let quiet = NewsTick {
            banner: None,
            changed: false,
        };
        let Some(open) = &mut self.reader else {
            return quiet;
        };
        let Some(request) = &mut open.body_request else {
            return quiet;
        };
        let reply = match request.try_recv() {
            Ok(reply) => reply,
            Err(oneshot::error::TryRecvError::Empty) => return quiet,
            // The load task ended without answering (it panicked): the
            // summary beats a loading line that never resolves.
            Err(oneshot::error::TryRecvError::Closed) => BodyReply::Failed,
        };
        open.body_request = None;
        let tick = match reply {
            BodyReply::Loaded(body) => {
                open.modal.apply_body(body);
                NewsTick {
                    banner: None,
                    changed: true,
                }
            }
            BodyReply::Failed => {
                open.modal.fail_load();
                NewsTick {
                    banner: Some(Banner::error("Couldn't load the article text.")),
                    changed: true,
                }
            }
        };
        // A "still pending" answer can cross the snapshot that said the body
        // landed, and that snapshot will not come again: ask once more now.
        self.reload_pending_reader();
        tick
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
                        // The delete cascaded through the shelf; re-read it.
                        self.queue_shelf_step(ShelfStep::Load);
                        banner = Some(Banner::success("Article deleted."));
                    }
                    ArticleEvent::SavesLoaded { user_id, saved } if self.user_id == user_id => {
                        self.saved_articles = saved;
                        self.rebuild_display();
                    }
                    ArticleEvent::SaveRefused { user_id } if self.user_id == user_id => {
                        banner = Some(save_banner(NewsSaveOutcome::Full));
                    }
                    ArticleEvent::ReadsLoaded { user_id, reads } if self.user_id == user_id => {
                        // Reads only grow, so a load merges: two writes'
                        // loads landing in either order never take a read
                        // back, and neither does a load older than a mark
                        // this session made itself. The video positions
                        // move, so the load replaces them.
                        self.watch_positions = reads
                            .iter()
                            .filter_map(|row| {
                                row.watch_position
                                    .map(|position| (row.article_id, position))
                            })
                            .collect();
                        let read_article_ids = reads.into_iter().map(|row| row.article_id);
                        match &mut self.reads {
                            Reads::Loading => {
                                self.reads = Reads::Loaded(read_article_ids.collect());
                            }
                            Reads::Loaded(read_ids) => read_ids.extend(read_article_ids),
                        }
                        if self.unread_count() == 0
                            && !self.preserve_marker
                            && let Reads::Loaded(read_ids) = &self.reads
                        {
                            self.marker = read_ids.clone();
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
