//! What the live strip shows this session, decided on the tick so a change
//! of height is a change of frame. `refresh` is the pure rule; `tick` and
//! `view` are the glue that reads the sources.

use std::{cell::Cell, time::Instant};

use chrono::{DateTime, Utc};
use late_core::models::{article::ArticleFeedItem, user::AudioSource};
use ratatui::{layout::Rect, text::Line};
use std::sync::Arc;
use uuid::Uuid;

use crate::app::{
    audio::{
        booth::live::{self as booth_live, TrackStripView},
        state::AudioState,
        thumbnail::Thumbnail,
    },
    chat::news::live::{self as news_live, ArticleStripView},
    files::inline_image::InlineImageRenderSettings,
    lobby::daily::{live::MatchStripView, state::DailyState},
};

use super::pick::{Featured, LiveCandidate, LiveSource, aim_overlay, pick_queued};

/// What the strip paints, by source.
pub enum LiveStripView<'a> {
    Match(MatchStripView<'a>),
    Track(TrackStripView),
    Article(ArticleStripView),
}

impl LiveStripView<'_> {
    /// What the key or a click on the strip opens. `None` for a result:
    /// its board has left the lobby.
    pub fn opens(&self) -> Option<LiveSource> {
        match self {
            Self::Match(strip) => match strip.finish {
                Some(_) => None,
                None => Some(LiveSource::DailyMatch(strip.view.item.id)),
            },
            Self::Track(track) => Some(LiveSource::BoothTrack(track.item.id)),
            Self::Article(article) => Some(LiveSource::NewsArticle(article.item.article.id)),
        }
    }
}

/// The featured booth track's thumbnail as this session's terminal paints
/// it, kept so the frame never renders it.
#[derive(Debug)]
struct TrackPicture {
    item_id: Uuid,
    thumbnail: Thumbnail,
    settings: InlineImageRenderSettings,
    /// `None` when the thumbnail would not render; the strip then keeps
    /// the drawn screen.
    lines: Option<Vec<Line<'static>>>,
}

#[derive(Debug, Default)]
pub struct LiveState {
    /// What the queue has up (`pick_queued`), kept for its minimum.
    queued: Option<Featured>,
    /// What the strip shows: the aim overlay, else what the queue has up.
    /// Going from nothing to something or back changes the card's height.
    shown: Option<LiveSource>,
    /// Whether the strip is showing something being acted on right now, so
    /// the frame that draws it rides the half-tick.
    aiming: bool,
    track_picture: Option<TrackPicture>,
    /// Where the strip drew this frame and what it showed, for the click
    /// that opens it. Render-recorded, cleared before every draw.
    pub hit: Cell<Option<(Rect, LiveSource)>>,
}

