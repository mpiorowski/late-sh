//! Invites: the three tables behind "bring a friend" (migration 218).
//!
//! - `invite_codes`: each account's one stable code, minted the first time
//!   its owner opens the Invites dialog.
//! - `referrals`: at most one row per invitee, naming who invited them and
//!   where the row is in its life (`pending` -> `qualified` -> `paid`, or
//!   `pending` -> `expired`). The status moves only through guarded
//!   `UPDATE ... WHERE status = ...` statements, so a sweep on every replica
//!   still moves each row once.
//! - `newcomer_activity_days`: active input minutes per UTC day, recorded
//!   only for young accounts, which is what a referral is judged on.
//!
//! The rules (who may attach, what qualifies, how much is paid, the monthly
//! cap) live in `late-ssh/src/app/referral`; this module is the SQL.

use anyhow::{Result, bail};
use chrono::{DateTime, NaiveDate, Utc};
use tokio_postgres::{GenericClient, Transaction};
use uuid::Uuid;

/// Invite codes are this many symbols of [`INVITE_CODE_ALPHABET`].
pub const INVITE_CODE_LEN: usize = 8;
/// The account-link alphabet, lowercased: no `0`, `1`, `i`, or `o`, so a code
/// read aloud or copied off a screenshot survives. The migration's CHECK
/// spells the same set.
pub const INVITE_CODE_ALPHABET: &[u8; 32] = b"23456789abcdefghjklmnpqrstuvwxyz";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferralSource {
    /// `ssh invite-<code>@late.sh` on the account's first connect.
    Ssh,
    /// Typed into Settings > Account > Invites.
    Settings,
}

impl ReferralSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ssh => "ssh",
            Self::Settings => "settings",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferralStatus {
    /// Judging window still open, bar not yet met.
    Pending,
    /// Bar met; waiting for the inviter's monthly cap to allow the payout.
    Qualified,
    /// Both sides credited.
    Paid,
    /// The judging window closed without the bar being met.
    Expired,
}

impl ReferralStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Qualified => "qualified",
            Self::Paid => "paid",
            Self::Expired => "expired",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "qualified" => Ok(Self::Qualified),
            "paid" => Ok(Self::Paid),
            "expired" => Ok(Self::Expired),
            other => bail!("unknown referral status {other:?}"),
        }
    }
}

/// The owner of an invite code, as the attach rules need them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InviteCodeOwner {
    pub user_id: Uuid,
    pub username: String,
    pub created: DateTime<Utc>,
}

/// One person this account invited, for the Invites dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvitedUser {
    pub username: String,
    pub status: ReferralStatus,
}

/// A referral the sweeper still has to judge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingReferral {
    pub invitee_id: Uuid,
    pub invitee_created: DateTime<Utc>,
    pub judged_until: DateTime<Utc>,
}

/// One UTC day of a newcomer's activity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActivityDay {
    pub day: NaiveDate,
    pub active_minutes: i32,
}

pub struct InviteCode;

impl InviteCode {
    /// The account's code, minted on first call. Two sessions minting at once
    /// both land on whichever insert won: the loser's insert hits the
    /// `user_id` key, does nothing, and the next pass reads the winner's.
    pub async fn ensure(client: &impl GenericClient, user_id: Uuid) -> Result<String> {
        for _ in 0..8 {
            if let Some(row) = client
                .query_opt(
                    "SELECT code FROM invite_codes WHERE user_id = $1",
                    &[&user_id],
                )
                .await?
            {
                return Ok(row.get("code"));
            }
            let inserted = client
                .query_opt(
                    "INSERT INTO invite_codes (user_id, code)
                     VALUES ($1, $2)
                     ON CONFLICT DO NOTHING
                     RETURNING code",
                    &[&user_id, &generate_code()],
                )
                .await?;
            if let Some(row) = inserted {
                return Ok(row.get("code"));
            }
        }
        bail!("could not allocate an invite code")
    }

    /// Who owns `code`, already normalized by the caller.
    pub async fn find_owner(
        client: &impl GenericClient,
        code: &str,
    ) -> Result<Option<InviteCodeOwner>> {
        let row = client
            .query_opt(
                "SELECT u.id, u.username, u.created
                 FROM invite_codes c
                 JOIN users u ON u.id = c.user_id
                 WHERE c.code = $1",
                &[&code],
            )
            .await?;
        Ok(row.map(|row| InviteCodeOwner {
            user_id: row.get("id"),
            username: row.get("username"),
            created: row.get("created"),
        }))
    }
}

