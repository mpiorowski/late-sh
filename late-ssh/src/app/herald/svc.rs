//! The herald: one picture a day on the Fediverse (AUDIENCE.md, "The
//! herald"). `HeraldService` runs on every replica and checks every
//! `HERALD_CHECK_INTERVAL`; once the day's post time
//! (`state::HERALD_POST_TIME`) has passed, the `herald_posts` row for the
//! day is the claim (root CONTEXT.md, multi-replica rule), so one replica
//! posts. A claimed day is: pick the picture (`state::pick_picture` over
//! what the last post carried, the waiting poster, yesterday's board),
//! fetch it from late-web's PNG route, upload it to the instance, post
//! the status with the row's id as the idempotency key, settle the row. A
//! failed day marks its row and the next check claims it again, up to
//! `HERALD_MAX_ATTEMPTS`; a claim whose replica died is taken over after
//! `HERALD_STALE_CLAIM`.
//!
//! Two switches gate it: `Config::herald.enabled` (off in dev without a
//! token, on in prod) and the `herald_enabled` row (`AppFlag`), the
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
use late_core::models::herald::{HeraldPost, HeraldPosted};
use late_core::telemetry::TracedExt;
use serde::{Deserialize, Serialize};
use tracing::Instrument;
use uuid::Uuid;

use super::state::{self, BoardCandidate, Picture};
use crate::config::HeraldConfig;
use crate::metrics::{self, HeraldPostResult};

/// How often each replica asks whether the day's post is due and
/// unclaimed. The steady state is one flag read per check.
pub const HERALD_CHECK_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// A `posting` claim older than this belongs to a replica that died
/// mid-call and is taken over. Well past the three HTTP calls with their
/// timeouts and the media polling.
pub const HERALD_STALE_CLAIM: chrono::Duration = chrono::Duration::minutes(20);

/// How many times one day is claimed before a failing post is left alone.
/// Three checks is fifteen minutes of retries; an instance outage or a
/// revoked token should not turn into a day-long retry storm.
pub const HERALD_MAX_ATTEMPTS: i32 = 3;

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const USER_AGENT: &str = "late.sh herald (+https://late.sh)";

/// An upload the instance answered 202 to is polled this often, this
/// many times, before the day fails. Images are processed in seconds.
const MEDIA_POLL_INTERVAL: Duration = Duration::from_secs(2);
const MEDIA_POLL_TRIES: u32 = 15;

/// The file name the upload carries; the instance keeps its own.
const UPLOAD_FILE_NAME: &str = "late-sh.png";

/// How much of an error body a log line keeps.
const BODY_EXCERPT: usize = 200;

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
    /// The public web, for the page the status links to.
    web_url: String,
}

/// `POST /api/v1/statuses` as the instance reads it.
#[derive(Serialize)]
struct StatusBody<'a> {
    status: &'a str,
    media_ids: Vec<&'a str>,
    visibility: &'a str,
    language: &'a str,
}

#[derive(Deserialize)]
struct MediaResponse {
    id: String,
}

