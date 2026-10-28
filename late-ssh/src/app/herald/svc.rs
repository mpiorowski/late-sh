//! The herald: one picture a day on the Fediverse and Bluesky
//! (AUDIENCE.md, "The herald"). `HeraldService` runs on every replica and
//! checks every `HERALD_CHECK_INTERVAL`; once the day's post time
//! (`state::HERALD_POST_TIME`) has passed, the `herald_posts` row for the
//! day is the claim (root CONTEXT.md, multi-replica rule), so one replica
//! posts. A claimed day is: find what it could post (the operator's
//! announcements the edition carries, the week's scoreboard on Sundays,
//! the piece over the door, yesterday's board with `state::BOARD_MIN_INK`
//! drawn cells, the day's paper) and pick one, an announcement first,
//! else by the week's schedule (`state::schedule`, `state::Found::choice`);
//! an announcement is briefed into words (`brief.rs`), a picture is
//! fetched from late-web's PNG route and looked at (`look.rs`: the alt
//! text and the warning); the pick and the look are stored on the row,
//! delivered to every configured network that has no delivery yet
//! (`mastodon.rs`, a note in place of a marked picture; `bluesky.rs`), and
//! the row settled once all have one. A failed day marks
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
use late_core::models::artboard_piece::ArtboardPiece;
use late_core::models::chat_message::ChatMessage;
use late_core::models::chat_room::ChatRoom;
use late_core::models::herald::{
    HeraldDay, HeraldDayStatus, HeraldNetwork, HeraldPick, HeraldPost, HeraldPosted, HeraldSwitch,
};
use late_core::models::leaderboard::{DayTally, day_tallies, week_bests};
use late_core::models::paper::{
    ANNOUNCEMENTS_LIMIT, ANNOUNCEMENTS_SLUG, PaperEdition, PaperSectionKind,
};
use tokio::sync::broadcast;
use tracing::Instrument;
use uuid::Uuid;

use super::state::{
    self, AnnouncementCandidate, AnnouncementFound, BoardFound, BoardInk, Brief, Choice, Found,
    HeraldCommand, Look, MastodonPost, PaperCandidate, Picture, PieceCandidate,
    ScoreboardCandidate, Words,
};
use super::{bluesky, brief, look, mastodon};
use crate::app::ai::svc::AiService;
use crate::app::common::primitives::Banner;
use crate::app::state::App;
use crate::config::HeraldConfig;
use crate::metrics::{
    self, HeraldBriefResult, HeraldDeliveryResult, HeraldLookResult, HeraldPostResult,
};

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
pub(crate) enum Outcome {
    /// Every network has the picture, marked as `look` says.
    Posted { posted: HeraldPosted, look: Look },
    /// Nothing worth posting: no kind had anything.
    Skipped,
}

/// The day's pick, ready to deliver: the picture, what the look saw in
/// it, and what goes up with the words.
struct Pick {
    picture: Picture,
    look: Look,
    media: Media,
}

