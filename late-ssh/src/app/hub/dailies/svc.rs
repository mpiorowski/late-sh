use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use late_core::{
    MutexRecover,
    db::Db,
    models::quest::{
        DailyQuestStreakSnapshot, QuestProgressUpdate, QuestSnapshotRow, apply_progress_event,
        ensure_current_assignments, get_daily_quest_streak_snapshot, list_active_snapshot_rows,
    },
};
use serde_json::Value;
use tokio::sync::{broadcast, mpsc, watch};
use uuid::Uuid;

use crate::pg_listener::{Channel, Signal, read_until_ok};

use crate::app::activity::{
    channel::ActivitySender,
    event::{ActivityEvent, ActivityKind},
};

#[derive(Clone, Debug, Default)]
pub struct QuestSnapshot {
    pub user_id: Option<Uuid>,
    pub daily: Vec<QuestItem>,
    pub weekly: Vec<QuestItem>,
    pub daily_streak: DailyQuestStreakSnapshot,
}

#[derive(Clone, Debug)]
pub struct QuestItem {
    pub title: String,
    pub description: String,
    pub cadence: String,
    pub domain: String,
    pub difficulty: String,
    pub progress: i32,
    pub target: i32,
    pub reward_chips: i64,
    pub completed_at: Option<DateTime<Utc>>,
    pub period_end: NaiveDate,
}

impl QuestItem {
    pub fn completed(&self) -> bool {
        self.completed_at.is_some() || self.progress >= self.target
    }

    pub fn visible_progress(&self) -> i32 {
        self.progress.min(self.target)
    }
}

#[derive(Clone, Debug)]
pub enum QuestEvent {
    Completed {
        user_id: Uuid,
        title: String,
        reward_chips: i64,
        streak_reward_chips: i64,
        streak_bonus_level: Option<i32>,
    },
}

#[derive(Clone)]
pub struct QuestService {
    db: Db,
    activity_tx: ActivitySender,
    snapshot_txs: Arc<Mutex<HashMap<Uuid, watch::Sender<QuestSnapshot>>>>,
    evt_tx: broadcast::Sender<QuestEvent>,
}

impl QuestService {
    pub fn new(db: Db, activity_tx: ActivitySender) -> Self {
        let (evt_tx, _) = broadcast::channel(512);
        Self {
            db,
            activity_tx,
            snapshot_txs: Arc::new(Mutex::new(HashMap::new())),
            evt_tx,
        }
    }

