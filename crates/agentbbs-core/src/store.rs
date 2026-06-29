//! Persistence for boards and messages.
//!
//! [`Store`] is the storage abstraction. [`MemoryStore`] is an always-available
//! in-memory implementation (used in tests, wasm, and ephemeral nodes). The
//! `native` feature additionally provides [`RedbStore`], a durable embedded
//! key-value store with no external database server — keeping a node
//! self-contained and easy to run anonymously.

use std::collections::BTreeMap;
use std::sync::RwLock;

use crate::board::{Board, Message, MessageId};
use crate::error::{Error, Result};

/// Storage abstraction for the BBS domain.
pub trait Store: Send + Sync {
    /// Create or replace a board.
    fn put_board(&self, board: &Board) -> Result<()>;
    /// Fetch a board by slug.
    fn get_board(&self, slug: &str) -> Result<Option<Board>>;
    /// List all boards, ordered by slug.
    fn list_boards(&self) -> Result<Vec<Board>>;

    /// Append a verified message. Idempotent on `message.id` (content-addressed),
    /// which makes federated replication safe to replay.
    fn put_message(&self, message: &Message) -> Result<()>;
    /// Fetch a message by id.
    fn get_message(&self, id: &MessageId) -> Result<Option<Message>>;
    /// List messages on a board, oldest first, newest-`limit` window.
    fn list_messages(&self, board: &str, limit: usize) -> Result<Vec<Message>>;
    /// Total number of stored messages.
    fn message_count(&self) -> Result<usize>;
}

/// In-memory store. Thread-safe, non-durable.
#[derive(Default)]
pub struct MemoryStore {
    boards: RwLock<BTreeMap<String, Board>>,
    // board slug -> ordered list of message ids
    by_board: RwLock<BTreeMap<String, Vec<MessageId>>>,
    messages: RwLock<BTreeMap<String, Message>>,
}

impl MemoryStore {
    /// Create an empty in-memory store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Store for MemoryStore {
    fn put_board(&self, board: &Board) -> Result<()> {
        self.boards
            .write()
            .unwrap()
            .insert(board.slug.clone(), board.clone());
        Ok(())
    }

    fn get_board(&self, slug: &str) -> Result<Option<Board>> {
        Ok(self.boards.read().unwrap().get(slug).cloned())
    }

    fn list_boards(&self) -> Result<Vec<Board>> {
        Ok(self.boards.read().unwrap().values().cloned().collect())
    }

    fn put_message(&self, message: &Message) -> Result<()> {
        let mut messages = self.messages.write().unwrap();
        if messages.contains_key(&message.id.0) {
            return Ok(()); // idempotent
        }
        messages.insert(message.id.0.clone(), message.clone());
        self.by_board
            .write()
            .unwrap()
            .entry(message.body.board.clone())
            .or_default()
            .push(message.id.clone());
        Ok(())
    }

    fn get_message(&self, id: &MessageId) -> Result<Option<Message>> {
        Ok(self.messages.read().unwrap().get(&id.0).cloned())
    }

    fn list_messages(&self, board: &str, limit: usize) -> Result<Vec<Message>> {
        let by_board = self.by_board.read().unwrap();
        let messages = self.messages.read().unwrap();
        let ids = match by_board.get(board) {
            Some(ids) => ids,
            None => return Ok(vec![]),
        };
        let start = ids.len().saturating_sub(limit);
        Ok(ids[start..]
            .iter()
            .filter_map(|id| messages.get(&id.0).cloned())
            .collect())
    }

    fn message_count(&self) -> Result<usize> {
        Ok(self.messages.read().unwrap().len())
    }
}

#[cfg(feature = "native")]
pub use redb_store::RedbStore;

#[cfg(feature = "native")]
mod redb_store {
    use super::*;
    use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
    use std::path::Path;

