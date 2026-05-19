//! Mutual friend relationships. A `Friendship` row records the directed
//! request: `requester_id` asked, `addressee_id` may or may not have accepted.
//! `accepted_at IS NULL` means the request is pending; non-NULL means mutual.
//!
//! Lookups about "is user X a friend of user Y?" are direction-agnostic: the
//! pair is normalised through `LEAST`/`GREATEST` for the unique index, and
//! `status()` checks both directions.

use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Friendship {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    pub requester_id: Uuid,
    pub addressee_id: Uuid,
    pub accepted_at: Option<DateTime<Utc>>,
}

impl From<tokio_postgres::Row> for Friendship {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            id: row.get("id"),
            created: row.get("created"),
            updated: row.get("updated"),
            requester_id: row.get("requester_id"),
            addressee_id: row.get("addressee_id"),
            accepted_at: row.get("accepted_at"),
        }
    }
}

/// The relationship between two users from a single user's point of view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FriendshipStatus {
    /// No edge in either direction.
    None,
    /// You sent a request, awaiting their acceptance.
    OutgoingPending,
    /// They sent you a request, awaiting your acceptance.
    IncomingPending,
    /// Mutual friendship.
    Friends,
}

#[derive(Clone, Debug)]
pub struct FriendSummary {
    pub user_id: Uuid,
    pub username: String,
    pub since: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct PendingRequest {
    pub other_user_id: Uuid,
    pub other_username: String,
    pub created: DateTime<Utc>,
}

/// Outcome of [`Friendship::send_request`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendOutcome {
    /// A new pending request was created.
    Sent,
    /// The other user had already sent us a pending request; that request was
    /// accepted instead of creating a new one.
    AutoAccepted,
    /// A pending or accepted edge already existed in this direction.
    AlreadyExists,
    /// Self-request — silently ignored.
    SelfRequest,
}

impl Friendship {
    /// Send a friend request from `requester` to `addressee`. If a reverse
    /// pending request exists (the addressee already asked us), it is
    /// auto-accepted. Self-requests are a silent no-op. Already-existing
    /// edges in either direction are a no-op.
    pub async fn send_request(
        client: &Client,
        requester: Uuid,
        addressee: Uuid,
    ) -> Result<SendOutcome> {
        if requester == addressee {
            return Ok(SendOutcome::SelfRequest);
        }

        let existing = client
            .query_opt(
                "SELECT * FROM friendships
                 WHERE (requester_id = $1 AND addressee_id = $2)
                    OR (requester_id = $2 AND addressee_id = $1)",
                &[&requester, &addressee],
            )
            .await?;

        if let Some(row) = existing {
            let edge = Self::from(row);
            if edge.accepted_at.is_some() {
                return Ok(SendOutcome::AlreadyExists);
            }
            if edge.requester_id == addressee {
                client
                    .execute(
                        "UPDATE friendships
                         SET accepted_at = current_timestamp,
                             updated = current_timestamp
                         WHERE id = $1",
                        &[&edge.id],
                    )
                    .await?;
                return Ok(SendOutcome::AutoAccepted);
            }
            return Ok(SendOutcome::AlreadyExists);
        }

        client
            .execute(
                "INSERT INTO friendships (requester_id, addressee_id) VALUES ($1, $2)",
                &[&requester, &addressee],
            )
            .await?;
        Ok(SendOutcome::Sent)
    }

    /// Accept a pending incoming request. Returns `true` if a pending edge
    /// was found and accepted; `false` if there was nothing pending.
    pub async fn accept(client: &Client, acceptor: Uuid, requester: Uuid) -> Result<bool> {
        let rows = client
            .execute(
                "UPDATE friendships
                 SET accepted_at = current_timestamp,
                     updated = current_timestamp
                 WHERE requester_id = $1
                   AND addressee_id = $2
                   AND accepted_at IS NULL",
                &[&requester, &acceptor],
            )
            .await?;
        Ok(rows > 0)
    }

