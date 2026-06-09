use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use late_core::{
    MutexRecover,
    db::Db,
    models::{chat_room_member::ChatRoomMember, game_room::GameRoom, user::User},
};
use serde_json::Value;
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::app::ai::ghost::DEALER_FINGERPRINT;

pub use late_core::models::game_room::GameKind;

const MAX_TABLES_PER_USER: i64 = 10;
const INACTIVE_TABLE_TTL: Duration = Duration::from_secs(60 * 60);
const INACTIVE_TABLE_CLEANUP_INTERVAL: Duration = Duration::from_secs(60 * 60);

#[derive(Clone)]
pub struct RoomsService {
    db: Db,
    status_generations: Arc<Mutex<HashMap<Uuid, u64>>>,
    status_update_lock: Arc<tokio::sync::Mutex<()>>,
    snapshot_tx: watch::Sender<RoomsSnapshot>,
    snapshot_rx: watch::Receiver<RoomsSnapshot>,
    event_tx: broadcast::Sender<RoomsEvent>,
}

#[derive(Clone, Debug, Default)]
pub struct RoomsSnapshot {
    pub rooms: Vec<RoomListItem>,
}

#[derive(Clone, Debug)]
pub struct RoomListItem {
    pub id: Uuid,
    pub chat_room_id: Uuid,
    pub game_kind: GameKind,
    pub slug: String,
    pub display_name: String,
    pub status: String,
    pub settings: Value,
    pub runtime_state: Value,
    pub created_by: Option<Uuid>,
    pub created_by_username: Option<String>,
}

#[derive(Clone, Debug)]
pub enum RoomsEvent {
    Created {
        user_id: Uuid,
        game_kind: GameKind,
        display_name: String,
    },
    Deleted {
        user_id: Uuid,
        display_name: String,
    },
    Error {
        user_id: Uuid,
        game_kind: GameKind,
        display_name: String,
        message: String,
    },
    DeleteError {
        user_id: Uuid,
        display_name: String,
        message: String,
    },
}

impl TryFrom<GameRoom> for RoomListItem {
    type Error = anyhow::Error;

    fn try_from(room: GameRoom) -> Result<Self, Self::Error> {
        Self::from_game_room(room, &HashMap::new())
    }
}

impl RoomListItem {
    fn from_game_room(
        room: GameRoom,
        creator_usernames: &HashMap<Uuid, String>,
    ) -> anyhow::Result<Self> {
        let created_by = room.created_by;
        Ok(Self {
            id: room.id,
            chat_room_id: room.chat_room_id,
            game_kind: room.kind()?,
            slug: room.slug,
            display_name: room.display_name,
            status: room.status,
            settings: room.settings,
            runtime_state: room.runtime_state,
            created_by,
            created_by_username: created_by.and_then(|id| creator_usernames.get(&id).cloned()),
        })
    }
}

impl RoomsService {
    pub fn new(db: Db) -> Self {
        let (snapshot_tx, snapshot_rx) = watch::channel(RoomsSnapshot::default());
        let (event_tx, _) = broadcast::channel(256);
        Self {
            db,
            status_generations: Arc::new(Mutex::new(HashMap::new())),
            status_update_lock: Arc::new(tokio::sync::Mutex::new(())),
            snapshot_tx,
            snapshot_rx,
            event_tx,
        }
    }

