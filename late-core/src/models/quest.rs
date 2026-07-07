use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde_json::Value;
use tokio_postgres::{Client, GenericClient};
use uuid::Uuid;

use super::chips::{CHIP_USER_CHANGED_CHANNEL, INITIAL_CHIP_BALANCE};

pub const QUEST_REWARD_REASON: &str = "quest_reward";
pub const QUEST_SOURCE_KIND: &str = "quest_assignment";
pub const DAILY_QUEST_STREAK_REWARD_REASON: &str = "daily_quest_streak_reward";
pub const DAILY_QUEST_STREAK_SOURCE_KIND: &str = "daily_quest_streak";
pub const QUEST_USER_CHANGED_CHANNEL: &str = "quest_user_changed";
pub const QUEST_ASSIGNMENTS_CHANGED_CHANNEL: &str = "quest_assignments_changed";
pub const MAX_DAILY_QUEST_STREAK_BONUS_LEVEL: i32 = 5;
pub const DAILY_QUEST_STREAK_BONUS_CHIPS_PER_LEVEL: i64 = 100;

#[derive(Clone, Debug)]
pub struct QuestTemplate {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    pub key: String,
    pub title: String,
    pub description: String,
    pub cadence: String,
    pub bucket: String,
    pub domain: String,
    pub difficulty: String,
    pub kind: String,
    pub params: Value,
    pub target: i32,
    pub reward_chips: i64,
    pub weight: i32,
    pub active: bool,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
pub struct RewardTemplateAdminRow {
    pub id: Uuid,
    pub key: String,
    pub title: String,
    pub description: String,
    pub cadence: Option<String>,
    pub bucket: Option<String>,
    pub domain: String,
    pub difficulty: Option<String>,
    pub kind: String,
    pub params: Value,
    pub target: i32,
    pub reward_chips: i64,
    pub weight: i32,
    pub is_quest: bool,
    pub claim_policy: String,
    pub cooldown_seconds: Option<i32>,
    pub active: bool,
}

impl From<tokio_postgres::Row> for RewardTemplateAdminRow {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            id: row.get("id"),
            key: row.get("key"),
            title: row.get("title"),
            description: row.get("description"),
            cadence: row.get("cadence"),
            bucket: row.get("bucket"),
            domain: row.get("domain"),
            difficulty: row.get("difficulty"),
            kind: row.get("kind"),
            params: row.get("params"),
            target: row.get("target"),
            reward_chips: row.get("reward_chips"),
            weight: row.get("weight"),
            is_quest: row.get("is_quest"),
            claim_policy: row.get("claim_policy"),
            cooldown_seconds: row.get("cooldown_seconds"),
            active: row.get("active"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RewardTemplateAdminUpdate {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub target: i32,
    pub reward_chips: i64,
    pub weight: i32,
    pub active: bool,
}

impl From<tokio_postgres::Row> for QuestTemplate {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            id: row.get("id"),
            created: row.get("created"),
            updated: row.get("updated"),
            key: row.get("key"),
            title: row.get("title"),
            description: row.get("description"),
            cadence: row.get("cadence"),
            bucket: row.get("bucket"),
            domain: row.get("domain"),
            difficulty: row.get("difficulty"),
            kind: row.get("kind"),
            params: row.get("params"),
            target: row.get("target"),
            reward_chips: row.get("reward_chips"),
            weight: row.get("weight"),
            active: row.get("active"),
            starts_at: row.get("starts_at"),
            ends_at: row.get("ends_at"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct QuestAssignment {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub cadence: String,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub slot: i32,
    pub template_id: Uuid,
}

impl From<tokio_postgres::Row> for QuestAssignment {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            id: row.get("id"),
            created: row.get("created"),
            cadence: row.get("cadence"),
            period_start: row.get("period_start"),
            period_end: row.get("period_end"),
            slot: row.get("slot"),
            template_id: row.get("template_id"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct UserQuestProgress {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    pub user_id: Uuid,
    pub assignment_id: Uuid,
    pub progress: i32,
    pub completed_at: Option<DateTime<Utc>>,
    pub rewarded_at: Option<DateTime<Utc>>,
}

impl From<tokio_postgres::Row> for UserQuestProgress {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            id: row.get("id"),
            created: row.get("created"),
            updated: row.get("updated"),
            user_id: row.get("user_id"),
            assignment_id: row.get("assignment_id"),
            progress: row.get("progress"),
            completed_at: row.get("completed_at"),
            rewarded_at: row.get("rewarded_at"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct UserDailyQuestStreak {
    pub user_id: Uuid,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    pub last_completed_date: NaiveDate,
    pub consecutive_days: i32,
    pub bonus_level: i32,
}

impl From<tokio_postgres::Row> for UserDailyQuestStreak {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            user_id: row.get("user_id"),
            created: row.get("created"),
            updated: row.get("updated"),
            last_completed_date: row.get("last_completed_date"),
            consecutive_days: row.get("consecutive_days"),
            bonus_level: row.get("bonus_level"),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct DailyQuestStreakSnapshot {
    pub consecutive_days: i32,
    pub bonus_level: i32,
    pub last_completed_date: Option<NaiveDate>,
    pub current_bonus_chips: i64,
    pub next_bonus_chips: i64,
}

#[derive(Clone, Debug)]
pub struct QuestSnapshotRow {
    pub assignment: QuestAssignment,
    pub template: QuestTemplate,
    pub progress: Option<UserQuestProgress>,
}

#[derive(Clone, Copy, Debug)]
pub enum QuestProgressUpdate {
    Increment(i32),
    Max(i32),
}

#[derive(Clone, Debug)]
pub struct QuestProgressOutcome {
    pub progress: UserQuestProgress,
    pub completed_now: bool,
    pub rewarded_chips: i64,
    pub streak_reward: Option<DailyQuestStreakReward>,
}

#[derive(Clone, Debug)]
pub struct DailyQuestStreakReward {
    pub bonus_level: i32,
    pub reward_chips: i64,
}

pub async fn listen_for_quest_changes(client: &Client) -> Result<()> {
    client
        .batch_execute(&format!(
            "LISTEN {QUEST_USER_CHANGED_CHANNEL};
             LISTEN {QUEST_ASSIGNMENTS_CHANGED_CHANNEL};"
        ))
        .await?;
    Ok(())
}

pub async fn list_reward_templates_for_admin(
    client: &impl deadpool_postgres::GenericClient,
) -> Result<Vec<RewardTemplateAdminRow>> {
    let rows = client
        .query(
            "SELECT
                 id, key, title, description, cadence, bucket, domain,
                 difficulty, kind, params, target, reward_chips, weight,
                 is_quest, claim_policy, cooldown_seconds, active
             FROM reward_templates
             ORDER BY
                 CASE
                     WHEN is_quest = true AND cadence = 'daily' THEN 0
                     WHEN is_quest = true AND cadence = 'weekly' THEN 1
                     WHEN is_quest = false AND domain = 'puzzle' THEN 2
                     ELSE 3
                 END,
                 domain ASC,
                 key ASC",
            &[],
        )
        .await?;
    Ok(rows.into_iter().map(RewardTemplateAdminRow::from).collect())
}

pub async fn update_reward_template_for_admin(
    client: &impl deadpool_postgres::GenericClient,
    update: RewardTemplateAdminUpdate,
) -> Result<RewardTemplateAdminRow> {
    ensure!(!update.title.trim().is_empty(), "title cannot be empty");
    ensure!(
        !update.description.trim().is_empty(),
        "description cannot be empty"
    );
    ensure!(update.target > 0, "target must be greater than 0");
    ensure!(update.reward_chips >= 0, "reward must be 0 or greater");
    ensure!(update.weight > 0, "weight must be greater than 0");

    let row = client
        .query_opt(
            "UPDATE reward_templates
             SET
                 title = $2,
                 description = $3,
                 target = $4,
                 reward_chips = $5,
                 weight = $6,
                 active = $7,
                 updated = current_timestamp
             WHERE id = $1
             RETURNING
                 id, key, title, description, cadence, bucket, domain,
                 difficulty, kind, params, target, reward_chips, weight,
                 is_quest, claim_policy, cooldown_seconds, active",
            &[
                &update.id,
                &update.title.trim(),
                &update.description.trim(),
                &update.target,
                &update.reward_chips,
                &update.weight,
                &update.active,
            ],
        )
        .await?;
    let row = row
        .map(RewardTemplateAdminRow::from)
        .with_context(|| format!("reward template {} not found", update.id))?;
    client
        .execute(
            "SELECT pg_notify($1, $2)",
            &[&QUEST_ASSIGNMENTS_CHANGED_CHANNEL, &row.key],
        )
        .await?;
    Ok(row)
}

pub fn daily_period(date: NaiveDate) -> (NaiveDate, NaiveDate) {
    (
        date,
        date.checked_add_signed(Duration::days(1)).unwrap_or(date),
    )
}

pub fn weekly_period(date: NaiveDate) -> (NaiveDate, NaiveDate) {
    let days_from_monday = i64::from(date.weekday().num_days_from_monday());
    let start = date
        .checked_sub_signed(Duration::days(days_from_monday))
        .unwrap_or(date);
    let end = start.checked_add_signed(Duration::days(7)).unwrap_or(start);
    (start, end)
}

pub async fn ensure_current_assignments(client: &mut Client, now: DateTime<Utc>) -> Result<()> {
    let today = now.date_naive();
    let daily = daily_period(today);
    let weekly = weekly_period(today);
    let tx = client.transaction().await?;

    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtext($1)::bigint)",
        &[&"late_sh_quest_assignment_draw"],
    )
    .await?;

    let mut changed = false;
    changed |= ensure_period_assignments(&tx, "daily", daily.0, daily.1, &[1, 2], now).await?;
    changed |= ensure_period_assignments(&tx, "weekly", weekly.0, weekly.1, &[1], now).await?;
    if changed {
        tx.execute(
            "SELECT pg_notify($1, $2)",
            &[&QUEST_ASSIGNMENTS_CHANGED_CHANNEL, &today.to_string()],
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn ensure_period_assignments(
    client: &impl GenericClient,
    cadence: &str,
    period_start: NaiveDate,
    period_end: NaiveDate,
    slots: &[i32],
    now: DateTime<Utc>,
) -> Result<bool> {
    let templates = list_active_templates(client, cadence, now).await?;
    if templates.is_empty() {
        return Ok(false);
    }

    let rows = client
        .query(
            "SELECT a.*, t.domain
             FROM quest_assignments a
             JOIN reward_templates t ON t.id = a.template_id
             WHERE a.cadence = $1 AND a.period_start = $2",
            &[&cadence, &period_start],
        )
        .await?;
    let mut selected_templates: Vec<Uuid> = Vec::new();
    let mut selected_domains: Vec<String> = Vec::new();
    let mut existing_slots: Vec<i32> = Vec::new();
    for row in rows {
        selected_templates.push(row.get("template_id"));
        selected_domains.push(row.get("domain"));
        existing_slots.push(row.get("slot"));
    }

    let mut changed = false;
    for slot in slots {
        if existing_slots.contains(slot) {
            continue;
        }
        let Some(template) = choose_template(
            &templates,
            cadence,
            period_start,
            *slot,
            &selected_templates,
            &selected_domains,
        ) else {
            continue;
        };

        let inserted = client
            .execute(
                "INSERT INTO quest_assignments
                    (cadence, period_start, period_end, slot, template_id)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (cadence, period_start, slot) DO NOTHING",
                &[&cadence, &period_start, &period_end, slot, &template.id],
            )
            .await?;
        if inserted > 0 {
            selected_templates.push(template.id);
            selected_domains.push(template.domain.clone());
            changed = true;
        }
    }
    Ok(changed)
}

async fn list_active_templates(
    client: &impl GenericClient,
    cadence: &str,
    now: DateTime<Utc>,
) -> Result<Vec<QuestTemplate>> {
    let rows = client
        .query(
            "SELECT *
             FROM reward_templates
             WHERE cadence = $1
               AND is_quest = true
               AND active = true
               AND (starts_at IS NULL OR starts_at <= $2)
               AND (ends_at IS NULL OR ends_at > $2)
             ORDER BY key ASC",
            &[&cadence, &now],
        )
        .await?;
    Ok(rows.into_iter().map(QuestTemplate::from).collect())
}

fn choose_template<'a>(
    templates: &'a [QuestTemplate],
    cadence: &str,
    period_start: NaiveDate,
    slot: i32,
    selected_templates: &[Uuid],
    selected_domains: &[String],
) -> Option<&'a QuestTemplate> {
    let buckets = slot_bucket_preferences(cadence, slot);
    let source = slot_source_preference(cadence, slot);
    let mut pool = filtered_pool(
        templates,
        buckets,
        source,
        selected_templates,
        selected_domains,
        true,
    );
    if pool.is_empty() {
        pool = filtered_pool(
            templates,
            buckets,
            source,
            selected_templates,
            selected_domains,
            false,
        );
    }
    if pool.is_empty() {
        pool = templates
            .iter()
            .filter(|template| !selected_templates.contains(&template.id))
            .filter(|template| source.is_none_or(|source| quest_source(template) == source))
            .collect();
    }
    weighted_pick(&pool, cadence, period_start, slot)
}

fn slot_bucket_preferences(cadence: &str, slot: i32) -> &'static [&'static str] {
    match (cadence, slot) {
        ("daily", 1) => &["quick", "skill"],
        ("daily", 2) => &["skill", "casino"],
        _ => &[],
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QuestSource {
    Arcade,
    Multiplayer,
    Other,
}

fn slot_source_preference(cadence: &str, slot: i32) -> Option<QuestSource> {
    match (cadence, slot) {
        ("daily", 1) => Some(QuestSource::Arcade),
        ("daily", 2) => Some(QuestSource::Multiplayer),
        _ => None,
    }
}

fn quest_source(template: &QuestTemplate) -> QuestSource {
    match template.kind.as_str() {
        "daily_puzzle_win" | "arcade_puzzle_solved" | "arcade_score" | "arcade_level" => {
            QuestSource::Arcade
        }
        "room_rounds_played" | "room_wins" => QuestSource::Multiplayer,
        _ => QuestSource::Other,
    }
}

fn filtered_pool<'a>(
    templates: &'a [QuestTemplate],
    buckets: &[&str],
    source: Option<QuestSource>,
    selected_templates: &[Uuid],
    selected_domains: &[String],
    avoid_domains: bool,
) -> Vec<&'a QuestTemplate> {
    templates
        .iter()
        .filter(|template| !selected_templates.contains(&template.id))
        .filter(|template| buckets.is_empty() || buckets.contains(&template.bucket.as_str()))
        .filter(|template| source.is_none_or(|source| quest_source(template) == source))
        .filter(|template| !avoid_domains || !selected_domains.contains(&template.domain))
        .collect()
}

fn weighted_pick<'a>(
    pool: &[&'a QuestTemplate],
    cadence: &str,
    period_start: NaiveDate,
    slot: i32,
) -> Option<&'a QuestTemplate> {
    let total: i64 = pool.iter().map(|template| i64::from(template.weight)).sum();
    if total <= 0 {
        return pool.first().copied();
    }

    let mut hasher = DefaultHasher::new();
    cadence.hash(&mut hasher);
    period_start.hash(&mut hasher);
    slot.hash(&mut hasher);
    "late-sh-quest-draw-v1".hash(&mut hasher);
    let mut roll = (hasher.finish() % total as u64) as i64;
    for template in pool {
        let weight = i64::from(template.weight);
        if roll < weight {
            return Some(*template);
        }
        roll -= weight;
    }
    pool.first().copied()
}

pub async fn list_active_snapshot_rows(
    client: &Client,
    user_id: Uuid,
    today: NaiveDate,
) -> Result<Vec<QuestSnapshotRow>> {
    let rows = client
        .query(
            "SELECT
                 a.id AS assignment_id,
                 a.created AS assignment_created,
                 a.cadence AS assignment_cadence,
                 a.period_start,
                 a.period_end,
                 a.slot,
                 a.template_id,
                 t.id AS template_id_full,
                 t.created AS template_created,
                 t.updated AS template_updated,
                 t.key,
                 t.title,
                 t.description,
                 t.cadence AS template_cadence,
                 t.bucket,
                 t.domain,
                 t.difficulty,
                 t.kind,
                 t.params,
                 t.target,
                 t.reward_chips,
                 t.weight,
                 t.active,
                 t.starts_at,
                 t.ends_at,
                 p.id AS progress_id,
                 p.created AS progress_created,
                 p.updated AS progress_updated,
                 p.user_id AS progress_user_id,
                 p.assignment_id AS progress_assignment_id,
                 p.progress,
                 p.completed_at,
                 p.rewarded_at
             FROM quest_assignments a
             JOIN reward_templates t ON t.id = a.template_id
             LEFT JOIN user_quest_progress p
               ON p.assignment_id = a.id AND p.user_id = $1
             WHERE a.period_start <= $2 AND a.period_end > $2
             ORDER BY
               CASE a.cadence WHEN 'daily' THEN 0 ELSE 1 END,
               a.slot ASC",
            &[&user_id, &today],
        )
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let progress = row
                .get::<_, Option<Uuid>>("progress_id")
                .map(|id| UserQuestProgress {
                    id,
                    created: row.get("progress_created"),
                    updated: row.get("progress_updated"),
                    user_id: row.get("progress_user_id"),
                    assignment_id: row.get("progress_assignment_id"),
                    progress: row.get("progress"),
                    completed_at: row.get("completed_at"),
                    rewarded_at: row.get("rewarded_at"),
                });
            QuestSnapshotRow {
                assignment: QuestAssignment {
                    id: row.get("assignment_id"),
                    created: row.get("assignment_created"),
                    cadence: row.get("assignment_cadence"),
                    period_start: row.get("period_start"),
                    period_end: row.get("period_end"),
                    slot: row.get("slot"),
                    template_id: row.get("template_id"),
                },
                template: QuestTemplate {
                    id: row.get("template_id_full"),
                    created: row.get("template_created"),
                    updated: row.get("template_updated"),
                    key: row.get("key"),
                    title: row.get("title"),
                    description: row.get("description"),
                    cadence: row.get("template_cadence"),
                    bucket: row.get("bucket"),
                    domain: row.get("domain"),
                    difficulty: row.get("difficulty"),
                    kind: row.get("kind"),
                    params: row.get("params"),
                    target: row.get("target"),
                    reward_chips: row.get("reward_chips"),
                    weight: row.get("weight"),
                    active: row.get("active"),
                    starts_at: row.get("starts_at"),
                    ends_at: row.get("ends_at"),
                },
                progress,
            }
        })
        .collect())
}

pub async fn get_daily_quest_streak_snapshot(
    client: &Client,
    user_id: Uuid,
    today: NaiveDate,
) -> Result<DailyQuestStreakSnapshot> {
    let row = client
        .query_opt(
            "SELECT *
             FROM user_daily_quest_streaks
             WHERE user_id = $1",
            &[&user_id],
        )
        .await?;
    let Some(row) = row else {
        return Ok(DailyQuestStreakSnapshot::default());
    };
    Ok(daily_streak_snapshot_from_row(
        UserDailyQuestStreak::from(row),
        today,
    ))
}

pub async fn apply_progress_event(
    client: &mut Client,
    user_id: Uuid,
    assignment_id: Uuid,
    event_id: Uuid,
    update: QuestProgressUpdate,
) -> Result<Option<QuestProgressOutcome>> {
    let tx = client.transaction().await?;

    tx.query_one(
        "SELECT pg_advisory_xact_lock(hashtext($1)::bigint)",
        &[&format!("late_sh_quest_progress:{user_id}")],
    )
    .await?;

    let Some(event_row) = tx
        .query_opt(
            "INSERT INTO quest_progress_events (user_id, assignment_id, event_id, amount)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (assignment_id, event_id) DO NOTHING
             RETURNING id",
            &[&user_id, &assignment_id, &event_id, &update.amount()],
        )
        .await?
    else {
        tx.commit().await?;
        return Ok(None);
    };
    let _: Uuid = event_row.get("id");

    let meta = tx
        .query_one(
            "SELECT a.cadence, a.period_start, t.target, t.reward_chips
             FROM quest_assignments a
             JOIN reward_templates t ON t.id = a.template_id
             WHERE a.id = $1",
            &[&assignment_id],
        )
        .await?;
    let assignment_cadence: String = meta.get("cadence");
    let assignment_period_start: NaiveDate = meta.get("period_start");
    let target: i32 = meta.get("target");
    let reward_chips: i64 = meta.get("reward_chips");

    tx.execute(
        "INSERT INTO user_quest_progress (user_id, assignment_id)
         VALUES ($1, $2)
         ON CONFLICT (user_id, assignment_id) DO NOTHING",
        &[&user_id, &assignment_id],
    )
    .await?;

    let existing = tx
        .query_one(
            "SELECT *
             FROM user_quest_progress
             WHERE user_id = $1 AND assignment_id = $2
             FOR UPDATE",
            &[&user_id, &assignment_id],
        )
        .await?;
    let existing_progress: i32 = existing.get("progress");
    let existing_completed_at = existing.get::<_, Option<DateTime<Utc>>>("completed_at");
    let existing_rewarded_at = existing.get::<_, Option<DateTime<Utc>>>("rewarded_at");

    let new_progress = match update {
        QuestProgressUpdate::Increment(amount) => existing_progress.saturating_add(amount),
        QuestProgressUpdate::Max(value) => existing_progress.max(value),
    }
    .max(0);
    let now = Utc::now();
    let completed_at = if new_progress >= target {
        existing_completed_at.or(Some(now))
    } else {
        existing_completed_at
    };
    let completed_now = existing_completed_at.is_none() && completed_at.is_some();

    let mut rewarded_at = existing_rewarded_at;
    let mut rewarded_chips = 0;
    if completed_at.is_some() && rewarded_at.is_none() {
        rewarded_at = Some(now);
        rewarded_chips = reward_chips;
    }

    let row = tx
        .query_one(
            "UPDATE user_quest_progress
             SET
                progress = $3,
                completed_at = $4,
                rewarded_at = $5,
                updated = current_timestamp
             WHERE user_id = $1 AND assignment_id = $2
             RETURNING *",
            &[
                &user_id,
                &assignment_id,
                &new_progress,
                &completed_at,
                &rewarded_at,
            ],
        )
        .await?;
    if rewarded_chips > 0 {
        credit_chip_reward(
            &tx,
            user_id,
            rewarded_chips,
            QUEST_REWARD_REASON,
            QUEST_SOURCE_KIND,
            &assignment_id.to_string(),
        )
        .await?;
    }

    let streak_reward = if completed_now && assignment_cadence == "daily" {
        record_daily_quest_streak_if_complete(&tx, user_id, assignment_period_start).await?
    } else {
        None
    };

    tx.execute(
        "SELECT pg_notify($1, $2)",
        &[&QUEST_USER_CHANGED_CHANNEL, &user_id.to_string()],
    )
    .await?;

    tx.commit().await?;
    Ok(Some(QuestProgressOutcome {
        progress: UserQuestProgress::from(row),
        completed_now,
        rewarded_chips,
        streak_reward,
    }))
}

async fn record_daily_quest_streak_if_complete(
    client: &impl GenericClient,
    user_id: Uuid,
    completion_date: NaiveDate,
) -> Result<Option<DailyQuestStreakReward>> {
    let completion = client
        .query_one(
            "SELECT
                count(*)::int AS total,
                count(*) FILTER (
                    WHERE p.completed_at IS NOT NULL OR p.progress >= t.target
                )::int AS completed
             FROM quest_assignments a
             JOIN reward_templates t ON t.id = a.template_id
             LEFT JOIN user_quest_progress p
               ON p.assignment_id = a.id AND p.user_id = $1
             WHERE a.cadence = 'daily' AND a.period_start = $2",
            &[&user_id, &completion_date],
        )
        .await?;
    let total: i32 = completion.get("total");
    let completed: i32 = completion.get("completed");
    if total == 0 || completed == 0 {
        return Ok(None);
    }

    let existing = client
        .query_opt(
            "SELECT *
             FROM user_daily_quest_streaks
             WHERE user_id = $1
             FOR UPDATE",
            &[&user_id],
        )
        .await?;
    let existing = existing.map(UserDailyQuestStreak::from);
    let Some(advance) = next_daily_streak_advance(
        existing
            .as_ref()
            .map(|streak| (streak.last_completed_date, streak.consecutive_days)),
        completion_date,
    ) else {
        return Ok(None);
    };

    client
        .execute(
            "INSERT INTO user_daily_quest_streaks
                (user_id, last_completed_date, consecutive_days, bonus_level)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (user_id) DO UPDATE SET
                last_completed_date = EXCLUDED.last_completed_date,
                consecutive_days = EXCLUDED.consecutive_days,
                bonus_level = EXCLUDED.bonus_level,
                updated = current_timestamp",
            &[
                &user_id,
                &completion_date,
                &advance.consecutive_days,
                &advance.bonus_level,
            ],
        )
        .await?;

    if advance.reward_chips > 0 {
        credit_chip_reward(
            client,
            user_id,
            advance.reward_chips,
            DAILY_QUEST_STREAK_REWARD_REASON,
            DAILY_QUEST_STREAK_SOURCE_KIND,
            &completion_date.to_string(),
        )
        .await?;
    }

    Ok(
        (advance.reward_chips > 0).then_some(DailyQuestStreakReward {
            bonus_level: advance.bonus_level,
            reward_chips: advance.reward_chips,
        }),
    )
}

async fn credit_chip_reward(
    client: &impl GenericClient,
    user_id: Uuid,
    amount: i64,
    reason: &str,
    source_kind: &str,
    source_ref: &str,
) -> Result<()> {
    client
        .execute(
            "INSERT INTO user_chips (user_id, balance)
             VALUES ($1, $2)
             ON CONFLICT (user_id) DO NOTHING",
            &[&user_id, &INITIAL_CHIP_BALANCE],
        )
        .await?;
    client
        .execute(
            "UPDATE user_chips
             SET balance = balance + $2, updated = current_timestamp
             WHERE user_id = $1",
            &[&user_id, &amount],
        )
        .await?;
    client
        .execute(
            "INSERT INTO chip_ledger (user_id, delta, reason, source_kind, source_ref)
             VALUES ($1, $2, $3, $4, $5)",
            &[&user_id, &amount, &reason, &source_kind, &source_ref],
        )
        .await?;
    client
        .execute(
            "SELECT pg_notify($1, $2)",
            &[&CHIP_USER_CHANGED_CHANNEL, &user_id.to_string()],
        )
        .await?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DailyStreakAdvance {
    consecutive_days: i32,
    bonus_level: i32,
    reward_chips: i64,
}

fn next_daily_streak_advance(
    existing: Option<(NaiveDate, i32)>,
    completion_date: NaiveDate,
) -> Option<DailyStreakAdvance> {
    let consecutive_days = match existing {
        Some((last_completed_date, _)) if last_completed_date == completion_date => return None,
        Some((last_completed_date, consecutive_days))
            if last_completed_date
                == completion_date
                    .checked_sub_signed(Duration::days(1))
                    .unwrap_or(completion_date) =>
        {
            consecutive_days.max(1).saturating_add(1)
        }
        _ => 1,
    };
    let bonus_level = (consecutive_days - 1).clamp(0, MAX_DAILY_QUEST_STREAK_BONUS_LEVEL);
    Some(DailyStreakAdvance {
        consecutive_days,
        bonus_level,
        reward_chips: i64::from(bonus_level) * DAILY_QUEST_STREAK_BONUS_CHIPS_PER_LEVEL,
    })
}

fn daily_streak_snapshot_from_row(
    streak: UserDailyQuestStreak,
    today: NaiveDate,
) -> DailyQuestStreakSnapshot {
    let yesterday = today.checked_sub_signed(Duration::days(1)).unwrap_or(today);
    if streak.last_completed_date != today && streak.last_completed_date != yesterday {
        return DailyQuestStreakSnapshot::default();
    }

    let next_bonus_level = streak
        .consecutive_days
        .clamp(0, MAX_DAILY_QUEST_STREAK_BONUS_LEVEL);
    DailyQuestStreakSnapshot {
        consecutive_days: streak.consecutive_days,
        bonus_level: streak.bonus_level,
        last_completed_date: Some(streak.last_completed_date),
        current_bonus_chips: i64::from(streak.bonus_level)
            * DAILY_QUEST_STREAK_BONUS_CHIPS_PER_LEVEL,
        next_bonus_chips: i64::from(next_bonus_level) * DAILY_QUEST_STREAK_BONUS_CHIPS_PER_LEVEL,
    }
}

impl QuestProgressUpdate {
    fn amount(self) -> i32 {
        match self {
            Self::Increment(amount) | Self::Max(amount) => amount,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn template(key: &str, bucket: &str, domain: &str, kind: &str) -> QuestTemplate {
        QuestTemplate {
            id: Uuid::now_v7(),
            created: DateTime::<Utc>::UNIX_EPOCH,
            updated: DateTime::<Utc>::UNIX_EPOCH,
            key: key.to_string(),
            title: key.to_string(),
            description: key.to_string(),
            cadence: "daily".to_string(),
            bucket: bucket.to_string(),
            domain: domain.to_string(),
            difficulty: "medium".to_string(),
            kind: kind.to_string(),
            params: json!({}),
            target: 1,
            reward_chips: 100,
            weight: 100,
            active: true,
            starts_at: None,
            ends_at: None,
        }
    }

    #[test]
    fn daily_slots_split_arcade_and_multiplayer_sources() {
        let period_start = NaiveDate::from_ymd_opt(2026, 5, 31).unwrap();
        let templates = vec![
            template("arcade", "skill", "arcade", "arcade_score"),
            template("room", "skill", "strategy", "room_rounds_played"),
        ];

        let slot_one = choose_template(&templates, "daily", period_start, 1, &[], &[]).unwrap();
        let slot_two = choose_template(&templates, "daily", period_start, 2, &[], &[]).unwrap();

        assert_eq!(slot_one.key, "arcade");
        assert_eq!(slot_two.key, "room");
    }

    #[test]
    fn daily_streak_bonus_starts_on_second_consecutive_full_daily_and_caps() {
        let day = NaiveDate::from_ymd_opt(2026, 5, 31).unwrap();
        let next_day = day.checked_add_signed(Duration::days(1)).unwrap();
        let sixth_day = day.checked_add_signed(Duration::days(5)).unwrap();
        let skipped_day = day.checked_add_signed(Duration::days(7)).unwrap();

        assert_eq!(
            next_daily_streak_advance(None, day),
            Some(DailyStreakAdvance {
                consecutive_days: 1,
                bonus_level: 0,
                reward_chips: 0
            })
        );
        assert_eq!(
            next_daily_streak_advance(Some((day, 1)), next_day),
            Some(DailyStreakAdvance {
                consecutive_days: 2,
                bonus_level: 1,
                reward_chips: 100
            })
        );
        assert_eq!(
            next_daily_streak_advance(Some((sixth_day, 6)), sixth_day),
            None
        );
        assert_eq!(
            next_daily_streak_advance(Some((day, 5)), skipped_day),
            Some(DailyStreakAdvance {
                consecutive_days: 1,
                bonus_level: 0,
                reward_chips: 0
            })
        );
        assert_eq!(
            next_daily_streak_advance(Some((sixth_day, 6)), sixth_day.succ_opt().unwrap()),
            Some(DailyStreakAdvance {
                consecutive_days: 7,
                bonus_level: 5,
                reward_chips: 500
            })
        );
    }
}
