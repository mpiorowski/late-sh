use late_core::{
    audio::VizFrame,
    models::{
        media_favorite::BOOTH_FAVORITES_LIMIT,
        user::{AudioSource, RadioStation},
    },
};
use std::time::Instant;
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::{
    svc::{
        AudioEvent, AudioService, FavoriteItemView, FavoritesStep, HistoryItemView, QueueSnapshot,
        RequeueSource,
    },
    thumbnail::Thumbnail,
    viz::{LiveBands, Spectrum},
};
use crate::app::{common::primitives::Banner, live::pick::LiveCandidate};

pub struct AudioTick {
    pub banner: Option<Banner>,
    /// True when the shared queue snapshot or the listener's favorites moved
    /// this tick: the booth modal and the sidebar music stage draw straight
    /// from them.
    pub changed: bool,
}

/// What `f` on a History row did to the session's own favorites, before
/// any write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FavoriteMove {
    /// The track joined the favorites; the write is queued.
    Moved,
    /// The favorites already hold `BOOTH_FAVORITES_LIMIT`: nothing moved,
    /// nothing written.
    Full,
}

pub struct AudioState {
    pub(crate) service: AudioService,
    user_id: Uuid,
    event_rx: broadcast::Receiver<AudioEvent>,
    snapshot_rx: watch::Receiver<QueueSnapshot>,
    /// The paired client's latest spectrum; `None` while no client is
    /// streaming one.
    spectrum: Option<Spectrum>,
    /// The listener's booth favorites, newest first, mirrored from
    /// `AudioEvent::BoothFavoritesLoaded` and moved by the session's own
    /// `f` ahead of the write. Empty until the booth first opens.
    favorites: Vec<FavoriteItemView>,
    /// The last favorites step this session asked for, which the next one
    /// waits on so they land in order.
    favorites_step: Option<JoinHandle<()>>,
}

impl AudioState {
    pub fn new(service: AudioService, user_id: Uuid) -> Self {
        let event_rx = service.subscribe_events();
        let snapshot_rx = service.subscribe_snapshot();
        Self {
            service,
            user_id,
            event_rx,
            snapshot_rx,
            spectrum: None,
            favorites: Vec::new(),
            favorites_step: None,
        }
    }

    /// Folds a paired client's `viz` frame into the eq's spectrum. Not a
    /// paint on its own: the eq repaints on the anim_half edge it already
    /// pays while visible, and picks up whatever landed since.
    pub fn apply_viz_frame(&mut self, frame: &VizFrame, now: Instant) {
        self.spectrum = Some(Spectrum::next(self.spectrum, frame, now));
    }

    /// Drops a spectrum the client stopped refreshing, so the eq falls back
    /// to the ambient band.
    pub fn expire_spectrum(&mut self, now: Instant) {
        match self.spectrum {
            Some(spectrum) if spectrum.is_stale(now) => self.spectrum = None,
            Some(_) | None => {}
        }
    }

    pub(crate) fn live_bands(&self) -> Option<LiveBands> {
        self.spectrum.as_ref().map(Spectrum::bands)
    }

    /// The booth's tracks as the live strip weighs them.
    pub fn live_candidates(&self) -> Vec<LiveCandidate> {
        super::booth::live::candidates(&self.snapshot_rx.borrow())
    }

    /// Whether a track is still in the booth, playing or queued.
    pub fn in_booth(&self, item_id: Uuid) -> bool {
        let snapshot = self.snapshot_rx.borrow();
        snapshot
            .current
            .iter()
            .chain(snapshot.queue.iter())
            .any(|item| item.id == item_id)
    }

    /// The thumbnail of one track in the booth, once it has been fetched.
    pub fn queue_thumbnail(&self, item_id: Uuid) -> Option<Thumbnail> {
        let snapshot = self.snapshot_rx.borrow();
        snapshot
            .current
            .iter()
            .chain(snapshot.queue.iter())
            .find(|item| item.id == item_id)
            .and_then(|item| item.thumbnail.clone())
    }

    pub fn queue_snapshot(&self) -> QueueSnapshot {
        self.snapshot_rx.borrow().clone()
    }

    pub fn youtube_source_count(&self) -> usize {
        self.service.youtube_source_count()
    }

    pub fn radio_source_count(&self) -> usize {
        self.service.radio_source_count()
    }

    pub fn user_id(&self) -> Uuid {
        self.user_id
    }

    pub fn service(&self) -> &AudioService {
        &self.service
    }

    pub fn submit_trusted(&self, url: String) {
        self.service.submit_trusted_url_task(self.user_id, url);
    }

    pub fn set_youtube_fallback(&self, url: String) {
        self.service
            .set_trusted_youtube_fallback_task(self.user_id, url);
    }

    pub fn skip_trusted(&self) {
        self.service.force_skip_task(self.user_id);
    }

    pub fn booth_submit_enabled(&self) -> bool {
        self.service.booth_submit_enabled()
    }

    pub fn booth_submit_public(&self, url: String) {
        self.service.booth_submit_public_task(self.user_id, url);
    }

