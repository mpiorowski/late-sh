use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::Result;
use chrono::{Datelike, Months, NaiveDate, Utc};
use late_core::db::Db;
use late_core::models::leaderboard::{
    LeaderboardData, OnlineTimeIncrement, apply_online_time_batch, fetch_leaderboard_data,
};
use late_core::models::profile_award::{
    AwardRoll, AwardRollEntry, CROWN_AWARD_CATEGORY, GALLERY_AWARD_CATEGORY,
    LATE_TIME_AWARD_CATEGORY, TOP_DRINKERS_AWARD_CATEGORY, all_award_categories,
    award_category_label,
    claim_previous_month_award_announcement, format_score_value, gallery_prize_chips,
    is_milestone_award, snapshot_previous_month_profile_awards,
};
use late_core::models::user::User;
use tokio::sync::{Notify, watch};
use uuid::Uuid;

use crate::app::activity::lounge::SYSTEM_FINGERPRINT;
use crate::app::chat::svc::ChatService;
use crate::app::common::primitives::thousands;
use crate::app::common::username_effect::CROWN_GLYPH;
use crate::metrics;

/// How often the leaderboard is rebuilt from the DB while at least one session
/// is watching it.
///
/// `fetch_leaderboard_data` is fifteen aggregate queries (several UNION ALL
/// over every game's win/score tables; the all-time windows read O(players)
/// sources, the `daily_win_totals` rollup, the legacy best-score tables, and
/// the one-row-per-player `mud_characters` blobs, so no query scans full
/// history), and it is a timer, not a reaction to
/// anything a user did. At the old 30 s cadence a previous shape of this pass
/// was 13% of all database execution time in prod (2026-07-26
/// `pg_stat_statements` ranking, SCALE.md). These standings tolerate minutes
/// of staleness. The one
/// latency-sensitive consumer, the per-session chip balance read in
/// `app/tick.rs`, does not wait for this loop: chip mutations notify
/// `chip_user_changed` and `ShopService` pushes the new balance per user.
///
/// It doubles as the staleness bound for the connect-triggered refresh in
/// `start_refresh_loop`: a session that connects to a snapshot older than this
/// buys one pass, so widening the interval also widens what a returning user is
/// willing to look at before the server rebuilds it.
const REFRESH_INTERVAL: Duration = Duration::from_secs(300);

/// How often the award snapshot loop checks whether a pass is due. The check
/// itself is a clock read; the DB pass runs only on a UTC month rollover or
/// the fallback below, so this cadence bounds how long freshly-completed
/// monthly badges stay missing after the 1st, not how often we query.
const AWARD_SNAPSHOT_CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// Fallback cadence for the award snapshot when no rollover happened. Existing
/// rows are frozen, so this pass is a cheap re-confirmation kept as a safety
/// net against a missed rollover check (clock skew, a long stall).
const AWARD_SNAPSHOT_FALLBACK: Duration = Duration::from_secs(24 * 60 * 60);

/// How a pass's #lounge award roll ended, for the metric. A pass whose month
/// was already announced records nothing: that is every pass but one a month.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AwardAnnouncementOutcome {
    /// Claimed and posted to #lounge.
    Posted,
    /// The system user lookup or the claim errored; nothing was claimed, so
    /// the next hourly tick retries.
    ClaimFailed,
    /// Claimed, but the post to #lounge errored. The claim is spent, so
    /// nobody retries and the month's roll is lost.
    PostFailed,
}

#[derive(Clone)]
pub struct LeaderboardService {
    db: Db,
    data_tx: Arc<watch::Sender<Arc<LeaderboardData>>>,
    /// Woken by `subscribe`, so the refresh loop can top up a snapshot that
    /// aged out while nobody was connected. See `start_refresh_loop`.
    connected: Arc<Notify>,
    /// When the last successful refresh published, or `None` before the first
    /// one. Read by `snapshot_age`; never held across an await.
    last_refresh: Arc<Mutex<Option<Instant>>>,
    online_time: OnlineTimeTracker,
    flush_lock: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Clone, Debug)]
