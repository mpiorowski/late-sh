//! Durable content classification, separate from applause and monthly awards.

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use deadpool_postgres::GenericClient;
use tokio_postgres::Row;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ArtContentRating {
    #[default]
    Sfw,
    Nsfw,
}

impl ArtContentRating {
    pub fn label(self) -> &'static str {
        match self {
            Self::Sfw => "SFW",
            Self::Nsfw => "NSFW",
        }
    }

    pub fn is_nsfw(self) -> bool {
        self == Self::Nsfw
    }

    pub fn from_nsfw(nsfw: bool) -> Self {
        if nsfw { Self::Nsfw } else { Self::Sfw }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RatingSource {
    Admin,
    Moderator,
    Owner,
    Community,
    Default,
}

impl RatingSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Admin => "admin marks",
            Self::Moderator => "moderator marks",
            Self::Owner => "owner flag",
            Self::Community => "community votes",
            Self::Default => "unmarked",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContentRatingSummary {
    pub owner_marked_nsfw: bool,
    pub sfw_votes: i64,
    pub nsfw_votes: i64,
    pub mod_sfw: i64,
    pub mod_nsfw: i64,
    pub admin_sfw: i64,
    pub admin_nsfw: i64,
    pub viewer_vote: Option<ArtContentRating>,
}

impl ContentRatingSummary {
    pub(crate) fn from_row(row: &Row) -> Self {
        Self {
            owner_marked_nsfw: row.get("owner_marked_nsfw"),
            sfw_votes: row.get("sfw_votes"),
            nsfw_votes: row.get("nsfw_votes"),
            mod_sfw: row.get("mod_sfw"),
            mod_nsfw: row.get("mod_nsfw"),
            admin_sfw: row.get("admin_sfw"),
            admin_nsfw: row.get("admin_nsfw"),
            viewer_vote: row
                .get::<_, Option<bool>>("viewer_content_vote")
                .map(ArtContentRating::from_nsfw),
        }
    }

    pub fn determination(self) -> (ArtContentRating, RatingSource) {
        let (nsfw, source) = if self.admin_sfw + self.admin_nsfw > 0 {
            (self.admin_nsfw > 0, RatingSource::Admin)
        } else if self.mod_sfw + self.mod_nsfw > 0 {
            (self.mod_nsfw >= self.mod_sfw, RatingSource::Moderator)
        } else if self.owner_marked_nsfw {
            (true, RatingSource::Owner)
        } else if self.sfw_votes + self.nsfw_votes > 0 {
            (
                self.nsfw_votes >= 2 && self.nsfw_votes > self.sfw_votes,
                RatingSource::Community,
            )
        } else {
            (false, RatingSource::Default)
        };
        (ArtContentRating::from_nsfw(nsfw), source)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaffAuthority {
    Moderator,
    Admin,
}

impl StaffAuthority {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Moderator => "moderator",
            Self::Admin => "admin",
        }
    }

    /// The column is CHECKed to these two spellings (migration 216).
    fn from_db(value: &str) -> Self {
        match value {
            "moderator" => Self::Moderator,
            "admin" => Self::Admin,
            other => panic!("unknown staff authority in the database: {other}"),
        }
    }
}

/// What a community vote did. A refusal is an outcome, not an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoteOutcome {
    Saved,
    OwnPiece,
    NotFound,
}

/// What a write to the hanger's own NSFW flag did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerFlagOutcome {
    Saved,
    NotYours,
    NotFound,
}

/// What a staff mark write did. `owner` is the piece's hanger, for the audit
/// row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaffMarkOutcome {
    Saved { owner: Uuid },
    NotFound,
}

/// What removing another actor's staff mark did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoveMarkOutcome {
    Removed {
        owner: Uuid,
        authority: StaffAuthority,
    },
    NoMark,
    NotFound,
}

pub struct StaffMark {
    pub actor_user_id: Uuid,
    pub username: Option<String>,
    pub authority: StaffAuthority,
    pub rating: ArtContentRating,
    pub reason: String,
    pub updated: DateTime<Utc>,
}

/// One community vote with its voter named. Staff only: other users see
/// counts, never who voted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentVote {
    pub user_id: Uuid,
    pub username: String,
    pub rating: ArtContentRating,
}

pub struct ArtboardPieceRating;

pub struct ArtSafetyPiece {
    pub id: Uuid,
    pub title: String,
    pub username: String,
    pub splash_on: Option<NaiveDate>,
    pub summary: ContentRatingSummary,
}