impl LiveState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Read the sources and decide what the strip shows. `articles` is the
    /// session's News snapshot. `reading` is
    /// whether the viewer has a message selected in the card: the strip then
    /// holds its height. `picture_settings` is how this session's terminal
    /// paints an image. True when what the strip draws changed.
    pub(crate) fn tick(
        &mut self,
        daily: &DailyState,
        audio: &AudioState,
        articles: &[ArticleFeedItem],
        reading: bool,
        picture_settings: InlineImageRenderSettings,
    ) -> bool {
        let mut candidates = daily.live_candidates();
        candidates.extend(audio.live_candidates());
        candidates.extend(news_live::candidates(articles));
        let changed = self.refresh(&candidates, Instant::now(), Utc::now(), reading);
        let thumbnail = match self.showing() {
            Some(LiveSource::BoothTrack(item_id)) => audio
                .queue_thumbnail(item_id)
                .map(|thumbnail| (item_id, thumbnail)),
            Some(LiveSource::DailyMatch(_))
            | Some(LiveSource::DailyResult(_))
            | Some(LiveSource::NewsArticle(_))
            | None => None,
        };
        let picture_changed = self.refresh_track_picture(thumbnail, picture_settings);
        changed || picture_changed
    }

    /// Render the featured track's thumbnail when it, the track, or the
    /// terminal's settings changed, and drop it when no track is up. True
    /// when the picture changed.
    fn refresh_track_picture(
        &mut self,
        thumbnail: Option<(Uuid, Thumbnail)>,
        settings: InlineImageRenderSettings,
    ) -> bool {
        let Some((item_id, thumbnail)) = thumbnail else {
            return self.track_picture.take().is_some();
        };
        if self.track_picture.as_ref().is_some_and(|picture| {
            picture.item_id == item_id
                && picture.settings == settings
                && Arc::ptr_eq(&picture.thumbnail, &thumbnail)
        }) {
            return false;
        }
        let lines = match booth_live::render_picture(&thumbnail, settings) {
            Ok(lines) => Some(lines),
            Err(error) => {
                tracing::warn!(error = ?error, %item_id, "failed to render booth thumbnail");
                None
            }
        };
        self.track_picture = Some(TrackPicture {
            item_id,
            thumbnail,
            settings,
            lines,
        });
        true
    }

    /// Re-run the queue (`pick_queued`) and lay the aim over it
    /// (`aim_overlay`). Going up or coming down waits while the viewer is
    /// reading, so the messages never shift under a selection, unless what
    /// the strip was showing is gone. True when what the strip draws
    /// changed.
    pub fn refresh(
        &mut self,
        candidates: &[LiveCandidate],
        now: Instant,
        now_utc: DateTime<Utc>,
        reading: bool,
    ) -> bool {
        self.queued = pick_queued(self.queued, candidates, now_utc);
        let overlay = aim_overlay(self.queued, candidates, now);
        let next = match overlay {
            Some(source) => Some(source),
            None => self.queued.map(|queued| queued.source),
        };
        let changed = self.show(next, candidates, reading);
        self.aiming = overlay.is_some() && self.shown == overlay;
        changed
    }

    fn show(
        &mut self,
        next: Option<LiveSource>,
        candidates: &[LiveCandidate],
        reading: bool,
    ) -> bool {
        if next == self.shown {
            return false;
        }
        let height_changes = next.is_none() != self.shown.is_none();
        let still_showable = match self.shown {
            Some(shown) => candidates.iter().any(|candidate| candidate.source == shown),
            None => true,
        };
        if reading && height_changes && still_showable {
            return false;
        }
        self.shown = next;
        true
    }

    /// The source the strip is showing, if it is up.
    pub fn showing(&self) -> Option<LiveSource> {
        self.shown
    }

    /// Whether the strip is drawing a cue right now.
    pub fn aiming(&self) -> bool {
        self.aiming
    }

    /// What the strip paints, if it is up. `listening_on` is the viewer's
    /// audio source, which decides what opening a booth track does;
    /// `articles` is the session's News snapshot.
    pub fn view<'a>(
        &self,
        daily: &'a DailyState,
        audio: &AudioState,
        listening_on: AudioSource,
        articles: &[ArticleFeedItem],
    ) -> Option<LiveStripView<'a>> {
        match self.showing()? {
            LiveSource::DailyMatch(match_id) => {
                daily.live_match_view(match_id).map(LiveStripView::Match)
            }
            LiveSource::DailyResult(match_id) => {
                daily.live_result_view(match_id).map(LiveStripView::Match)
            }
            LiveSource::BoothTrack(item_id) => {
                let picture = self
                    .track_picture
                    .as_ref()
                    .filter(|picture| picture.item_id == item_id)
                    .and_then(|picture| picture.lines.clone());
                booth_live::view(&audio.queue_snapshot(), item_id, listening_on, picture)
                    .map(LiveStripView::Track)
            }
            LiveSource::NewsArticle(article_id) => {
                news_live::view(articles, article_id).map(LiveStripView::Article)
            }
        }
    }

    /// What the key opens: the source the strip is showing, if it opens
    /// anything. A result opens nothing: its board has left the lobby.
    pub fn opens(&self) -> Option<LiveSource> {
        match self.showing()? {
            LiveSource::DailyResult(_) => None,
            source @ (LiveSource::DailyMatch(_)
            | LiveSource::BoothTrack(_)
            | LiveSource::NewsArticle(_)) => Some(source),
        }
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