    const BOARDS: TableDefinition<&str, &[u8]> = TableDefinition::new("boards");
    const MESSAGES: TableDefinition<&str, &[u8]> = TableDefinition::new("messages");
    // board slug -> json array of message ids (ordered)
    const BOARD_INDEX: TableDefinition<&str, &[u8]> = TableDefinition::new("board_index");

    /// A durable, embedded, single-file store backed by [`redb`].
    pub struct RedbStore {
        db: Database,
    }

    impl RedbStore {
        /// Open (or create) a store at `path`.
        pub fn open(path: impl AsRef<Path>) -> Result<Self> {
            let db = Database::create(path).map_err(|e| Error::Storage(e.to_string()))?;
            // Ensure tables exist.
            let wtx = db
                .begin_write()
                .map_err(|e| Error::Storage(e.to_string()))?;
            {
                wtx.open_table(BOARDS)
                    .map_err(|e| Error::Storage(e.to_string()))?;
                wtx.open_table(MESSAGES)
                    .map_err(|e| Error::Storage(e.to_string()))?;
                wtx.open_table(BOARD_INDEX)
                    .map_err(|e| Error::Storage(e.to_string()))?;
            }
            wtx.commit().map_err(|e| Error::Storage(e.to_string()))?;
            Ok(RedbStore { db })
        }

        fn read_index(&self, board: &str) -> Result<Vec<MessageId>> {
            let rtx = self
                .db
                .begin_read()
                .map_err(|e| Error::Storage(e.to_string()))?;
            let table = rtx
                .open_table(BOARD_INDEX)
                .map_err(|e| Error::Storage(e.to_string()))?;
            match table
                .get(board)
                .map_err(|e| Error::Storage(e.to_string()))?
            {
                Some(v) => Ok(serde_json::from_slice(v.value())?),
                None => Ok(vec![]),
            }
        }
    }

    impl Store for RedbStore {
        fn put_board(&self, board: &Board) -> Result<()> {
            let bytes = serde_json::to_vec(board)?;
            let wtx = self
                .db
                .begin_write()
                .map_err(|e| Error::Storage(e.to_string()))?;
            {
                let mut t = wtx
                    .open_table(BOARDS)
                    .map_err(|e| Error::Storage(e.to_string()))?;
                t.insert(board.slug.as_str(), bytes.as_slice())
                    .map_err(|e| Error::Storage(e.to_string()))?;
            }
            wtx.commit().map_err(|e| Error::Storage(e.to_string()))?;
            Ok(())
        }

        fn get_board(&self, slug: &str) -> Result<Option<Board>> {
            let rtx = self
                .db
                .begin_read()
                .map_err(|e| Error::Storage(e.to_string()))?;
            let t = rtx
                .open_table(BOARDS)
                .map_err(|e| Error::Storage(e.to_string()))?;
            match t.get(slug).map_err(|e| Error::Storage(e.to_string()))? {
                Some(v) => Ok(Some(serde_json::from_slice(v.value())?)),
                None => Ok(None),
            }
        }

        fn list_boards(&self) -> Result<Vec<Board>> {
            let rtx = self
                .db
                .begin_read()
                .map_err(|e| Error::Storage(e.to_string()))?;
            let t = rtx
                .open_table(BOARDS)
                .map_err(|e| Error::Storage(e.to_string()))?;
            let mut out = Vec::new();
            for row in t.iter().map_err(|e| Error::Storage(e.to_string()))? {
                let (_k, v) = row.map_err(|e| Error::Storage(e.to_string()))?;
                out.push(serde_json::from_slice(v.value())?);
            }
            Ok(out)
        }