impl ArtboardPieceRating {
    /// Staff overview: metadata and ratings in one query, without canvases.
    pub async fn list_safety(
        client: &impl GenericClient,
        owner: Option<Uuid>,
    ) -> Result<Vec<ArtSafetyPiece>> {
        Ok(client
            .query(
                "SELECT p.id, p.title, u.username, p.splash_on, r.*,
                        NULL::boolean AS viewer_content_vote
                 FROM artboard_pieces p
                 JOIN users u ON u.id = p.user_id
                 JOIN artboard_piece_content_ratings r ON r.piece_id = p.id
                 WHERE p.removed_at IS NULL AND ($1::uuid IS NULL OR p.user_id = $1)
                 ORDER BY p.created DESC, p.id DESC",
                &[&owner],
            )
            .await?
            .into_iter()
            .map(|row| ArtSafetyPiece {
                id: row.get("id"),
                title: row.get("title"),
                username: row.get("username"),
                splash_on: row.get("splash_on"),
                summary: ContentRatingSummary::from_row(&row),
            })
            .collect())
    }

    pub async fn read(
        client: &impl GenericClient,
        piece_id: Uuid,
        viewer_id: Uuid,
    ) -> Result<Option<ContentRatingSummary>> {
        Self::read_for_day(client, piece_id, viewer_id, None).await
    }

