// Orchestration for the door log pipe: one connect-with-retry task per
// enabled door (spawned from main.rs), consuming the host's stats stream
// frame by frame. Each frame is one log line; its whole effect (fact insert +
// cursor advance) commits in one transaction, so a crash never skips a line
// and at worst replays one, which the fact tables' unique
// (game, source_file, source_offset) keys absorb.
//
// Identity: the playname on every line is the account's arcade handle.
// Reserved shapes (`late`, `late_*` — the pre-handle derived playnames) and
// handles whose account is gone (graveyard rows) are skipped: the cursor
// advances, nothing is attributed.
//
// Awards go through the DoorAwards sink (lifetime-idempotent, so they fire
// for every win/orb line, backfill included — owner-approved). Feed events
// are gated on insert freshness AND recency instead: a backfill of years of
// history must not flood #lounge, and a replayed line must not repost.

use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use late_core::db::Db;
use late_core::models::arcade_handle::{ArcadeHandle, handle_reserved};
use late_core::models::door_log_cursor::DoorLogCursor;
use late_core::models::door_milestone::{DoorMilestone, DoorMilestoneKind, NewDoorMilestone};
use late_core::models::door_run::{DoorRun, DoorRunResult, NewDoorRun};
use late_core::models::leaderboard::DoorGame;
use late_core::shutdown::CancellationToken;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::award::{DoorAwards, DoorBadge};
use super::dcss::{DcssMilestone, DcssRun, parse_logfile_line, parse_milestone_line};
use super::nethack::{NethackMilestone, NethackRun, parse_livelog_line, parse_xlogfile_line};
use super::stream::{StatsFrame, StreamConfig, run_stats_stream};
use crate::app::activity::event::ActivityGame;
use crate::app::activity::publisher::ActivityPublisher;
use crate::app::games::chips::svc::ChipService;

/// Backoff between stats-session attempts (host restarts, rollouts, network
/// blips). The stream idles between games, so there is no hurry.
const RETRY_DELAY: Duration = Duration::from_secs(30);

/// Feed events post only for lines this recent. The tail pushes within
/// seconds, so live events always qualify; a backfill or cursor-reset replay
/// of old history never does.
const FEED_RECENCY: chrono::Duration = chrono::Duration::minutes(10);

/// Connection knobs for one door's stats session, from main.rs config.
pub struct DoorIngestTarget {
    pub host: String,
    pub port: u16,
    pub secret: String,
}

/// The doors this service can ingest, one variant per host stats session.
/// Each maps to its roster game, its own identity derivation (per-door blake3
/// domain), and its own frame dispatch.
#[derive(Clone, Copy, Debug)]
enum DoorKind {
    Dcss,
    Nethack,
}

impl DoorKind {
    const fn game(self) -> DoorGame {
        match self {
            Self::Dcss => DoorGame::Dcss,
            Self::Nethack => DoorGame::Nethack,
        }
    }

    fn derive_client_key(self, secret: &str) -> russh::keys::PrivateKey {
        match self {
            Self::Dcss => crate::app::door::dcss::identity::derive_client_key(secret),
            Self::Nethack => crate::app::door::nethack::identity::derive_client_key(secret),
        }
    }
}

#[derive(Clone)]
pub struct DoorIngestService {
    db: Db,
    awards: DoorAwards,
    activity: ActivityPublisher,
}

impl DoorIngestService {
    pub fn new(db: Db, chip_svc: ChipService, activity: ActivityPublisher) -> Self {
        Self {
            awards: DoorAwards::new(chip_svc, db.clone()),
            db,
            activity,
        }
    }

    /// Spawn the DCSS ingestion loop: connect, stream, land facts; on any
    /// end (host rollout, DB error, network drop) reconnect after
    /// [`RETRY_DELAY`] from the last committed cursors.
    pub fn start_dcss_task(
        self,
        target: DoorIngestTarget,
        shutdown: CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        self.start_task(DoorKind::Dcss, target, shutdown)
    }

    /// The NetHack twin: xlogfile (finished games) + livelog (achievements).
    pub fn start_nethack_task(
        self,
        target: DoorIngestTarget,
        shutdown: CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        self.start_task(DoorKind::Nethack, target, shutdown)
    }

