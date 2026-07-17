// Green Dragon clans (LoGD clan.php + lib/clan/*): the clan rows only —
// membership (clan_id / clan_rank / clan_joined_at) lives on the character
// blobs, exactly as upstream keeps it on `accounts`. Name and tag are
// case-insensitively unique (upstream's MySQL collation); the MOTD,
// description, and custom talk verb are officer/leader edits whose author
// columns snapshot the editor's character name at write time.

use anyhow::Result;
use tokio_postgres::Client;
use uuid::Uuid;

crate::model! {
    table = "greendragon_clans";
    params = GreenDragonClanParams;
    struct GreenDragonClan {
        @data
        pub name: String,
        pub tag: String,
        pub motd: String,
        pub motd_author: String,
        pub description: String,
        pub desc_author: String,
        pub custom_verb: String,
    }
}

/// Why a founding was refused (the registrar's two "already taken" forms).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClanNameClash {
    Name,
    Tag,
}

impl GreenDragonClan {
    /// File a new clan (`applicant_new.php`'s approval): insert unless the
    /// name or tag is already taken, case-insensitively. Returns the new
    /// clan's id, or which uniqueness check refused it. The caller validates
    /// shape (lengths, charset) and charges the fee.
    pub async fn found(
        client: &impl deadpool_postgres::GenericClient,
        name: &str,
        tag: &str,
    ) -> Result<std::result::Result<Uuid, ClanNameClash>> {
        let clash = client
            .query_opt(
                "SELECT lower(name) = lower($1) AS by_name FROM greendragon_clans
                 WHERE lower(name) = lower($1) OR lower(tag) = lower($2)
                 LIMIT 1",
                &[&name, &tag],
            )
            .await?;
        if let Some(row) = clash {
            return Ok(Err(if row.get::<_, bool>("by_name") {
                ClanNameClash::Name
            } else {
                ClanNameClash::Tag
            }));
        }
        let row = client
            .query_one(
                "INSERT INTO greendragon_clans (name, tag) VALUES ($1, $2) RETURNING id",
                &[&name, &tag],
            )
            .await?;
        Ok(Ok(row.get("id")))
    }

    /// One clan's row, if it still stands. Usable inside a transaction
    /// (unlike the generated `get`).
    pub async fn load(
        client: &impl deadpool_postgres::GenericClient,
        id: Uuid,
    ) -> Result<Option<GreenDragonClan>> {
        let row = client
            .query_opt("SELECT * FROM greendragon_clans WHERE id = $1", &[&id])
            .await?;
        Ok(row.map(Self::from))
    }

    /// Update the MOTD, stamping the editor's name (`clan_motd.php`).
    pub async fn set_motd(client: &Client, id: Uuid, motd: &str, author: &str) -> Result<()> {
        client
            .execute(
                "UPDATE greendragon_clans
                 SET motd = $2, motd_author = $3, updated = current_timestamp
                 WHERE id = $1",
                &[&id, &motd, &author],
            )
            .await?;
        Ok(())
    }

    /// Update the description, stamping the editor's name.
    pub async fn set_description(
        client: &Client,
        id: Uuid,
        description: &str,
        author: &str,
    ) -> Result<()> {
        client
            .execute(
                "UPDATE greendragon_clans
                 SET description = $2, desc_author = $3, updated = current_timestamp
                 WHERE id = $1",
                &[&id, &description, &author],
            )
            .await?;
        Ok(())
    }

    /// Update the custom talk verb (leader+; blank means "says").
    pub async fn set_custom_verb(client: &Client, id: Uuid, verb: &str) -> Result<()> {
        client
            .execute(
                "UPDATE greendragon_clans
                 SET custom_verb = $2, updated = current_timestamp
                 WHERE id = $1",
                &[&id, &verb],
            )
            .await?;
        Ok(())
    }

    /// Delete a clan whose last real member left (the withdraw path) or
    /// that a list render found empty (the lazy sweep). Usable inside a
    /// transaction (unlike the generated `delete`).
    pub async fn remove(client: &impl deadpool_postgres::GenericClient, id: Uuid) -> Result<()> {
        client
            .execute("DELETE FROM greendragon_clans WHERE id = $1", &[&id])
            .await?;
        Ok(())
    }
}