    /// Login reads metadata only, checking the day and removal in the same
    /// statement as the classification. The canvas stays cached.
    pub async fn read_for_day(
        client: &impl GenericClient,
        piece_id: Uuid,
        viewer_id: Uuid,
        day: Option<NaiveDate>,
    ) -> Result<Option<ContentRatingSummary>> {
        let row = client
            .query_opt(
                "SELECT r.*, (SELECT nsfw FROM artboard_piece_content_votes
                          WHERE piece_id = p.id AND user_id = $2) AS viewer_content_vote
             FROM artboard_pieces p JOIN artboard_piece_content_ratings r ON r.piece_id = p.id
             WHERE p.id = $1 AND p.removed_at IS NULL
               AND ($3::date IS NULL OR p.splash_on = $3)",
                &[&piece_id, &viewer_id, &day],
            )
            .await?;
        Ok(row.as_ref().map(ContentRatingSummary::from_row))
    }

    /// Call inside a transaction; serializes classification writers with
    /// removal so a stale session cannot write to a piece already taken down.
    /// The hanger's id, or `None` when the piece is no longer hanging.
    async fn lock_piece(client: &impl GenericClient, piece_id: Uuid) -> Result<Option<Uuid>> {
        let row = client
            .query_opt(
                "SELECT user_id FROM artboard_pieces
                 WHERE id = $1 AND removed_at IS NULL FOR UPDATE",
                &[&piece_id],
            )
            .await?;
        Ok(row.map(|row| row.get("user_id")))
    }

    pub async fn set_vote(
        client: &impl GenericClient,
        piece_id: Uuid,
        user_id: Uuid,
        rating: Option<ArtContentRating>,
    ) -> Result<VoteOutcome> {
        let Some(owner) = Self::lock_piece(client, piece_id).await? else {
            return Ok(VoteOutcome::NotFound);
        };
        if owner == user_id {
            return Ok(VoteOutcome::OwnPiece);
        }
        match rating {
            Some(rating) => {
                client
                    .execute(
                        "INSERT INTO artboard_piece_content_votes
                             (piece_id, user_id, author_user_id, nsfw)
                         VALUES ($1, $2, $3, $4)
                         ON CONFLICT (piece_id, user_id) DO UPDATE SET nsfw = EXCLUDED.nsfw",
                        &[&piece_id, &user_id, &owner, &rating.is_nsfw()],
                    )
                    .await?;
            }
            None => {
                client
                    .execute(
                        "DELETE FROM artboard_piece_content_votes
                         WHERE piece_id = $1 AND user_id = $2",
                        &[&piece_id, &user_id],
                    )
                    .await?;
            }
        }
        Ok(VoteOutcome::Saved)
    }

    pub async fn set_owner_flag(
        client: &impl GenericClient,
        piece_id: Uuid,
        user_id: Uuid,
        nsfw: bool,
    ) -> Result<OwnerFlagOutcome> {
        let Some(owner) = Self::lock_piece(client, piece_id).await? else {
            return Ok(OwnerFlagOutcome::NotFound);
        };
        if owner != user_id {
            return Ok(OwnerFlagOutcome::NotYours);
        }
        client
            .execute(
                "UPDATE artboard_pieces SET owner_marked_nsfw = $3 WHERE id = $1 AND user_id = $2",
                &[&piece_id, &user_id, &nsfw],
            )
            .await?;
        Ok(OwnerFlagOutcome::Saved)
    }

    /// Set (`Some`) or clear (`None`) the actor's own mark at `authority`.
    pub async fn set_staff_mark(
        client: &impl GenericClient,
        piece_id: Uuid,
        actor_user_id: Uuid,
        authority: StaffAuthority,
        rating: Option<ArtContentRating>,
        reason: &str,
    ) -> Result<StaffMarkOutcome> {
        let Some(owner) = Self::lock_piece(client, piece_id).await? else {
            return Ok(StaffMarkOutcome::NotFound);
        };
        match rating {
            Some(rating) => {
                client
                    .execute(
                        "INSERT INTO artboard_piece_staff_marks
                             (piece_id, actor_user_id, authority, nsfw, reason)
                         VALUES ($1, $2, $3, $4, $5)
                         ON CONFLICT (piece_id, actor_user_id) DO UPDATE
                         SET authority = EXCLUDED.authority, nsfw = EXCLUDED.nsfw,
                             reason = EXCLUDED.reason, updated = current_timestamp",
                        &[
                            &piece_id,
                            &actor_user_id,
                            &authority.as_str(),
                            &rating.is_nsfw(),
                            &reason,
                        ],
                    )
                    .await?;
            }
            None => {
                client
                    .execute(
                        "DELETE FROM artboard_piece_staff_marks
                         WHERE piece_id = $1 AND actor_user_id = $2 AND authority = $3",
                        &[&piece_id, &actor_user_id, &authority.as_str()],
                    )
                    .await?;
            }
        }
        Ok(StaffMarkOutcome::Saved { owner })
    }

    /// Remove the mark `actor_user_id` left on the piece, at whichever tier
    /// it holds. Marks outlive the actor's role and account, so this is the
    /// only way a mark by a demoted or deleted admin ever comes off.
    pub async fn remove_staff_mark(
        client: &impl GenericClient,
        piece_id: Uuid,
        actor_user_id: Uuid,
    ) -> Result<RemoveMarkOutcome> {
        let Some(owner) = Self::lock_piece(client, piece_id).await? else {
            return Ok(RemoveMarkOutcome::NotFound);
        };
        let row = client
            .query_opt(
                "DELETE FROM artboard_piece_staff_marks
                 WHERE piece_id = $1 AND actor_user_id = $2
                 RETURNING authority",
                &[&piece_id, &actor_user_id],
            )
            .await?;
        match row {
            Some(row) => Ok(RemoveMarkOutcome::Removed {
                owner,
                authority: StaffAuthority::from_db(row.get("authority")),
            }),
            None => Ok(RemoveMarkOutcome::NoMark),
        }
    }

    /// Who voted on a piece, NSFW votes first, then by name.
    pub async fn content_votes(
        client: &impl GenericClient,
        piece_id: Uuid,
    ) -> Result<Vec<ContentVote>> {
        Ok(client
            .query(
                "SELECT v.user_id, u.username, v.nsfw
                 FROM artboard_piece_content_votes v
                 JOIN users u ON u.id = v.user_id
                 WHERE v.piece_id = $1
                 ORDER BY v.nsfw DESC, LOWER(u.username), v.user_id",
                &[&piece_id],
            )
            .await?
            .into_iter()
            .map(|row| ContentVote {
                user_id: row.get("user_id"),
                username: row.get("username"),
                rating: ArtContentRating::from_nsfw(row.get("nsfw")),
            })
            .collect())
    }

    pub async fn staff_marks(
        client: &impl GenericClient,
        piece_id: Uuid,
    ) -> Result<Vec<StaffMark>> {
        Ok(client
            .query(
                "SELECT m.*, u.username FROM artboard_piece_staff_marks m
             LEFT JOIN users u ON u.id = m.actor_user_id
             WHERE m.piece_id = $1 ORDER BY m.authority, m.updated, m.actor_user_id",
                &[&piece_id],
            )
            .await?
            .into_iter()
            .map(|row| StaffMark {
                actor_user_id: row.get("actor_user_id"),
                username: row.get("username"),
                authority: StaffAuthority::from_db(row.get("authority")),
                rating: ArtContentRating::from_nsfw(row.get("nsfw")),
                reason: row.get("reason"),
                updated: row.get("updated"),
            })
            .collect())
    }
}

#[cfg(test)]
#[path = "artboard_piece_rating_test.rs"]
mod tests;