struct OnlineTimeBatch {
    id: Uuid,
    increments: Vec<OnlineTimeIncrement>,
}

struct PreparedOnlineTimeBatch {
    batch: OnlineTimeBatch,
    was_retry: bool,
}

#[derive(Default)]
struct OnlineTimeState {
    /// The first authenticated connection's start time. The shared
    /// `active_users` map owns connection ref-counting; this map only sees its
    /// zero-to-one and one-to-zero transitions.
    active_since: HashMap<Uuid, ActiveOnlineTime>,
    pending_milliseconds: HashMap<(Uuid, NaiveDate), i64>,
    /// Retained until Postgres confirms it. A retry uses the same id so the
    /// upsert can distinguish it from a new increment.
    in_flight: Option<OnlineTimeBatch>,
}

#[derive(Clone, Copy)]
struct ActiveOnlineTime {
    since: Instant,
    month_start: NaiveDate,
}

#[derive(Clone, Default)]
struct OnlineTimeTracker {
    state: Arc<Mutex<OnlineTimeState>>,
}

impl OnlineTimeTracker {
    fn connected(&self, user_id: Uuid) {
        self.connected_at(user_id, Instant::now(), current_month_start());
    }

    fn connected_at(&self, user_id: Uuid, now: Instant, month_start: NaiveDate) {
        self.state
            .lock()
            .expect("online time tracker poisoned")
            .active_since
            .entry(user_id)
            .or_insert(ActiveOnlineTime {
                since: now,
                month_start,
            });
    }

    fn disconnected(&self, user_id: Uuid) {
        self.disconnected_at(user_id, Instant::now());
    }

    fn disconnected_at(&self, user_id: Uuid, now: Instant) {
        let mut state = self.state.lock().expect("online time tracker poisoned");
        let Some(active) = state.active_since.remove(&user_id) else {
            return;
        };
        add_elapsed(
            &mut state.pending_milliseconds,
            user_id,
            active.month_start,
            active.since,
            now,
        );
    }

    fn begin_batch(&self) -> Option<PreparedOnlineTimeBatch> {
        self.begin_batch_at(Instant::now(), current_month_start())
    }

    fn begin_batch_at(
        &self,
        now: Instant,
        month_start: NaiveDate,
    ) -> Option<PreparedOnlineTimeBatch> {
        let mut state = self.state.lock().expect("online time tracker poisoned");
        if let Some(batch) = state.in_flight.clone() {
            return Some(PreparedOnlineTimeBatch {
                batch,
                was_retry: true,
            });
        }

        let mut checkpoints = Vec::with_capacity(state.active_since.len());
        for (&user_id, active) in &mut state.active_since {
            checkpoints.push((user_id, *active));
            *active = ActiveOnlineTime {
                since: now,
                month_start,
            };
        }
        for (user_id, active) in checkpoints {
            add_elapsed(
                &mut state.pending_milliseconds,
                user_id,
                active.month_start,
                active.since,
                now,
            );
        }

        let pending = std::mem::take(&mut state.pending_milliseconds);
        let mut increments: Vec<_> = pending
            .into_iter()
            .filter_map(|((user_id, month_start), milliseconds)| {
                (milliseconds > 0).then_some(OnlineTimeIncrement {
                    user_id,
                    month_start,
                    milliseconds,
                })
            })
            .collect();
        if increments.is_empty() {
            return None;
        }
        increments.sort_unstable_by_key(|value| (value.user_id, value.month_start));

        let batch = OnlineTimeBatch {
            id: Uuid::now_v7(),
            increments,
        };
        state.in_flight = Some(batch.clone());
        Some(PreparedOnlineTimeBatch {
            batch,
            was_retry: false,
        })
    }