#[derive(Deserialize)]
struct StatusResponse {
    id: String,
    url: Option<String>,
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
                instance: config.instance.trim_end_matches('/').to_string(),
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
    /// it ours. Returns whether this replica settled the day (posted,
    /// skipped, or failed trying). `finished_for` memoizes a day this
    /// replica saw finished, so the rest of that day costs nothing.
    pub async fn check(&self, now: DateTime<Utc>, finished_for: &mut Option<NaiveDate>) -> bool {
        if !self.config.enabled {
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
        match self.post_claimed(day, key).await {
            Ok(Outcome::Posted(posted)) => {
                *finished_for = Some(day);
                metrics::record_herald_post(HeraldPostResult::Posted);
                tracing::info!(
                    %day,
                    kind = posted.kind.as_str(),
                    subject = %posted.subject,
                    url = %posted.status_url,
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
    async fn post_claimed(&self, day: NaiveDate, key: Uuid) -> anyhow::Result<Outcome> {
        match self.post_day(day, key).await {
            Ok(Outcome::Posted(posted)) => {
                let client = self.db.get().await?;
                HeraldPost::finish_posted(&client, day, &posted).await?;
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

    /// Pick, fetch, upload, post.
    async fn post_day(&self, day: NaiveDate, key: Uuid) -> anyhow::Result<Outcome> {
        let Some(picture) = self.pick(day).await? else {
            return Ok(Outcome::Skipped);
        };
        let png = self.fetch_picture(&picture).await?;
        let media_id = self.upload_media(png, &state::alt_text(&picture)).await?;
        let status_url = self
            .post_status(key, &state::status_text(&picture, &self.web_url), &media_id)
            .await?;
        Ok(Outcome::Posted(HeraldPosted {
            kind: picture.kind(),
            subject: picture.subject(),
            status_url,
        }))
    }

    /// The rotation's inputs, then the rule.
    async fn pick(&self, day: NaiveDate) -> anyhow::Result<Option<Picture>> {
        let client = self.db.get().await?;
        let last = HeraldPost::last_posted_kind(&client, day).await?;
        let poster = HeraldPost::next_poster_candidate(&client).await?;
        let shown = state::shown_day(day);
        let key = state::board_key(shown);
        let board = match Snapshot::find_by_board_key(&client, &key).await? {
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

    /// `POST /api/v2/media`: the picture with its alt text. The instance
    /// answers 200 with the attachment ready, or 202 while it processes
    /// one, polled until it is.
    async fn upload_media(&self, png: Vec<u8>, alt: &str) -> anyhow::Result<String> {
        let part = reqwest::multipart::Part::bytes(png)
            .file_name(UPLOAD_FILE_NAME)
            .mime_str("image/png")
            .context("building the png part")?;
        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("description", alt.to_string());
        let url = format!("{}/api/v2/media", self.config.instance);
        let response = self
            .http
            .post(&url)
            .bearer_auth(self.token())
            .multipart(form)
            .send_traced()
            .await
            .with_context(|| format!("posting {url}"))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .with_context(|| format!("reading {url}"))?;
        match status.as_u16() {
            200 => Ok(parse_media(&body)?.id),
            202 => {
                let id = parse_media(&body)?.id;
                self.await_media(&id).await?;
                Ok(id)
            }
            _ => bail!("{url} answered {status}: {}", excerpt(&body)),
        }
    }

    /// `GET /api/v1/media/{id}` until the instance answers 200 (206 while
    /// it is still processing).
    async fn await_media(&self, id: &str) -> anyhow::Result<()> {
        let url = format!("{}/api/v1/media/{id}", self.config.instance);
        for _ in 0..MEDIA_POLL_TRIES {
            tokio::time::sleep(MEDIA_POLL_INTERVAL).await;
            let response = self
                .http
                .get(&url)
                .bearer_auth(self.token())
                .send_traced()
                .await
                .with_context(|| format!("getting {url}"))?;
            let status = response.status();
            match status.as_u16() {
                200 => return Ok(()),
                206 => continue,
                _ => {
                    let body = response.text().await.unwrap_or_default();
                    bail!("{url} answered {status}: {}", excerpt(&body));
                }
            }
        }
        bail!("media {id} was still processing after {MEDIA_POLL_TRIES} polls")
    }

    /// `POST /api/v1/statuses` with the day's key as `Idempotency-Key`: a
    /// retry after a lost answer gets the status the instance already made
    /// instead of a second one. The status's public URL comes back.
    async fn post_status(&self, key: Uuid, text: &str, media_id: &str) -> anyhow::Result<String> {
        let url = format!("{}/api/v1/statuses", self.config.instance);
        let body = StatusBody {
            status: text,
            media_ids: vec![media_id],
            visibility: self.config.visibility.as_str(),
            language: "en",
        };
        let response = self
            .http
            .post(&url)
            .bearer_auth(self.token())
            .header("Idempotency-Key", key.to_string())
            .json(&body)
            .send_traced()
            .await
            .with_context(|| format!("posting {url}"))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .with_context(|| format!("reading {url}"))?;
        if !status.is_success() {
            bail!("{url} answered {status}: {}", excerpt(&body));
        }
        let posted: StatusResponse =
            serde_json::from_str(&body).context("decoding the status answer")?;
        match posted.url {
            Some(url) => Ok(url),
            None => bail!("status {} came back without a url", posted.id),
        }
    }

    /// `Config::validate` refuses an enabled herald without a token, and
    /// nothing here runs disabled.
    fn token(&self) -> &str {
        self.config
            .token
            .as_deref()
            .expect("an enabled herald carries a token (Config::validate)")
    }
}

fn parse_media(body: &str) -> anyhow::Result<MediaResponse> {
    serde_json::from_str(body).context("decoding the media answer")
}

fn excerpt(body: &str) -> String {
    body.chars().take(BODY_EXCERPT).collect()
}
