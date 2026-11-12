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
//! replicas may run it (root CONTEXT.md, multi-replica rule). Every login
//! reads the day's piece off the watch (`splash_piece_for_login`) and the
//! door shows it all day, or the coffee cup when there is none. Only users
//! who chose Always View on the Artboard content disclaimer see the piece;
//! everyone else, newcomers first, gets the cup.
//!
//! The #lounge cards (a hung piece's line in chat, drawn as the piece) are
//! process-wide too: one `watch` of the newest pieces that hold a card, by
//! chat message id, re-read on `artboard_pieces_changed` (migration 285)
//! so every replica follows a card posted, a piece taken down and applause
//! from anywhere. Hanging posts the card; a take-down deletes it. The piece
//! modal over #lounge (`card/`) reads and applauds through its own
//! channel (`CardResult`).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use dartboard_core::Canvas;
use late_core::db::Db;
use late_core::models::artboard_piece::{
    ApplauseOutcome, ArtboardPiece, GALLERY_LOUNGE_CARD_LIMIT, HangOutcome, HangParams,
    ListingCounts, PieceListing, TakeDownOutcome,
};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use crate::app::artboard::provenance::ArtboardProvenance;
use crate::app::chat::svc::{ChatService, SendError, Wording};
use crate::metrics::{self, GalleryApplauseResult, GalleryHangResult, GalleryTakeDownResult};
use crate::pg_listener::{Channel, Refresh, Signal, read_until_ok};

use super::frame::{Credit, FramedPiece};

/// How often the splash wall is re-read. It changes once a day, at UTC
/// midnight, and a mod removal is the only thing that could change it in
/// between; a replica is at most an hour behind either.
const SPLASH_REFRESH_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// The day's piece over the door, and the UTC day it holds. Read once
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
        piece: Option<Box<SplashPiece>>,
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

/// How a hung piece's card in #lounge went (`GalleryService::post_lounge_card`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GalleryCardOutcome {
    Posted,
    /// The room's rules turned the line away (a ban, slow mode).
    Refused,
    PostFailed,
    /// Posted, but the piece could not be pointed at it: the line stays a
    /// plain message.
    LinkFailed,
    /// The piece was taken down before its card could be linked; the card
    /// is withdrawn.
    PieceGone,
}

/// What the piece modal over #lounge hears back (`card/state.rs`).
#[derive(Clone, Debug, PartialEq)]
pub enum CardResult {
    /// The piece as this viewer sees it, their own applause marked. `None`
    /// when it was taken down after its card was drawn.
    Viewed(Option<Box<GalleryPiece>>),
    /// The read failed; the modal keeps the card's own copy of the piece.
    ViewFailed,
    Applause(ApplauseOutcome),
    ApplauseFailed(&'static str),
}

/// The #lounge cards by chat message id, as every session draws them.
pub type LoungeCards = Arc<HashMap<Uuid, GalleryPiece>>;

const APPLAUSE_FAILED: &str = "Your applause did not land. Try again.";

#[derive(Clone)]
pub struct GalleryService {
    db: Option<Db>,
    /// Where a hung piece's card is posted and withdrawn. `Some` exactly
    /// when `db` is.
    chat: Option<ChatService>,
    /// The site a card links its piece on (`/gallery/pieces/{id}`).
    web_url: String,
    splash_tx: Arc<watch::Sender<Option<SplashPiece>>>,
    splash_rx: watch::Receiver<Option<SplashPiece>>,
    lounge_cards_tx: Arc<watch::Sender<LoungeCards>>,
    lounge_cards_rx: watch::Receiver<LoungeCards>,
}

impl GalleryService {
    pub const CHANNELS: &'static [Channel] = &[Channel::ArtboardPiecesChanged];

    pub fn new(db: Db, chat: ChatService, web_url: String) -> Self {
        let (splash_tx, splash_rx) = watch::channel(None);
        let (lounge_cards_tx, lounge_cards_rx) = watch::channel(LoungeCards::default());
        Self {
            db: Some(db),
            chat: Some(chat),
            web_url,
            splash_tx: Arc::new(splash_tx),
            splash_rx,
            lounge_cards_tx: Arc::new(lounge_cards_tx),
            lounge_cards_rx,
        }
    }

    /// No database: every listing is empty, nothing hangs, no card is
    /// posted.
    pub fn disabled() -> Self {
        let (splash_tx, splash_rx) = watch::channel(None);
        let (lounge_cards_tx, lounge_cards_rx) = watch::channel(LoungeCards::default());
        Self {
            db: None,
            chat: None,
            web_url: String::new(),
            splash_tx: Arc::new(splash_tx),
            splash_rx,
            lounge_cards_tx: Arc::new(lounge_cards_tx),
            lounge_cards_rx,
        }
    }

