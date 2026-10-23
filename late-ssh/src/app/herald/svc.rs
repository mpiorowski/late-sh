//! The herald: one picture a day on the Fediverse and Bluesky
//! (AUDIENCE.md, "The herald"). `HeraldService` runs on every replica and
//! checks every `HERALD_CHECK_INTERVAL`; once the day's post time
//! (`state::HERALD_POST_TIME`) has passed, the `herald_posts` row for the
//! day is the claim (root CONTEXT.md, multi-replica rule), so one replica
//! posts. A claimed day is: pick the picture (`state::pick_picture` over
//! what the last post carried, the waiting poster, yesterday's board) and
//! store it on the row, fetch it from late-web's PNG route, deliver it to
//! every configured network that has no delivery yet (`mastodon.rs`,
//! `bluesky.rs`), settle the row once all have one. A failed day marks
//! its row and the next check claims it again, up to
//! `HERALD_MAX_ATTEMPTS`, reusing the stored pick and skipping the
//! networks already delivered; a claim whose replica died is taken over
//! after `HERALD_STALE_CLAIM`.
//!
//! Two switches gate it: `Config::herald` (a network configured, off in
//! dev without its pair) and the `herald_enabled` row (`AppFlag`), the
//! operator's, read at every check so a flip needs no deploy.
//!
//! This is the orchestration layer: every failure is logged and counted
//! here, and nothing below it logs.

use std::time::Duration;

use anyhow::{Context, bail};
use chrono::{DateTime, NaiveDate, Utc};
use late_core::db::Db;
use late_core::models::app_flag::AppFlag;
use late_core::models::artboard::Snapshot;
use late_core::models::herald::{HeraldNetwork, HeraldPost, HeraldPosted};
use tracing::Instrument;
use uuid::Uuid;

use super::state::{self, BoardCandidate, Picture};
use super::{bluesky, mastodon};
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

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const USER_AGENT: &str = "late.sh herald (+https://late.sh)";

/// What a claimed day came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Posted(HeraldPosted),
    /// Nothing worth posting: no waiting poster, no board with ink.
    Skipped,
}

#[derive(Clone)]
pub struct HeraldService {
    db: Db,
    http: reqwest::Client,
    config: HeraldConfig,
    /// The public web, for the page the post links to.
    web_url: String,
}