pub struct Referral;

impl Referral {
    /// Attach `invitee_id` to `inviter_id`. `false` when the invitee already
    /// has an inviter: the first attach is the only one, ever.
    pub async fn attach(
        client: &impl GenericClient,
        invitee_id: Uuid,
        inviter_id: Uuid,
        source: ReferralSource,
        judged_until: DateTime<Utc>,
    ) -> Result<bool> {
        let attached = client
            .execute(
                "INSERT INTO referrals (invitee_id, inviter_id, source, judged_until)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (invitee_id) DO NOTHING",
                &[&invitee_id, &inviter_id, &source.as_str(), &judged_until],
            )
            .await?;
        Ok(attached == 1)
    }

    /// The username of whoever invited `invitee_id`, if anyone did.
    pub async fn inviter_username(
        client: &impl GenericClient,
        invitee_id: Uuid,
    ) -> Result<Option<String>> {
        let row = client
            .query_opt(
                "SELECT u.username
                 FROM referrals r
                 JOIN users u ON u.id = r.inviter_id
                 WHERE r.invitee_id = $1",
                &[&invitee_id],
            )
            .await?;
        Ok(row.map(|row| row.get("username")))
    }

    /// Everyone `inviter_id` invited, newest first.
    pub async fn list_for_inviter(
        client: &impl GenericClient,
        inviter_id: Uuid,
    ) -> Result<Vec<InvitedUser>> {
        let rows = client
            .query(
                "SELECT u.username, r.status
                 FROM referrals r
                 JOIN users u ON u.id = r.invitee_id
                 WHERE r.inviter_id = $1
                 ORDER BY r.created DESC",
                &[&inviter_id],
            )
            .await?;
        rows.into_iter()
            .map(|row| {
                Ok(InvitedUser {
                    username: row.get("username"),
                    status: ReferralStatus::parse(row.get("status"))?,
                })
            })
            .collect()
    }

    /// Every referral still being judged, oldest first.
    pub async fn list_pending(client: &impl GenericClient) -> Result<Vec<PendingReferral>> {
        let rows = client
            .query(
                "SELECT r.invitee_id, u.created AS invitee_created, r.judged_until
                 FROM referrals r
                 JOIN users u ON u.id = r.invitee_id
                 WHERE r.status = 'pending'
                 ORDER BY r.created",
                &[],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| PendingReferral {
                invitee_id: row.get("invitee_id"),
                invitee_created: row.get("invitee_created"),
                judged_until: row.get("judged_until"),
            })
            .collect())
    }

    /// Every referral waiting on its payout, in the order they qualified.
    pub async fn list_qualified(client: &impl GenericClient) -> Result<Vec<Uuid>> {
        let rows = client
            .query(
                "SELECT invitee_id FROM referrals
                 WHERE status = 'qualified'
                 ORDER BY qualified_at, invitee_id",
                &[],
            )
            .await?;
        Ok(rows.into_iter().map(|row| row.get("invitee_id")).collect())
    }

    /// `pending` -> `qualified`. `false` when another sweep got there first.
    pub async fn mark_qualified(client: &impl GenericClient, invitee_id: Uuid) -> Result<bool> {
        let updated = client
            .execute(
                "UPDATE referrals
                 SET status = 'qualified', qualified_at = current_timestamp
                 WHERE invitee_id = $1 AND status = 'pending'",
                &[&invitee_id],
            )
            .await?;
        Ok(updated == 1)
    }

    /// `pending` -> `expired` for one row whose window has closed. `false`
    /// when the window is still open or another sweep settled it.
    pub async fn expire(client: &impl GenericClient, invitee_id: Uuid) -> Result<bool> {
        let updated = client
            .execute(
                "UPDATE referrals
                 SET status = 'expired'
                 WHERE invitee_id = $1
                   AND status = 'pending'
                   AND judged_until <= current_timestamp",
                &[&invitee_id],
            )
            .await?;
        Ok(updated == 1)
    }

    /// Lock a qualified row for payment and return its inviter. `None` when
    /// it is not `qualified` any more (another replica paid it).
    pub async fn lock_qualified(tx: &Transaction<'_>, invitee_id: Uuid) -> Result<Option<Uuid>> {
        let row = tx
            .query_opt(
                "SELECT inviter_id FROM referrals
                 WHERE invitee_id = $1 AND status = 'qualified'
                 FOR UPDATE",
                &[&invitee_id],
            )
            .await?;
        Ok(row.map(|row| row.get("inviter_id")))
    }

    /// Serialize every payout to one inviter for the rest of the
    /// transaction, so two replicas paying two of their invitees at once
    /// cannot both read the same monthly count under the cap.
    pub async fn lock_inviter(tx: &Transaction<'_>, inviter_id: Uuid) -> Result<()> {
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
            &[&format!("referral_inviter:{inviter_id}")],
        )
        .await?;
        Ok(())
    }

    /// How many of `inviter_id`'s referrals were paid this UTC month.
    pub async fn paid_this_month(tx: &Transaction<'_>, inviter_id: Uuid) -> Result<i64> {
        let row = tx
            .query_one(
                "SELECT COUNT(*) AS paid FROM referrals
                 WHERE inviter_id = $1
                   AND status = 'paid'
                   AND paid_at >= date_trunc('month', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'",
                &[&inviter_id],
            )
            .await?;
        Ok(row.get("paid"))
    }

    /// `qualified` -> `paid`, under the row lock [`Self::lock_qualified`]
    /// took.
    pub async fn mark_paid(tx: &Transaction<'_>, invitee_id: Uuid) -> Result<()> {
        let updated = tx
            .execute(
                "UPDATE referrals
                 SET status = 'paid', paid_at = current_timestamp
                 WHERE invitee_id = $1 AND status = 'qualified'",
                &[&invitee_id],
            )
            .await?;
        if updated != 1 {
            bail!("locked qualified referral {invitee_id} was not updated");
        }
        Ok(())
    }
}

