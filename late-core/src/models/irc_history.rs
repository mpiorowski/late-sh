//! Read-only IRC history: CHATHISTORY selections turned into bounded page
//! reads. The queries live with their tables (`chat_message.rs` for pages
//! and anchors, `chat_room.rs` for access and target discovery); every one
//! enforces the viewer's current access, and the session supplies only the
//! IDs of channels it has actually joined.

use chrono::{DateTime, Duration, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

use super::{
    chat_message::{ChatMessage, IrcHistoryBound, IrcHistoryMessage},
    chat_room::{ChatRoom, IrcHistoryTarget},
};

pub const MAX_MESSAGES: usize = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reference {
    Timestamp(DateTime<Utc>),
    MessageId(Uuid),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    Latest(Option<Reference>),
    Before(Reference),
    After(Reference),
    Around(Reference),
    Between(Reference, Reference),
}

#[derive(Debug)]
pub enum ReadError {
    InvalidTarget,
    InvalidReference,
    Database(anyhow::Error),
}

impl From<anyhow::Error> for ReadError {
    fn from(error: anyhow::Error) -> Self {
        Self::Database(error)
    }
}

type Result<T> = std::result::Result<T, ReadError>;

pub struct Page<T> {
    pub items: Vec<T>,
    pub exhausted: bool,
}

pub struct HistoryReader<'a> {
    pub client: &'a Client,
    pub viewer: Uuid,
    pub joined_channels: &'a [Uuid],
    pub ignored_authors: &'a [Uuid],
}

#[derive(Clone, Copy)]
struct Cursor {
    created: DateTime<Utc>,
    id: Option<Uuid>,
}

impl Cursor {
    fn pivot(self) -> IrcHistoryBound {
        IrcHistoryBound {
            created: self.created,
            id: self.id.unwrap_or(Uuid::nil()),
            inclusive: true,
        }
    }

    fn lower(self) -> IrcHistoryBound {
        match self.id {
            Some(id) => IrcHistoryBound {
                created: self.created,
                id,
                inclusive: false,
            },
            // Timestamp selectors exclude the entire millisecond represented
            // by server-time, while UUID selectors retain full DB precision.
            None => IrcHistoryBound {
                created: self.created + Duration::milliseconds(1),
                id: Uuid::nil(),
                inclusive: true,
            },
        }
    }

    fn upper(self) -> IrcHistoryBound {
        IrcHistoryBound {
            inclusive: false,
            ..self.pivot()
        }
    }

    fn key(self) -> (DateTime<Utc>, Uuid) {
        (self.created, self.id.unwrap_or(Uuid::nil()))
    }
}

impl HistoryReader<'_> {
    async fn resolve(&self, room_id: Uuid, reference: Reference) -> Result<Cursor> {
        match reference {
            Reference::Timestamp(created) => Ok(Cursor { created, id: None }),
            Reference::MessageId(id) => {
                let created = ChatMessage::irc_history_anchor(
                    self.client,
                    self.viewer,
                    self.joined_channels,
                    self.ignored_authors,
                    room_id,
                    id,
                )
                .await?;
                match created {
                    Some(created) => Ok(Cursor {
                        created,
                        id: Some(id),
                    }),
                    None => Err(ReadError::InvalidReference),
                }
            }
        }
    }

    /// Read at most `limit` stored messages, in chronological order.
    pub async fn messages(
        &self,
        room_id: Uuid,
        selection: Selection,
        limit: usize,
    ) -> Result<Page<IrcHistoryMessage>> {
        let accessible = ChatRoom::irc_history_accessible(
            self.client,
            self.viewer,
            self.joined_channels,
            room_id,
        )
        .await?;
        if !accessible {
            return Err(ReadError::InvalidTarget);
        }
        let limit = limit.clamp(1, MAX_MESSAGES);
        let (lower, upper, descending) = match selection {
            Selection::Latest(None) => (None, None, true),
            Selection::Latest(Some(reference)) => (
                Some(self.resolve(room_id, reference).await?.lower()),
                None,
                true,
            ),
            Selection::Before(reference) => (
                None,
                Some(self.resolve(room_id, reference).await?.upper()),
                true,
            ),
            Selection::After(reference) => (
                Some(self.resolve(room_id, reference).await?.lower()),
                None,
                false,
            ),
            Selection::Between(first, second) => {
                let first = self.resolve(room_id, first).await?;
                let second = self.resolve(room_id, second).await?;
                if first.key() <= second.key() {
                    (Some(first.lower()), Some(second.upper()), false)
                } else {
                    (Some(second.lower()), Some(first.upper()), true)
                }
            }
            Selection::Around(reference) => {
                let pivot = self.resolve(room_id, reference).await?;
                let mut before = self
                    .range(room_id, None, Some(pivot.upper()), true, limit + 1)
                    .await?;
                let mut after = self
                    .range(room_id, Some(pivot.pivot()), None, false, limit + 1)
                    .await?;
                let mut before_count = before.len().min(limit / 2);
                let after_count = after.len().min(limit - before_count);
                before_count = before.len().min(limit - after_count);
                let exhausted = before.len() + after.len() <= limit;
                before.truncate(before_count);
                before.reverse();
                after.truncate(after_count);
                before.extend(after);
                return Ok(Page {
                    items: before,
                    exhausted,
                });
            }
        };
        let mut items = self
            .range(room_id, lower, upper, descending, limit + 1)
            .await?;
        let exhausted = items.len() <= limit;
        items.truncate(limit);
        if descending {
            items.reverse();
        }
        Ok(Page { items, exhausted })
    }

    async fn range(
        &self,
        room_id: Uuid,
        lower: Option<IrcHistoryBound>,
        upper: Option<IrcHistoryBound>,
        descending: bool,
        limit: usize,
    ) -> Result<Vec<IrcHistoryMessage>> {
        let items = ChatMessage::irc_history_range(
            self.client,
            self.viewer,
            self.joined_channels,
            self.ignored_authors,
            room_id,
            lower,
            upper,
            descending,
            limit as i64,
        )
        .await?;
        Ok(items)
    }

    /// Discover buffers by their latest visible message, not any message in
    /// the interval. Both timestamps are exclusive, to the server-time
    /// millisecond; `first > second` selects from the newer end.
    pub async fn targets(
        &self,
        first: DateTime<Utc>,
        second: DateTime<Utc>,
        limit: usize,
    ) -> Result<Page<IrcHistoryTarget>> {
        let descending = first > second;
        let (lower, upper) = if descending {
            (second, first)
        } else {
            (first, second)
        };
        let limit = limit.clamp(1, MAX_MESSAGES);
        let rows = ChatRoom::irc_history_targets(
            self.client,
            self.viewer,
            self.joined_channels,
            self.ignored_authors,
            lower + Duration::milliseconds(1),
            upper,
            descending,
            (limit + 1) as i64,
        )
        .await?;
        let exhausted = rows.len() <= limit;
        let items = rows.into_iter().take(limit).collect();
        Ok(Page { items, exhausted })
    }
}
