//! Orchestration for the lounge spawns (GAME.md, "The lounge pass"): the
//! spawner on every replica (the day's dice, the claim, the insert), the
//! sweeper (a spawn past its time slips away; a slain one nobody paid is
//! paid), the hit for a session, the pay after a kill, the wire's lines,
//! and the notify worker that keeps every replica's copy of the live
//! spawn in step. The only lounge layer touching the database, logging,
//! and metrics; `state.rs` decides, this file does.
//!
//! Replica rule: nothing here is a process-local source of truth. The
//! day's budget is a row with conditional claims, the spawn is a row with
//! one conditional update per hit, the pay is a claim on the slain row,
//! and the spawner on every replica rolls the same question against the
//! same rows. Whoever wins a statement does the one thing it allows.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use late_core::db::Db;
use late_core::models::deadchannel_spawn::{
    HitResult, LoungeDay, Spawn, SpawnHit, SpawnKind, SpawnOutcome, notify_spawn,
    parse_spawn_payload,
};
use tokio::sync::{mpsc, watch};
use tracing::{Instrument, info_span};
use uuid::Uuid;

use super::data;
use super::state::{
    Due, SPAWN_GAP_SECS, due, hits_needed, is_stale, lifetime_secs, pay_for, roll_dice,
};
use crate::app::chat::svc::ChatService;
use crate::app::common::primitives::Screen;
use crate::app::deadchannel::fight::svc::FightService;
use crate::app::deadchannel::fight::ui::weapon_name;
use crate::app::deadchannel::runner::svc::RunnerLooks;
use crate::app::presence::svc::PresenceService;
use crate::app::state::App;
use crate::metrics::{self, LoungeBeat};
use crate::pg_listener::{Channel, Refresh, Signal, read_until_ok};

/// How often every replica asks whether a spawn is due and sweeps the
/// ended ones.
const SPAWNER_TICK: Duration = Duration::from_secs(20);
/// How long a slain spawn may sit unpaid before any sweeper settles it:
/// long enough for the replica that slew it to pay in the ordinary case.
const UNPAID_GRACE_SECS: i64 = 30;

/// What a session's `/hit` came back with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HitOutcome {
    /// The hit is on the row: `left` more put it down (zero: it is down).
    Landed {
        kind: SpawnKind,
        left: i32,
        weapon: &'static str,
    },
    AlreadyHit,
    /// Nothing live to hit.
    Nothing,
    /// The runner's signal is down until the roll.
    Down,
    /// No standing runner: the composer should not have parsed the
    /// command; logged as such.
    NoRunner,
    Failed,
}

/// What sessions share: the latest spawn heard, live or in its aftermath.
pub type LiveSpawn = Option<Spawn>;

#[derive(Clone)]
pub struct LoungeService {
    db: Db,
    fight: FightService,
    chat: ChatService,
    presence: PresenceService,
    runners_rx: watch::Receiver<RunnerLooks>,
    spawn_tx: watch::Sender<LiveSpawn>,
    is_draining: Arc<AtomicBool>,
}

impl LoungeService {
    pub fn new(
        db: Db,
        fight: FightService,
        chat: ChatService,
        presence: PresenceService,
        runners_rx: watch::Receiver<RunnerLooks>,
        is_draining: Arc<AtomicBool>,
    ) -> Self {
        let (spawn_tx, _) = watch::channel(None);
        Self {
            db,
            fight,
            chat,
            presence,
            runners_rx,
            spawn_tx,
            is_draining,
        }
    }

    /// The process-shared spawn. `None` until the listener's first load
    /// or the first spawn heard.
    pub fn subscribe(&self) -> watch::Receiver<LiveSpawn> {
        self.spawn_tx.subscribe()
    }

    /// What the notify worker subscribes to.
    pub const CHANNELS: &'static [Channel] = &[Channel::DeadchannelSpawn];

