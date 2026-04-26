use anyhow::Result;
use serde_json::Value;
use tokio_postgres::Client;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameKind {
    Blackjack,
}

impl GameKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Blackjack => "blackjack",
        }
    }
}

impl std::fmt::Display for GameKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TryFrom<&str> for GameKind {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "blackjack" => Ok(Self::Blackjack),
            _ => Err(anyhow::anyhow!("unknown game kind: {}", value)),
        }
    }
}

crate::model! {
    table = "game_rooms";
    params = GameRoomParams;
    struct GameRoom {
        @data
        pub chat_room_id: Uuid,
        pub game_kind: String,
        pub slug: String,
        pub display_name: String,
        pub status: String,
        pub settings: Value,
        pub created_by: Option<Uuid>,
    }
}

impl GameRoom {
    pub const STATUS_OPEN: &'static str = "open";
    pub const STATUS_IN_ROUND: &'static str = "in_round";
    pub const STATUS_PAUSED: &'static str = "paused";
    pub const STATUS_CLOSED: &'static str = "closed";

    pub fn kind(&self) -> Result<GameKind> {
        GameKind::try_from(self.game_kind.as_str())
    }

    pub async fn create_with_chat_room(
        client: &Client,
        game_kind: GameKind,
        slug: &str,
        display_name: &str,
        settings: Value,
        created_by: Option<Uuid>,
    ) -> Result<Self> {
        let game_kind = game_kind.as_str();
        let row = client
            .query_one(
                "WITH chat AS (
                     INSERT INTO chat_rooms (kind, visibility, auto_join, slug, game_kind)
                     VALUES ('game', 'public', false, $1, $2)
                     ON CONFLICT (game_kind, slug) WHERE kind = 'game'
                     DO UPDATE SET updated = current_timestamp
                     RETURNING id
                 )
                 INSERT INTO game_rooms (
                     chat_room_id,
                     game_kind,
                     slug,
                     display_name,
                     status,
                     settings,
                     created_by
                 )
                 SELECT
                     chat.id,
                     $2,
                     $1,
                     $3,
                     $4,
                     $5,
                     $6
                 FROM chat
                 RETURNING *",
                &[
                    &slug,
                    &game_kind,
                    &display_name,
                    &Self::STATUS_OPEN,
                    &settings,
                    &created_by,
                ],
            )
            .await?;
        Ok(Self::from(row))
    }

    pub async fn find_by_chat_room_id(client: &Client, chat_room_id: Uuid) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM game_rooms WHERE chat_room_id = $1",
                &[&chat_room_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn find_by_slug(client: &Client, slug: &str) -> Result<Option<Self>> {
        let row = client
            .query_opt("SELECT * FROM game_rooms WHERE slug = $1", &[&slug])
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn list_by_kind(client: &Client, game_kind: GameKind) -> Result<Vec<Self>> {
        let game_kind = game_kind.as_str();
        let rows = client
            .query(
                "SELECT *
                 FROM game_rooms
                 WHERE game_kind = $1
                 ORDER BY created ASC, slug ASC, id ASC",
                &[&game_kind],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }

    pub async fn list_open(client: &Client) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT *
                 FROM game_rooms
                 WHERE status <> 'closed'
                 ORDER BY game_kind ASC, created ASC, slug ASC, id ASC",
                &[],
            )
            .await?;
        Ok(rows.into_iter().map(Self::from).collect())
    }
}