        fn put_message(&self, message: &Message) -> Result<()> {
            let wtx = self
                .db
                .begin_write()
                .map_err(|e| Error::Storage(e.to_string()))?;
            {
                let mut mt = wtx
                    .open_table(MESSAGES)
                    .map_err(|e| Error::Storage(e.to_string()))?;
                if mt
                    .get(message.id.0.as_str())
                    .map_err(|e| Error::Storage(e.to_string()))?
                    .is_some()
                {
                    return Ok(()); // idempotent
                }
                let bytes = serde_json::to_vec(message)?;
                mt.insert(message.id.0.as_str(), bytes.as_slice())
                    .map_err(|e| Error::Storage(e.to_string()))?;

                let mut idx_table = wtx
                    .open_table(BOARD_INDEX)
                    .map_err(|e| Error::Storage(e.to_string()))?;
                let mut ids: Vec<MessageId> = match idx_table
                    .get(message.body.board.as_str())
                    .map_err(|e| Error::Storage(e.to_string()))?
                {
                    Some(v) => serde_json::from_slice(v.value())?,
                    None => vec![],
                };
                ids.push(message.id.clone());
                let idx_bytes = serde_json::to_vec(&ids)?;
                idx_table
                    .insert(message.body.board.as_str(), idx_bytes.as_slice())
                    .map_err(|e| Error::Storage(e.to_string()))?;
            }
            wtx.commit().map_err(|e| Error::Storage(e.to_string()))?;
            Ok(())
        }

        fn get_message(&self, id: &MessageId) -> Result<Option<Message>> {
            let rtx = self
                .db
                .begin_read()
                .map_err(|e| Error::Storage(e.to_string()))?;
            let t = rtx
                .open_table(MESSAGES)
                .map_err(|e| Error::Storage(e.to_string()))?;
            match t
                .get(id.0.as_str())
                .map_err(|e| Error::Storage(e.to_string()))?
            {
                Some(v) => Ok(Some(serde_json::from_slice(v.value())?)),
                None => Ok(None),
            }
        }

        fn list_messages(&self, board: &str, limit: usize) -> Result<Vec<Message>> {
            let ids = self.read_index(board)?;
            let start = ids.len().saturating_sub(limit);
            let mut out = Vec::new();
            for id in &ids[start..] {
                if let Some(m) = self.get_message(id)? {
                    out.push(m);
                }
            }
            Ok(out)
        }

        fn message_count(&self) -> Result<usize> {
            let rtx = self
                .db
                .begin_read()
                .map_err(|e| Error::Storage(e.to_string()))?;
            let t = rtx
                .open_table(MESSAGES)
                .map_err(|e| Error::Storage(e.to_string()))?;
            Ok(t.len().map_err(|e| Error::Storage(e.to_string()))? as usize)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{Board, Message, MessageBody};
    use crate::identity::Identity;
    use chrono::Utc;

    fn msg(id: &Identity, board: &str, text: &str) -> Message {
        let body = MessageBody {
            board: board.into(),
            parent: None,
            subject: "s".into(),
            body: text.into(),
            author: id.id(),
            handle: "h".into(),
            created_at: Utc::now(),
        };
        Message::sign(id, body).unwrap()
    }

    fn exercise(store: &dyn Store) {
        let id = Identity::generate();
        let board = Board::new("general", "General", id.id());
        store.put_board(&board).unwrap();
        assert_eq!(store.list_boards().unwrap().len(), 1);

        let m1 = msg(&id, "general", "one");
        let m2 = msg(&id, "general", "two");
        store.put_message(&m1).unwrap();
        store.put_message(&m2).unwrap();
        // idempotent replay
        store.put_message(&m1).unwrap();
        assert_eq!(store.message_count().unwrap(), 2);

        let listed = store.list_messages("general", 10).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].body.body, "one");
        assert!(store.get_message(&m1.id).unwrap().is_some());
        assert_eq!(store.list_messages("nope", 10).unwrap().len(), 0);
    }

    #[test]
    fn memory_store_roundtrip() {
        exercise(&MemoryStore::new());
    }

    #[cfg(feature = "native")]
    #[test]
    fn redb_store_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = RedbStore::open(dir.path().join("bbs.redb")).unwrap();
        exercise(&store);
    }
}
