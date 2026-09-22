//! The gallery's I/O: listings, hanging, applause, and the splash piece.
//!
//! Every DB call runs as a spawned task that reports back over the
//! session's channel (`GalleryResult`), the way the archive loader does, so
//! the tick and render paths never wait on Postgres. This module is the
//! orchestration layer for the gallery: it owns the logs and metrics for
//! every outcome. The decisions themselves live in
//! `late_core::models::artboard_piece` (the SQL rails) and `frame.rs` (the
//! local ones).
//!
//! The splash wall (one hung piece a day over the door, in hang order)
//! is process-wide: one `watch` holding today's piece, refreshed hourly.
//! The refresh is also the assignment: the first replica awake on a UTC
//! day stamps the queue's head with the day (`ArtboardPiece::splash_for_day`,
//! a row claim on a unique index), the rest read it back, so any number of
//! replicas may run it (root CONTEXT.md, multi-replica rule). Each login
//! claims its account's one view of the day's piece
//! (`claim_splash_piece`) and the door shows that piece, or the coffee cup
//! once the account has seen it.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use dartboard_core::Canvas;
use late_core::db::Db;
use late_core::models::app_flag::AppFlags;
use late_core::models::artboard_piece::{
    ApplauseOutcome, ArtboardPiece, HangOutcome, HangParams, ListingCounts, PieceListing,
    TakeDownOutcome,
};
use late_core::models::user::User;
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use crate::app::artboard::provenance::ArtboardProvenance;
use crate::metrics::{self, GalleryApplauseResult, GalleryHangResult, GalleryTakeDownResult};

use super::frame::{Credit, FramedPiece};

/// How often the splash wall is re-read. It changes once a day, at UTC
/// midnight, and a mod removal is the only thing that could change it in
/// between; a replica is at most an hour behind either.
const SPLASH_REFRESH_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// The day's piece over the door, and the UTC day it holds. Claimed once
/// at bootstrap; the session never re-reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct SplashPiece {
    pub shown_on: NaiveDate,
    pub piece: GalleryPiece,
}

impl SplashPiece {
    fn decode(shown_on: NaiveDate, piece: ArtboardPiece) -> Result<Self> {
        Ok(Self {
            shown_on,
            piece: GalleryPiece::decode(piece)?,
        })
    }
}

/// What one refresh found. `Off` is no database or the gallery's switch
/// off: nothing was read, so there is no queue to count. `Wall` is the
/// day's piece (`None` on an empty queue) and how many pieces still wait
/// for a day (hung before it, never shown), recorded as a gauge so the
/// backlog is measurable before anyone decides on a cap.
#[derive(Clone, Debug, PartialEq)]
pub enum SplashRefresh {
    Off,
    Wall {
        piece: Option<SplashPiece>,
        queued: i64,
    },
}

/// A piece as the page draws it: the row decoded into a canvas, with the
/// credits read off its provenance.
#[derive(Clone, Debug, PartialEq)]
pub struct GalleryPiece {
    pub id: Uuid,
    pub user_id: Uuid,
    pub username: String,
    pub title: String,
    pub width: usize,
    pub height: usize,
    pub canvas: Canvas,
    /// Everyone with a glyph in the piece, most glyphs first.
    pub credits: Vec<Credit>,
    pub applause: i64,
    pub applauded_by_viewer: bool,
    pub created: DateTime<Utc>,
    pub period_month: NaiveDate,
}