    pub fn subscribe_snapshot(&self, user_id: Uuid) -> watch::Receiver<QuestSnapshot> {
        self.snapshot_sender(user_id).subscribe()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<QuestEvent> {
        self.evt_tx.subscribe()
    }

    fn snapshot_sender(&self, user_id: Uuid) -> watch::Sender<QuestSnapshot> {
        let mut channels = self.snapshot_txs.lock_recover();
        let make = || watch::channel(QuestSnapshot::default()).0;
        let sender = channels.entry(user_id).or_insert_with(&make);
        if sender.is_closed() {
            *sender = make();
        }
        sender.clone()
    }

    fn has_active_snapshot_receiver(&self, user_id: Uuid) -> bool {
        self.snapshot_txs
            .lock_recover()
            .get(&user_id)
            .is_some_and(|sender| sender.receiver_count() > 0)
    }

    fn active_snapshot_users(&self) -> Vec<Uuid> {
        self.snapshot_txs
            .lock_recover()
            .iter()
            .filter_map(|(user_id, sender)| (sender.receiver_count() > 0).then_some(*user_id))
            .collect()
    }

    fn publish_event(&self, event: QuestEvent) {
        let _ = self.evt_tx.send(event);
    }

    pub async fn refresh_user(&self, user_id: Uuid) -> Result<QuestSnapshot> {
        let snapshot = self.load_snapshot(user_id).await?;
        let _ = self.snapshot_sender(user_id).send(snapshot.clone());
        Ok(snapshot)
    }

    async fn refresh_user_if_active(&self, user_id: Uuid) -> Result<()> {
        if self.has_active_snapshot_receiver(user_id) {
            self.refresh_user(user_id).await?;
        }
        Ok(())
    }

    async fn refresh_active_users(&self) -> Result<()> {
        for user_id in self.active_snapshot_users() {
            self.refresh_user(user_id).await?;
        }
        Ok(())
    }

    pub fn refresh_user_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(error) = svc.refresh_user(user_id).await {
                tracing::warn!(error = ?error, user_id = %user_id, "failed to refresh quest snapshot");
            }
        });
    }

    async fn load_snapshot(&self, user_id: Uuid) -> Result<QuestSnapshot> {
        let mut client = self.db.get().await?;
        let now = Utc::now();
        ensure_current_assignments(&mut client, now).await?;
        let today = now.date_naive();
        let rows = list_active_snapshot_rows(&client, user_id, today).await?;
        let daily_streak = get_daily_quest_streak_snapshot(&client, user_id, today).await?;
        Ok(snapshot_from_rows(user_id, rows, daily_streak))
    }

    pub fn start_activity_task(&self) -> tokio::task::JoinHandle<()> {
        let svc = self.clone();
        let mut rx = self.activity_tx.subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if let Err(error) = svc.apply_activity_event(event).await {
                            tracing::warn!(error = ?error, "failed to apply quest activity event");
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "quest activity receiver lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        })
    }

    async fn apply_activity_event(&self, event: ActivityEvent) -> Result<()> {
        let Some(user_id) = event.user_id else {
            return Ok(());
        };

        let mut client = self.db.get().await?;
        ensure_current_assignments(&mut client, event.occurred_at).await?;
        let rows =
            list_active_snapshot_rows(&client, user_id, event.occurred_at.date_naive()).await?;

        let mut completed = Vec::new();
        for row in rows {
            if row
                .progress
                .as_ref()
                .is_some_and(|progress| progress.completed_at.is_some())
            {
                continue;
            }
            let Some(update) = progress_update_for_event(&row, &event) else {
                continue;
            };
            let Some(outcome) =
                apply_progress_event(&mut client, user_id, row.assignment.id, event.id, update)
                    .await?
            else {
                continue;
            };
            if outcome.completed_now {
                completed.push((
                    row.template.title.clone(),
                    outcome.rewarded_chips,
                    outcome
                        .streak_reward
                        .as_ref()
                        .map(|reward| (reward.reward_chips, reward.bonus_level)),
                ));
            }
        }

        if !completed.is_empty() {
            self.refresh_user_if_active(user_id).await?;
            for (title, reward_chips, streak_reward) in completed {
                self.publish_event(QuestEvent::Completed {
                    user_id,
                    title,
                    reward_chips,
                    streak_reward_chips: streak_reward
                        .map(|(reward_chips, _)| reward_chips)
                        .unwrap_or(0),
                    streak_bonus_level: streak_reward.map(|(_, bonus_level)| bonus_level),
                });
            }
        }

        Ok(())
    }

    /// What the notify worker subscribes to.
    pub const CHANNELS: &'static [Channel] =
        &[Channel::QuestUserChanged, Channel::QuestAssignmentsChanged];

    /// Keep this replica's open quest boards in step with
    /// `quest_user_changed` and `quest_assignments_changed`. A resync, and
    /// any failed signal, re-reads every active user's board, retrying
    /// until it lands, so a change missed while the listener reconnected
    /// is caught up.
    pub fn start_notify_worker(
        &self,
        mut signals: mpsc::UnboundedReceiver<Signal>,
    ) -> tokio::task::JoinHandle<()> {
        let svc = self.clone();
        tokio::spawn(async move {
            while let Some(signal) = signals.recv().await {
                let Err(error) = svc.apply_signal(signal).await else {
                    continue;
                };
                tracing::warn!(error = ?error, "quest notify failed, refreshing active users");
                read_until_ok("active quest boards", || svc.refresh_active_users()).await;
            }
        })
    }

    async fn apply_signal(&self, signal: Signal) -> Result<()> {
        match signal {
            Signal::Resync => self.refresh_active_users().await?,
            Signal::Notify {
                channel: Channel::QuestUserChanged,
                payload,
            } => {
                if let Ok(user_id) = payload.parse::<Uuid>() {
                    self.refresh_user_if_active(user_id).await?;
                }
            }
            Signal::Notify {
                channel: Channel::QuestAssignmentsChanged,
                ..
            } => self.refresh_active_users().await?,
            Signal::Notify { channel, .. } => {
                unreachable!("quests subscribed only to quest channels, got {channel:?}")
            }
        }
        Ok(())
    }
}

