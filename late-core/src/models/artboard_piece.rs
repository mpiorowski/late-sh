//! The Artboard gallery: pieces hung off the shared board, and the applause
//! they gather.
//!
//! A piece is an immutable crop of the live board taken the moment it was
//! hung, so the monthly wipe and the next vandal cannot touch it. The crop,
//! the ownership share, and the content hash are computed by the hanger
//! (`late-ssh` `app/artboard/gallery/frame.rs`); this module owns every read
//! and write of `artboard_pieces` and `artboard_piece_votes` (migration 174)
//! and the rails that are enforced in SQL: the daily cap, the per-month
//! duplicate refusal, one applause per person, and no applause for your own
//! piece.

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use deadpool_postgres::GenericClient;
use serde_json::Value;
use tokio_postgres::{Row, error::SqlState};
use uuid::Uuid;

/// Fewest non-blank glyphs a frame may hold. A smiley is not a piece.
pub const PIECE_MIN_GLYPHS: usize = 40;
/// Largest frame, in cells. Every piece fits a terminal, which is what makes
/// the splash, the paper, and the profile able to show one without cropping.
pub const PIECE_MAX_WIDTH: usize = 100;
pub const PIECE_MAX_HEIGHT: usize = 40;
/// The share of a frame's glyphs the hanger must have painted, per cell
/// provenance. Below it the frame is somebody else's work, or a collage.
pub const PIECE_MIN_OWN_SHARE_PERCENT: u32 = 75;
/// Pieces one account may hang per UTC day.
pub const PIECE_DAILY_CAP: i64 = 3;
pub const PIECE_TITLE_MAX_CHARS: usize = 40;
/// Applause a piece needs before it counts toward the monthly award at all.
/// Two friends clapping is not a competition.
pub const GALLERY_AWARD_MIN_APPLAUSE: i64 = 3;
/// How many pieces one listing returns. The month's board and the newest
/// list both stop here; the gallery is a wall, not an archive.
pub const GALLERY_LISTING_LIMIT: i64 = 100;
/// How many past months the hall of fame walks back.
pub const HALL_OF_FAME_MONTHS: i64 = 36;

/// A piece as the gallery shows it: the row, its author's name, and the
/// applause count with the viewer's own hand marked.
#[derive(Clone, Debug, PartialEq)]
pub struct ArtboardPiece {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub user_id: Uuid,
    pub username: String,
    pub title: String,
    pub width: i32,
    pub height: i32,
    pub canvas: Value,
    pub provenance: Value,
    pub glyph_count: i32,
    pub own_share_percent: i32,
    pub period_month: NaiveDate,
    pub applause: i64,
    pub applauded_by_viewer: bool,
}