    fn start_task(
        self,
        kind: DoorKind,
        target: DoorIngestTarget,
        shutdown: CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                match self.run_session(kind, &target).await {
                    Ok(()) => tracing::info!(?kind, "door stats stream ended; reconnecting"),
                    Err(error) => {
                        tracing::warn!(?kind, ?error, "door stats session failed; reconnecting")
                    }
                }
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    _ = tokio::time::sleep(RETRY_DELAY) => {}
                }
            }
        })
    }

    async fn run_session(&self, kind: DoorKind, target: &DoorIngestTarget) -> Result<()> {
        let game = kind.game().key();
        let cursors = {
            let client = self.db.get().await.context("db client for cursors")?;
            DoorLogCursor::all_for_game(&client, game)
                .await
                .with_context(|| format!("loading {game} cursors"))?
        };
        let key = kind.derive_client_key(&target.secret);
        let (tx, mut rx) = mpsc::channel::<StatsFrame>(256);
        let stream = tokio::spawn(run_stats_stream(
            StreamConfig {
                host: target.host.clone(),
                port: target.port,
                key,
                cursors,
            },
            tx,
        ));

        let mut result = Ok(());
        while let Some(frame) = rx.recv().await {
            let handled = match kind {
                DoorKind::Dcss => self.handle_dcss_frame(&frame).await,
                DoorKind::Nethack => self.handle_nethack_frame(&frame).await,
            };
            if let Err(error) = handled {
                // Do not advance past this line: drop the stream and let the
                // retry loop resume from the last committed cursor.
                result = Err(error).with_context(|| {
                    format!("handling {game} frame {}@{}", frame.file, frame.next_offset)
                });
                break;
            }
        }
        drop(rx);
        match stream.await {
            Ok(stream_result) => result.and(stream_result),
            Err(join_error) => result.and(Err(join_error).context("stats stream task panicked")),
        }
    }

    /// Land one log line. Every path advances the cursor; only lines that
    /// map to a live account insert a fact row.
    pub(crate) async fn handle_dcss_frame(&self, frame: &StatsFrame) -> Result<()> {
        let game = DoorGame::Dcss.key();
        match frame.file.as_str() {
            "logfile" => match parse_logfile_line(&frame.line) {
                Some(run) => self.land_dcss_run(frame, run).await,
                None => {
                    tracing::warn!(line = %frame.line, "unparseable dcss logfile line; skipping");
                    self.advance_cursor(game, frame).await
                }
            },
            "milestones" => match parse_milestone_line(&frame.line) {
                Some(milestone) => self.land_dcss_milestone(frame, milestone).await,
                None => {
                    tracing::warn!(line = %frame.line, "unparseable dcss milestone line; skipping");
                    self.advance_cursor(game, frame).await
                }
            },
            other => {
                tracing::warn!(file = other, "unknown dcss stats file; skipping");
                self.advance_cursor(game, frame).await
            }
        }
    }

    async fn land_dcss_run(&self, frame: &StatsFrame, run: DcssRun) -> Result<()> {
        let game = DoorGame::Dcss.key();
        let Some(user_id) = self.resolve_handle(&run.name).await? else {
            return self.advance_cursor(game, frame).await;
        };

        let new_run = NewDoorRun {
            game,
            user_id,
            ended_at: run.ended_at,
            result: run.result,
            score: run.score,
            depth: run.depth,
            turns: run.turns,
            raw: run.raw.clone(),
            source_file: frame.file.clone(),
            source_offset: frame.next_offset,
        };
        let mut client = self.db.get().await.context("db client for run insert")?;
        let tx = client.transaction().await?;
        let fresh = DoorRun::insert_ignore(&tx, &new_run).await?;
        DoorLogCursor::upsert(&tx, game, &frame.file, frame.next_offset).await?;
        tx.commit().await?;

        // Awards are lifetime-idempotent, so they fire on every sighting
        // (fresh or replayed) — that heals a crash between insert and grant.
        if run.result == DoorRunResult::Win {
            self.awards.grant(user_id, DoorBadge::DcssWin);
        }

        let recent = Utc::now().signed_duration_since(run.ended_at) < FEED_RECENCY;
        if fresh && recent {
            match run.result {
                DoorRunResult::Win => {
                    self.activity
                        .game_won_task(user_id, ActivityGame::Dcss, None, None);
                }
                DoorRunResult::Death => {
                    self.activity
                        .game_event_task(user_id, ActivityGame::Dcss, death_action(&run));
                }
                // Walking away is not a story.
                DoorRunResult::Quit | DoorRunResult::Leaving => {}
                // No DCSS mapping produces Mastery (Brogue, Phase 3).
                DoorRunResult::Mastery => {}
            }
        }
        Ok(())
    }

    async fn land_dcss_milestone(
        &self,
        frame: &StatsFrame,
        milestone: DcssMilestone,
    ) -> Result<()> {
        let game = DoorGame::Dcss.key();
        // Untracked milestone type: cursor forward, nothing persisted.
        let Some(kind) = milestone.kind else {
            return self.advance_cursor(game, frame).await;
        };
        let Some(user_id) = self.resolve_handle(&milestone.name).await? else {
            return self.advance_cursor(game, frame).await;
        };

        let new_milestone = NewDoorMilestone {
            game,
            user_id,
            kind,
            occurred_at: milestone.occurred_at,
            raw: milestone.raw.clone(),
            source_file: frame.file.clone(),
            source_offset: frame.next_offset,
        };
        let mut client = self
            .db
            .get()
            .await
            .context("db client for milestone insert")?;
        let tx = client.transaction().await?;
        let fresh = DoorMilestone::insert_ignore(&tx, &new_milestone).await?;
        DoorLogCursor::upsert(&tx, game, &frame.file, frame.next_offset).await?;
        tx.commit().await?;

        if kind == DoorMilestoneKind::Orb {
            self.awards.grant(user_id, DoorBadge::DcssOrb);
            let recent = Utc::now().signed_duration_since(milestone.occurred_at) < FEED_RECENCY;
            if fresh && recent {
                self.activity.game_event_task(
                    user_id,
                    ActivityGame::Dcss,
                    "found the Orb of Zot".to_string(),
                );
            }
        }
        Ok(())
    }

    /// Land one NetHack log line. Every path advances the cursor; only lines
    /// that map to a live account insert a fact row.
    pub(crate) async fn handle_nethack_frame(&self, frame: &StatsFrame) -> Result<()> {
        let game = DoorGame::Nethack.key();
        match frame.file.as_str() {
            "xlogfile" => match parse_xlogfile_line(&frame.line) {
                Some(run) => self.land_nethack_run(frame, run).await,
                None => {
                    tracing::warn!(line = %frame.line, "unparseable nethack xlogfile line; skipping");
                    self.advance_cursor(game, frame).await
                }
            },
            "livelog" => match parse_livelog_line(&frame.line) {
                Some(milestone) => self.land_nethack_milestone(frame, milestone).await,
                None => {
                    tracing::warn!(line = %frame.line, "unparseable nethack livelog line; skipping");
                    self.advance_cursor(game, frame).await
                }
            },
            other => {
                tracing::warn!(file = other, "unknown nethack stats file; skipping");
                self.advance_cursor(game, frame).await
            }
        }
    }

    async fn land_nethack_run(&self, frame: &StatsFrame, run: NethackRun) -> Result<()> {
        let game = DoorGame::Nethack.key();
        // Wizard/explore games still write an xlogfile line (flagged); they
        // are non-scoring by NetHack's own rules and must not reach boards,
        // badges, or the feed.
        if run.cheat_mode {
            return self.advance_cursor(game, frame).await;
        }
        let Some(user_id) = self.resolve_handle(&run.name).await? else {
            return self.advance_cursor(game, frame).await;
        };

        let new_run = NewDoorRun {
            game,
            user_id,
            ended_at: run.ended_at,
            result: run.result,
            score: run.score,
            depth: run.depth,
            turns: run.turns,
            raw: run.raw.clone(),
            source_file: frame.file.clone(),
            source_offset: frame.next_offset,
        };
        let mut client = self.db.get().await.context("db client for run insert")?;
        let tx = client.transaction().await?;
        let fresh = DoorRun::insert_ignore(&tx, &new_run).await?;
        DoorLogCursor::upsert(&tx, game, &frame.file, frame.next_offset).await?;
        tx.commit().await?;

        // Awards are lifetime-idempotent, so they fire on every sighting
        // (fresh or replayed) — that heals a crash between insert and grant.
        // The end-of-run achieve bit also back-fills an Amulet pickup the
        // livelog stream missed.
        if run.result == DoorRunResult::Win {
            self.awards.grant(user_id, DoorBadge::NethackAscension);
        } else if run.amulet {
            self.awards.grant(user_id, DoorBadge::NethackAmulet);
        }

        let recent = Utc::now().signed_duration_since(run.ended_at) < FEED_RECENCY;
        if fresh && recent {
            match run.result {
                DoorRunResult::Win => {
                    self.activity
                        .game_won_task(user_id, ActivityGame::Nethack, None, None);
                }
                DoorRunResult::Death => {
                    self.activity.game_event_task(
                        user_id,
                        ActivityGame::Nethack,
                        nethack_death_action(&run),
                    );
                }
                // Walking away is not a story.
                DoorRunResult::Quit | DoorRunResult::Leaving => {}
                // No NetHack mapping produces Mastery (Brogue, Phase 3).
                DoorRunResult::Mastery => {}
            }
        }
        Ok(())
    }

    async fn land_nethack_milestone(
        &self,
        frame: &StatsFrame,
        milestone: NethackMilestone,
    ) -> Result<()> {
        let game = DoorGame::Nethack.key();
        // Untracked achievement message: cursor forward, nothing persisted.
        let Some(kind) = milestone.kind else {
            return self.advance_cursor(game, frame).await;
        };
        let Some(user_id) = self.resolve_handle(&milestone.name).await? else {
            return self.advance_cursor(game, frame).await;
        };

        let new_milestone = NewDoorMilestone {
            game,
            user_id,
            kind,
            occurred_at: milestone.occurred_at,
            raw: milestone.raw.clone(),
            source_file: frame.file.clone(),
            source_offset: frame.next_offset,
        };
        let mut client = self
            .db
            .get()
            .await
            .context("db client for milestone insert")?;
        let tx = client.transaction().await?;
        let fresh = DoorMilestone::insert_ignore(&tx, &new_milestone).await?;
        DoorLogCursor::upsert(&tx, game, &frame.file, frame.next_offset).await?;
        tx.commit().await?;

        if kind == DoorMilestoneKind::Amulet {
            self.awards.grant(user_id, DoorBadge::NethackAmulet);
            let recent = Utc::now().signed_duration_since(milestone.occurred_at) < FEED_RECENCY;
            if fresh && recent {
                self.activity.game_event_task(
                    user_id,
                    ActivityGame::Nethack,
                    "acquired the Amulet of Yendor".to_string(),
                );
            }
        }
        Ok(())
    }

    /// The live account behind a playname, or `None` for names the pipe must
    /// never attribute: reserved `late`/`late_*` shapes (legacy derived
    /// playnames) and handles whose account was deleted.
    async fn resolve_handle(&self, name: &str) -> Result<Option<Uuid>> {
        if handle_reserved(name) {
            return Ok(None);
        }
        let client = self.db.get().await.context("db client for handle lookup")?;
        ArcadeHandle::find_user_by_handle(&client, name)
            .await
            .context("resolving arcade handle")
    }

    /// Persist the cursor past a line that landed nothing.
    async fn advance_cursor(&self, game: &'static str, frame: &StatsFrame) -> Result<()> {
        let client = self.db.get().await.context("db client for cursor")?;
        DoorLogCursor::upsert(&client, game, &frame.file, frame.next_offset).await
    }
}

/// The #lounge line for a death: place and crawl's own preformatted death
/// message when present, e.g. "died in DCSS on D:10, slain by an orc warrior".
fn death_action(run: &DcssRun) -> String {
    match (run.place.as_deref(), run.death_message.as_deref()) {
        (Some(place), Some(msg)) => format!("died in DCSS on {place}, {msg}"),
        (Some(place), None) => format!("died in DCSS on {place}"),
        (None, Some(msg)) => format!("died in DCSS, {msg}"),
        (None, None) => "died in DCSS".to_string(),
    }
}

/// The #lounge line for a NetHack death: the level died on (`deathlev`) plus
/// NetHack's own death text, e.g.
/// "died in NetHack on dungeon level 6, killed by a soldier ant".
fn nethack_death_action(run: &NethackRun) -> String {
    match run.death_level {
        Some(dlvl) => format!("died in NetHack on dungeon level {dlvl}, {}", run.death),
        None => format!("died in NetHack, {}", run.death),
    }
}
