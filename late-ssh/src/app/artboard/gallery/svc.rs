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
//! The splash podium (last month's top three) is process-wide: one `watch`
//! refreshed hourly; each login claims the next place for its account
//! (`claim_splash_piece`) and the door shows that piece, or the coffee cup
//! once the account has seen the podium. Reading the watch is all it
//! does, so any number of replicas may run it (root CONTEXT.md,
//! multi-replica rule).

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use dartboard_core::Canvas;
use late_core::db::Db;
use late_core::models::app_flag::AppFlags;
use late_core::models::artboard_piece::{
    ApplauseOutcome, ArtboardPiece, HangOutcome, HangParams, ListingCounts, PieceListing,
};
use late_core::models::user::User;
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use crate::app::artboard::provenance::ArtboardProvenance;
use crate::metrics::{self, GalleryApplauseResult, GalleryHangResult};

use super::frame::{Credit, FramedPiece};

/// How often the splash podium is re-read. It changes once a month, and a
/// mod removal is the only thing that could change it in between.
const SPLASH_REFRESH_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// What the door shows this session: a podium place (1 is the winner) and
/// its piece. Claimed once at bootstrap; the session never re-reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct SplashPiece {
    pub place: i64,
    pub piece: GalleryPiece,
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
            Self::Duplicate => "Those exact cells already hang in the gallery this month.",
            Self::Disabled => "The gallery is closed right now.",
        }
    }
}

/// What a spawned gallery task reports back.
#[derive(Debug)]
pub enum GalleryResult {
    Counts(ListingCounts),
    CountsFailed(String),
    Listed {
        listing: PieceListing,
        pieces: Vec<GalleryPiece>,
    },
    ListFailed {
        listing: PieceListing,
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
}

#[derive(Clone)]
pub struct GalleryService {
    db: Option<Db>,
    flags_rx: watch::Receiver<Option<AppFlags>>,
    splash_tx: Arc<watch::Sender<Vec<GalleryPiece>>>,
    splash_rx: watch::Receiver<Vec<GalleryPiece>>,
}

impl GalleryService {
    pub fn new(db: Db, flags_rx: watch::Receiver<Option<AppFlags>>) -> Self {
        let (splash_tx, splash_rx) = watch::channel(Vec::new());
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
        let (splash_tx, splash_rx) = watch::channel(Vec::new());
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

    /// The podium as this replica last read it, best first. Tests and the
    /// claim below read it; sessions get their piece through
    /// `claim_splash_piece`.
    pub fn splash_podium(&self) -> Vec<GalleryPiece> {
        self.splash_rx.borrow().clone()
    }

    /// The piece this login shows over the door, if the account has a
    /// podium place left this month. One `UPDATE` per login while places
    /// remain, none once the account has seen them all (the stamp's
    /// `WHERE` fails and nothing is written). A failed claim is the cup:
    /// the door is not worth failing a login over.
    pub async fn claim_splash_piece(&self, user_id: Uuid) -> Option<SplashPiece> {
        let Some(db) = self.db.as_ref() else {
            return None;
        };
        if !self.is_enabled() {
            return None;
        }
        let podium = self.splash_podium();
        let Some(month) = podium.first().map(|piece| piece.period_month) else {
            return None;
        };
        let claimed = async {
            let client = db.get().await?;
            User::claim_splash_podium_slot(&client, user_id, month, podium.len() as i64).await
        }
        .await;
        match claimed {
            Ok(Some(place)) => match podium.into_iter().nth((place - 1) as usize) {
                Some(piece) => Some(SplashPiece { place, piece }),
                None => {
                    tracing::warn!(
                        %user_id,
                        place,
                        "artboard gallery splash claim landed past the podium"
                    );
                    None
                }
            },
            Ok(None) => None,
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

    /// Hourly re-read of last month's podium for the splash. Runs at start
    /// so the first login after a deploy already has it.
    pub fn start_splash_refresh_task(&self) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(SPLASH_REFRESH_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                match service.refresh_splash().await {
                    Ok(podium) if podium.is_empty() => {
                        tracing::debug!("artboard gallery has no splash podium")
                    }
                    Ok(podium) => tracing::debug!(
                        winner_id = %podium[0].id,
                        places = podium.len(),
                        "artboard gallery splash podium refreshed"
                    ),
                    Err(error) => tracing::warn!(
                        error = ?error,
                        "artboard gallery splash refresh failed"
                    ),
                }
            }
        })
    }

    /// The kill switch covers this read too: while the gallery is off the
    /// splash goes back to the coffee cup on the next refresh, so a piece
    /// that has to come down fast is off the highest-traffic surface
    /// within the hour without waiting for `/mod artboard remove`.
    pub async fn refresh_splash(&self) -> Result<Vec<GalleryPiece>> {
        let Some(db) = self.db.as_ref() else {
            return Ok(Vec::new());
        };
        if !self.is_enabled() {
            let _ = self.splash_tx.send(Vec::new());
            return Ok(Vec::new());
        }
        let client = db.get().await?;
        let podium = ArtboardPiece::previous_month_podium(&client)
            .await?
            .into_iter()
            .map(GalleryPiece::decode)
            .collect::<Result<Vec<_>>>()?;
        let _ = self.splash_tx.send(podium.clone());
        Ok(podium)
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
        tx: mpsc::UnboundedSender<GalleryResult>,
    ) {
        let Some(db) = self.db.clone() else {
            let _ = tx.send(GalleryResult::Listed {
                listing,
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
                Ok(pieces) => GalleryResult::Listed { listing, pieces },
                Err(error) => {
                    tracing::warn!(
                        error = ?error,
                        ?listing,
                        %viewer_id,
                        "artboard gallery listing failed"
                    );
                    GalleryResult::ListFailed {
                        listing,
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
}
