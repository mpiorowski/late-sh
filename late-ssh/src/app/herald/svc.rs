//! The herald: one picture a day on the Fediverse and Bluesky
//! (AUDIENCE.md, "The herald"). `HeraldService` runs on every replica and
//! checks every `HERALD_CHECK_INTERVAL`; once the day's post time
//! (`state::HERALD_POST_TIME`) has passed, the `herald_posts` row for the
//! day is the claim (root CONTEXT.md, multi-replica rule), so one replica
//! posts. A claimed day is: pick the picture (yesterday's board, once it
//! has `state::BOARD_MIN_INK` drawn cells; nothing otherwise) and store
//! it on the row, fetch it from late-web's PNG route, deliver it to
//! every configured network that has no delivery yet (`mastodon.rs`,
//! `bluesky.rs`), settle the row once all have one. A failed day marks
//! its row and the next check claims it again, up to
//! `HERALD_MAX_ATTEMPTS`, reusing the stored pick and skipping the
//! networks already delivered; a claim whose replica died is taken over
//! after `HERALD_STALE_CLAIM`, and never after `HERALD_CLAIM_WINDOW` from
//! the day's first claim, the hour Mastodon honours the idempotency key.
//!
//! Two switches gate the checker: `Config::herald` (a network configured,
//! off in dev without its pair) and the `herald_settings` row
//! (`HeraldSwitch`), the operator's, read at every check so a flip needs
//! no deploy. An admin flips the row and runs the day by hand from chat
//! (`/herald`, parsed in `state.rs`): `request` runs the command off the
//! session and answers over a broadcast channel that `tick` drains into a
//! banner, the shape of the paper's press hooks.
//!
//! This is the orchestration layer: every failure is logged and counted
//! here, and nothing below it logs.

use std::time::Duration;

use anyhow::{Context, bail};
use chrono::{DateTime, NaiveDate, Utc};
use late_core::db::Db;
use late_core::models::artboard::Snapshot;
use late_core::models::herald::{
    HeraldDay, HeraldDayStatus, HeraldNetwork, HeraldPost, HeraldPosted, HeraldSwitch,
};
use tokio::sync::broadcast;
use tracing::Instrument;
use uuid::Uuid;

use super::state::{self, BoardLook, HeraldCommand, Picture};
use super::{bluesky, mastodon};
use crate::app::common::primitives::Banner;
use crate::app::state::App;
use crate::config::HeraldConfig;
use crate::metrics::{self, HeraldDeliveryResult, HeraldPostResult};

/// How often each replica asks whether the day's post is due and
/// unclaimed. The steady state is one flag read per check.
pub const HERALD_CHECK_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// A `posting` claim older than this belongs to a replica that died
/// mid-call and is taken over. Well past every network's calls with their
/// timeouts and the media polling.
pub const HERALD_STALE_CLAIM: chrono::Duration = chrono::Duration::minutes(20);

/// How many times one day is claimed before a failing post is left alone.
/// Three checks is fifteen minutes of retries; a network outage or a
/// revoked token should not turn into a day-long retry storm.
pub const HERALD_MAX_ATTEMPTS: i32 = 3;

/// How long after a day's first claim it may be claimed again at all.
/// Mastodon keeps an `Idempotency-Key` for about an hour; a retry outside
/// it would be a second status, so past this a stale claim rests as
/// failed. Three claims twenty minutes apart fit inside.
pub const HERALD_CLAIM_WINDOW: chrono::Duration = chrono::Duration::minutes(50);

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const USER_AGENT: &str = "late.sh herald (+https://late.sh)";
const EVENT_CHANNEL_CAP: usize = 64;

/// What a claimed day came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Posted(HeraldPosted),
    /// Nothing worth posting: no board with ink.
    Skipped,
}

/// What working a day came to, for the checker and for `/herald post`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DayWorked {
    Posted(HeraldPosted),
    Skipped,
    /// Another replica holds the day right now.
    Held,
    /// The day is settled: posted, skipped, or failed at the cap.
    Finished,
    /// The claim or the post failed; logged and counted here.
    Failed,
}