    pub fn booth_vote(&self, item_id: Uuid, value: i16) {
        self.service.cast_vote_task(self.user_id, item_id, value);
    }

    pub fn booth_clear_vote(&self, item_id: Uuid) {
        self.service.clear_vote_task(self.user_id, item_id);
    }

    pub fn booth_skip_vote(&self) {
        self.service.cast_skip_vote_task(self.user_id);
    }

    pub fn booth_delete(&self, item_id: Uuid) {
        self.service.delete_queue_item_task(self.user_id, item_id);
    }

    pub fn booth_toggle_unskippable(&self, item_id: Uuid) {
        self.service.toggle_unskippable_task(self.user_id, item_id);
    }

    pub fn booth_requeue(&self, source: RequeueSource) {
        self.service.requeue_track_task(self.user_id, source);
    }

    pub fn booth_favorites(&self) -> &[FavoriteItemView] {
        &self.favorites
    }

    /// Re-read the listener's favorites, which `tick` folds into the mirror.
    pub fn booth_load_favorites(&mut self) {
        self.queue_favorites_step(FavoritesStep::Load);
    }

    /// Keep a History track as a favorite: the session's own list moves
    /// now, so the row changes section under the key, and the write goes
    /// out as a favorites step whose load brings every session of the
    /// listener's in line. The session names the favorite's id, so the row
    /// it shows now is the one the table keeps. A full list refuses here,
    /// before any write; one another session filled meanwhile refuses in
    /// the service and comes back as `BoothFavoritesFull`.
    pub fn booth_add_favorite(&mut self, played: &HistoryItemView) -> FavoriteMove {
        if self.favorites.len() as i64 >= BOOTH_FAVORITES_LIMIT {
            self.service.record_refused_favorite();
            return FavoriteMove::Full;
        }
        let favorite = FavoriteItemView {
            id: Uuid::now_v7(),
            video_id: played.video_id.clone(),
            title: played.title.clone(),
            channel: played.channel.clone(),
            duration_ms: played.duration_ms,
            is_stream: played.is_stream,
        };
        let step = FavoritesStep::Add {
            history_item_id: played.id,
            favorite_id: favorite.id,
        };
        self.favorites.insert(0, favorite);
        self.queue_favorites_step(step);
        FavoriteMove::Moved
    }

    /// Let a favorite go, the session's own list first, the way
    /// `booth_add_favorite` keeps one.
    pub fn booth_remove_favorite(&mut self, favorite_id: Uuid) {
        self.favorites.retain(|favorite| favorite.id != favorite_id);
        self.queue_favorites_step(FavoritesStep::Remove(favorite_id));
    }

    /// Queue one favorites step behind the session's previous one; it ends
    /// with `BoothFavoritesLoaded`, which `tick` folds into the mirror.
    fn queue_favorites_step(&mut self, step: FavoritesStep) {
        let after = self.favorites_step.take();
        self.favorites_step = Some(self.service.favorites_task(self.user_id, step, after));
    }

    pub fn booth_history_delete(&self, item_id: Uuid) {
        self.service.delete_history_item_task(self.user_id, item_id);
    }

    /// Spawn an audio-source persist task that surfaces failures as banners
    /// via `AudioEvent::AudioSourcePersistFailed`. Caller is expected to have
    /// already optimistically updated local UI state.
    pub fn persist_audio_source(&self, source: AudioSource) {
        self.service.persist_audio_source_task(self.user_id, source);
    }

    pub fn persist_radio_slot(&self, index: usize, slot: Option<RadioStation>) {
        self.service
            .persist_radio_slot_task(self.user_id, index, slot);
    }

    pub fn persist_radio_station(&self, station: RadioStation) {
        self.service
            .persist_radio_station_task(self.user_id, station);
    }