    /// Keep every replica's copy of the spawn in step with the channel. A
    /// notify carries the whole row; a resync re-reads the live row, so a
    /// replica that boots mid-spawn finds it.
    pub fn start_notify_worker(
        &self,
        mut signals: mpsc::UnboundedReceiver<Signal>,
    ) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            while let Some(signal) = signals.recv().await {
                match signal {
                    Signal::Resync => {
                        tracing::debug!("lounge spawn resync");
                        read_until_ok(Refresh::LoungeSpawn, || service.refresh()).await;
                    }
                    Signal::Notify { payload, .. } => match parse_spawn_payload(&payload) {
                        Some(spawn) => {
                            let stale = is_stale(service.spawn_tx.borrow().as_ref(), &spawn);
                            match stale {
                                true => {
                                    tracing::debug!(spawn_id = %spawn.id, "older deadchannel spawn payload dropped");
                                }
                                false => {
                                    service.spawn_tx.send_replace(Some(spawn));
                                }
                            }
                        }
                        None => tracing::warn!(payload, "unreadable deadchannel spawn payload"),
                    },
                }
            }
        })
    }

    /// Re-read the live spawn. A row that ended is left to the sessions'
    /// aftermath clocks: nothing to read means nothing live.
    pub async fn refresh(&self) -> Result<()> {
        let client = self.db.get().await?;
        if let Some(spawn) = Spawn::live(&**client).await? {
            self.spawn_tx.send_replace(Some(spawn));
        }
        Ok(())
    }

    /// The spawner and sweeper, every [`SPAWNER_TICK`], for the life of
    /// the process. A replica draining for a deploy stops asking: the
    /// newer one owns the rows.
    pub fn start_spawner_task(&self) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(SPAWNER_TICK).await;
                if service.is_draining.load(Ordering::Relaxed) {
                    continue;
                }
                service
                    .sweep(Utc::now())
                    .instrument(info_span!("deadchannel.lounge.sweep"))
                    .await;
            }
        })
    }

    /// One tick of the spawner: end what slipped, pay what was left
    /// unpaid, and land what is due. Every failure is logged here.
    pub(crate) async fn sweep(&self, now: DateTime<Utc>) {
        if let Err(error) = self.end_slipped(now).await {
            tracing::error!(error = ?error, "failed to end the slipped lounge spawns");
        }
        if let Err(error) = self.pay_the_unpaid(now).await {
            tracing::error!(error = ?error, "failed to settle the unpaid lounge spawns");
        }
        match self.land_due(now).await {
            Ok(Some(spawn)) => {
                metrics::record_deadchannel_lounge(LoungeBeat::Spawned);
                tracing::info!(spawn_id = %spawn.id, kind = spawn.kind.as_str(), "lounge spawn landed");
            }
            Ok(None) => {}
            Err(error) => {
                tracing::error!(error = ?error, "failed to land a lounge spawn");
            }
        }
    }

    /// Distinct runners looking at #lounge, on every replica.
    fn runners_looking(&self) -> usize {
        let runners = self.runners_rx.borrow().clone();
        self.presence
            .records()
            .iter()
            .filter(|record| record.lounge && runners.contains_key(&record.user_id))
            .map(|record| record.user_id)
            .collect::<HashSet<Uuid>>()
            .len()
    }

    /// Whether this replica's copy says a spawn is live at `now`. The
    /// one-live index is the guard; this only saves the round trip.
    fn live_here(&self, now: DateTime<Utc>) -> bool {
        self.spawn_tx
            .borrow()
            .as_ref()
            .is_some_and(|spawn| spawn.is_live() && now.timestamp_millis() < spawn.expires_at_ms)
    }

    /// Land what the day's row says is due, if anything. The claim and
    /// the insert are one transaction: a second replica landing the same
    /// spawn loses the claim, and one landing another kind at the same
    /// moment loses the one-live index, which rolls its claim back.
    async fn land_due(&self, now: DateTime<Utc>) -> Result<Option<Spawn>> {
        let looking = self.runners_looking();
        if looking == 0 || self.live_here(now) {
            return Ok(None);
        }
        // The dice are rolled before the first await: the thread's rng is
        // not `Send`, and only the first replica to touch the day keeps
        // them anyway.
        let dice = roll_dice(&mut rand::thread_rng());
        let mut client = self.db.get().await?;
        let day = LoungeDay::ensure(&**client, now.date_naive(), dice).await?;
        let live = Spawn::live(&**client).await?.is_some();
        let spawn = match due(&day, now, looking, live) {
            Due::Nothing => return Ok(None),
            Due::Flicker(window) => {
                let tx = client.transaction().await?;
                let gap_closed_at = now - chrono::Duration::seconds(SPAWN_GAP_SECS);
                if !LoungeDay::claim_flicker(&*tx, day.day, window, now, gap_closed_at).await? {
                    return Ok(None);
                }
                let spawn = Self::insert(&tx, SpawnKind::Flicker, now).await?;
                tx.commit().await.context("committing the flicker")?;
                spawn
            }
            Due::Howler => {
                let tx = client.transaction().await?;
                let gap_closed_at = now - chrono::Duration::seconds(SPAWN_GAP_SECS);
                if !LoungeDay::claim_howler(&*tx, day.day, now, gap_closed_at).await? {
                    return Ok(None);
                }
                let spawn = Self::insert(&tx, SpawnKind::Howler, now).await?;
                tx.commit().await.context("committing the howler")?;
                spawn
            }
        };
        announce(&client, &spawn).await;
        Ok(Some(spawn))
    }

    async fn insert(
        tx: &tokio_postgres::Transaction<'_>,
        kind: SpawnKind,
        now: DateTime<Utc>,
    ) -> Result<Spawn> {
        Spawn::insert(
            tx,
            Uuid::now_v7(),
            kind,
            hits_needed(kind),
            now,
            now + chrono::Duration::seconds(lifetime_secs(kind)),
        )
        .await
    }

    /// End every spawn past its time; whoever ends one tells the wire,
    /// unless nobody hit it.
    async fn end_slipped(&self, now: DateTime<Utc>) -> Result<()> {
        let client = self.db.get().await?;
        for spawn in Spawn::end_expired(&**client, now).await? {
            metrics::record_deadchannel_lounge(LoungeBeat::Slipped);
            tracing::info!(spawn_id = %spawn.id, kind = spawn.kind.as_str(), hits = spawn.hits.len(), "lounge spawn slipped away");
            announce(&client, &spawn).await;
            if let Some(line) = data::slipped_line(&spawn) {
                self.chat.post_wire_line_task(line);
            }
        }
        Ok(())
    }

    /// Settle slain spawns left unpaid past the grace: what a replica
    /// that died between the kill and the pay left behind.
    async fn pay_the_unpaid(&self, now: DateTime<Utc>) -> Result<()> {
        let client = self.db.get().await?;
        let ended_by = now - chrono::Duration::seconds(UNPAID_GRACE_SECS);
        for spawn in Spawn::unpaid_slain(&**client, ended_by).await? {
            tracing::warn!(spawn_id = %spawn.id, "lounge spawn found unpaid; settling");
            self.settle(&spawn, now).await;
        }
        Ok(())
    }

    /// Run one `/hit` for a session and answer on `reply`.
    pub(crate) fn hit_task(
        &self,
        user_id: Uuid,
        username: String,
        reply: mpsc::UnboundedSender<HitOutcome>,
    ) {
        let svc = self.clone();
        let span =
            info_span!("deadchannel.lounge.hit_task", user_id = %user_id, username = %username);
        tokio::spawn(
            async move {
                let outcome = match svc.hit(user_id, &username, Utc::now()).await {
                    Ok(outcome) => {
                        match &outcome {
                            HitOutcome::Landed { kind, left, .. } => {
                                metrics::record_deadchannel_lounge(LoungeBeat::Hit);
                                tracing::info!(kind = kind.as_str(), left, "lounge spawn hit");
                            }
                            HitOutcome::AlreadyHit | HitOutcome::Nothing | HitOutcome::Down => {
                                metrics::record_deadchannel_lounge(LoungeBeat::HitRefused);
                                tracing::debug!(outcome = ?outcome, "lounge hit refused");
                            }
                            HitOutcome::NoRunner => {
                                metrics::record_deadchannel_lounge(LoungeBeat::HitRefused);
                                tracing::warn!("lounge hit from a user with no standing runner");
                            }
                            HitOutcome::Failed => {}
                        }
                        outcome
                    }
                    Err(error) => {
                        metrics::record_deadchannel_lounge(LoungeBeat::HitFailed);
                        tracing::error!(error = ?error, "failed to apply a lounge hit");
                        HitOutcome::Failed
                    }
                };
                // A closed channel is a session that left; nothing to tell.
                let _ = reply.send(outcome);
            }
            .instrument(span),
        );
    }

    /// One hit: the runner's standing and signal read off the settled row
    /// (the day rolls there if it has not), then one conditional update on
    /// the live spawn. The hit that makes the count settles the kill, and
    /// the pay lands before the slain spawn is announced: every hitter's
    /// session re-reads its sheet when the kill reaches it, and what it
    /// reads has to hold the leavings.
    pub(crate) async fn hit(
        &self,
        user_id: Uuid,
        username: &str,
        now: DateTime<Utc>,
    ) -> Result<HitOutcome> {
        let Some(sheet) = self.fight.settled_sheet(user_id).await? else {
            return Ok(HitOutcome::NoRunner);
        };
        if sheet.is_down() {
            return Ok(HitOutcome::Down);
        }
        let client = self.db.get().await?;
        let Some(spawn) = Spawn::live(&**client).await? else {
            return Ok(HitOutcome::Nothing);
        };
        if now.timestamp_millis() >= spawn.expires_at_ms {
            return Ok(HitOutcome::Nothing);
        }
        let hit = SpawnHit {
            user_id,
            username: username.to_string(),
            at_ms: now.timestamp_millis(),
        };
        match Spawn::hit(&**client, spawn.id, &hit, now).await? {
            HitResult::Landed(after) => {
                if after.outcome == Some(SpawnOutcome::Slain) {
                    self.settle(&after, now).await;
                }
                announce(&client, &after).await;
                Ok(HitOutcome::Landed {
                    kind: after.kind,
                    left: after.hits_left(),
                    weapon: weapon_name(&sheet),
                })
            }
            HitResult::AlreadyHit => Ok(HitOutcome::AlreadyHit),
            HitResult::Gone => Ok(HitOutcome::Nothing),
        }
    }

    /// Pay every hitter of a slain spawn and tell the wire, once: the pay
    /// claim on the row picks the one caller that does it. A hitter whose
    /// pay fails is logged and counted and not retried: the claim is
    /// taken, and a replica paying twice is the worse failure.
    async fn settle(&self, spawn: &Spawn, now: DateTime<Utc>) {
        let claimed = async {
            let client = self.db.get().await?;
            Spawn::claim_pay(&**client, spawn.id, now).await
        }
        .await;
        let spawn = match claimed {
            Ok(Some(spawn)) => spawn,
            Ok(None) => return,
            Err(error) => {
                tracing::error!(error = ?error, spawn_id = %spawn.id, "failed to claim the lounge spawn's pay");
                return;
            }
        };
        metrics::record_deadchannel_lounge(LoungeBeat::Slain);
        tracing::info!(spawn_id = %spawn.id, kind = spawn.kind.as_str(), hitters = spawn.hits.len(), "lounge spawn put down");
        let leavings = pay_for(spawn.kind);
        for hit in &spawn.hits {
            match self.fight.pocket(hit.user_id, leavings).await {
                Ok(Some(_)) => metrics::record_deadchannel_lounge(LoungeBeat::Paid),
                Ok(None) => {
                    metrics::record_deadchannel_lounge(LoungeBeat::PayLeft);
                    tracing::warn!(user_id = %hit.user_id, spawn_id = %spawn.id, "lounge hitter has no standing runner; leavings left");
                }
                Err(error) => {
                    metrics::record_deadchannel_lounge(LoungeBeat::PayFailed);
                    tracing::error!(error = ?error, user_id = %hit.user_id, spawn_id = %spawn.id, "failed to pay a lounge hitter");
                }
            }
        }
        self.chat.post_wire_line_task(data::slain_line(&spawn));
    }
}

