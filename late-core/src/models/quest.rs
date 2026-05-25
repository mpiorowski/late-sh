use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use anyhow::Result;
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde_json::Value;
use tokio_postgres::{Client, GenericClient};
use uuid::Uuid;

use super::chips::{CHIP_USER_CHANGED_CHANNEL, INITIAL_CHIP_BALANCE};

pub const QUEST_REWARD_REASON: &str = "quest_reward";
pub const QUEST_SOURCE_KIND: &str = "quest_assignment";
pub const QUEST_USER_CHANGED_CHANNEL: &str = "quest_user_changed";
pub const QUEST_ASSIGNMENTS_CHANGED_CHANNEL: &str = "quest_assignments_changed";

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
    let mut pool = filtered_pool(
        templates,
        buckets,
        selected_templates,
        selected_domains,
        true,
    );
    if pool.is_empty() {
        pool = filtered_pool(
            templates,
            buckets,
            selected_templates,
            selected_domains,
            false,
        );
    }
    if pool.is_empty() {
        pool = templates
            .iter()
            .filter(|template| !selected_templates.contains(&template.id))
            .collect();
    }
    weighted_pick(&pool, cadence, period_start, slot)
}

fn slot_bucket_preferences(cadence: &str, slot: i32) -> &'static [&'static str] {
    match (cadence, slot) {
        ("daily", 1) => &["quick", "social", "casino"],
        ("daily", 2) => &["skill", "puzzle", "arcade"],
        _ => &[],
    }
}

fn filtered_pool<'a>(
    templates: &'a [QuestTemplate],
    buckets: &[&str],
    selected_templates: &[Uuid],
    selected_domains: &[String],
    avoid_domains: bool,
) -> Vec<&'a QuestTemplate> {
    templates
        .iter()
        .filter(|template| !selected_templates.contains(&template.id))
        .filter(|template| buckets.is_empty() || buckets.contains(&template.bucket.as_str()))
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

pub async fn apply_progress_event(
    client: &mut Client,
    user_id: Uuid,
    assignment_id: Uuid,
    event_id: Uuid,
    update: QuestProgressUpdate,
) -> Result<Option<QuestProgressOutcome>> {
    let tx = client.transaction().await?;

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
            "SELECT t.target, t.reward_chips
             FROM quest_assignments a
             JOIN reward_templates t ON t.id = a.template_id
             WHERE a.id = $1",
            &[&assignment_id],
        )
        .await?;
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
        if reward_chips > 0 {
            tx.execute(
                "INSERT INTO user_chips (user_id, balance)
                 VALUES ($1, $2)
                 ON CONFLICT (user_id) DO NOTHING",
                &[&user_id, &INITIAL_CHIP_BALANCE],
            )
            .await?;
            tx.execute(
                "UPDATE user_chips
                 SET balance = balance + $2, updated = current_timestamp
                 WHERE user_id = $1",
                &[&user_id, &reward_chips],
            )
            .await?;
            tx.execute(
                "INSERT INTO chip_ledger (user_id, delta, reason, source_kind, source_ref)
                 VALUES ($1, $2, $3, $4, $5)",
                &[
                    &user_id,
                    &reward_chips,
                    &QUEST_REWARD_REASON,
                    &QUEST_SOURCE_KIND,
                    &assignment_id.to_string(),
                ],
            )
            .await?;
            tx.execute(
                "SELECT pg_notify($1, $2)",
                &[&CHIP_USER_CHANGED_CHANNEL, &user_id.to_string()],
            )
            .await?;
        }
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
    }))
}

impl QuestProgressUpdate {
    fn amount(self) -> i32 {
        match self {
            Self::Increment(amount) | Self::Max(amount) => amount,
        }
    }
}
