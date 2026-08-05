// Per-account config files for the roguelike doors: NetHack's .nethackrc and
// DCSS's init.txt. The row here is the source of truth; the door client pushes
// the content to the game host on every launch, where it becomes an ephemeral
// per-player file the child reads (NETHACKOPTIONS for nethack, `-rc` for
// crawl). Users author it by pasting into the Games hub config box; the hosts
// never write back.

use anyhow::Result;
use uuid::Uuid;

/// The doors that take a pushed config file. Closed roster: adding a game
/// means an arm in every `match self` here plus the client/host push wiring.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum DoorRcGame {
    Nethack,
    Dcss,
}

impl DoorRcGame {
    /// The stable DB key for the `game` column.
    pub fn as_key(self) -> &'static str {
        match self {
            DoorRcGame::Nethack => "nethack",
            DoorRcGame::Dcss => "dcss",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "nethack" => Some(DoorRcGame::Nethack),
            "dcss" => Some(DoorRcGame::Dcss),
            _ => None,
        }
    }

    /// The upstream file name users know this config by.
    pub fn file_label(self) -> &'static str {
        match self {
            DoorRcGame::Nethack => ".nethackrc",
            DoorRcGame::Dcss => "init.txt",
        }
    }
}

/// Size cap for one rc, enforced at the paste boundary (and mirrored by the
/// hosts when they decode the pushed copy). Real nethackrc/init.txt files are
/// a few KB; 16KB leaves generous room without letting a paste become a blob
/// store.
pub const MAX_RC_BYTES: usize = 16 * 1024;

/// Whether pasted rc content is storable: within the size cap and free of
/// NULs (the one byte that would corrupt the C games' config parsing).
pub fn content_acceptable(content: &str) -> bool {
    content.len() <= MAX_RC_BYTES && !content.contains('\0')
}

pub struct DoorRc;

impl DoorRc {
    /// The account's rc for one game, if configured.
    pub async fn get(
        client: &impl deadpool_postgres::GenericClient,
        user_id: Uuid,
        game: DoorRcGame,
    ) -> Result<Option<String>> {
        let row = client
            .query_opt(
                "SELECT content FROM door_rcs WHERE user_id = $1 AND game = $2",
                &[&user_id, &game.as_key()],
            )
            .await?;
        Ok(row.map(|r| r.get("content")))
    }

    /// Every configured rc for the account, for session-init preloading.
    /// Unknown `game` keys (a roster row from a newer deploy) are skipped.
    pub async fn list_for_user(
        client: &impl deadpool_postgres::GenericClient,
        user_id: Uuid,
    ) -> Result<Vec<(DoorRcGame, String)>> {
        let rows = client
            .query(
                "SELECT game, content FROM door_rcs WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                DoorRcGame::from_key(row.get("game")).map(|game| (game, row.get("content")))
            })
            .collect())
    }

    /// Set (or replace) the account's rc for one game.
    pub async fn upsert(
        client: &impl deadpool_postgres::GenericClient,
        user_id: Uuid,
        game: DoorRcGame,
        content: &str,
    ) -> Result<()> {
        client
            .execute(
                "INSERT INTO door_rcs (user_id, game, content) VALUES ($1, $2, $3)
                 ON CONFLICT (user_id, game)
                 DO UPDATE SET content = EXCLUDED.content, updated = current_timestamp",
                &[&user_id, &game.as_key(), &content],
            )
            .await?;
        Ok(())
    }

    /// Remove the account's rc for one game (back to upstream defaults).
    pub async fn clear(
        client: &impl deadpool_postgres::GenericClient,
        user_id: Uuid,
        game: DoorRcGame,
    ) -> Result<()> {
        client
            .execute(
                "DELETE FROM door_rcs WHERE user_id = $1 AND game = $2",
                &[&user_id, &game.as_key()],
            )
            .await?;
        Ok(())
    }
}