impl From<Row> for ArtboardPiece {
    fn from(row: Row) -> Self {
        Self {
            id: row.get("id"),
            created: row.get("created"),
            user_id: row.get("user_id"),
            username: row.get("username"),
            title: row.get("title"),
            width: row.get("width"),
            height: row.get("height"),
            canvas: row.get("canvas"),
            provenance: row.get("provenance"),
            glyph_count: row.get("glyph_count"),
            own_share_percent: row.get("own_share_percent"),
            period_month: row.get("period_month"),
            applause: row.get("applause"),
            applauded_by_viewer: row.get("applauded_by_viewer"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct HangParams {
    pub user_id: Uuid,
    pub title: String,
    pub width: i32,
    pub height: i32,
    pub canvas: Value,
    pub provenance: Value,
    pub glyph_count: i32,
    pub own_share_percent: i32,
    pub content_hash: String,
}

/// What a hang attempt came to. The two refusals are the rails SQL enforces;
/// everything the hanger checks itself (size, share) never reaches here.
#[derive(Clone, Debug, PartialEq)]
pub enum HangOutcome {
    Hung(ArtboardPiece),
    DailyCapReached,
    Duplicate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplauseOutcome {
    /// Applause landed; the piece's new count.
    Applauded(i64),
    /// The viewer's earlier applause was withdrawn; the piece's new count.
    Withdrawn(i64),
    NotFound,
    OwnPiece,
}

/// The gallery's listings, one query each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceListing {
    /// This UTC month's pieces, most applauded first: the live standings.
    ThisMonth,
    /// Newest first, any month.
    Newest,
    /// Each past month's most applauded piece, newest month first, only
    /// months whose winner cleared [`GALLERY_AWARD_MIN_APPLAUSE`].
    HallOfFame,
    /// The viewer's own pieces, newest first.
    Mine,
}

/// The gallery line on a profile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GalleryCounts {
    pub pieces: i64,
    pub applause: i64,
}

/// How many rows each listing holds, for the page rail before a listing
/// is opened. One query, no canvases.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListingCounts {
    pub this_month: i64,
    pub newest: i64,
    pub hall_of_fame: i64,
    pub mine: i64,
}

impl ListingCounts {
    pub fn get(self, listing: PieceListing) -> i64 {
        match listing {
            PieceListing::ThisMonth => self.this_month,
            PieceListing::Newest => self.newest,
            PieceListing::HallOfFame => self.hall_of_fame,
            PieceListing::Mine => self.mine,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemovedPiece {
    pub id: Uuid,
    pub user_id: Uuid,
    pub title: String,
}

/// How a mod's id prefix resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PieceLookup {
    One(Uuid),
    NotFound,
    Ambiguous(usize),
}

/// Fewest characters of a piece id a mod command accepts, so a slip of the
/// hand cannot match half the gallery.
pub const PIECE_ID_PREFIX_MIN_CHARS: usize = 8;

/// Every listing and lookup reads through this shape: the row, the author's
/// name, the applause count, and whether `$1` (the viewer) applauded it.
const PIECE_VIEW_SQL: &str =
    "SELECT p.id, p.created, p.user_id, u.username, p.title, p.width, p.height,
            p.canvas, p.provenance, p.glyph_count, p.own_share_percent, p.period_month,
            (SELECT count(*) FROM artboard_piece_votes v WHERE v.piece_id = p.id) AS applause,
            EXISTS (
                SELECT 1 FROM artboard_piece_votes v
                WHERE v.piece_id = p.id AND v.user_id = $1
            ) AS applauded_by_viewer
     FROM artboard_pieces p
     JOIN users u ON u.id = p.user_id";

impl ArtboardPiece {
    /// Hang a piece. The per-month duplicate is the unique index, so two
    /// devices hanging the same cells at once get exactly one `Hung`. The
    /// daily cap is counted in the insert's own guard with no lock on the
    /// user's rows, so under READ COMMITTED two hangs from one account in
    /// the same instant can both read two and both land: the cap is
    /// best-effort across concurrent sessions, exact within one (the hang
    /// flow holds `Submitting` until the answer). The exact shape would be
    /// the pot's (`Pot::buy_in_tx` under `lock_open_for_buy`); a fourth
    /// piece on a day is not worth the lock.
    pub async fn hang(client: &impl GenericClient, params: HangParams) -> Result<HangOutcome> {
        let inserted = client
            .query_opt(
                "INSERT INTO artboard_pieces
                    (user_id, title, width, height, canvas, provenance, glyph_count,
                     own_share_percent, content_hash, period_month)
                 SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9,
                        date_trunc('month', now() AT TIME ZONE 'UTC')::date
                 WHERE (
                    SELECT count(*) FROM artboard_pieces
                    WHERE user_id = $1
                      AND created >= ((now() AT TIME ZONE 'UTC')::date AT TIME ZONE 'UTC')
                 ) < $10
                 RETURNING id",
                &[
                    &params.user_id,
                    &params.title,
                    &params.width,
                    &params.height,
                    &params.canvas,
                    &params.provenance,
                    &params.glyph_count,
                    &params.own_share_percent,
                    &params.content_hash,
                    &PIECE_DAILY_CAP,
                ],
            )
            .await;
        let row = match inserted {
            Ok(row) => row,
            Err(error) if error.code() == Some(&SqlState::UNIQUE_VIOLATION) => {
                return Ok(HangOutcome::Duplicate);
            }
            Err(error) => return Err(error.into()),
        };
        let Some(row) = row else {
            return Ok(HangOutcome::DailyCapReached);
        };
        let id: Uuid = row.get("id");
        match Self::find(client, params.user_id, id).await? {
            Some(piece) => Ok(HangOutcome::Hung(piece)),
            None => anyhow::bail!("hung artboard piece {id} vanished before it could be read back"),
        }
    }

    pub async fn find(
        client: &impl GenericClient,
        viewer_id: Uuid,
        piece_id: Uuid,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                &format!("{PIECE_VIEW_SQL} WHERE p.id = $2"),
                &[&viewer_id, &piece_id],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    pub async fn list(
        client: &impl GenericClient,
        viewer_id: Uuid,
        listing: PieceListing,
    ) -> Result<Vec<Self>> {
        let rows = match listing {
            PieceListing::ThisMonth => {
                client
                    .query(
                        &format!(
                            "{PIECE_VIEW_SQL}
                             WHERE p.period_month = date_trunc('month', now() AT TIME ZONE 'UTC')::date
                             ORDER BY applause DESC, p.created ASC
                             LIMIT $2"
                        ),
                        &[&viewer_id, &GALLERY_LISTING_LIMIT],
                    )
                    .await?
            }
            PieceListing::Newest => {
                client
                    .query(
                        &format!(
                            "{PIECE_VIEW_SQL}
                             ORDER BY p.created DESC
                             LIMIT $2"
                        ),
                        &[&viewer_id, &GALLERY_LISTING_LIMIT],
                    )
                    .await?
            }
            PieceListing::HallOfFame => {
                client
                    .query(
                        &format!(
                            "SELECT * FROM (
                                SELECT DISTINCT ON (period_month) *
                                FROM ({PIECE_VIEW_SQL}) pieces
                                WHERE period_month < date_trunc('month', now() AT TIME ZONE 'UTC')::date
                                  AND applause >= $2
                                ORDER BY period_month DESC, applause DESC, created ASC
                             ) winners
                             ORDER BY period_month DESC
                             LIMIT $3"
                        ),
                        &[&viewer_id, &GALLERY_AWARD_MIN_APPLAUSE, &HALL_OF_FAME_MONTHS],
                    )
                    .await?
            }
            PieceListing::Mine => {
                client
                    .query(
                        &format!(
                            "{PIECE_VIEW_SQL}
                             WHERE p.user_id = $1
                             ORDER BY p.created DESC
                             LIMIT $2"
                        ),
                        &[&viewer_id, &GALLERY_LISTING_LIMIT],
                    )
                    .await?
            }
        };
        Ok(rows.into_iter().map(Self::from).collect())
    }

    /// Last month's most applauded piece, if any cleared the award floor:
    /// what the splash hangs over the door. Ties break toward the earlier
    /// hang, the same way the award query ranks.
    pub async fn previous_month_winner(client: &impl GenericClient) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                &format!(
                    "SELECT * FROM ({PIECE_VIEW_SQL}) pieces
                     WHERE period_month = (date_trunc('month', now() AT TIME ZONE 'UTC')::date - INTERVAL '1 month')::date
                       AND applause >= $2
                     ORDER BY applause DESC, created ASC
                     LIMIT 1"
                ),
                &[&Uuid::nil(), &GALLERY_AWARD_MIN_APPLAUSE],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// The most applauded piece hung on one UTC day: the paper's wall
    /// column. Any applause count qualifies, a quiet day is `None`.
    pub async fn most_applauded_hung_on(
        client: &impl GenericClient,
        day: NaiveDate,
    ) -> Result<Option<Self>> {
        let row = client
            .query_opt(
                &format!(
                    "SELECT * FROM ({PIECE_VIEW_SQL}) pieces
                     WHERE created >= ($2::date AT TIME ZONE 'UTC')
                       AND created < (($2::date + INTERVAL '1 day') AT TIME ZONE 'UTC')
                     ORDER BY applause DESC, created ASC
                     LIMIT 1"
                ),
                &[&Uuid::nil(), &day],
            )
            .await?;
        Ok(row.map(Self::from))
    }

    /// Applaud a piece, or take the applause back if it is already there.
    /// The insert carries the author id along so the no-self-applause CHECK
    /// and the `ON CONFLICT` guard both hold in the one statement.
    pub async fn toggle_applause(
        client: &impl GenericClient,
        piece_id: Uuid,
        user_id: Uuid,
    ) -> Result<ApplauseOutcome> {
        let withdrawn = client
            .execute(
                "DELETE FROM artboard_piece_votes WHERE piece_id = $1 AND user_id = $2",
                &[&piece_id, &user_id],
            )
            .await?;
        if withdrawn > 0 {
            let count = Self::applause_count(client, piece_id).await?;
            return Ok(ApplauseOutcome::Withdrawn(count));
        }
        let author = client
            .query_opt(
                "SELECT user_id FROM artboard_pieces WHERE id = $1",
                &[&piece_id],
            )
            .await?;
        let Some(author) = author else {
            return Ok(ApplauseOutcome::NotFound);
        };
        let author_user_id: Uuid = author.get("user_id");
        if author_user_id == user_id {
            return Ok(ApplauseOutcome::OwnPiece);
        }
        client
            .execute(
                "INSERT INTO artboard_piece_votes (piece_id, user_id, author_user_id)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (piece_id, user_id) DO NOTHING",
                &[&piece_id, &user_id, &author_user_id],
            )
            .await?;
        let count = Self::applause_count(client, piece_id).await?;
        Ok(ApplauseOutcome::Applauded(count))
    }

    pub async fn applause_count(client: &impl GenericClient, piece_id: Uuid) -> Result<i64> {
        let row = client
            .query_one(
                "SELECT count(*) AS applause FROM artboard_piece_votes WHERE piece_id = $1",
                &[&piece_id],
            )
            .await?;
        Ok(row.get("applause"))
    }

    pub async fn counts_for_user(
        client: &impl GenericClient,
        user_id: Uuid,
    ) -> Result<GalleryCounts> {
        let row = client
            .query_one(
                "SELECT
                    (SELECT count(*) FROM artboard_pieces WHERE user_id = $1) AS pieces,
                    (SELECT count(*) FROM artboard_piece_votes WHERE author_user_id = $1) AS applause",
                &[&user_id],
            )
            .await?;
        Ok(GalleryCounts {
            pieces: row.get("pieces"),
            applause: row.get("applause"),
        })
    }

    /// The four listing sizes in one round trip: the rail's numbers. The
    /// hall of fame counts past months whose best piece cleared the award
    /// floor, the same months `list(HallOfFame)` returns.
    pub async fn listing_counts(
        client: &impl GenericClient,
        viewer_id: Uuid,
    ) -> Result<ListingCounts> {
        let row = client
            .query_one(
                "SELECT
                    (SELECT count(*) FROM artboard_pieces
                      WHERE period_month = date_trunc('month', now() AT TIME ZONE 'UTC')::date) AS this_month,
                    (SELECT count(*) FROM artboard_pieces) AS newest,
                    (SELECT count(DISTINCT p.period_month) FROM artboard_pieces p
                      WHERE p.period_month < date_trunc('month', now() AT TIME ZONE 'UTC')::date
                        AND (SELECT count(*) FROM artboard_piece_votes v WHERE v.piece_id = p.id) >= $2) AS hall_of_fame,
                    (SELECT count(*) FROM artboard_pieces WHERE user_id = $1) AS mine",
                &[&viewer_id, &GALLERY_AWARD_MIN_APPLAUSE],
            )
            .await?;
        Ok(ListingCounts {
            this_month: row.get("this_month"),
            newest: row.get("newest"),
            hall_of_fame: row.get("hall_of_fame"),
            mine: row.get("mine"),
        })
    }

    /// Resolve a mod's id prefix to one piece. Shorter than
    /// [`PIECE_ID_PREFIX_MIN_CHARS`] is not looked up at all.
    pub async fn lookup_by_id_prefix(
        client: &impl GenericClient,
        prefix: &str,
    ) -> Result<PieceLookup> {
        if prefix.chars().count() < PIECE_ID_PREFIX_MIN_CHARS
            || !prefix.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-')
        {
            return Ok(PieceLookup::NotFound);
        }
        let pattern = format!("{}%", prefix.to_ascii_lowercase());
        let rows = client
            .query(
                "SELECT id FROM artboard_pieces WHERE id::text LIKE $1 LIMIT 2",
                &[&pattern],
            )
            .await?;
        match rows.as_slice() {
            [] => Ok(PieceLookup::NotFound),
            [row] => Ok(PieceLookup::One(row.get("id"))),
            _ => Ok(PieceLookup::Ambiguous(rows.len())),
        }
    }

    /// Take a piece down. Its applause goes with it (cascade); an award
    /// already snapshotted from it stays, the way every award does.
    pub async fn remove(
        client: &impl GenericClient,
        piece_id: Uuid,
    ) -> Result<Option<RemovedPiece>> {
        let row = client
            .query_opt(
                "DELETE FROM artboard_pieces WHERE id = $1 RETURNING id, user_id, title",
                &[&piece_id],
            )
            .await?;
        Ok(row.map(|row| RemovedPiece {
            id: row.get("id"),
            user_id: row.get("user_id"),
            title: row.get("title"),
        }))
    }
}
