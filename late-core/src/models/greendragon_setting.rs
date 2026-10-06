// Green Dragon shared game state: a key/value store for the few values that
// are one shared global across all players (LoGD module settings). The keys
// are seeded by migrations; the game reads and atomically updates them.

use anyhow::Result;
use tokio_postgres::Client;

pub struct GreenDragonSetting;

impl GreenDragonSetting {
    /// Read one shared value, if the key exists.
    pub async fn get(client: &Client, key: &str) -> Result<Option<i64>> {
        let row = client
            .query_opt(
                "SELECT value FROM greendragon_settings WHERE key = $1",
                &[&key],
            )
            .await?;
        Ok(row.map(|r| r.get("value")))
    }

    /// Settle one Five Sixes play against the shared jackpot, atomically:
    /// the pot grows by the play's `cost` (clamped to `max_pot` — overflow is
    /// "pocketed by the house", exactly upstream's cap), then pays out by the
    /// number of sixes rolled: 5 takes the whole pot (which resets to 100),
    /// 4 takes 10% and 3 takes 5% (deducted from the pot), fewer take nothing.
    /// Returns `(pot_after_the_bump, pot_left_after_the_payout)`; the win is
    /// the difference (or the whole bumped pot on a jackpot).
    pub async fn settle_fivesix(
        client: &Client,
        cost: i64,
        max_pot: i64,
        sixes: u32,
    ) -> Result<(i64, i64)> {
        let sixes = sixes as i64;
        let row = client
            .query_one(
                "WITH bumped AS (
                     SELECT key, LEAST(value + $1, $2) AS pot
                     FROM greendragon_settings
                     WHERE key = 'fivesix_jackpot'
                     FOR UPDATE
                 )
                 UPDATE greendragon_settings s
                 SET value = CASE
                         WHEN $3 >= 5 THEN 100
                         WHEN $3 = 4 THEN b.pot - ROUND(b.pot * 0.10)
                         WHEN $3 = 3 THEN b.pot - ROUND(b.pot * 0.05)
                         ELSE b.pot
                     END,
                     updated = current_timestamp
                 FROM bumped b
                 WHERE s.key = b.key
                 RETURNING b.pot AS pot, s.value AS left_over",
                &[&cost, &max_pot, &sixes],
            )
            .await?;
        Ok((row.get("pot"), row.get("left_over")))
    }
}