    pub fn tick(&mut self) -> AudioTick {
        // Mark the queue snapshot seen so the peek only fires once per
        // publish; renders keep reading it through `queue_snapshot()`.
        let mut changed = self.snapshot_rx.has_changed().unwrap_or(false);
        if changed {
            let _ = self.snapshot_rx.borrow_and_update();
        }
        let mut banner = None;
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AudioEvent::TrustedSubmitQueued {
                    user_id,
                    position,
                    reward_chips,
                } if user_id == self.user_id => {
                    banner = Some(Banner::success(&submitted_line(
                        "Queued audio",
                        position,
                        reward_chips,
                    )));
                }
                AudioEvent::TrustedSubmitFailed { user_id, message } if user_id == self.user_id => {
                    banner = Some(Banner::error(&message));
                }
                AudioEvent::YoutubeFallbackSet { user_id } if user_id == self.user_id => {
                    banner = Some(Banner::success("Set YouTube fallback"));
                }
                AudioEvent::YoutubeFallbackFailed { user_id, message }
                    if user_id == self.user_id =>
                {
                    banner = Some(Banner::error(&message));
                }
                AudioEvent::TrustedSkipFired { user_id } if user_id == self.user_id => {
                    banner = Some(Banner::success("Skipped audio"));
                }
                AudioEvent::TrustedSkipFailed { user_id, message } if user_id == self.user_id => {
                    banner = Some(Banner::error(&message));
                }
                AudioEvent::BoothSubmitQueued {
                    user_id,
                    position,
                    reward_chips,
                } if user_id == self.user_id => {
                    banner = Some(Banner::success(&submitted_line(
                        "Submitted",
                        position,
                        reward_chips,
                    )));
                }
                AudioEvent::BoothSubmitFailed { user_id, message } if user_id == self.user_id => {
                    banner = Some(Banner::error(&message));
                }
                AudioEvent::BoothVoteApplied { user_id, score, .. } if user_id == self.user_id => {
                    banner = Some(Banner::success(&format!("Vote registered (score {score})")));
                }
                AudioEvent::BoothVoteFailed { user_id, message } if user_id == self.user_id => {
                    banner = Some(Banner::error(&message));
                }
                AudioEvent::BoothSkipFired { user_id } if user_id == self.user_id => {
                    banner = Some(Banner::success("Skip threshold reached"));
                }
                AudioEvent::BoothItemDeleted { user_id } if user_id == self.user_id => {
                    banner = Some(Banner::success("Deleted track"));
                }
                AudioEvent::BoothItemDeleteFailed { user_id, message }
                    if user_id == self.user_id =>
                {
                    banner = Some(Banner::error(&message));
                }
                AudioEvent::BoothItemUnskippableToggled {
                    user_id,
                    unskippable,
                } if user_id == self.user_id => {
                    banner = Some(Banner::success(if unskippable {
                        "Locked - skip-vote disabled"
                    } else {
                        "Unlocked - skip-vote enabled"
                    }));
                }
                AudioEvent::BoothItemUnskippableFailed { user_id, message }
                    if user_id == self.user_id =>
                {
                    banner = Some(Banner::error(&message));
                }
                AudioEvent::BoothSkipProgress {
                    user_id,
                    votes,
                    threshold,
                } if user_id == self.user_id => {
                    banner = Some(Banner::success(&format!(
                        "Skip vote registered ({votes}/{threshold})"
                    )));
                }
                AudioEvent::BoothHistoryRequeued {
                    user_id,
                    source,
                    position,
                    reward_chips,
                } if user_id == self.user_id => {
                    let verb = match source {
                        RequeueSource::History(_) => "Queued from history",
                        RequeueSource::Favorite(_) => "Queued from favorites",
                    };
                    banner = Some(Banner::success(&submitted_line(
                        verb,
                        position,
                        reward_chips,
                    )));
                }
                AudioEvent::BoothHistoryRequeueFailed { user_id, message }
                    if user_id == self.user_id =>
                {
                    banner = Some(Banner::error(&message));
                }
                AudioEvent::BoothHistoryItemDeleted { user_id } if user_id == self.user_id => {
                    banner = Some(Banner::success("Deleted history track"));
                }
                AudioEvent::BoothHistoryItemDeleteFailed { user_id, message }
                    if user_id == self.user_id =>
                {
                    banner = Some(Banner::error(&message));
                }
                AudioEvent::BoothFavoritesLoaded { user_id, favorites }
                    if user_id == self.user_id =>
                {
                    self.favorites = favorites;
                    changed = true;
                }
                AudioEvent::BoothFavoriteAdded { user_id } if user_id == self.user_id => {
                    banner = Some(Banner::success("Added to favorites"));
                }
                AudioEvent::BoothFavoriteRemoved { user_id } if user_id == self.user_id => {
                    banner = Some(Banner::success("Removed from favorites"));
                }
                AudioEvent::BoothFavoritesFull { user_id } if user_id == self.user_id => {
                    banner = Some(favorites_full_banner());
                }
                AudioEvent::BoothFavoriteFailed { user_id, message } if user_id == self.user_id => {
                    banner = Some(Banner::error(&message));
                }
                AudioEvent::AudioSourcePersistFailed { user_id, message }
                    if user_id == self.user_id =>
                {
                    banner = Some(Banner::error(&message));
                }
                _ => {}
            }
        }
        AudioTick { banner, changed }
    }
}

/// The banner for an add past `BOOTH_FAVORITES_LIMIT`, whether the session
/// refused it or the table did.
pub(crate) fn favorites_full_banner() -> Banner {
    Banner::error(&format!(
        "Favorites full ({BOOTH_FAVORITES_LIMIT}) - unfavorite one first"
    ))
}

/// The banner a submission gets: where it landed in the queue, and what
/// bringing it paid. The chips half is dropped entirely when nothing was
/// minted, which only happens past the day's cap, because a "+0 chips" is
/// worse than silence.
fn submitted_line(verb: &str, position: i64, reward_chips: i64) -> String {
    let place = match position {
        0 => "up next".to_string(),
        n => format!("#{n} in line"),
    };
    match reward_chips {
        0 => format!("{verb} - {place}"),
        chips => format!("{verb} - {place} (+{chips} chips)"),
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
