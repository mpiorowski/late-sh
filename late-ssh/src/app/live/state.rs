//! What the live strip shows this session, decided on the tick so a change
//! of height is a change of frame. `refresh` is the pure rule; `tick` and
//! `view` are the glue that reads the sources.

use std::{cell::Cell, time::Duration, time::Instant};

use chrono::{DateTime, Utc};
use late_core::models::user::AudioSource;
use ratatui::layout::Rect;

use crate::app::{
    audio::{
        booth::live::{self as booth_live, TrackStripView},
        state::AudioState,
    },
    lobby::daily::{live::MatchStripView, state::DailyState},
};

use super::pick::{
    Featured, LIVE_AIM_WINDOW, LiveCandidate, LiveSource, pick_featured, strip_is_fresh,
};

/// A daily match that just ended holds the strip this long with its final
/// board and the result, the best advert the lobby has.
pub const LIVE_FINISH_LINGER: Duration = Duration::from_secs(60);

/// What the strip is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Showing {
    /// The featured source (`pick_featured`).
    Featured(LiveSource),
    /// The daily match that just ended (`DailyState::live_finish_view`).
    DailyFinish,
}

/// Which of the two the strip holds; the featured source itself is
/// `LiveState::featured`, and may change while the strip stays up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StripPick {
    Featured,
    DailyFinish,
}

/// What the strip paints, by source.
pub enum LiveStripView<'a> {
    Match(MatchStripView<'a>),
    Track(TrackStripView),
}

impl LiveStripView<'_> {
    /// What the key or a click on the strip opens. `None` for a finished
    /// match: its board has left the lobby.
    pub fn opens(&self) -> Option<LiveSource> {
        match self {
            Self::Match(strip) => match strip.finish {
                Some(_) => None,
                None => Some(LiveSource::DailyMatch(strip.view.item.id)),
            },
            Self::Track(track) => Some(LiveSource::BoothTrack(track.item.id)),
        }
    }
}

#[derive(Debug, Default)]
pub struct LiveState {
    featured: Option<Featured>,
    strip: Option<StripPick>,
    /// Whether the strip is showing something being acted on right now, so
    /// the frame that draws it rides the half-tick.
    aiming: bool,
    /// Where the strip drew this frame and what it showed, for the click
    /// that opens it. Render-recorded, cleared before every draw.
    pub hit: Cell<Option<(Rect, LiveSource)>>,
}

impl LiveState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Read the sources and decide what the strip shows. `reading` is
    /// whether the viewer has a message selected in the card: the strip then
    /// holds its height. True when what the strip draws changed.
    pub fn tick(&mut self, daily: &DailyState, audio: &AudioState, reading: bool) -> bool {
        let mut candidates = daily.live_candidates();
        candidates.extend(audio.live_candidates());
        self.refresh(
            &candidates,
            daily.live_finish_at(),
            Instant::now(),
            Utc::now(),
            reading,
        )
    }

    /// Re-pick the featured source, then decide what the strip shows: a
    /// fresh aim beats everything, a match that ended inside
    /// `LIVE_FINISH_LINGER` beats other news, news inside
    /// `LIVE_STRIP_LINGER` beats nothing. Going up or coming down waits
    /// while the viewer is reading, so the messages never shift under a
    /// selection. True when what the strip draws changed.
    pub fn refresh(
        &mut self,
        candidates: &[LiveCandidate],
        finished_at: Option<Instant>,
        now: Instant,
        now_utc: DateTime<Utc>,
        reading: bool,
    ) -> bool {
        let next = pick_featured(self.featured, candidates, now_utc, now);
        let featured_changed =
            next.map(|featured| featured.source) != self.featured.map(|featured| featured.source);
        self.featured = next;

        let live = self.featured.and_then(|featured| {
            candidates
                .iter()
                .find(|candidate| candidate.source == featured.source)
        });
        let aiming = live.is_some_and(|candidate| {
            candidate
                .aimed_at
                .is_some_and(|at| now.saturating_duration_since(at) < LIVE_AIM_WINDOW)
        });
        let live_fresh = live.is_some_and(|candidate| {
            strip_is_fresh(candidate.updated, candidate.aimed_at, now_utc, now)
        });
        let finish_fresh =
            finished_at.is_some_and(|at| now.saturating_duration_since(at) < LIVE_FINISH_LINGER);
        let want = if aiming {
            Some(StripPick::Featured)
        } else if finish_fresh {
            Some(StripPick::DailyFinish)
        } else if live_fresh {
            Some(StripPick::Featured)
        } else {
            None
        };
        let strip_changed = self.set_strip(want, live.is_some(), finished_at.is_some(), reading);
        self.aiming = aiming && self.strip == Some(StripPick::Featured);
        // A new featured source is only news while the strip is showing it.
        strip_changed || (featured_changed && self.strip == Some(StripPick::Featured))
    }

    fn set_strip(
        &mut self,
        want: Option<StripPick>,
        has_featured: bool,
        has_finish: bool,
        reading: bool,
    ) -> bool {
        if want == self.strip {
            return false;
        }
        let height_changes = want.is_none() || self.strip.is_none();
        let still_showable = match self.strip {
            Some(StripPick::Featured) => has_featured,
            Some(StripPick::DailyFinish) => has_finish,
            None => true,
        };
        if reading && height_changes && still_showable {
            return false;
        }
        self.strip = want;
        true
    }

    pub fn showing(&self) -> Option<Showing> {
        match self.strip? {
            StripPick::Featured => Some(Showing::Featured(self.featured?.source)),
            StripPick::DailyFinish => Some(Showing::DailyFinish),
        }
    }

    /// Whether the strip is drawing a cue right now.
    pub fn aiming(&self) -> bool {
        self.aiming
    }

    /// What the strip paints, if it is up. `listening_on` is the viewer's
    /// audio source, which decides what opening a booth track does.
    pub fn view<'a>(
        &self,
        daily: &'a DailyState,
        audio: &AudioState,
        listening_on: AudioSource,
    ) -> Option<LiveStripView<'a>> {
        match self.showing()? {
            Showing::Featured(LiveSource::DailyMatch(match_id)) => {
                daily.live_match_view(match_id).map(LiveStripView::Match)
            }
            Showing::Featured(LiveSource::BoothTrack(item_id)) => {
                booth_live::view(&audio.queue_snapshot(), item_id, listening_on)
                    .map(LiveStripView::Track)
            }
            Showing::DailyFinish => daily.live_finish_view().map(LiveStripView::Match),
        }
    }

    /// What the key opens: the source the strip is showing, if it opens
    /// anything.
    pub fn opens(&self) -> Option<LiveSource> {
        match self.showing()? {
            Showing::Featured(source) => Some(source),
            Showing::DailyFinish => None,
        }
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