impl HeraldService {
    pub fn new(db: Db, config: HeraldConfig, web_url: String) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(HTTP_TIMEOUT)
            .build()
            .expect("reqwest client with a user agent and a timeout builds");
        Self {
            db,
            http,
            config: HeraldConfig {
                web_internal_url: config.web_internal_url.trim_end_matches('/').to_string(),
                ..config
            },
            web_url: web_url.trim_end_matches('/').to_string(),
        }
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
        let key = match self.claim(day).await {
            Ok(Some(key)) => key,
            Ok(None) => {
                match self.finished(day).await {
                    Ok(true) => *finished_for = Some(day),
                    Ok(false) => {}
                    Err(error) => {
                        tracing::warn!(error = ?error, %day, "failed to read the herald's day");
                    }
                }
                metrics::record_herald_post(HeraldPostResult::Lost);
                return false;
            }
            Err(error) => {
                late_core::error_span!(
                    "herald_claim_failed",
                    error = ?error,
                    %day,
                    "failed to claim the herald's day"
                );
                return false;
            }
        };
        match self.post_claimed(day, key, now).await {
            Ok(Outcome::Posted(posted)) => {
                *finished_for = Some(day);
                metrics::record_herald_post(HeraldPostResult::Posted);
                tracing::info!(
                    %day,
                    kind = posted.kind.as_str(),
                    subject = %posted.subject,
                    urls = ?posted.deliveries.iter().map(|d| d.url.as_str()).collect::<Vec<_>>(),
                    "the herald posted"
                );
            }
            Ok(Outcome::Skipped) => {
                *finished_for = Some(day);
                metrics::record_herald_post(HeraldPostResult::Skipped);
                tracing::info!(%day, "the herald had nothing worth posting");
            }
            Err(error) => {
                metrics::record_herald_post(HeraldPostResult::Failed);
                late_core::error_span!(
                    "herald_post_failed",
                    error = ?error,
                    %day,
                    "the herald's post failed"
                );
            }
        }
        true
    }

    async fn switch_on(&self) -> anyhow::Result<bool> {
        let client = self.db.get().await?;
        AppFlag::HeraldEnabled.is_enabled(&client).await
    }

    async fn claim(&self, day: NaiveDate) -> anyhow::Result<Option<Uuid>> {
        let client = self.db.get().await?;
        let stale_before = Utc::now() - HERALD_STALE_CLAIM;
        HeraldPost::claim(&client, day, stale_before, HERALD_MAX_ATTEMPTS).await
    }

    async fn finished(&self, day: NaiveDate) -> anyhow::Result<bool> {
        let client = self.db.get().await?;
        HeraldPost::is_finished(&client, day, HERALD_MAX_ATTEMPTS).await
    }

    /// The claimed day: post it, then settle the row. An error marks the
    /// row failed for the next check and is passed up for the one log
    /// line in `check`. Clients are scoped to each query; nothing holds a
    /// pooled connection across an HTTP call.
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
        if let Some(config) = &self.config.mastodon {
            if !delivered.contains(&HeraldNetwork::Mastodon) {
                let sent = mastodon::deliver(
                    &self.http,
                    config,
                    mastodon::Post {
                        key,
                        png: &png,
                        text: &text,
                        alt: &alt,
                        visibility: self.config.visibility,
                    },
                )
                .await;
                failed += self.note_delivery(day, HeraldNetwork::Mastodon, sent).await;
            }
        }
        if let Some(config) = &self.config.bluesky {
            if !delivered.contains(&HeraldNetwork::Bluesky) {
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
        let Some(picture) = self.pick(&client, day).await? else {
            return Ok(None);
        };
        let stored = serde_json::to_value(&picture).context("encoding the picture")?;
        HeraldPost::store_picture(&client, day, picture.kind(), &picture.subject(), &stored)
            .await?;
        Ok(Some(picture))
    }

    /// The rotation's inputs, then the rule.
    async fn pick(
        &self,
        client: &tokio_postgres::Client,
        day: NaiveDate,
    ) -> anyhow::Result<Option<Picture>> {
        let last = HeraldPost::last_posted_kind(client, day).await?;
        let poster = HeraldPost::next_poster_candidate(client).await?;
        let shown = state::shown_day(day);
        let key = state::board_key(shown);
        let board = match Snapshot::find_by_board_key(client, &key).await? {
            Some(snapshot) => {
                let canvas: dartboard_core::Canvas = serde_json::from_value(snapshot.canvas)
                    .with_context(|| format!("decoding artboard snapshot {key}"))?;
                if state::ink_cells(&canvas) >= state::BOARD_MIN_INK {
                    Some(BoardCandidate { key, shown })
                } else {
                    None
                }
            }
            None => None,
        };
        Ok(state::pick_picture(last, poster, board))
    }

    /// The picture from late-web's PNG route, as bytes.
    async fn fetch_picture(&self, picture: &Picture) -> anyhow::Result<Vec<u8>> {
        use late_core::telemetry::TracedExt;
        let url = format!("{}{}", self.config.web_internal_url, picture.picture_path());
        let response = self
            .http
            .get(&url)
            .send_traced()
            .await
            .with_context(|| format!("getting {url}"))?;
        let status = response.status();
        if !status.is_success() {
            bail!("{url} answered {status}");
        }
        let bytes = response
            .bytes()
            .await
            .with_context(|| format!("reading {url}"))?;
        Ok(bytes.to_vec())
    }
}