    fn acknowledge(&self, flush_id: Uuid) {
        let mut state = self.state.lock().expect("online time tracker poisoned");
        if state
            .in_flight
            .as_ref()
            .is_some_and(|batch| batch.id == flush_id)
        {
            state.in_flight = None;
        }
    }

    #[cfg(test)]
    fn is_active(&self, user_id: Uuid) -> bool {
        self.state
            .lock()
            .expect("online time tracker poisoned")
            .active_since
            .contains_key(&user_id)
    }
}

fn current_month_start() -> NaiveDate {
    let today = Utc::now().date_naive();
    today.with_day(1).expect("every UTC month has a first day")
}

fn add_elapsed(
    pending: &mut HashMap<(Uuid, NaiveDate), i64>,
    user_id: Uuid,
    month_start: NaiveDate,
    since: Instant,
    now: Instant,
) {
    let milliseconds = now.saturating_duration_since(since).as_millis();
    let milliseconds = i64::try_from(milliseconds).unwrap_or(i64::MAX);
    if milliseconds > 0 {
        let total = pending.entry((user_id, month_start)).or_default();
        *total = total.saturating_add(milliseconds);
    }
}

/// What woke the refresh loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wake {
    Timer,
    Connect,
}

/// The refresh loop's entire decision, extracted so it is testable without a
/// runtime, a DB, or a clock.
///
/// `age` is how long ago the last successful refresh published, `None` before
/// the first one.
fn should_refresh(wake: Wake, has_subscribers: bool, age: Option<Duration>) -> bool {
    // A refresh nobody is watching is fifteen aggregate queries published to
    // nobody, whatever woke us.
    if !has_subscribers {
        return false;
    }
    match wake {
        Wake::Timer => true,
        // A connect only earns a pass when the snapshot it just seeded from has
        // aged out. Without this bound every session connecting to a busy server
        // would run the pass, which is exactly the timer cost the 300s interval
        // was chosen to cut.
        Wake::Connect => age.is_none_or(|age| age >= REFRESH_INTERVAL),
    }
}

/// The award snapshot loop's entire decision, extracted so it is testable
/// without a runtime, a DB, or a clock.
///
/// `target_month` is the previous completed UTC month a pass would write rows
/// for; `last_run` is the month the last successful pass wrote and how long
/// ago it ran, `None` before the first one.
fn should_snapshot_awards(
    target_month: NaiveDate,
    last_run: Option<(NaiveDate, Duration)>,
) -> bool {
    match last_run {
        // Startup: run immediately, so a restart doubles as manual catch-up.
        None => true,
        // The month rolled over since the last pass. The rows the chat-label
        // query now filters on do not exist until this pass writes them, so
        // monthly badges are blank for everyone until it runs.
        Some((month, _)) if month != target_month => true,
        // Same month already written: rows are frozen, a pass is a no-op.
        Some((_, elapsed)) => elapsed >= AWARD_SNAPSHOT_FALLBACK,
    }
}

/// A month's roll this process won the claim for, with the author to post
/// it as.
struct ClaimedAwardRoll {
    system_user_id: Uuid,
    roll: AwardRoll,
}

/// The #lounge roll call for a settled month: a headline, one line per board
/// in badge order with its placements in rank order, and a sign-off. Every
/// winner is an @mention, so the roll reaches the people on it even when they
/// slept through the rollover. Boards nobody placed on are left out.
fn award_roll_body(roll: &AwardRoll) -> String {
    let wearing_month = roll
        .period_month
        .checked_add_months(Months::new(1))
        .expect("the month after a settled month exists");
    let mut lines = vec![format!(
        "\u{1F3C6} The {} awards are in! Winners wear their badges all {}.",
        roll.period_month.format("%B %Y"),
        wearing_month.format("%B")
    )];
    for category in all_award_categories() {
        if is_milestone_award(category) {
            continue;
        }
        let placements: Vec<String> = roll
            .entries
            .iter()
            .filter(|entry| entry.category == category)
            .map(award_roll_placement)
            .collect();
        if placements.is_empty() {
            continue;
        }
        lines.push(format!(
            "{}: {}",
            award_category_label(category),
            placements.join(" \u{00B7} ")
        ));
    }
    lines.push("Congrats! Full standings on the Leaderboards page.".to_string());
    lines.join("\n")
}