    pub fn subscribe_snapshot(&self) -> watch::Receiver<RoomsSnapshot> {
        self.snapshot_rx.clone()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<RoomsEvent> {
        self.event_tx.subscribe()
    }

    pub fn refresh_task(&self) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.refresh().await {
                tracing::error!(error = ?e, "failed to refresh rooms");
            }
        });
    }

    pub fn cleanup_inactive_tables_task(&self) {
        let svc = self.clone();
        tokio::spawn(async move {
            loop {
                if let Err(e) = svc.delete_inactive_tables(INACTIVE_TABLE_TTL).await {
                    tracing::error!(error = ?e, "failed to delete inactive game rooms");
                }
                tokio::time::sleep(INACTIVE_TABLE_CLEANUP_INTERVAL).await;
            }
        });
    }

    pub fn reconcile_round_statuses_task(&self) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.reconcile_round_statuses().await {
                tracing::error!(error = ?e, "failed to reconcile game room statuses");
            }
        });
    }

    async fn refresh(&self) -> anyhow::Result<()> {
        let client = self.db.get().await?;
        self.publish_rooms(&client).await
    }

    async fn publish_rooms(&self, client: &tokio_postgres::Client) -> anyhow::Result<()> {
        let game_rooms = GameRoom::list_open(client).await?;
        let mut creator_ids: Vec<Uuid> = game_rooms
            .iter()
            .filter_map(|room| room.created_by)
            .collect();
        creator_ids.sort();
        creator_ids.dedup();
        let creator_usernames = User::list_usernames_by_ids(client, &creator_ids).await?;
        let rooms = game_rooms
            .into_iter()
            .map(|room| RoomListItem::from_game_room(room, &creator_usernames))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let _ = self.snapshot_tx.send(RoomsSnapshot { rooms });
        Ok(())
    }

    async fn delete_inactive_tables(&self, ttl: Duration) -> anyhow::Result<u64> {
        let client = self.db.get().await?;
        let deleted = delete_inactive_rooms(&client, ttl).await?;
        if deleted > 0 {
            tracing::info!(deleted, "deleted inactive game rooms");
            self.publish_rooms(&client).await?;
        }
        Ok(deleted)
    }

    async fn reconcile_round_statuses(&self) -> anyhow::Result<u64> {
        let client = self.db.get().await?;
        let reconciled = GameRoom::reconcile_in_round_after_restart(&client).await?;
        if reconciled > 0 {
            tracing::info!(reconciled, "reconciled stale in-round game rooms");
            self.publish_rooms(&client).await?;
        }
        Ok(reconciled)
    }

    pub fn touch_room_task(&self, room_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.touch_room(room_id).await {
                tracing::error!(error = ?e, %room_id, "failed to touch game room");
            }
        });
    }

    async fn touch_room(&self, room_id: Uuid) -> anyhow::Result<()> {
        let client = self.db.get().await?;
        touch_room_activity(&client, room_id).await
    }

    pub fn sync_room_status_task(
        &self,
        room_id: Uuid,
        room_in_round: Arc<AtomicBool>,
        in_round: bool,
    ) {
        let previous = room_in_round.swap(in_round, Ordering::AcqRel);
        if previous == in_round {
            return;
        }
        let status = if in_round {
            GameRoom::STATUS_IN_ROUND
        } else {
            GameRoom::STATUS_OPEN
        };
        self.set_room_status_task(room_id, status);
    }

    fn set_room_status_task(&self, room_id: Uuid, status: &'static str) {
        let svc = self.clone();
        let generation = {
            let mut generations = self.status_generations.lock_recover();
            let generation = generations
                .get(&room_id)
                .copied()
                .unwrap_or_default()
                .wrapping_add(1);
            generations.insert(room_id, generation);
            generation
        };
        tokio::spawn(async move {
            loop {
                match svc
                    .set_room_status_if_current(room_id, status, generation)
                    .await
                {
                    Ok(()) => break,
                    Err(e) => {
                        tracing::error!(
                            error = ?e,
                            %room_id,
                            status,
                            "failed to update game room status"
                        );
                        if svc.current_status_generation(room_id) != Some(generation) {
                            break;
                        }
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        if svc.current_status_generation(room_id) != Some(generation) {
                            break;
                        }
                    }
                }
            }
        });
    }

    fn current_status_generation(&self, room_id: Uuid) -> Option<u64> {
        self.status_generations
            .lock_recover()
            .get(&room_id)
            .copied()
    }

    async fn set_room_status_if_current(
        &self,
        room_id: Uuid,
        status: &str,
        generation: u64,
    ) -> anyhow::Result<()> {
        let _guard = self.status_update_lock.lock().await;
        if self
            .status_generations
            .lock_recover()
            .get(&room_id)
            .copied()
            != Some(generation)
        {
            return Ok(());
        }
        let client = self.db.get().await?;
        GameRoom::update_status(&client, room_id, status).await?;
        self.publish_rooms(&client).await?;
        Ok(())
    }

    pub fn save_runtime_state_task(&self, room_id: Uuid, runtime_state: Value) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.save_runtime_state(room_id, runtime_state).await {
                tracing::error!(error = ?e, %room_id, "failed to save game room runtime state");
            }
        });
    }

    async fn save_runtime_state(&self, room_id: Uuid, runtime_state: Value) -> anyhow::Result<()> {
        let client = self.db.get().await?;
        GameRoom::update_runtime_state(&client, room_id, runtime_state).await?;
        Ok(())
    }

    pub fn create_game_room_task(
        &self,
        user_id: Uuid,
        game_kind: GameKind,
        slug_prefix: &'static str,
        label: &'static str,
        display_name: String,
        settings: Value,
    ) {
        let svc = self.clone();
        tokio::spawn(async move {
            match svc
                .create_game_room(
                    user_id,
                    game_kind,
                    slug_prefix,
                    label,
                    &display_name,
                    settings,
                )
                .await
            {
                Ok(room) => {
                    let _ = svc.event_tx.send(RoomsEvent::Created {
                        user_id,
                        game_kind,
                        display_name: room.display_name,
                    });
                }
                Err(e) => {
                    tracing::error!(
                        error = ?e,
                        %user_id,
                        game_kind = game_kind.as_str(),
                        display_name,
                        "failed to create game room"
                    );
                    let _ = svc.event_tx.send(RoomsEvent::Error {
                        user_id,
                        game_kind,
                        display_name,
                        message: room_create_error_message(&e),
                    });
                }
            }
        });
    }

    async fn create_game_room(
        &self,
        user_id: Uuid,
        game_kind: GameKind,
        slug_prefix: &str,
        label: &str,
        display_name: &str,
        settings: Value,
    ) -> anyhow::Result<GameRoom> {
        let display_name = sanitize_room_display_name(display_name);
        if display_name.is_empty() {
            anyhow::bail!("table name is required");
        }

        let client = self.db.get().await?;
        let existing_count = count_open_rooms_created_by(&client, user_id, game_kind).await?;
        if existing_count >= MAX_TABLES_PER_USER {
            anyhow::bail!(
                "table limit reached: max {} open {} tables per user",
                MAX_TABLES_PER_USER,
                label
            );
        }

        let slug = generate_room_slug(slug_prefix);
        let room = GameRoom::create_with_chat_room(
            &client,
            game_kind,
            &slug,
            &display_name,
            settings,
            Some(user_id),
        )
        .await?;
        add_dealer_to_game_room_chat(&client, room.chat_room_id).await?;
        self.publish_rooms(&client).await?;
        Ok(room)
    }

    pub fn delete_game_room_task(&self, user_id: Uuid, room_id: Uuid, display_name: String) {
        let svc = self.clone();
        tokio::spawn(async move {
            match svc.delete_game_room(room_id).await {
                Ok(()) => {
                    let _ = svc.event_tx.send(RoomsEvent::Deleted {
                        user_id,
                        display_name,
                    });
                }
                Err(e) => {
                    tracing::error!(
                        error = ?e,
                        %user_id,
                        %room_id,
                        display_name,
                        "failed to delete game room"
                    );
                    let _ = svc.event_tx.send(RoomsEvent::DeleteError {
                        user_id,
                        display_name,
                        message: room_error_message(&e),
                    });
                }
            }
        });
    }

    async fn delete_game_room(&self, room_id: Uuid) -> anyhow::Result<()> {
        let client = self.db.get().await?;
        let count = GameRoom::delete_by_id(&client, room_id).await?;
        if count == 0 {
            anyhow::bail!("table already deleted");
        }
        self.publish_rooms(&client).await?;
        Ok(())
    }
}