    /// Decline an incoming pending request (or cancel an outgoing one — same
    /// row, same effect). Returns the number of rows deleted (0 or 1).
    pub async fn decline_or_cancel(client: &Client, user: Uuid, other: Uuid) -> Result<u64> {
        let rows = client
            .execute(
                "DELETE FROM friendships
                 WHERE accepted_at IS NULL
                   AND ((requester_id = $1 AND addressee_id = $2)
                     OR (requester_id = $2 AND addressee_id = $1))",
                &[&user, &other],
            )
            .await?;
        Ok(rows)
    }

    /// Remove an existing mutual friendship. Returns the number of rows
    /// deleted (0 or 1).
    pub async fn unfriend(client: &Client, user: Uuid, other: Uuid) -> Result<u64> {
        let rows = client
            .execute(
                "DELETE FROM friendships
                 WHERE accepted_at IS NOT NULL
                   AND ((requester_id = $1 AND addressee_id = $2)
                     OR (requester_id = $2 AND addressee_id = $1))",
                &[&user, &other],
            )
            .await?;
        Ok(rows)
    }

    /// Direction-agnostic status from `user`'s point of view.
    pub async fn status(client: &Client, user: Uuid, other: Uuid) -> Result<FriendshipStatus> {
        if user == other {
            return Ok(FriendshipStatus::None);
        }
        let row = client
            .query_opt(
                "SELECT requester_id, addressee_id, accepted_at FROM friendships
                 WHERE (requester_id = $1 AND addressee_id = $2)
                    OR (requester_id = $2 AND addressee_id = $1)",
                &[&user, &other],
            )
            .await?;
        let Some(row) = row else {
            return Ok(FriendshipStatus::None);
        };
        let accepted_at: Option<DateTime<Utc>> = row.get("accepted_at");
        if accepted_at.is_some() {
            return Ok(FriendshipStatus::Friends);
        }
        let requester: Uuid = row.get("requester_id");
        Ok(if requester == user {
            FriendshipStatus::OutgoingPending
        } else {
            FriendshipStatus::IncomingPending
        })
    }

    /// All accepted friends of `user`, joined to usernames, newest acceptance
    /// first.
    pub async fn list_friends(client: &Client, user: Uuid) -> Result<Vec<FriendSummary>> {
        let rows = client
            .query(
                "SELECT u.id AS other_id, u.username, f.accepted_at AS since
                 FROM friendships f
                 JOIN users u
                   ON u.id = CASE WHEN f.requester_id = $1 THEN f.addressee_id ELSE f.requester_id END
                 WHERE (f.requester_id = $1 OR f.addressee_id = $1)
                   AND f.accepted_at IS NOT NULL
                 ORDER BY f.accepted_at DESC",
                &[&user],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| FriendSummary {
                user_id: row.get("other_id"),
                username: row.get("username"),
                since: row.get("since"),
            })
            .collect())
    }

    /// Pending requests where the addressee is `user` (incoming).
    pub async fn list_incoming(client: &Client, user: Uuid) -> Result<Vec<PendingRequest>> {
        let rows = client
            .query(
                "SELECT u.id AS other_id, u.username, f.created AS created
                 FROM friendships f
                 JOIN users u ON u.id = f.requester_id
                 WHERE f.addressee_id = $1 AND f.accepted_at IS NULL
                 ORDER BY f.created DESC",
                &[&user],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| PendingRequest {
                other_user_id: row.get("other_id"),
                other_username: row.get("username"),
                created: row.get("created"),
            })
            .collect())
    }

    /// Pending requests where the requester is `user` (outgoing).
    pub async fn list_outgoing(client: &Client, user: Uuid) -> Result<Vec<PendingRequest>> {
        let rows = client
            .query(
                "SELECT u.id AS other_id, u.username, f.created AS created
                 FROM friendships f
                 JOIN users u ON u.id = f.addressee_id
                 WHERE f.requester_id = $1 AND f.accepted_at IS NULL
                 ORDER BY f.created DESC",
                &[&user],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| PendingRequest {
                other_user_id: row.get("other_id"),
                other_username: row.get("username"),
                created: row.get("created"),
            })
            .collect())
    }
}
