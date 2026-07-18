// One public arcade handle per account: the human-readable name a user carries
// into door games whose upstream binaries key saves and public score files by
// player name (DCSS today; NetHack may adopt it later). Claimed once from a
// door's first-launch prompt, then immutable: the handle keys saves on the game
// hosts and appears in publicly served logfiles, so renames would orphan
// characters and fork score history. Uniqueness is case-insensitive, enforced
// by a unique index on lower(handle); rows outlive their account (user_id goes
// NULL on user deletion) so a dead account's handle can never be re-claimed to
// open its saves.

use anyhow::Result;
use uuid::Uuid;

crate::model! {
    table = "arcade_handles";
    params = ArcadeHandleParams;
    struct ArcadeHandle {
        @data
        pub user_id: Option<Uuid>,
        pub handle: String,
    }
}

/// Shape bounds for a claimable handle. The cap stays comfortably inside every
/// downstream limit (crawl's MAX_NAME_LENGTH is 30) while leaving room for a
/// door to suffix the handle if it ever needs to.
pub const HANDLE_MIN_LEN: usize = 3;
pub const HANDLE_MAX_LEN: usize = 20;

/// Whether the text is a well-formed handle: a letter followed by letters,
/// digits, or underscores, within the length bounds. Deliberately a strict
/// subset of every consumer's rules: crawl's name charset (alnum plus
/// `- . _ space`), the door hosts' playname sanitizers (`[A-Za-z0-9_]`), and
/// what travels safely as an SSH username.
pub fn handle_shape_valid(handle: &str) -> bool {
    if !(HANDLE_MIN_LEN..=HANDLE_MAX_LEN).contains(&handle.len()) {
        return false;
    }
    let mut bytes = handle.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    first.is_ascii_alphabetic() && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Whether the name is reserved from claiming: the bare fallback name the door
/// hosts substitute for an unusable playname, and the `late_` prefix used by
/// the derived hash playnames (NetHack still keys live saves by `late_<hex>`;
/// letting users claim those shapes would let them open another account's save
/// when that door adopts handles).
pub fn handle_reserved(handle: &str) -> bool {
    let lower = handle.to_ascii_lowercase();
    lower == "late" || lower.starts_with("late_")
}

/// Outcome of a claim attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaimOutcome {
    /// The name is now this account's handle.
    Claimed,
    /// The account already holds a handle (they are immutable); here it is.
    AlreadyClaimed(String),
    /// Another account, or a deleted account's graveyard row, holds the name.
    Taken,
}

impl ArcadeHandle {
    /// The account's claimed handle, if any.
    pub async fn find_by_user_id(
        client: &impl deadpool_postgres::GenericClient,
        user_id: Uuid,
    ) -> Result<Option<String>> {
        let row = client
            .query_opt(
                "SELECT handle FROM arcade_handles WHERE user_id = $1",
                &[&user_id],
            )
            .await?;
        Ok(row.map(|r| r.get("handle")))
    }

    /// Claim a handle for an account. First claim wins and is immutable. The
    /// caller validates shape ([`handle_shape_valid`]) and reserved names
    /// ([`handle_reserved`]); this enforces one-per-account and global
    /// case-insensitive uniqueness, race-safely via the table's unique
    /// constraints (ON CONFLICT DO NOTHING covers both).
    pub async fn claim(
        client: &impl deadpool_postgres::GenericClient,
        user_id: Uuid,
        handle: &str,
    ) -> Result<ClaimOutcome> {
        if let Some(existing) = Self::find_by_user_id(client, user_id).await? {
            return Ok(ClaimOutcome::AlreadyClaimed(existing));
        }
        let inserted = client
            .execute(
                "INSERT INTO arcade_handles (user_id, handle) VALUES ($1, $2)
                 ON CONFLICT DO NOTHING",
                &[&user_id, &handle],
            )
            .await?;
        if inserted == 1 {
            return Ok(ClaimOutcome::Claimed);
        }
        // The insert was refused by a unique constraint: either our own
        // double-submit landed first (one handle per account) or someone else
        // holds the name.
        match Self::find_by_user_id(client, user_id).await? {
            Some(existing) => Ok(ClaimOutcome::AlreadyClaimed(existing)),
            None => Ok(ClaimOutcome::Taken),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_accepts_plain_handles() {
        assert!(handle_shape_valid("mat"));
        assert!(handle_shape_valid("srcrip"));
        assert!(handle_shape_valid("Gnoll_Fan_99"));
        assert!(handle_shape_valid("a2345678901234567890")); // 20 chars
    }

    #[test]
    fn shape_rejects_bad_lengths() {
        assert!(!handle_shape_valid(""));
        assert!(!handle_shape_valid("ab"));
        assert!(!handle_shape_valid("a23456789012345678901")); // 21 chars
    }

    #[test]
    fn shape_rejects_bad_charset_and_leading_char() {
        assert!(!handle_shape_valid("1mat")); // must start with a letter
        assert!(!handle_shape_valid("_mat"));
        assert!(!handle_shape_valid("mat p")); // crawl allows spaces; we don't
        assert!(!handle_shape_valid("mat-p")); // host sanitizers strip hyphens
        assert!(!handle_shape_valid("mat.p"));
        assert!(!handle_shape_valid("måt")); // ascii only
    }

    #[test]
    fn reserved_blocks_fallback_and_hash_shapes() {
        assert!(handle_reserved("late"));
        assert!(handle_reserved("LATE"));
        assert!(handle_reserved("late_0dd47727a40681b9"));
        assert!(handle_reserved("Late_anything"));
        // Names merely containing or resembling it stay claimable.
        assert!(!handle_reserved("latecomer"));
        assert!(!handle_reserved("chocolate"));
    }
}
