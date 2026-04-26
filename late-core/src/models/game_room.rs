use anyhow::Result;
use serde_json::Value;
use tokio_postgres::Client;
use uuid::Uuid;

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

    pub async fn list_by_kind(client: &Client, game_kind: &str) -> Result<Vec<Self>> {
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