/// When the first post after `/herald on` goes out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NextPost {
    /// Today's day is due and unclaimed: the next check posts it.
    NextCheck,
    /// Today is not due yet, or already has its row.
    AtHour,
    /// Yesterday was due while the switch was off; it stays unposted.
    AtHourAfterSkipping(NaiveDate),
}

/// The answer to an admin's `/herald` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CommandOutcome {
    SwitchedOn(NextPost),
    SwitchedOff,
    /// The row and today's day as they stand.
    Status {
        on: bool,
        day: NaiveDate,
        today: Option<HeraldDay>,
    },
    /// What `/herald post` would send today, and where.
    Preview {
        day: NaiveDate,
        look: BoardLook,
        networks: Vec<HeraldNetwork>,
    },
    Worked(DayWorked),
    /// No network is configured on this deployment.
    Unconfigured,
    Failed,
}

/// What the service tells sessions.
#[derive(Clone, Debug)]
pub(crate) enum HeraldEvent {
    Command {
        user_id: Uuid,
        outcome: CommandOutcome,
    },
}

#[derive(Clone)]
pub struct HeraldService {
    db: Db,
    http: reqwest::Client,
    config: HeraldConfig,
    /// The public web, for the page the post links to.
    web_url: String,
    event_tx: broadcast::Sender<HeraldEvent>,
}