/// One winner on a board's line. Nothing follows a name directly but a
/// space, so the mention parser takes the whole name and only the name.
fn award_roll_placement(entry: &AwardRollEntry) -> String {
    let name = &entry.username;
    let value = entry.score_value;
    let medal = award_roll_medal(entry.rank);
    match entry.category.as_str() {
        CROWN_AWARD_CATEGORY => {
            format!(
                "{CROWN_GLYPH} @{name} took it last for {} chips",
                thousands(value)
            )
        }
        LATE_TIME_AWARD_CATEGORY => format!(
            "\u{1F319} @{name} with {}",
            format_score_value(LATE_TIME_AWARD_CATEGORY, value)
        ),
        TOP_DRINKERS_AWARD_CATEGORY => {
            format!("\u{1F37A} @{name} with {} buzz", thousands(value))
        }
        GALLERY_AWARD_CATEGORY => match gallery_prize_chips(entry.rank) {
            Some(prize) => format!(
                "{medal} @{name} {} applause (+{} chips)",
                thousands(value),
                thousands(prize)
            ),
            None => format!("{medal} @{name} {} applause", thousands(value)),
        },
        "top_chips" => format!("{medal} @{name} {} chips", thousands(value)),
        "arcade_wins" => format!("{medal} @{name} {} pts", thousands(value)),
        // The score boards: Lateris, 2048, Snake. A bare best score.
        _ => format!("{medal} @{name} {}", thousands(value)),
    }
}

fn award_roll_medal(rank: i32) -> String {
    match rank {
        1 => "\u{1F947}".to_string(),
        2 => "\u{1F948}".to_string(),
        3 => "\u{1F949}".to_string(),
        // Migration 081 caps placements at three; a deeper row still reads.
        rank => format!("#{rank}"),
    }
}

/// First day of the UTC month before the one containing `today`: the month
/// `snapshot_previous_month_profile_awards` writes rows for.
fn previous_utc_month(today: NaiveDate) -> NaiveDate {
    let first = today.with_day(1).expect("every month has a day 1");
    let last_of_previous = first.pred_opt().expect("date has a predecessor");
    last_of_previous
        .with_day(1)
        .expect("every month has a day 1")
}

