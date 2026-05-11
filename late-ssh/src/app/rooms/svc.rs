use std::time::Duration;

use late_core::{
    db::Db,
    models::{
        chat_room_member::ChatRoomMember,
        game_room::{GameRoom, ROOM_SEAT_SEPARATOR},
    },
};
use serde_json::Value;
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::app::ai::ghost::DEALER_FINGERPRINT;

pub use late_core::models::game_room::GameKind;

const MAX_TABLES_PER_USER: i64 = 3;
const INACTIVE_TABLE_TTL: Duration = Duration::from_secs(12 * 60 * 60);
const INACTIVE_TABLE_CLEANUP_INTERVAL: Duration = Duration::from_secs(60 * 60);

#[derive(Clone)]
pub struct RoomsService {
    db: Db,
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
        Ok(Self {
            id: room.id,
            chat_room_id: room.chat_room_id,
            game_kind: room.kind()?,
            slug: room.slug,
            display_name: room.display_name,
            status: room.status,
            settings: room.settings,
        })
    }
}

impl RoomsService {
    pub fn new(db: Db) -> Self {
        let (snapshot_tx, snapshot_rx) = watch::channel(RoomsSnapshot::default());
        let (event_tx, _) = broadcast::channel(256);
        Self {
            db,
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
                if let Err(e) = svc.close_inactive_tables(INACTIVE_TABLE_TTL).await {
                    tracing::error!(error = ?e, "failed to close inactive game rooms");
                }
                tokio::time::sleep(INACTIVE_TABLE_CLEANUP_INTERVAL).await;
            }
        });
    }

    async fn refresh(&self) -> anyhow::Result<()> {
        let client = self.db.get().await?;
        self.publish_rooms(&client).await
    }

    async fn publish_rooms(&self, client: &tokio_postgres::Client) -> anyhow::Result<()> {
        let rooms = GameRoom::list_open(client)
            .await?
            .into_iter()
            .map(RoomListItem::try_from)
            .collect::<anyhow::Result<Vec<_>>>()?;
        let _ = self.snapshot_tx.send(RoomsSnapshot { rooms });
        Ok(())
    }

    async fn close_inactive_tables(&self, ttl: Duration) -> anyhow::Result<u64> {
        let client = self.db.get().await?;
        let closed = close_inactive_rooms(&client, ttl).await?;
        if closed > 0 {
            tracing::info!(closed, "closed inactive game rooms");
            self.publish_rooms(&client).await?;
        }
        Ok(closed)
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
        let count = GameRoom::close_by_id(&client, room_id).await?;
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

async fn close_inactive_rooms(
    client: &tokio_postgres::Client,
    ttl: Duration,
) -> anyhow::Result<u64> {
    GameRoom::close_inactive(client, ttl).await
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
        .replace(ROOM_SEAT_SEPARATOR, " | ")
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