pub struct NewcomerActivity;

impl NewcomerActivity {
    /// Count `minute` (any instant inside it) as active for `user_id`. A
    /// minute already counted, by this session or another, counts nothing.
    pub async fn record_minute(
        client: &impl GenericClient,
        user_id: Uuid,
        minute: DateTime<Utc>,
    ) -> Result<bool> {
        let updated = client
            .execute(
                "INSERT INTO newcomer_activity_days (user_id, day, active_minutes, last_minute)
                 VALUES ($1, (date_trunc('minute', $2::timestamptz) AT TIME ZONE 'UTC')::date, 1,
                         date_trunc('minute', $2::timestamptz))
                 ON CONFLICT (user_id, day) DO UPDATE SET
                     active_minutes = newcomer_activity_days.active_minutes + 1,
                     last_minute = EXCLUDED.last_minute
                 WHERE newcomer_activity_days.last_minute < EXCLUDED.last_minute",
                &[&user_id, &minute],
            )
            .await?;
        Ok(updated == 1)
    }

    /// The days of `user_id`'s activity inside `[from, until)`, by UTC day.
    pub async fn days(
        client: &impl GenericClient,
        user_id: Uuid,
        from: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<Vec<ActivityDay>> {
        let rows = client
            .query(
                "SELECT day, active_minutes FROM newcomer_activity_days
                 WHERE user_id = $1
                   AND day >= ($2::timestamptz AT TIME ZONE 'UTC')::date
                   AND day < ($3::timestamptz AT TIME ZONE 'UTC')::date
                 ORDER BY day",
                &[&user_id, &from, &until],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| ActivityDay {
                day: row.get("day"),
                active_minutes: row.get("active_minutes"),
            })
            .collect())
    }
}

/// Normalize a typed or SSH-supplied code: case-folded, everything outside
/// the alphabet dropped. `None` unless exactly a code's worth survives.
pub fn normalize_code(raw: &str) -> Option<String> {
    let code: String = raw
        .trim()
        .chars()
        .map(|ch| ch.to_ascii_lowercase())
        .filter(|ch| ch.is_ascii() && INVITE_CODE_ALPHABET.contains(&(*ch as u8)))
        .collect();
    match code.len() == INVITE_CODE_LEN {
        true => Some(code),
        false => None,
    }
}

fn generate_code() -> String {
    Uuid::new_v4()
        .as_bytes()
        .iter()
        .take(INVITE_CODE_LEN)
        .map(|byte| INVITE_CODE_ALPHABET[(*byte & 31) as usize] as char)
        .collect()
}
