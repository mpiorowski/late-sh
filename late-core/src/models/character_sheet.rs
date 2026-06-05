use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

/// Maximum length of a character sheet name (chars, enforced UI-side).
pub const SHEET_NAME_MAX_CHARS: usize = 48;
/// Maximum length of a character sheet body (chars, enforced UI-side).
pub const SHEET_BODY_MAX_CHARS: usize = 4000;

crate::model! {
    table = "character_sheets";
    params = CharacterSheetParams;
    struct CharacterSheet {
        @data
        pub user_id: Uuid,
        pub room_id: Uuid,
        pub name: String,
        pub body: String,
    }
}

impl CharacterSheet {
    pub async fn find_by_user_room(
        client: &Client,
        user_id: Uuid,
        room_id: Uuid,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                "SELECT * FROM character_sheets WHERE user_id = $1 AND room_id = $2",
                &[&user_id, &room_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn upsert(
        client: &Client,
        user_id: Uuid,
        room_id: Uuid,
        name: &str,
        body: &str,
    ) -> Result<Self> {
        let row = client
            .query_one(
                "INSERT INTO character_sheets (user_id, room_id, name, body)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (user_id, room_id) DO UPDATE SET
                    name = $3,
                    body = $4,
                    updated = current_timestamp
                 RETURNING *",
                &[&user_id, &room_id, &name, &body],
            )
            .await?;
        Ok(Self::from(row))
    }
}