fn snapshot_from_rows(
    user_id: Uuid,
    rows: Vec<QuestSnapshotRow>,
    daily_streak: DailyQuestStreakSnapshot,
) -> QuestSnapshot {
    let mut snapshot = QuestSnapshot {
        user_id: Some(user_id),
        daily: Vec::new(),
        weekly: Vec::new(),
        daily_streak,
    };
    for row in rows {
        let progress_value = row
            .progress
            .as_ref()
            .map(|progress| progress.progress)
            .unwrap_or(0);
        let completed_at = row
            .progress
            .as_ref()
            .and_then(|progress| progress.completed_at);
        let item = QuestItem {
            title: row.template.title,
            description: row.template.description,
            cadence: row.assignment.cadence.clone(),
            domain: row.template.domain,
            difficulty: row.template.difficulty,
            progress: progress_value,
            target: row.template.target,
            reward_chips: row.template.reward_chips,
            completed_at,
            period_end: row.assignment.period_end,
        };
        if item.cadence == "weekly" {
            snapshot.weekly.push(item);
        } else {
            snapshot.daily.push(item);
        }
    }
    snapshot
}

fn progress_update_for_event(
    row: &QuestSnapshotRow,
    event: &ActivityEvent,
) -> Option<QuestProgressUpdate> {
    match row.template.kind.as_str() {
        "daily_puzzle_win" => match_daily_puzzle_win(&row.template.params, event),
        "arcade_puzzle_solved" => match_arcade_puzzle_solved(&row.template.params, event),
        "arcade_score" => match_arcade_score(&row.template.params, event),
        "arcade_level" => match_arcade_level(&row.template.params, event),
        "bonsai_watered" => match_bonsai_watered(event),
        "login_once" => match_login_once(event),
        _ => None,
    }
}

fn match_arcade_puzzle_solved(
    params: &Value,
    event: &ActivityEvent,
) -> Option<QuestProgressUpdate> {
    match_daily_puzzle_win(params, event)
}

fn match_daily_puzzle_win(params: &Value, event: &ActivityEvent) -> Option<QuestProgressUpdate> {
    let ActivityKind::GameWon { game, detail, .. } = &event.kind else {
        return None;
    };
    let expected_game = param_str(params, "game")?;
    if game.key() != expected_game {
        return None;
    }
    if let Some(expected_difficulty) = param_str(params, "difficulty")
        && detail.as_deref() != Some(expected_difficulty)
    {
        return None;
    }
    Some(QuestProgressUpdate::Increment(1))
}

fn match_arcade_score(params: &Value, event: &ActivityEvent) -> Option<QuestProgressUpdate> {
    let ActivityKind::GameScored { game, score, .. } = &event.kind else {
        return None;
    };
    let expected_game = param_str(params, "game")?;
    if game.key() == expected_game {
        Some(QuestProgressUpdate::Max(*score))
    } else {
        None
    }
}

fn match_arcade_level(params: &Value, event: &ActivityEvent) -> Option<QuestProgressUpdate> {
    let ActivityKind::GameScored { game, level, .. } = &event.kind else {
        return None;
    };
    let expected_game = param_str(params, "game")?;
    if game.key() == expected_game {
        level.map(QuestProgressUpdate::Max)
    } else {
        None
    }
}

fn match_bonsai_watered(event: &ActivityEvent) -> Option<QuestProgressUpdate> {
    matches!(event.kind, ActivityKind::BonsaiWatered).then_some(QuestProgressUpdate::Increment(1))
}

fn match_login_once(event: &ActivityEvent) -> Option<QuestProgressUpdate> {
    matches!(event.kind, ActivityKind::UserJoined).then_some(QuestProgressUpdate::Increment(1))
}

fn param_str<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(Value::as_str)
}
