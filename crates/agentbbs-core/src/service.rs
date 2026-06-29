//! The `Bbs` service — capability-enforcing operations over a [`Store`],
//! emitting [`Event`]s to a [`Reporter`]. This is the seam every front end
//! (TUI, SSH, MCP, federation) drives so authorization and reporting live in
//! exactly one place.

use std::sync::Arc;

use crate::board::{Board, Message, MessageBody, MessageId};
use crate::caps::{require, Caps};
use crate::error::{Error, Result};
use crate::identity::AgentId;
use crate::report::{Event, EventKind, MemoryReporter, Reporter};
use crate::store::Store;

/// The capability-enforcing BBS façade.
pub struct Bbs {
    store: Arc<dyn Store>,
    reporter: Arc<dyn Reporter>,
}

impl Bbs {
    /// Build a service over `store`, reporting to `reporter`.
    pub fn new(store: Arc<dyn Store>, reporter: Arc<dyn Reporter>) -> Self {
        Bbs { store, reporter }
    }

    /// Convenience constructor with an in-memory reporter.
    pub fn with_memory_reporter(store: Arc<dyn Store>) -> (Self, Arc<MemoryReporter>) {
        let reporter = Arc::new(MemoryReporter::default());
        (
            Bbs {
                store,
                reporter: reporter.clone(),
            },
            reporter,
        )
    }

    /// Underlying store handle (read paths that need no capability).
    pub fn store(&self) -> &Arc<dyn Store> {
        &self.store
    }

    fn emit(&self, event: Event) {
        // Reporting must never break a domain operation.
        let _ = self.reporter.report(event);
    }

    /// Create a board. Requires [`Caps::CREATE_BOARD`].
    pub fn create_board(&self, caps: Caps, board: Board) -> Result<()> {
        require(caps, Caps::CREATE_BOARD, "CREATE_BOARD")?;
        if self.store.get_board(&board.slug)?.is_some() {
            return Err(Error::AlreadyExists(format!("board {}", board.slug)));
        }
        self.store.put_board(&board)?;
        self.emit(
            Event::now(EventKind::BoardCreate, board.slug.clone())
                .by(board.founder)
                .with(serde_json::json!({ "title": board.title })),
        );
        Ok(())
    }

    /// List boards. Requires [`Caps::READ`].
    pub fn list_boards(&self, caps: Caps) -> Result<Vec<Board>> {
        require(caps, Caps::READ, "READ")?;
        self.store.list_boards()
    }

    /// Read recent messages on a board. Requires [`Caps::READ`].
    pub fn read_board(&self, caps: Caps, slug: &str, limit: usize) -> Result<Vec<Message>> {
        require(caps, Caps::READ, "READ")?;
        self.store.list_messages(slug, limit)
    }

    /// Post a pre-signed message. Requires [`Caps::POST`]. The message must
    /// verify, target an existing unlocked board, and be authored by `caps`'
    /// holder is *not* assumed — authorship is proven by the signature.
    pub fn post(&self, caps: Caps, message: Message) -> Result<MessageId> {
        require(caps, Caps::POST, "POST")?;
        message.verify().map_err(|_| {
            self.emit(
                Event::now(EventKind::Security, "post.bad_signature")
                    .by(message.body.author),
            );
            Error::BadSignature
        })?;
        let board = self
            .store
            .get_board(&message.body.board)?
            .ok_or_else(|| Error::NotFound(format!("board {}", message.body.board)))?;
        if board.locked {
            return Err(Error::PermissionDenied("board locked"));
        }
        let id = message.id.clone();
        self.store.put_message(&message)?;
        self.emit(
            Event::now(EventKind::Post, message.body.board.clone())
                .by(message.body.author)
                .with(serde_json::json!({ "id": id.0, "subject": message.body.subject })),
        );
        Ok(id)
    }

    /// Helper that builds, signs, and posts a message from an owned identity.
    pub fn post_text(
        &self,
        caps: Caps,
        identity: &crate::identity::Identity,
        board: &str,
        subject: &str,
        text: &str,
    ) -> Result<MessageId> {
        let body = MessageBody {
            board: board.into(),
            parent: None,
            subject: subject.into(),
            body: text.into(),
            author: identity.id(),
            handle: String::new(),
            created_at: chrono::Utc::now(),
        };
        let message = Message::sign(identity, body)?;
        self.post(caps, message)
    }

    /// Lock or unlock a board. Requires [`Caps::MODERATE`].
    pub fn set_locked(&self, caps: Caps, slug: &str, locked: bool, by: AgentId) -> Result<()> {
        require(caps, Caps::MODERATE, "MODERATE")?;
        let mut board = self
            .store
            .get_board(slug)?
            .ok_or_else(|| Error::NotFound(format!("board {slug}")))?;
        board.locked = locked;
        self.store.put_board(&board)?;
        self.emit(
            Event::now(EventKind::Moderation, slug.to_string())
                .by(by)
                .with(serde_json::json!({ "action": "set_locked", "locked": locked })),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caps::Role;
    use crate::identity::Identity;
    use crate::store::MemoryStore;

    fn svc() -> (Bbs, Arc<MemoryReporter>) {
        Bbs::with_memory_reporter(Arc::new(MemoryStore::new()))
    }

    #[test]
    fn post_requires_existing_board() {
        let (bbs, _) = svc();
        let id = Identity::generate();
        let err = bbs.post_text(Caps::default(), &id, "ghost", "s", "hi");
        assert!(matches!(err, Err(Error::NotFound(_))));
    }

    #[test]
    fn full_flow_with_reporting() {
        let (bbs, rep) = svc();
        let sysop = Identity::generate();
        let agent = Identity::generate();

        bbs.create_board(Role::Sysop.caps(), Board::new("general", "General", sysop.id()))
            .unwrap();
        let id = bbs
            .post_text(Caps::default(), &agent, "general", "hello", "first post")
            .unwrap();
        assert!(!id.0.is_empty());

        let msgs = bbs.read_board(Caps::READ, "general", 10).unwrap();
        assert_eq!(msgs.len(), 1);

        // BoardCreate + Post reported.
        let kinds: Vec<_> = rep.snapshot().iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&EventKind::BoardCreate));
        assert!(kinds.contains(&EventKind::Post));
    }

    #[test]
    fn guest_cannot_post() {
        let (bbs, _) = svc();
        let sysop = Identity::generate();
        bbs.create_board(Role::Sysop.caps(), Board::new("g", "G", sysop.id()))
            .unwrap();
        let agent = Identity::generate();
        let err = bbs.post_text(Role::Guest.caps(), &agent, "g", "s", "x");
        assert!(matches!(err, Err(Error::PermissionDenied(_))));
    }

    #[test]
    fn locked_board_rejects_posts() {
        let (bbs, _) = svc();
        let sysop = Identity::generate();
        bbs.create_board(Role::Sysop.caps(), Board::new("g", "G", sysop.id()))
            .unwrap();
        bbs.set_locked(Role::Moderator.caps(), "g", true, sysop.id())
            .unwrap();
        let agent = Identity::generate();
        let err = bbs.post_text(Caps::default(), &agent, "g", "s", "x");
        assert!(matches!(err, Err(Error::PermissionDenied(_))));
    }
}
