use late_core::{db::Db, models::game_room::GameRoom};
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

pub use late_core::models::game_room::GameKind;

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
}

#[derive(Clone, Debug)]
pub enum RoomsEvent {
    Created {
        user_id: Uuid,
        game_kind: GameKind,
        display_name: String,
    },
    Error {
        user_id: Uuid,
        game_kind: GameKind,
        display_name: String,
        message: String,
    },
}

pub(crate) fn game_kind_label(game_kind: GameKind) -> &'static str {
    match game_kind {
        GameKind::Blackjack => "Blackjack",
    }
}

fn game_kind_slug_prefix(game_kind: GameKind) -> &'static str {
    match game_kind {
        GameKind::Blackjack => "bj",
    }
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

    pub fn create_game_room_task(&self, user_id: Uuid, game_kind: GameKind, display_name: String) {
        let svc = self.clone();
        tokio::spawn(async move {
            match svc
                .create_game_room(user_id, game_kind, &display_name)
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
        display_name: &str,
    ) -> anyhow::Result<GameRoom> {
        let client = self.db.get().await?;
        let slug = generate_room_slug(game_kind);
        let room = GameRoom::create_with_chat_room(
            &client,
            game_kind,
            &slug,
            display_name,
            serde_json::json!({}),
            Some(user_id),
        )
        .await?;
        self.publish_rooms(&client).await?;
        Ok(room)
    }
}

fn generate_room_slug(game_kind: GameKind) -> String {
    let id = Uuid::now_v7().simple().to_string();
    format!("{}-{}", game_kind_slug_prefix(game_kind), &id[..12])
}

fn room_create_error_message(error: &anyhow::Error) -> String {
    error.root_cause().to_string()
}