impl GalleryPiece {
    pub fn decode(piece: ArtboardPiece) -> Result<Self> {
        let canvas: Canvas = serde_json::from_value(piece.canvas)
            .with_context(|| format!("decoding canvas of artboard piece {}", piece.id))?;
        let provenance: ArtboardProvenance = serde_json::from_value(piece.provenance)
            .with_context(|| format!("decoding provenance of artboard piece {}", piece.id))?;
        let credits = provenance
            .glyph_counts_by_username()
            .into_iter()
            .map(|(username, glyphs)| Credit { username, glyphs })
            .collect();
        Ok(Self {
            id: piece.id,
            user_id: piece.user_id,
            username: piece.username,
            title: piece.title,
            width: piece.width.max(1) as usize,
            height: piece.height.max(1) as usize,
            canvas,
            credits,
            applause: piece.applause,
            applauded_by_viewer: piece.applauded_by_viewer,
            created: piece.created,
            period_month: piece.period_month,
        })
    }

    /// The caption under a piece wherever it hangs: title, hanger, applause.
    pub fn caption(&self) -> String {
        format!(
            "\"{}\" by @{} · {}",
            self.title,
            self.username,
            applause_label(self.applause)
        )
    }
}

pub fn applause_label(applause: i64) -> String {
    match applause {
        1 => "1 applause".to_string(),
        n => format!("{n} applause"),
    }
}

/// Why a hang did not land, in the words the notice uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HangRefusal {
    DailyCap,
    Duplicate,
    Disabled,
}

impl HangRefusal {
    pub fn notice(self) -> &'static str {
        match self {
            Self::DailyCap => "You have hung today's three pieces already. Tomorrow.",
            Self::Duplicate => {
                "Those exact cells hung in the gallery this month already, even if taken down since."
            }
            Self::Disabled => "The gallery is closed right now.",
        }
    }
}

/// What a spawned gallery task reports back.
#[derive(Debug)]
pub enum GalleryResult {
    Counts(ListingCounts),
    CountsFailed(String),
    /// `generation` is the section's request counter at the time this
    /// listing was asked for, so the state can tell a stale answer from
    /// the one it is waiting on.
    Listed {
        listing: PieceListing,
        generation: u64,
        pieces: Vec<GalleryPiece>,
    },
    ListFailed {
        listing: PieceListing,
        generation: u64,
        error: String,
    },
    Hung(Box<GalleryPiece>),
    HangRefused(HangRefusal),
    HangFailed(String),
    Applause {
        piece_id: Uuid,
        outcome: ApplauseOutcome,
    },
    ApplauseFailed {
        piece_id: Uuid,
        error: String,
    },
    TakeDown {
        piece_id: Uuid,
        outcome: TakeDownOutcome,
    },
    TakeDownFailed {
        piece_id: Uuid,
        error: String,
    },
}

#[derive(Clone)]
pub struct GalleryService {
    db: Option<Db>,
    flags_rx: watch::Receiver<Option<AppFlags>>,
    splash_tx: Arc<watch::Sender<Option<SplashPiece>>>,
    splash_rx: watch::Receiver<Option<SplashPiece>>,
}

impl GalleryService {
    pub fn new(db: Db, flags_rx: watch::Receiver<Option<AppFlags>>) -> Self {
        let (splash_tx, splash_rx) = watch::channel(None);
        Self {
            db: Some(db),
            flags_rx,
            splash_tx: Arc::new(splash_tx),
            splash_rx,
        }
    }

    /// No database and no switches: every listing is empty, nothing hangs.
    pub fn disabled() -> Self {
        let (_flags_tx, flags_rx) = watch::channel(None);
        let (splash_tx, splash_rx) = watch::channel(None);
        Self {
            db: None,
            flags_rx,
            splash_tx: Arc::new(splash_tx),
            splash_rx,
        }
    }

    /// The kill switch, as this replica last read it. Nothing loaded yet
    /// reads as off, like every `app_flags` switch.
    pub fn is_enabled(&self) -> bool {
        self.db.is_some()
            && self
                .flags_rx
                .borrow()
                .is_some_and(|flags| flags.artboard_gallery_enabled)
    }

    /// The day's piece as this replica last read it. Tests read it;
    /// sessions get their view through `claim_splash_piece`.
    pub fn splash_wall(&self) -> Option<SplashPiece> {
        self.splash_rx.borrow().clone()
    }