/// Tell every replica the spawn's row moved. The row is the truth and
/// has landed; a notify that fails costs the copies their freshness
/// until the next one or a resync, never the hit or the spawn. Logged
/// and counted here.
async fn announce(client: &tokio_postgres::Client, spawn: &Spawn) {
    if let Err(error) = notify_spawn(client, spawn).await {
        metrics::record_deadchannel_lounge(LoungeBeat::NotifyFailed);
        tracing::error!(error = ?error, spawn_id = %spawn.id, "failed to announce the lounge spawn");
    }
}

/// One tick of a session's lounge: whether the composer parses `/hit`
/// (a standing runner, looking at #lounge, where the spawn and every
/// answer to a hit are drawn), the hit it asked for, the spawn copy and
/// the word. A frame is owed only where the line is drawn: Home with
/// #lounge up.
pub(crate) fn tick(app: &mut App, anim_half: bool) -> bool {
    app.chat
        .set_hits_on(app.is_runner() && app.looking_at_lounge());
    let now_ms = crate::app::presence::svc::now_ms();
    if app.chat.take_requested_hit() {
        app.lounge.hit(now_ms);
    }
    let tick = app.lounge.tick(now_ms, anim_half);
    if tick.reload_sheet {
        app.fight.reload();
    }
    tick.changed && app.screen == Screen::Dashboard
}

/// Whether the scene on the #lounge card is playing frames: a strike or
/// a dissolve, for a runner looking at it. The wake cadence's seam.
pub(crate) fn animates(app: &App) -> bool {
    app.is_runner()
        && app.looking_at_lounge()
        && app.lounge.animating(crate::app::presence::svc::now_ms())
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