async fn add_dealer_to_game_room_chat(
    client: &tokio_postgres::Client,
    chat_room_id: Uuid,
) -> anyhow::Result<()> {
    ChatRoomMember::join_user_by_fingerprint(client, chat_room_id, DEALER_FINGERPRINT).await?;
    Ok(())
}

async fn count_open_rooms_created_by(
    client: &tokio_postgres::Client,
    user_id: Uuid,
    game_kind: GameKind,
) -> anyhow::Result<i64> {
    GameRoom::count_open_created_by(client, user_id, game_kind).await
}

async fn delete_inactive_rooms(
    client: &tokio_postgres::Client,
    ttl: Duration,
) -> anyhow::Result<u64> {
    GameRoom::delete_inactive_open(client, ttl).await
}

async fn touch_room_activity(client: &tokio_postgres::Client, room_id: Uuid) -> anyhow::Result<()> {
    GameRoom::touch_activity(client, room_id).await?;
    Ok(())
}

fn generate_room_slug(slug_prefix: &str) -> String {
    let id = Uuid::now_v7().simple().to_string();
    format!("{}-{}", slug_prefix, &id[..12])
}

pub(crate) fn sanitize_room_display_name(input: &str) -> String {
    input
        .replace(" || ", " | ")
        .replace('@', "＠")
        .replace(['\n', '\r'], " ")
        .trim()
        .to_string()
}

fn room_create_error_message(error: &anyhow::Error) -> String {
    room_error_message(error)
}

fn room_error_message(error: &anyhow::Error) -> String {
    error.root_cause().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_room_display_name_neutralizes_chat_reserved_text() {
        assert_eq!(
            sanitize_room_display_name(" @alice Casual || Fun\n "),
            "＠alice Casual | Fun"
        );
    }
}