impl LeaderboardService {
    pub fn new(db: Db) -> Self {
        let (tx, _) = watch::channel(Arc::new(LeaderboardData::default()));
        Self {
            db,
            data_tx: Arc::new(tx),
            connected: Arc::new(Notify::new()),
            last_refresh: Arc::new(Mutex::new(None)),
            online_time: OnlineTimeTracker::default(),
            flush_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Starts one user's connected interval. Call only when the shared human
    /// presence map transitions from zero connections to one.
    pub fn online_user_connected(&self, user_id: Uuid) {
        self.online_time.connected(user_id);
    }

    /// Stops one user's connected interval. Call only when the shared human
    /// presence map transitions from one connection to zero.
    pub fn online_user_disconnected(&self, user_id: Uuid) {
        self.online_time.disconnected(user_id);
    }

    #[cfg(test)]
    pub(crate) fn online_user_is_active(&self, user_id: Uuid) -> bool {
        self.online_time.is_active(user_id)
    }

    /// Hands a session the current snapshot and wakes the refresh loop.
    ///
    /// The wake matters because the loop skips its pass whenever nobody is
    /// subscribed: after a quiet stretch the published snapshot is as old as the
    /// last session's departure, and the caller seeds from it immediately (see
    /// `App::new`). `should_refresh` re-checks the age before spending a pass,
    /// so a connect storm into a warm process costs one wake each, zero queries.
    pub fn subscribe(&self) -> watch::Receiver<Arc<LeaderboardData>> {
        let rx = self.data_tx.subscribe();
        self.connected.notify_one();
        rx
    }

    /// How long ago the last successful refresh published, `None` before the
    /// first one.
    fn snapshot_age(&self) -> Option<Duration> {
        self.last_refresh
            .lock()
            .expect("last_refresh poisoned")
            .map(|at| at.elapsed())
    }

    /// Whether any session is currently watching the leaderboard. Every SSH
    /// session subscribes at bootstrap, so this is "is anyone connected". A
    /// refresh with no subscribers is fifteen aggregate queries published to
    /// nobody, so the loop skips it.
    fn has_subscribers(&self) -> bool {
        self.data_tx.receiver_count() > 0
    }

    pub async fn refresh(&self) -> Result<()> {
        let client = self.db.get().await?;
        let data = fetch_leaderboard_data(&client).await?;
        self.publish(data);
        *self.last_refresh.lock().expect("last_refresh poisoned") = Some(Instant::now());
        Ok(())
    }

    /// Checkpoints every active user and persists all completed intervals in
    /// one statement. The steady-state path writes at most once per five-minute
    /// tick; after an earlier uncertain failure it may first retry that retained
    /// batch and then write the time accumulated since it was prepared.
    pub async fn flush_online_time(&self) -> Result<()> {
        let _flush_guard = self.flush_lock.lock().await;
        let Some(prepared) = self.online_time.begin_batch() else {
            return Ok(());
        };
        let retrying = prepared.was_retry;
        let client = self.db.get().await?;
        self.apply_online_time_batch(&client, prepared.batch)
            .await?;

        if retrying && let Some(follow_up) = self.online_time.begin_batch() {
            self.apply_online_time_batch(&client, follow_up.batch)
                .await?;
        }
        Ok(())
    }

    async fn apply_online_time_batch(
        &self,
        client: &tokio_postgres::Client,
        batch: OnlineTimeBatch,
    ) -> Result<()> {
        let rows = batch.increments.len();
        let milliseconds: i64 = batch
            .increments
            .iter()
            .map(|value| value.milliseconds)
            .sum();
        apply_online_time_batch(client, batch.id, &batch.increments).await?;
        self.online_time.acknowledge(batch.id);
        tracing::debug!(rows, milliseconds, "flushed online time");
        Ok(())
    }

    fn publish(&self, data: LeaderboardData) {
        // The initial refresh runs before any SSH session may have subscribed.
        // `send` discards the value when there are no receivers; `send_replace`
        // retains it so the first session can seed from the warm snapshot.
        self.data_tx.send_replace(Arc::new(data));
    }

    pub fn start_refresh_loop(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            if let Err(e) = self.refresh().await {
                tracing::error!(error = ?e, "initial leaderboard refresh failed");
            }
            let mut interval = tokio::time::interval(REFRESH_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            interval.tick().await;
            loop {
                // Two ways to wake: the timer, or a session connecting. Both
                // `Interval::tick` and `Notify::notified` are cancel-safe, so
                // the losing branch loses nothing.
                let wake = tokio::select! {
                    _ = interval.tick() => Wake::Timer,
                    _ = self.connected.notified() => Wake::Connect,
                };
                if wake == Wake::Timer {
                    match self.flush_online_time().await {
                        Ok(()) => {
                            crate::metrics::record_online_time_flush(
                                crate::metrics::OnlineTimeFlushResult::Flushed,
                            );
                        }
                        Err(e) => {
                            crate::metrics::record_online_time_flush(
                                crate::metrics::OnlineTimeFlushResult::Failed,
                            );
                            tracing::warn!(error = ?e, "online time flush failed");
                        }
                    }
                }
                if !should_refresh(wake, self.has_subscribers(), self.snapshot_age()) {
                    tracing::debug!(?wake, "skipping leaderboard refresh");
                    continue;
                }
                if let Err(e) = self.refresh().await {
                    tracing::warn!(error = ?e, "leaderboard refresh failed");
                }
            }
        })
    }

    /// The monthly award loop: snapshot last month's placements, then post
    /// the roll to #lounge as the `system` user. `chat` is required because
    /// the roll is part of the rollover, not an optional extra.
    pub fn start_profile_award_snapshot_loop(
        self,
        chat: ChatService,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut last_run: Option<(NaiveDate, Instant)> = None;
            let mut interval = tokio::time::interval(AWARD_SNAPSHOT_CHECK_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                // The first tick completes immediately, so startup snapshots.
                interval.tick().await;
                let target_month = previous_utc_month(Utc::now().date_naive());
                let due = should_snapshot_awards(
                    target_month,
                    last_run.map(|(month, at)| (month, at.elapsed())),
                );
                if !due {
                    continue;
                }
                match self.snapshot_profile_awards().await {
                    Ok(()) => {}
                    // `last_run` is untouched, so the next hourly tick retries
                    // instead of waiting out the fallback.
                    Err(e) => {
                        tracing::warn!(error = ?e, "profile award snapshot failed");
                        continue;
                    }
                }
                match self.announce_award_roll().await {
                    Ok(None) => last_run = Some((target_month, Instant::now())),
                    Ok(Some(claimed)) => {
                        // The month is claimed whatever the post does, so the
                        // pass is done either way: a retry could not post it.
                        last_run = Some((target_month, Instant::now()));
                        let posted = chat
                            .send_lounge_message(
                                claimed.system_user_id,
                                award_roll_body(&claimed.roll),
                                true,
                            )
                            .await;
                        match posted {
                            Ok(()) => {
                                metrics::record_award_announcement(
                                    AwardAnnouncementOutcome::Posted,
                                );
                                tracing::info!(
                                    period_month = %claimed.roll.period_month,
                                    placements = claimed.roll.entries.len(),
                                    "monthly award roll posted to lounge"
                                );
                            }
                            Err(e) => {
                                metrics::record_award_announcement(
                                    AwardAnnouncementOutcome::PostFailed,
                                );
                                tracing::error!(
                                    error = ?e,
                                    period_month = %claimed.roll.period_month,
                                    "monthly award roll claimed but not posted"
                                );
                            }
                        }
                    }
                    // Unclaimed, so the next hourly tick retries the roll; the
                    // snapshot above is idempotent and re-runs harmlessly.
                    Err(e) => {
                        metrics::record_award_announcement(AwardAnnouncementOutcome::ClaimFailed);
                        tracing::warn!(error = ?e, "monthly award roll claim failed");
                    }
                }
            }
        })
    }

    async fn snapshot_profile_awards(&self) -> Result<()> {
        let mut client = self.db.get().await?;
        let outcome = snapshot_previous_month_profile_awards(&mut client).await?;
        tracing::debug!(
            changed = outcome.inserted,
            "profile award snapshot refreshed"
        );
        for (user_id, rank, chips) in outcome.gallery_prizes_paid {
            tracing::info!(%user_id, rank, chips, "artboard gallery prize paid");
        }
        Ok(())
    }

    /// Claim last month's #lounge roll. `None` when another pass, replica or
    /// restart already posted it, or when the month has no awards yet. The
    /// system user is looked up before the claim so a missing author can
    /// never spend it.
    async fn announce_award_roll(&self) -> Result<Option<ClaimedAwardRoll>> {
        let client = self.db.get().await?;
        let system_user_id = match User::find_by_fingerprint(&client, SYSTEM_FINGERPRINT).await? {
            Some(user) => user.id,
            None => anyhow::bail!("system user not ensured yet"),
        };
        match claim_previous_month_award_announcement(&client).await? {
            Some(roll) => Ok(Some(ClaimedAwardRoll {
                system_user_id,
                roll,
            })),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