impl HeraldService {
    pub fn new(db: Db, config: HeraldConfig, web_url: String) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(HTTP_TIMEOUT)
            .build()
            .expect("reqwest client with a user agent and a timeout builds");
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAP);
        Self {
            db,
            http,
            config: HeraldConfig {
                web_internal_url: config.web_internal_url.trim_end_matches('/').to_string(),
                ..config
            },
            web_url: web_url.trim_end_matches('/').to_string(),
            event_tx,
        }
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<HeraldEvent> {
        self.event_tx.subscribe()
    }

    /// Every replica runs it; the row claims decide who posts.
    pub fn start_task(&self) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(HERALD_CHECK_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut finished_for: Option<NaiveDate> = None;
            loop {
                interval.tick().await;
                service
                    .check(Utc::now(), &mut finished_for)
                    .instrument(tracing::info_span!("herald.check"))
                    .await;
            }
        })
    }

    /// One check at `now`: is the herald on, is the due day unclaimed, is
    /// it ours. Returns whether this replica worked the day (posted,
    /// skipped, or failed trying). `finished_for` memoizes a day this
    /// replica saw finished, so the rest of that day costs nothing.
    pub async fn check(&self, now: DateTime<Utc>, finished_for: &mut Option<NaiveDate>) -> bool {
        if !self.config.enabled() {
            return false;
        }
        let day = state::post_due_day(now);
        if *finished_for == Some(day) {
            return false;
        }
        match self.switch_on().await {
            Ok(true) => {}
            Ok(false) => return false,
            Err(error) => {
                late_core::error_span!(
                    "herald_flag_failed",
                    error = ?error,
                    "failed to read the herald switch"
                );
                return false;
            }
        }
        match self.work_day(day, now).await {
            DayWorked::Posted(_) | DayWorked::Skipped => {
                *finished_for = Some(day);
                true
            }
            DayWorked::Finished => {
                *finished_for = Some(day);
                false
            }
            DayWorked::Held => false,
            DayWorked::Failed => true,
        }
    }

    /// An admin's `/herald` command, run off the session; the answer
    /// comes back as a [`HeraldEvent`] for the caller's banner.
    pub(crate) fn request(&self, user_id: Uuid, command: HeraldCommand) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let outcome = service.run_command(command, Utc::now()).await;
                let _ = service
                    .event_tx
                    .send(HeraldEvent::Command { user_id, outcome });
            }
            .instrument(tracing::info_span!("herald.command", user_id = %user_id, ?command)),
        );
    }

    /// The command itself. `/herald post` works today's day regardless of
    /// the row: an admin asking is the switch for that one post. The
    /// 16:00 check then finds the day settled and leaves it.
    pub(crate) async fn run_command(
        &self,
        command: HeraldCommand,
        now: DateTime<Utc>,
    ) -> CommandOutcome {
        let today = now.date_naive();
        let result: anyhow::Result<CommandOutcome> = async {
            match command {
                HeraldCommand::On => {
                    // The day the switch slept through is settled before the
                    // row flips, or the next check would catch it up with a
                    // two-day-old board and post again at 16:00.
                    let client = self.db.get().await?;
                    let due = state::post_due_day(now);
                    let next = if due < today {
                        if HeraldPost::settle_unclaimed(&client, due).await? {
                            tracing::info!(day = %due, "the herald left the day it slept through unposted");
                            NextPost::AtHourAfterSkipping(due)
                        } else {
                            NextPost::AtHour
                        }
                    } else {
                        match HeraldPost::day(&client, today).await? {
                            None => NextPost::NextCheck,
                            Some(_) => NextPost::AtHour,
                        }
                    };
                    HeraldSwitch::set(&client, true).await?;
                    Ok(CommandOutcome::SwitchedOn(next))
                }
                HeraldCommand::Off => {
                    let client = self.db.get().await?;
                    HeraldSwitch::set(&client, false).await?;
                    Ok(CommandOutcome::SwitchedOff)
                }
                HeraldCommand::Status => {
                    let client = self.db.get().await?;
                    let on = HeraldSwitch::is_on(&client).await?;
                    let today_row = HeraldPost::day(&client, today).await?;
                    Ok(CommandOutcome::Status {
                        on,
                        day: today,
                        today: today_row,
                    })
                }
                HeraldCommand::Preview => {
                    if !self.config.enabled() {
                        return Ok(CommandOutcome::Unconfigured);
                    }
                    let client = self.db.get().await?;
                    let look = self.look(&client, today).await?;
                    Ok(CommandOutcome::Preview {
                        day: today,
                        look,
                        networks: self.networks(),
                    })
                }
                HeraldCommand::Post => {
                    if !self.config.enabled() {
                        return Ok(CommandOutcome::Unconfigured);
                    }
                    Ok(CommandOutcome::Worked(self.work_day(today, now).await))
                }
            }
        }
        .await;
        match result {
            Ok(outcome) => outcome,
            Err(error) => {
                late_core::error_span!(
                    "herald_command_failed",
                    error = ?error,
                    command = ?command,
                    "the herald's command failed"
                );
                CommandOutcome::Failed
            }
        }
    }

    /// The networks this deployment posts to, in delivery order.
    fn networks(&self) -> Vec<HeraldNetwork> {
        let mut networks = Vec::new();
        if self.config.mastodon.is_some() {
            networks.push(HeraldNetwork::Mastodon);
        }
        if self.config.bluesky.is_some() {
            networks.push(HeraldNetwork::Bluesky);
        }
        networks
    }

    async fn switch_on(&self) -> anyhow::Result<bool> {
        let client = self.db.get().await?;
        HeraldSwitch::is_on(&client).await
    }

    async fn claim(&self, day: NaiveDate) -> anyhow::Result<Option<Uuid>> {
        let client = self.db.get().await?;
        let now = Utc::now();
        HeraldPost::claim(
            &client,
            day,
            now - HERALD_STALE_CLAIM,
            now - HERALD_CLAIM_WINDOW,
            HERALD_MAX_ATTEMPTS,
        )
        .await
    }

    async fn finished(&self, day: NaiveDate) -> anyhow::Result<bool> {
        let client = self.db.get().await?;
        HeraldPost::is_finished(
            &client,
            day,
            Utc::now() - HERALD_CLAIM_WINDOW,
            HERALD_MAX_ATTEMPTS,
        )
        .await
    }

    /// Claim the day and post it. Every outcome is counted and logged
    /// here, once, for the checker and `/herald post` alike.
    async fn work_day(&self, day: NaiveDate, now: DateTime<Utc>) -> DayWorked {
        let key = match self.claim(day).await {
            Ok(Some(key)) => key,
            Ok(None) => {
                return match self.finished(day).await {
                    Ok(true) => DayWorked::Finished,
                    Ok(false) => {
                        metrics::record_herald_post(HeraldPostResult::Lost);
                        DayWorked::Held
                    }
                    Err(error) => {
                        tracing::warn!(error = ?error, %day, "failed to read the herald's day");
                        DayWorked::Held
                    }
                };
            }
            Err(error) => {
                metrics::record_herald_post(HeraldPostResult::Failed);
                late_core::error_span!(
                    "herald_claim_failed",
                    error = ?error,
                    %day,
                    "failed to claim the herald's day"
                );
                return DayWorked::Failed;
            }
        };
        match self.post_claimed(day, key, now).await {
            Ok(Outcome::Posted(posted)) => {
                metrics::record_herald_post(HeraldPostResult::Posted);
                tracing::info!(
                    %day,
                    kind = posted.kind.as_str(),
                    subject = %posted.subject,
                    urls = ?posted.deliveries.iter().map(|d| d.url.as_str()).collect::<Vec<_>>(),
                    "the herald posted"
                );
                DayWorked::Posted(posted)
            }
            Ok(Outcome::Skipped) => {
                metrics::record_herald_post(HeraldPostResult::Skipped);
                tracing::info!(%day, "the herald had nothing worth posting");
                DayWorked::Skipped
            }
            Err(error) => {
                metrics::record_herald_post(HeraldPostResult::Failed);
                late_core::error_span!(
                    "herald_post_failed",
                    error = ?error,
                    %day,
                    "the herald's post failed"
                );
                DayWorked::Failed
            }
        }
    }

    /// The claimed day: post it, then settle the row. An error marks the
    /// row failed for the next check and is passed up for the one log
    /// line in `work_day`. Clients are scoped to each query; nothing
    /// holds a pooled connection across an HTTP call.
    async fn post_claimed(
        &self,
        day: NaiveDate,
        key: Uuid,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Outcome> {
        match self.post_day(day, key, now).await {
            Ok(Outcome::Posted(posted)) => {
                let client = self.db.get().await?;
                HeraldPost::finish_posted(&client, day).await?;
                Ok(Outcome::Posted(posted))
            }
            Ok(Outcome::Skipped) => {
                let client = self.db.get().await?;
                HeraldPost::finish_skipped(&client, day).await?;
                Ok(Outcome::Skipped)
            }
            Err(error) => {
                let marked = async {
                    let client = self.db.get().await?;
                    HeraldPost::mark_failed(&client, day).await
                }
                .await;
                if let Err(mark_error) = marked {
                    // A second, separate failure: the claim stays until
                    // the stale bound reclaims it.
                    tracing::warn!(error = ?mark_error, %day, "failed to mark the herald's day failed");
                }
                Err(error)
            }
        }
    }

    /// Pick (or reuse the stored pick), fetch, deliver to every network
    /// still missing. Each network's failure is logged and counted here,
    /// once; the day fails when any did, with the ones that landed kept.
    async fn post_day(
        &self,
        day: NaiveDate,
        key: Uuid,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Outcome> {
        let Some(picture) = self.picture_for(day).await? else {
            return Ok(Outcome::Skipped);
        };
        let png = self.fetch_picture(&picture).await?;
        let text = state::status_text(&picture, &self.web_url);
        let alt = state::alt_text(&picture);
        let delivered: Vec<HeraldNetwork> = {
            let client = self.db.get().await?;
            HeraldPost::deliveries(&client, day)
                .await?
                .into_iter()
                .map(|delivery| delivery.network)
                .collect()
        };

        let mut failed = 0;
        if let Some(config) = &self.config.mastodon
            && !delivered.contains(&HeraldNetwork::Mastodon)
        {
            let sent = mastodon::deliver(
                &self.http,
                config,
                mastodon::Post {
                    key,
                    png: &png,
                    text: &text,
                    alt: &alt,
                },
            )
            .await;
            failed += self.note_delivery(day, HeraldNetwork::Mastodon, sent).await;
        }
        if let Some(config) = &self.config.bluesky
            && !delivered.contains(&HeraldNetwork::Bluesky)
        {
            let sent = bluesky::deliver(
                &self.http,
                config,
                bluesky::Post {
                    key,
                    png: &png,
                    text: &text,
                    alt: &alt,
                    created_at: now,
                },
            )
            .await;
            failed += self.note_delivery(day, HeraldNetwork::Bluesky, sent).await;
        }
        if failed > 0 {
            bail!("{failed} delivery(ies) failed");
        }
        let client = self.db.get().await?;
        Ok(Outcome::Posted(HeraldPosted {
            kind: picture.kind(),
            subject: picture.subject(),
            deliveries: HeraldPost::deliveries(&client, day).await?,
        }))
    }

    /// Record a network's answer: the delivery row and the counter on
    /// success, the counter and the one log line on failure. Returns how
    /// many failed (0 or 1) for the day's tally. A delivery that landed
    /// but could not be recorded counts as failed too: the retry sends it
    /// again under the same key, which the network dedupes.
    async fn note_delivery(
        &self,
        day: NaiveDate,
        network: HeraldNetwork,
        sent: anyhow::Result<String>,
    ) -> u32 {
        match sent {
            Ok(url) => {
                let recorded = async {
                    let client = self.db.get().await?;
                    HeraldPost::record_delivery(&client, day, network, &url).await
                }
                .await;
                match recorded {
                    Ok(()) => {
                        metrics::record_herald_delivery(network, HeraldDeliveryResult::Delivered);
                        tracing::info!(%day, network = network.as_str(), %url, "the herald delivered");
                        0
                    }
                    Err(error) => {
                        metrics::record_herald_delivery(network, HeraldDeliveryResult::Failed);
                        late_core::error_span!(
                            "herald_delivery_failed",
                            error = ?error,
                            %day,
                            network = network.as_str(),
                            %url,
                            "the herald delivered but could not record it"
                        );
                        1
                    }
                }
            }
            Err(error) => {
                metrics::record_herald_delivery(network, HeraldDeliveryResult::Failed);
                late_core::error_span!(
                    "herald_delivery_failed",
                    error = ?error,
                    %day,
                    network = network.as_str(),
                    "the herald's delivery failed"
                );
                1
            }
        }
    }

    /// The day's picture: the one an earlier claim stored, else a fresh
    /// pick stored now. `None` is a day with nothing worth posting.
    async fn picture_for(&self, day: NaiveDate) -> anyhow::Result<Option<Picture>> {
        let client = self.db.get().await?;
        if let Some(stored) = HeraldPost::stored_picture(&client, day).await? {
            let picture: Picture =
                serde_json::from_value(stored).context("decoding the stored picture")?;
            return Ok(Some(picture));
        }
        let Some(picture) = self.look(&client, day).await?.picture() else {
            return Ok(None);
        };
        let stored = serde_json::to_value(&picture).context("encoding the picture")?;
        HeraldPost::store_picture(&client, day, picture.kind(), &picture.subject(), &stored)
            .await?;
        Ok(Some(picture))
    }

    /// Yesterday's board as the rollover archived it, with its ink
    /// counted; `state::BoardLook::picture` says whether it is a post.
    async fn look(
        &self,
        client: &tokio_postgres::Client,
        day: NaiveDate,
    ) -> anyhow::Result<BoardLook> {
        let shown = state::shown_day(day);
        let key = Snapshot::daily_key(shown);
        let ink = match Snapshot::find_by_board_key(client, &key).await? {
            Some(snapshot) => {
                let canvas: dartboard_core::Canvas = serde_json::from_value(snapshot.canvas)
                    .with_context(|| format!("decoding artboard snapshot {key}"))?;
                Some(state::ink_cells(&canvas))
            }
            None => None,
        };
        Ok(BoardLook { key, shown, ink })
    }

    /// The picture from late-web's PNG route, as bytes.
    async fn fetch_picture(&self, picture: &Picture) -> anyhow::Result<Vec<u8>> {
        use late_core::telemetry::TracedExt;
        let url = format!("{}{}", self.config.web_internal_url, picture.picture_path());
        let bytes = self
            .http
            .get(&url)
            .send_traced()
            .await
            .with_context(|| format!("getting {url}"))?
            .bytes()
            .await
            .with_context(|| format!("reading {url}"))?;
        Ok(bytes.to_vec())
    }
}

/// The session side: drain the service's answers for this user into a
/// banner, and hand `/herald` from the composer to the service.
pub(crate) fn tick(app: &mut App) -> bool {
    let mut changed = drain_events(app);
    changed |= tick_commands(app);
    changed
}

fn drain_events(app: &mut App) -> bool {
    use tokio::sync::broadcast::error::TryRecvError;
    let mut changed = false;
    loop {
        let event = match app.herald.rx.try_recv() {
            Ok(event) => event,
            Err(TryRecvError::Lagged(_)) => continue,
            Err(TryRecvError::Empty | TryRecvError::Closed) => break,
        };
        match event {
            HeraldEvent::Command { user_id, outcome } => {
                if user_id != app.user_id {
                    continue;
                }
                changed = true;
                app.banner = Some(banner_for(outcome));
            }
        }
    }
    changed
}

/// Drain `/herald` from the composer. Every subcommand reaches here only
/// from admins (the composer refuses them for anyone else with a banner).
fn tick_commands(app: &mut App) -> bool {
    let Some(command) = app.chat.take_requested_herald() else {
        return false;
    };
    let working = match command {
        HeraldCommand::On | HeraldCommand::Off | HeraldCommand::Status | HeraldCommand::Preview => {
            None
        }
        HeraldCommand::Post => Some("Posting today's picture…"),
    };
    if let Some(working) = working {
        app.banner = Some(Banner::info(working));
    }
    app.herald.service.request(app.user_id, command);
    true
}

fn names(networks: &[HeraldNetwork]) -> String {
    networks
        .iter()
        .map(|network| network.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Today's row in one clause, for `/herald status`.
fn describe_day(day: &HeraldDay) -> String {
    match day.status {
        HeraldDayStatus::Posting => "posting right now".to_string(),
        HeraldDayStatus::Posted => format!(
            "posted to {}",
            names(
                &day.deliveries
                    .iter()
                    .map(|delivery| delivery.network)
                    .collect::<Vec<_>>()
            )
        ),
        HeraldDayStatus::Skipped => "skipped, nothing worth posting".to_string(),
        HeraldDayStatus::Failed => format!(
            "failed after {} attempt(s), {} delivered",
            day.attempts,
            day.deliveries.len()
        ),
    }
}

/// One line per outcome, the admin's whole answer.
fn banner_for(outcome: CommandOutcome) -> Banner {
    match outcome {
        CommandOutcome::SwitchedOn(NextPost::NextCheck) => Banner::success(
            "The herald is on: today's post goes out at the next check, within 5 minutes",
        ),
        CommandOutcome::SwitchedOn(NextPost::AtHour) => {
            Banner::success("The herald is on: the next post goes out at 16:00 UTC")
        }
        CommandOutcome::SwitchedOn(NextPost::AtHourAfterSkipping(day)) => {
            Banner::success(&format!(
                "The herald is on: {day} was due while it was off and stays unposted; the next post goes out at 16:00 UTC"
            ))
        }
        CommandOutcome::SwitchedOff => {
            Banner::success("The herald is off: nothing posts until /herald on")
        }
        CommandOutcome::Status { on, day, today } => {
            let switch = if on { "on" } else { "off" };
            let state = match today {
                None => "no post yet".to_string(),
                Some(row) => describe_day(&row),
            };
            Banner::info(&format!("Herald {switch}. Today, {day}: {state}"))
        }
        CommandOutcome::Preview {
            day,
            look,
            networks,
        } => match look.picture() {
            Some(picture) => Banner::info(&format!(
                "Would post {} ({} drawn cells) to {} for {day}",
                picture.subject(),
                look.ink.unwrap_or(0),
                names(&networks),
            )),
            None => Banner::info(&format!("Nothing to post for {day}: {}", look.describe())),
        },
        CommandOutcome::Worked(DayWorked::Posted(posted)) => Banner::success(&format!(
            "Posted {} to {}",
            posted.subject,
            posted
                .deliveries
                .iter()
                .map(|delivery| format!("{} ({})", delivery.network.as_str(), delivery.url))
                .collect::<Vec<_>>()
                .join(", "),
        )),
        CommandOutcome::Worked(DayWorked::Skipped) => {
            Banner::info("Nothing worth posting today; the day is settled as skipped")
        }
        CommandOutcome::Worked(DayWorked::Held) => {
            Banner::info("Another replica is posting today right now")
        }
        CommandOutcome::Worked(DayWorked::Finished) => {
            Banner::info("Today is already settled; /herald status says how")
        }
        CommandOutcome::Worked(DayWorked::Failed) => {
            Banner::error("The herald's post failed; see the logs")
        }
        CommandOutcome::Unconfigured => Banner::error("No network is configured here"),
        CommandOutcome::Failed => Banner::error("The herald's command failed; see the logs"),
    }
}
