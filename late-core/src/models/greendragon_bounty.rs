// Green Dragon bounties (LoGD modules/dag.php): one row per contract placed
// on a warrior's head. A bounty matures `e_rand(0, 14400)` seconds after
// placement (`set_at` is stamped in the future); only matured rows are
// visible, collectable, or counted against the target's own reading — but
// the per-target open-total cap counts immature rows too, exactly as
// upstream. Collection happens inside the PvP victory settlement's
// transaction; rows the winner set themselves stay open for the next hunter
// (upstream never closes them). Closure to the house (dragon kill, character
// deletion) leaves `winner` NULL.

use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::Client;
use uuid::Uuid;

/// Days a closed bounty stays in the ledger before the prune reaps it
/// (upstream sweeps `status=1` rows older than `expirecontent`/10 = 18 days).
pub const BOUNTY_CLOSED_RETENTION_DAYS: i32 = 18;

crate::model! {
    table = "greendragon_bounties";
    params = GreenDragonBountyParams;
    struct GreenDragonBounty {
        @data
        pub target: Uuid,
        pub setter: Option<Uuid>,
        pub amount: i64,
        pub set_at: DateTime<Utc>,
        pub open: bool,
        pub winner: Option<Uuid>,
        pub closed_at: Option<DateTime<Utc>>,
    }
}

impl GreenDragonBounty {
    /// Place a bounty. `delay_secs` is the activation delay already rolled by
    /// the caller (`e_rand(0, 14400)`); the bounty matures once that moment
    /// passes.
    pub async fn place(
        client: &impl deadpool_postgres::GenericClient,
        target: Uuid,
        setter: Option<Uuid>,
        amount: i64,
        delay_secs: i64,
    ) -> Result<()> {
        client
            .execute(
                "INSERT INTO greendragon_bounties (target, setter, amount, set_at)
                 VALUES ($1, $2, $3, current_timestamp + make_interval(secs => $4::bigint))",
                &[&target, &setter, &amount, &delay_secs],
            )
            .await?;
        Ok(())
    }

    /// Every open bounty on `target`, matured or not — the `200·level`
    /// placement cap counts immature rows too (upstream's sum has no date
    /// filter).
    pub async fn open_total_on(
        client: &impl deadpool_postgres::GenericClient,
        target: Uuid,
    ) -> Result<i64> {
        let row = client
            .query_one(
                "SELECT COALESCE(sum(amount), 0)::bigint AS total
                 FROM greendragon_bounties WHERE open AND target = $1",
                &[&target],
            )
            .await?;
        Ok(row.get("total"))
    }

    /// The matured open total on `target` — what Dag admits to when the
    /// target asks about their own head.
    pub async fn matured_total_on(client: &Client, target: Uuid) -> Result<i64> {
        let row = client
            .query_one(
                "SELECT COALESCE(sum(amount), 0)::bigint AS total
                 FROM greendragon_bounties
                 WHERE open AND set_at <= current_timestamp AND target = $1",
                &[&target],
            )
            .await?;
        Ok(row.get("total"))
    }

    /// The wanted list: matured open bounties aggregated per target,
    /// unordered (the game joins the roster and sorts).
    pub async fn wanted_list(client: &Client) -> Result<Vec<(Uuid, i64)>> {
        let rows = client
            .query(
                "SELECT target, sum(amount)::bigint AS total
                 FROM greendragon_bounties
                 WHERE open AND set_at <= current_timestamp
                 GROUP BY target",
                &[],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| (r.get("target"), r.get("total")))
            .collect())
    }

    /// Collect on a PvP kill: close every matured open bounty on `victim`
    /// not set by the winner (those stay open for the next hunter, exactly
    /// as upstream never closes them) and return the payout.
    pub async fn collect(
        client: &impl deadpool_postgres::GenericClient,
        victim: Uuid,
        winner: Uuid,
    ) -> Result<i64> {
        let row = client
            .query_one(
                "WITH paid AS (
                     UPDATE greendragon_bounties
                     SET open = false, winner = $2, closed_at = current_timestamp,
                         updated = current_timestamp
                     WHERE open AND set_at <= current_timestamp AND target = $1
                       AND setter IS DISTINCT FROM $2
                     RETURNING amount
                 )
                 SELECT COALESCE(sum(amount), 0)::bigint AS total FROM paid",
                &[&victim, &winner],
            )
            .await?;
        Ok(row.get("total"))
    }

    /// The matured open total on `victim` set by `setter` themselves — the
    /// share Dag "keeps" when the setter does their own killing.
    pub async fn forfeited_total(
        client: &impl deadpool_postgres::GenericClient,
        victim: Uuid,
        setter: Uuid,
    ) -> Result<i64> {
        let row = client
            .query_one(
                "SELECT COALESCE(sum(amount), 0)::bigint AS total
                 FROM greendragon_bounties
                 WHERE open AND set_at <= current_timestamp
                   AND target = $1 AND setter = $2",
                &[&victim, &setter],
            )
            .await?;
        Ok(row.get("total"))
    }

    /// Close every open bounty on `target` to the house, no payout (the
    /// target slew the dragon, or their character was deleted).
    pub async fn close_all_on(client: &Client, target: Uuid) -> Result<()> {
        client
            .execute(
                "UPDATE greendragon_bounties
                 SET open = false, winner = NULL, closed_at = current_timestamp,
                     updated = current_timestamp
                 WHERE open AND target = $1",
                &[&target],
            )
            .await?;
        Ok(())
    }

    /// Close open bounties whose target no longer has a saved character
    /// (upstream lazily closes deleted targets' bounties at list render).
    pub async fn sweep_stray(client: &Client) -> Result<()> {
        client
            .execute(
                "UPDATE greendragon_bounties
                 SET open = false, winner = NULL, closed_at = current_timestamp,
                     updated = current_timestamp
                 WHERE open AND target NOT IN
                       (SELECT user_id FROM greendragon_characters)",
                &[],
            )
            .await?;
        Ok(())
    }

    /// Reap closed bounties older than the retention window.
    pub async fn prune_closed(client: &Client) -> Result<u64> {
        Ok(client
            .execute(
                "DELETE FROM greendragon_bounties
                 WHERE NOT open
                   AND closed_at < current_timestamp - make_interval(days => $1)",
                &[&BOUNTY_CLOSED_RETENTION_DAYS],
            )
            .await?)
    }
}