    /// This replica's #lounge cards, replaced whole on every change.
    pub fn subscribe_lounge_cards(&self) -> watch::Receiver<LoungeCards> {
        self.lounge_cards_rx.clone()
    }

    /// Keep this replica's cards current: re-read them on every
    /// `artboard_pieces_changed` and whenever the listener (re)connects.
    pub fn start_notify_worker(
        &self,
        mut signals: mpsc::UnboundedReceiver<Signal>,
    ) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            while let Some(signal) = signals.recv().await {
                match signal {
                    Signal::Resync | Signal::Notify { .. } => {
                        read_until_ok(Refresh::GalleryCards, || service.refresh_lounge_cards())
                            .await;
                    }
                }
            }
        })
    }

    /// Read the newest cards and publish them. A piece that will not
    /// decode fails the whole read, so the worker logs it and retries
    /// instead of publishing a set with a card missing.
    pub async fn refresh_lounge_cards(&self) -> Result<()> {
        let Some(db) = &self.db else {
            return Ok(());
        };
        let client = db.get().await?;
        let rows = ArtboardPiece::list_lounge_cards(&client, GALLERY_LOUNGE_CARD_LIMIT).await?;
        let mut cards = HashMap::with_capacity(rows.len());
        for row in rows {
            let message_id = row
                .lounge_message_id
                .expect("list_lounge_cards returns only pieces with a card");
            cards.insert(message_id, GalleryPiece::decode(row)?);
        }
        let cards = Arc::new(cards);
        // The signal also fires for changes no card shows, and a re-read
        // that found the same cards must not make every session redraw.
        self.lounge_cards_tx
            .send_if_modified(|current| match *current == cards {
                true => false,
                false => {
                    *current = cards;
                    true
                }
            });
        Ok(())
    }

    /// Post `piece`'s card in #lounge, in its hanger's name, and point the
    /// piece at it. Every way this ends is counted
    /// (`late_ssh_gallery_cards_total`); the piece hangs either way.
    async fn post_lounge_card(&self, db: &Db, chat: &ChatService, piece: &GalleryPiece) {
        let title = piece.title.split_whitespace().collect::<Vec<_>>().join(" ");
        let body = format!(
            "🎨 \"{title}\" {}/gallery/pieces/{}",
            self.web_url.trim_end_matches('/'),
            piece.id
        );
        let posted = chat
            .send_lounge_message_returning(piece.user_id, body, true, Wording::HouseCard)
            .await;
        let message = match posted {
            Ok(Some(message)) => message,
            Ok(None) => unreachable!("a card's line is never empty"),
            Err(SendError::Refused(refusal)) => {
                metrics::record_gallery_card(GalleryCardOutcome::Refused);
                tracing::info!(refusal = ?refusal, piece_id = %piece.id, "gallery card refused by room rules");
                return;
            }
            Err(SendError::Failed(error)) => {
                metrics::record_gallery_card(GalleryCardOutcome::PostFailed);
                late_core::error_span!(
                    "gallery_card_post_failed",
                    error = ?error,
                    piece_id = %piece.id,
                    "failed to post the gallery card"
                );
                return;
            }
        };
        let linked = async {
            let client = db.get().await?;
            ArtboardPiece::link_lounge_message(&client, piece.user_id, piece.id, message.id).await
        }
        .await;
        match linked {
            Ok(0) => {
                metrics::record_gallery_card(GalleryCardOutcome::PieceGone);
                withdraw_card(chat, piece.user_id, piece.id, Some(message.id)).await;
            }
            Ok(_) => {
                metrics::record_gallery_card(GalleryCardOutcome::Posted);
                tracing::info!(message_id = %message.id, piece_id = %piece.id, "gallery card posted");
            }
            Err(error) => {
                metrics::record_gallery_card(GalleryCardOutcome::LinkFailed);
                late_core::error_span!(
                    "gallery_card_link_failed",
                    error = ?error,
                    message_id = %message.id,
                    "failed to point the piece at its card"
                );
            }
        }
    }

    /// Whether this service has a database behind it. A
    /// [`GalleryService::disabled`] one (tests, a DB-less boot) hides the
    /// gallery.
    pub fn is_enabled(&self) -> bool {
        self.db.is_some()
    }

    /// The day's piece over the door, as this replica last read it.
    /// Every login shows it for the whole UTC day; `None` (no database,
    /// empty queue) is the coffee cup.
    pub fn splash_piece(&self) -> Option<SplashPiece> {
        self.splash_rx.borrow().clone()
    }

    /// The authenticated login gate: `None` (the coffee cup) while the
    /// user still has the Artboard content disclaimer on, so a hung piece
    /// greets only those who chose Always View. The canvas comes off the
    /// watch; whether it is still up is read fresh.
    pub async fn splash_piece_for_login(&self, artboard_disclaimer: bool) -> Option<SplashPiece> {
        self.splash_piece_for_day(artboard_disclaimer, Utc::now().date_naive())
            .await
    }

    async fn splash_piece_for_day(
        &self,
        artboard_disclaimer: bool,
        day: NaiveDate,
    ) -> Option<SplashPiece> {
        if artboard_disclaimer {
            return None;
        }
        let splash = match self.splash_piece()? {
            splash if splash.shown_on == day => splash,
            // The hourly refresh has not crossed midnight yet, so the cache
            // still holds an earlier day's piece. Assign and publish this
            // day's here rather than show the cup until the next refresh.
            _stale => match self.refresh_splash(day).await {
                Ok(SplashRefresh::Wall {
                    piece: Some(piece), ..
                }) => *piece,
                Ok(SplashRefresh::Wall { piece: None, .. }) | Ok(SplashRefresh::Off) => {
                    return None;
                }
                Err(error) => {
                    tracing::warn!(
                        error = ?error,
                        "artboard gallery splash refresh at login failed"
                    );
                    return None;
                }
            },
        };
        let db = self.db.as_ref()?;
        let result = async {
            let client = db.get().await?;
            ArtboardPiece::still_splash_on(&client, splash.piece.id, day).await
        }
        .await;
        match result {
            Ok(true) => Some(splash),
            Ok(false) => None,
            Err(error) => {
                tracing::warn!(error = ?error, piece_id = %splash.piece.id, "artboard splash removal check failed");
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
    /// into the watch. `Off` is a service with no database.
    pub async fn refresh_splash(&self, day: NaiveDate) -> Result<SplashRefresh> {
        let Some(db) = self.db.as_ref() else {
            return Ok(SplashRefresh::Off);
        };
        let client = db.get().await?;
        let piece = match ArtboardPiece::splash_for_day(&client, day).await? {
            Some(piece) => Some(SplashPiece::decode(day, piece)?),
            None => None,
        };
        let queued = ArtboardPiece::splash_queue_depth(&client, day).await?;
        let _ = self.splash_tx.send(piece.clone());
        Ok(SplashRefresh::Wall {
            piece: piece.map(Box::new),
            queued,
        })
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
        let (Some(db), Some(chat)) = (self.db.clone(), self.chat.clone()) else {
            let _ = tx.send(GalleryResult::HangRefused(HangRefusal::Disabled));
            return;
        };
        let service = self.clone();
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
                Ok(HangOutcome::Hung(piece)) => match GalleryPiece::decode(*piece) {
                    Ok(piece) => {
                        metrics::record_gallery_hang(GalleryHangResult::Hung);
                        tracing::info!(
                            %user_id,
                            piece_id = %piece.id,
                            width = piece.width,
                            height = piece.height,
                            "artboard piece hung"
                        );
                        // Its own task: the hanger hears `Hung` at once, the
                        // card follows.
                        let card_piece = piece.clone();
                        tokio::spawn(async move {
                            service.post_lounge_card(&db, &chat, &card_piece).await;
                        });
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

    /// Applause from the gallery page, answered on its results channel.
    pub fn applaud_task(
        &self,
        piece_id: Uuid,
        user_id: Uuid,
        tx: mpsc::UnboundedSender<GalleryResult>,
    ) {
        let Some(db) = self.db.clone() else {
            return;
        };
        tokio::spawn(async move {
            let msg = match applaud(&db, piece_id, user_id).await {
                Ok(outcome) => GalleryResult::Applause { piece_id, outcome },
                Err(error) => GalleryResult::ApplauseFailed {
                    piece_id,
                    error: error.to_string(),
                },
            };
            let _ = tx.send(msg);
        });
    }

    /// Applause from the piece modal over #lounge, answered on its own
    /// channel. The same toggle, metric and failure log as the page's.
    pub fn applaud_card_task(
        &self,
        piece_id: Uuid,
        user_id: Uuid,
        tx: mpsc::UnboundedSender<CardResult>,
    ) {
        let Some(db) = self.db.clone() else {
            return;
        };
        tokio::spawn(async move {
            let msg = match applaud(&db, piece_id, user_id).await {
                Ok(outcome) => CardResult::Applause(outcome),
                Err(error) => CardResult::ApplauseFailed(error),
            };
            let _ = tx.send(msg);
        });
    }

    /// The piece behind a #lounge card as `viewer_id` sees it. The cards
    /// are one set for everyone, so the modal asks once who is looking to
    /// learn whether their own applause is on it.
    pub fn view_card_task(
        &self,
        piece_id: Uuid,
        viewer_id: Uuid,
        tx: mpsc::UnboundedSender<CardResult>,
    ) {
        let Some(db) = self.db.clone() else {
            return;
        };
        tokio::spawn(async move {
            let result = async {
                let client = db.get().await?;
                let row = ArtboardPiece::find(&client, viewer_id, piece_id).await?;
                row.map(GalleryPiece::decode).transpose()
            }
            .await;
            let msg = match result {
                Ok(piece) => CardResult::Viewed(piece.map(Box::new)),
                Err(error) => {
                    late_core::error_span!(
                        "artboard_gallery_card_view",
                        error = ?error,
                        %viewer_id,
                        %piece_id,
                        "artboard piece could not be read for its lounge card"
                    );
                    CardResult::ViewFailed
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
        let (Some(db), Some(chat)) = (self.db.clone(), self.chat.clone()) else {
            return;
        };
        tokio::spawn(async move {
            let result = async {
                let client = db.get().await?;
                ArtboardPiece::take_down(&client, piece_id, user_id).await
            }
            .await;
            let msg = match result {
                Ok(outcome) => {
                    metrics::record_gallery_take_down(match outcome {
                        TakeDownOutcome::TakenDown { .. } => GalleryTakeDownResult::TakenDown,
                        TakeDownOutcome::NotFound => GalleryTakeDownResult::NotFound,
                        TakeDownOutcome::NotYours => GalleryTakeDownResult::NotYours,
                        TakeDownOutcome::Closed => GalleryTakeDownResult::Closed,
                    });
                    match outcome {
                        TakeDownOutcome::TakenDown { lounge_message_id } => {
                            tracing::info!(%user_id, %piece_id, "artboard gallery piece taken down by its hanger");
                            withdraw_card(&chat, user_id, piece_id, lounge_message_id).await;
                        }
                        TakeDownOutcome::NotFound
                        | TakeDownOutcome::NotYours
                        | TakeDownOutcome::Closed => {}
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

/// One hand of applause given or taken back, with its metric and, when
/// the database would not have it, its log. Both doors applaud through
/// here: the gallery page and the piece modal over #lounge.
async fn applaud(db: &Db, piece_id: Uuid, user_id: Uuid) -> Result<ApplauseOutcome, &'static str> {
    let result = async {
        let client = db.get().await?;
        ArtboardPiece::toggle_applause(&client, piece_id, user_id).await
    }
    .await;
    match result {
        Ok(outcome) => {
            metrics::record_gallery_applause(match outcome {
                ApplauseOutcome::Applauded(_) => GalleryApplauseResult::Applauded,
                ApplauseOutcome::Withdrawn(_) => GalleryApplauseResult::Withdrawn,
                ApplauseOutcome::OwnPiece => GalleryApplauseResult::OwnPiece,
                ApplauseOutcome::NotFound => GalleryApplauseResult::NotFound,
                ApplauseOutcome::Closed => GalleryApplauseResult::Closed,
            });
            Ok(outcome)
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
            Err(APPLAUSE_FAILED)
        }
    }
}

/// Delete the #lounge card of a piece that just came down, in the name of
/// `user_id`, who took it down: its hanger or a moderator. The one place a
/// card goes with its piece. The piece is down either way; a card that
/// would not go is logged, stays as its plain line for its author or a mod
/// to delete, and answers false.
pub(crate) async fn withdraw_card(
    chat: &ChatService,
    user_id: Uuid,
    piece_id: Uuid,
    lounge_message_id: Option<Uuid>,
) -> bool {
    let Some(message_id) = lounge_message_id else {
        return true;
    };
    match chat.delete_house_card(user_id, message_id).await {
        Ok(()) => true,
        Err(error) => {
            late_core::error_span!(
                "gallery_card_delete_failed",
                error = ?error,
                %user_id,
                %piece_id,
                %message_id,
                "failed to delete the card of a taken-down gallery piece"
            );
            false
        }
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