    /// The piece this login shows over the door, if the account has not
    /// seen the day's piece yet. One `UPDATE` per login on a fresh day,
    /// none after (the stamp's `WHERE` fails and nothing is written). The
    /// stamp is the piece's own day, not the clock's, so a watch that is
    /// still on yesterday's piece for an hour past midnight shows it only
    /// to accounts that missed it and never spends today's view on it. A
    /// failed claim is the cup: the door is not worth failing a login over.
    pub async fn claim_splash_piece(&self, user_id: Uuid) -> Option<SplashPiece> {
        let db = self.db.as_ref()?;
        if !self.is_enabled() {
            return None;
        }
        let splash = self.splash_wall()?;
        let claimed = async {
            let client = db.get().await?;
            User::claim_splash_shown(&client, user_id, splash.shown_on).await
        }
        .await;
        match claimed {
            Ok(true) => Some(splash),
            Ok(false) => None,
            Err(error) => {
                tracing::warn!(
                    error = ?error,
                    %user_id,
                    "artboard gallery splash claim failed"
                );
                None
            }
        }
    }

    /// Hourly re-read of the splash wall, assigning the day's piece when
    /// nobody has yet. Runs at start so the first login after a deploy
    /// already has it.
    pub fn start_splash_refresh_task(&self) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(SPLASH_REFRESH_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                match service.refresh_splash(Utc::now().date_naive()).await {
                    Ok(SplashRefresh::Off) => {
                        tracing::debug!("artboard gallery splash wall is off")
                    }
                    Ok(SplashRefresh::Wall {
                        piece: None,
                        queued,
                    }) => {
                        metrics::record_gallery_splash_queue_depth(queued);
                        tracing::debug!(queued, "artboard gallery has no splash piece today")
                    }
                    Ok(SplashRefresh::Wall {
                        piece: Some(piece),
                        queued,
                    }) => {
                        metrics::record_gallery_splash_queue_depth(queued);
                        tracing::debug!(
                            piece_id = %piece.piece.id,
                            shown_on = %piece.shown_on,
                            queued,
                            "artboard gallery splash wall refreshed"
                        )
                    }
                    Err(error) => tracing::warn!(
                        error = ?error,
                        "artboard gallery splash refresh failed"
                    ),
                }
            }
        })
    }

    /// Read (and, on the first pass of the day, assign) `day`'s piece
    /// into the watch. The kill switch covers this read too: while the
    /// gallery is off the splash goes back to the coffee cup on the next
    /// refresh, so a piece that has to come down fast is off the
    /// highest-traffic surface within the hour without waiting for
    /// `/mod artboard remove`, and no day is assigned while it is off.
    pub async fn refresh_splash(&self, day: NaiveDate) -> Result<SplashRefresh> {
        let Some(db) = self.db.as_ref() else {
            return Ok(SplashRefresh::Off);
        };
        if !self.is_enabled() {
            let _ = self.splash_tx.send(None);
            return Ok(SplashRefresh::Off);
        }
        let client = db.get().await?;
        let piece = match ArtboardPiece::splash_for_day(&client, day).await? {
            Some(piece) => Some(SplashPiece::decode(day, piece)?),
            None => None,
        };
        let queued = ArtboardPiece::splash_queue_depth(&client, day).await?;
        let _ = self.splash_tx.send(piece.clone());
        Ok(SplashRefresh::Wall { piece, queued })
    }

    /// The rail's numbers, one query. Without a database everything is
    /// zero.
    pub fn counts_task(&self, viewer_id: Uuid, tx: mpsc::UnboundedSender<GalleryResult>) {
        let Some(db) = self.db.clone() else {
            let _ = tx.send(GalleryResult::Counts(ListingCounts::default()));
            return;
        };
        tokio::spawn(async move {
            let result = async {
                let client = db.get().await?;
                ArtboardPiece::listing_counts(&client, viewer_id).await
            }
            .await;
            let msg = match result {
                Ok(counts) => GalleryResult::Counts(counts),
                Err(error) => {
                    tracing::warn!(
                        error = ?error,
                        %viewer_id,
                        "artboard gallery counts failed"
                    );
                    GalleryResult::CountsFailed(format!("{error:#}"))
                }
            };
            let _ = tx.send(msg);
        });
    }

    pub fn list_task(
        &self,
        viewer_id: Uuid,
        listing: PieceListing,
        generation: u64,
        tx: mpsc::UnboundedSender<GalleryResult>,
    ) {
        let Some(db) = self.db.clone() else {
            let _ = tx.send(GalleryResult::Listed {
                listing,
                generation,
                pieces: Vec::new(),
            });
            return;
        };
        tokio::spawn(async move {
            let result = async {
                let client = db.get().await?;
                let rows = ArtboardPiece::list(&client, viewer_id, listing).await?;
                rows.into_iter()
                    .map(GalleryPiece::decode)
                    .collect::<Result<Vec<_>>>()
            }
            .await;
            let msg = match result {
                Ok(pieces) => GalleryResult::Listed {
                    listing,
                    generation,
                    pieces,
                },
                Err(error) => {
                    tracing::warn!(
                        error = ?error,
                        ?listing,
                        %viewer_id,
                        "artboard gallery listing failed"
                    );
                    GalleryResult::ListFailed {
                        listing,
                        generation,
                        error: "The gallery could not be loaded.".to_string(),
                    }
                }
            };
            let _ = tx.send(msg);
        });
    }

    pub fn hang_task(
        &self,
        user_id: Uuid,
        title: String,
        framed: FramedPiece,
        tx: mpsc::UnboundedSender<GalleryResult>,
    ) {
        if !self.is_enabled() {
            let _ = tx.send(GalleryResult::HangRefused(HangRefusal::Disabled));
            return;
        }
        let Some(db) = self.db.clone() else {
            let _ = tx.send(GalleryResult::HangRefused(HangRefusal::Disabled));
            return;
        };
        tokio::spawn(async move {
            let result: Result<HangOutcome> = async {
                let params = HangParams {
                    user_id,
                    title,
                    width: framed.width as i32,
                    height: framed.height as i32,
                    canvas: serde_json::to_value(&framed.canvas)
                        .context("encoding the piece's canvas")?,
                    provenance: serde_json::to_value(&framed.provenance)
                        .context("encoding the piece's provenance")?,
                    glyph_count: framed.glyph_count as i32,
                    own_share_percent: framed.own_share_percent as i32,
                    content_hash: framed.content_hash,
                };
                let client = db.get().await?;
                ArtboardPiece::hang(&client, params).await
            }
            .await;
            let msg = match result {
                Ok(HangOutcome::Hung(piece)) => match GalleryPiece::decode(piece) {
                    Ok(piece) => {
                        metrics::record_gallery_hang(GalleryHangResult::Hung);
                        tracing::info!(
                            %user_id,
                            piece_id = %piece.id,
                            width = piece.width,
                            height = piece.height,
                            "artboard piece hung"
                        );
                        GalleryResult::Hung(Box::new(piece))
                    }
                    Err(error) => {
                        metrics::record_gallery_hang(GalleryHangResult::Failed);
                        late_core::error_span!(
                            "artboard_gallery_hang",
                            error = ?error,
                            %user_id,
                            "hung artboard piece could not be decoded"
                        );
                        GalleryResult::HangFailed("The piece could not be hung.".to_string())
                    }
                },
                Ok(HangOutcome::DailyCapReached) => {
                    metrics::record_gallery_hang(GalleryHangResult::DailyCap);
                    GalleryResult::HangRefused(HangRefusal::DailyCap)
                }
                Ok(HangOutcome::Duplicate) => {
                    metrics::record_gallery_hang(GalleryHangResult::Duplicate);
                    GalleryResult::HangRefused(HangRefusal::Duplicate)
                }
                Err(error) => {
                    metrics::record_gallery_hang(GalleryHangResult::Failed);
                    late_core::error_span!(
                        "artboard_gallery_hang",
                        error = ?error,
                        %user_id,
                        "artboard piece could not be hung"
                    );
                    GalleryResult::HangFailed("The piece could not be hung.".to_string())
                }
            };
            let _ = tx.send(msg);
        });
    }

    pub fn applaud_task(
        &self,
        piece_id: Uuid,
        user_id: Uuid,
        tx: mpsc::UnboundedSender<GalleryResult>,
    ) {
        let Some(db) = self.db.clone() else {
            return;
        };
        if !self.is_enabled() {
            return;
        }
        tokio::spawn(async move {
            let result = async {
                let client = db.get().await?;
                ArtboardPiece::toggle_applause(&client, piece_id, user_id).await
            }
            .await;
            let msg = match result {
                Ok(outcome) => {
                    metrics::record_gallery_applause(match outcome {
                        ApplauseOutcome::Applauded(_) => GalleryApplauseResult::Applauded,
                        ApplauseOutcome::Withdrawn(_) => GalleryApplauseResult::Withdrawn,
                        ApplauseOutcome::OwnPiece => GalleryApplauseResult::OwnPiece,
                        ApplauseOutcome::NotFound => GalleryApplauseResult::NotFound,
                        ApplauseOutcome::Closed => GalleryApplauseResult::Closed,
                    });
                    GalleryResult::Applause { piece_id, outcome }
                }
                Err(error) => {
                    metrics::record_gallery_applause(GalleryApplauseResult::Failed);
                    late_core::error_span!(
                        "artboard_gallery_applause",
                        error = ?error,
                        %user_id,
                        %piece_id,
                        "artboard applause could not be recorded"
                    );
                    GalleryResult::ApplauseFailed {
                        piece_id,
                        error: "Your applause did not land. Try again.".to_string(),
                    }
                }
            };
            let _ = tx.send(msg);
        });
    }

    /// The hanger takes their own piece down. Owner and month are checked
    /// in the row's own `UPDATE` (`ArtboardPiece::take_down`).
    pub fn take_down_task(
        &self,
        piece_id: Uuid,
        user_id: Uuid,
        tx: mpsc::UnboundedSender<GalleryResult>,
    ) {
        let Some(db) = self.db.clone() else {
            return;
        };
        if !self.is_enabled() {
            return;
        }
        tokio::spawn(async move {
            let result = async {
                let client = db.get().await?;
                ArtboardPiece::take_down(&client, piece_id, user_id).await
            }
            .await;
            let msg = match result {
                Ok(outcome) => {
                    metrics::record_gallery_take_down(match outcome {
                        TakeDownOutcome::TakenDown => GalleryTakeDownResult::TakenDown,
                        TakeDownOutcome::NotFound => GalleryTakeDownResult::NotFound,
                        TakeDownOutcome::NotYours => GalleryTakeDownResult::NotYours,
                        TakeDownOutcome::Closed => GalleryTakeDownResult::Closed,
                    });
                    if outcome == TakeDownOutcome::TakenDown {
                        tracing::info!(%user_id, %piece_id, "artboard gallery piece taken down by its hanger");
                    }
                    GalleryResult::TakeDown { piece_id, outcome }
                }
                Err(error) => {
                    metrics::record_gallery_take_down(GalleryTakeDownResult::Failed);
                    late_core::error_span!(
                        "artboard_gallery_take_down",
                        error = ?error,
                        %user_id,
                        %piece_id,
                        "artboard piece could not be taken down"
                    );
                    GalleryResult::TakeDownFailed {
                        piece_id,
                        error: "The piece did not come down. Try again.".to_string(),
                    }
                }
            };
            let _ = tx.send(msg);
        });
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