/// What a post carries besides its words.
enum Media {
    Png {
        png: Vec<u8>,
        alt: String,
    },
    /// An announcement: the words are the post, with the site's picture
    /// as the thumb of its link card where a network wants one.
    Words {
        thumb: Vec<u8>,
    },
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
    /// What `/herald post` would send today, and where: what the day
    /// could post and, when it is a post, the day's pick, briefed or with
    /// a fresh look at it. `found` is boxed: it carries every kind's
    /// candidate and would otherwise size the whole enum.
    Preview {
        day: NaiveDate,
        found: Box<Found>,
        post: Option<(Picture, Look)>,
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
    /// The look's model.
    ai: AiService,
    http: reqwest::Client,
    config: HeraldConfig,
    /// The public web, for the page the post links to.
    web_url: String,
    event_tx: broadcast::Sender<HeraldEvent>,
}

impl HeraldService {
    pub fn new(db: Db, ai: AiService, config: HeraldConfig, web_url: String) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(HTTP_TIMEOUT)
            .build()
            .expect("reqwest client with a user agent and a timeout builds");
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAP);
        Self {
            db,
            ai,
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
                    let found = self.find(today).await?;
                    let post = match self.choose(&found, today).await {
                        Some(picture) => {
                            let (look, _) = self.look_and_media(&picture).await?;
                            Some((picture, look))
                        }
                        None => None,
                    };
                    Ok(CommandOutcome::Preview {
                        day: today,
                        found: Box::new(found),
                        post,
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
            Ok(Outcome::Posted { posted, look }) => {
                metrics::record_herald_post(HeraldPostResult::Posted);
                tracing::info!(
                    %day,
                    kind = posted.kind.as_str(),
                    subject = %posted.subject,
                    warning = look.as_str(),
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
            Ok(Outcome::Posted { posted, look }) => {
                let client = self.db.get().await?;
                HeraldPost::finish_posted(&client, day).await?;
                Ok(Outcome::Posted { posted, look })
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

    /// Pick (or reuse the stored pick), deliver to every network still
    /// missing, each marked as the look says. Each network's failure is
    /// logged and counted here, once; the day fails when any did, with the
    /// ones that landed kept.
    async fn post_day(
        &self,
        day: NaiveDate,
        key: Uuid,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Outcome> {
        let Some(Pick {
            picture,
            look,
            media,
        }) = self.pick_for(day).await?
        else {
            return Ok(Outcome::Skipped);
        };
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
            let (text, body) = match (state::mastodon_post(&look), &media) {
                (MastodonPost::Picture, Media::Png { png, alt }) => (
                    state::status_text(&picture, &self.web_url, Words::Plain),
                    mastodon::Body::Picture { png, alt },
                ),
                (MastodonPost::Picture, Media::Words { .. }) => (
                    state::status_text(&picture, &self.web_url, Words::Plain),
                    mastodon::Body::Words,
                ),
                (MastodonPost::Note { warning }, Media::Png { .. } | Media::Words { .. }) => (
                    state::note_text(&picture, &self.web_url),
                    mastodon::Body::Note { warning },
                ),
            };
            let sent = mastodon::deliver(
                &self.http,
                config,
                mastodon::Post {
                    key,
                    text: &text,
                    body,
                },
            )
            .await;
            failed += self.note_delivery(day, HeraldNetwork::Mastodon, sent).await;
        }
        if let Some(config) = &self.config.bluesky
            && !delivered.contains(&HeraldNetwork::Bluesky)
        {
            let mark = state::bluesky_mark(&look);
            let text = state::status_text(&picture, &self.web_url, mark.words);
            let card = match &picture {
                Picture::Announcement(announcement) => Some(announcement.link_card(&self.web_url)),
                Picture::Board(_)
                | Picture::Piece(_)
                | Picture::Scoreboard(_)
                | Picture::Paper(_) => None,
            };
            let embed = match (&media, &card) {
                (Media::Png { png, alt }, _) => bluesky::Embed::Picture { png, alt },
                (Media::Words { thumb }, Some(card)) => bluesky::Embed::Card {
                    uri: &card.uri,
                    title: &card.title,
                    description: &card.description,
                    thumb,
                },
                (Media::Words { .. }, None) => bail!("words alone come with a link card"),
            };
            let sent = bluesky::deliver(
                &self.http,
                config,
                bluesky::Post {
                    key,
                    embed,
                    text: &text,
                    label: mark.label,
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
        Ok(Outcome::Posted {
            posted: HeraldPosted {
                kind: picture.kind(),
                subject: picture.subject(),
                deliveries: HeraldPost::deliveries(&client, day).await?,
            },
            look,
        })
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

    /// The day's pick with its picture fetched: the one an earlier claim
    /// stored, look and all, else a fresh pick briefed or looked at and
    /// stored now, before the first network call. `None` is a day with
    /// nothing worth posting.
    async fn pick_for(&self, day: NaiveDate) -> anyhow::Result<Option<Pick>> {
        let stored = {
            let client = self.db.get().await?;
            HeraldPost::stored_pick(&client, day).await?
        };
        if let Some(stored) = stored {
            let picture: Picture =
                serde_json::from_value(stored.picture).context("decoding the stored picture")?;
            let look: Look =
                serde_json::from_value(stored.look).context("decoding the stored look")?;
            let media = self.fetch_media(&picture, &look).await?;
            return Ok(Some(Pick {
                picture,
                look,
                media,
            }));
        }
        let found = self.find(day).await?;
        let Some(picture) = self.choose(&found, day).await else {
            return Ok(None);
        };
        let (look, media) = self.look_and_media(&picture).await?;
        let stored = HeraldPick {
            picture: serde_json::to_value(&picture).context("encoding the picture")?,
            look: serde_json::to_value(&look).context("encoding the look")?,
        };
        let client = self.db.get().await?;
        HeraldPost::store_pick(&client, day, picture.kind(), &picture.subject(), &stored).await?;
        Ok(Some(Pick {
            picture,
            look,
            media,
        }))
    }

    /// The day's post from what was found: an announcement briefed into
    /// its words, else the schedule's picture.
    async fn choose(&self, found: &Found, day: NaiveDate) -> Option<Picture> {
        match found.choice(day)? {
            Choice::Picture(picture) => Some(picture),
            Choice::Announcement(found) => Some(Picture::Announcement(self.brief(&found).await)),
        }
    }

    /// The words of an announcement, for a fresh pick or an admin's
    /// preview: the model's, counted here, or the note as written and
    /// cut when AI is off or the model failed, the failure logged here,
    /// once. The post goes out either way.
    async fn brief(&self, found: &AnnouncementFound) -> AnnouncementCandidate {
        let (words, brief, result) = match brief::brief(&self.ai, &found.text).await {
            Ok(Some(words)) => (words, Brief::Model, HeraldBriefResult::Model),
            Ok(None) => (found.text.clone(), Brief::Cut, HeraldBriefResult::AsWritten),
            Err(error) => {
                late_core::error_span!(
                    "herald_brief_failed",
                    error = ?error,
                    edition = %found.edition,
                    "the herald's brief failed; posting the note as written, cut"
                );
                (found.text.clone(), Brief::Cut, HeraldBriefResult::Failed)
            }
        };
        metrics::record_herald_brief(result);
        AnnouncementCandidate::of(found.edition, &words, brief, found.summary.clone())
    }

    /// A fresh pick's look and media: the picture fetched and looked at,
    /// or words alone, drawn, with the site's picture for their card.
    async fn look_and_media(&self, picture: &Picture) -> anyhow::Result<(Look, Media)> {
        match picture.picture_path() {
            Some(path) => {
                let png = self.fetch_picture(&path).await?;
                let look = self.look_at(picture, &png).await;
                let Some(alt) = state::alt_text(picture, &look) else {
                    bail!("a picture and its alt text come together");
                };
                Ok((look, Media::Png { png, alt }))
            }
            None => {
                let thumb = self.fetch_picture(state::SITE_IMAGE_PATH).await?;
                Ok((Look::Drawn, Media::Words { thumb }))
            }
        }
    }

    /// A stored pick's media: the picture fetched again, its alt text from
    /// the stored look, or words alone with the site's picture.
    async fn fetch_media(&self, picture: &Picture, look: &Look) -> anyhow::Result<Media> {
        match (picture.picture_path(), state::alt_text(picture, look)) {
            (Some(path), Some(alt)) => Ok(Media::Png {
                png: self.fetch_picture(&path).await?,
                alt,
            }),
            (None, None) => Ok(Media::Words {
                thumb: self.fetch_picture(state::SITE_IMAGE_PATH).await?,
            }),
            (Some(_), None) | (None, Some(_)) => {
                bail!("a picture and its alt text come together")
            }
        }
    }

    /// The look at a picture, for a fresh pick or an admin's preview:
    /// counted here, and a failure logged here, once, and taken as
    /// `Unseen` so the post still goes out, marked not reviewed.
    async fn look_at(&self, picture: &Picture, png: &[u8]) -> Look {
        let look = match look::look(&self.ai, picture, png).await {
            Ok(look) => look,
            Err(error) => {
                late_core::error_span!(
                    "herald_look_failed",
                    error = ?error,
                    subject = %picture.subject(),
                    "the herald's look failed; posting not reviewed"
                );
                Look::Unseen
            }
        };
        metrics::record_herald_look(match &look {
            Look::Seen { warning: None, .. } => HeraldLookResult::Clean,
            Look::Seen {
                warning: Some(_), ..
            } => HeraldLookResult::Warned,
            Look::Unseen => HeraldLookResult::Unseen,
            Look::Drawn => HeraldLookResult::Drawn,
        });
        look
    }

    /// Everything the day could post, one read per kind; the day's choice
    /// (`state::Found::choice`) picks.
    async fn find(&self, day: NaiveDate) -> anyhow::Result<Found> {
        let client = self.db.get().await?;
        let scoreboard = match state::scoreboard_week(day) {
            Some(last) => ScoreboardCandidate::of(last, &week_bests(&client, last).await?),
            None => None,
        };
        let issue = PaperEdition::load(&client, day).await?;
        let paper = Self::find_paper(&client, &issue).await?;
        let summary = paper
            .as_ref()
            .and_then(|paper| paper.outside.clone().or_else(|| paper.reading.clone()));
        Ok(Found {
            announcement: Self::find_announcement(&client, &issue, summary).await?,
            scoreboard,
            piece: self.find_piece(&client, day).await?,
            board: self.find_board(&client, day).await?,
            paper,
        })
    }

    /// The operator's `#announcements` over the edition's window, as
    /// written, oldest first, once the press has swept the edition (the
    /// same line the paper draws, `app/paper`): the post goes out the day
    /// the note is on the edition's page. No room, or nothing said, is
    /// `None`. `summary` is the edition's first line for the link card.
    async fn find_announcement(
        client: &tokio_postgres::Client,
        issue: &PaperEdition,
        summary: Option<String>,
    ) -> anyhow::Result<Option<AnnouncementFound>> {
        if !issue.is_swept() {
            return Ok(None);
        }
        let Some(room) = ChatRoom::find_public_non_dm_by_slug(client, ANNOUNCEMENTS_SLUG).await?
        else {
            return Ok(None);
        };
        let (floor, ceiling) = PaperEdition::window(issue.edition);
        let posts = ChatMessage::list_public_room_between_with_author(
            client,
            room.id,
            floor,
            ceiling,
            ANNOUNCEMENTS_LIMIT,
        )
        .await?;
        if posts.is_empty() {
            return Ok(None);
        }
        Ok(Some(AnnouncementFound {
            edition: issue.edition,
            summary,
            text: posts
                .iter()
                .map(|post| post.body.as_str())
                .collect::<Vec<_>>()
                .join("\n\n"),
        }))
    }

    /// The piece over the door on `day`: the splash wall's piece for it
    /// (`app/artboard/gallery`), claimed through the wall's own claim when
    /// the gallery's hourly refresh has not yet, so the post and the login
    /// screen show one piece. The wall is one queue in hang order, a day
    /// each, so nothing here remembers what went out.
    async fn find_piece(
        &self,
        client: &late_core::db::PooledClient,
        day: NaiveDate,
    ) -> anyhow::Result<Option<PieceCandidate>> {
        Ok(ArtboardPiece::splash_for_day(client, day)
            .await?
            .map(|piece| PieceCandidate {
                id: piece.id,
                title: piece.title,
                artist: piece.username,
            }))
    }

    /// The edition dated the day, which reads the day before, once a
    /// public column of it printed, with the covered day's most played
    /// arcade game (the card's headline, read by the same rule late-web
    /// draws it with).
    async fn find_paper(
        client: &tokio_postgres::Client,
        issue: &PaperEdition,
    ) -> anyhow::Result<Option<PaperCandidate>> {
        let covered = PaperEdition::window(issue.edition).0.date_naive();
        let tallies = day_tallies(client, covered).await?;
        Ok(PaperCandidate::of(
            issue.edition,
            DayTally::most_played_arcade(&tallies),
            issue.ready_text(PaperSectionKind::Yesterday),
            issue.ready_text(PaperSectionKind::Reading),
            issue.ready_text(PaperSectionKind::Outside),
        ))
    }

    /// Yesterday's board as the rollover archived it, with its ink counted
    /// and located; `state::BoardFound::picture` says whether it is a post.
    async fn find_board(
        &self,
        client: &tokio_postgres::Client,
        day: NaiveDate,
    ) -> anyhow::Result<BoardFound> {
        let shown = state::shown_day(day);
        let key = Snapshot::daily_key(shown);
        let ink = match Snapshot::find_by_board_key(client, &key).await? {
            Some(snapshot) => {
                let canvas: dartboard_core::Canvas = serde_json::from_value(snapshot.canvas)
                    .with_context(|| format!("decoding artboard snapshot {key}"))?;
                Some(BoardInk::of(&canvas))
            }
            None => None,
        };
        Ok(BoardFound { key, shown, ink })
    }

    /// The picture from late-web's PNG route, as bytes. Anything but a
    /// PNG is an error: a 404 (a piece taken down since its pick), or the
    /// redirect to the landing page an older late-web answers for a route
    /// it does not have yet, would otherwise go out as the picture.
    async fn fetch_picture(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        use late_core::telemetry::TracedExt;
        let url = format!("{}{path}", self.config.web_internal_url);
        let response = self
            .http
            .get(&url)
            .send_traced()
            .await
            .with_context(|| format!("getting {url}"))?
            .error_for_status()
            .with_context(|| format!("getting {url}"))?;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        match content_type.as_deref() {
            Some("image/png") => {}
            other => bail!("{url} answered {other:?}, not a png"),
        }
        let bytes = response
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

/// What the look said, in one clause, for `/herald preview`.
fn describe_look(look: &Look) -> String {
    match look {
        Look::Seen { warning: None, .. } => "clean".to_string(),
        Look::Seen {
            warning: Some(warning),
            reason,
            ..
        } => format!("warned {} ({reason})", warning.as_str()),
        Look::Unseen => "not reviewed (the look failed or AI is off)".to_string(),
        Look::Drawn => "drawn by late.sh from its own words".to_string(),
    }
}

/// What Mastodon gets, in a clause, for `/herald preview`.
fn describe_mastodon(look: &Look) -> &'static str {
    match state::mastodon_post(look) {
        MastodonPost::Picture => "the picture on Mastodon",
        MastodonPost::Note { .. } => "a note in place of the picture on Mastodon",
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
            found,
            post,
            networks,
        } => match post {
            Some((picture, look)) => {
                let shown = match &picture {
                    Picture::Announcement(announcement) => format!(
                        "Words ({}): {}",
                        announcement.brief.as_str(),
                        announcement.text
                    ),
                    Picture::Board(_)
                    | Picture::Piece(_)
                    | Picture::Scoreboard(_)
                    | Picture::Paper(_) => format!(
                        "Alt: {}",
                        state::alt_text(&picture, &look).expect("a picture has alt text")
                    ),
                };
                Banner::info(&format!(
                    "Would post {} to {} for {day}, {}, {}. {shown}",
                    picture.subject(),
                    names(&networks),
                    describe_look(&look),
                    describe_mastodon(&look),
                ))
            }
            None => Banner::info(&format!("Nothing to post for {day}: {}", found.describe())),
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
