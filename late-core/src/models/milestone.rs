//! Burn milestones: the permanent badges whose only product is the receipt.
//!
//! A milestone is bought once, never expires, and is never equipped. It is
//! not a badge slot: it renders as a fourth glyph on top of whatever badge
//! and flag a player is renting, so nothing it does can be hidden by a
//! hundred-chip rental. Where every other name-adjacent item is a
//! `shop_consumable_effects` row with an `ends_at`, a milestone is a plain
//! `user_purchases` row, which is why it lives here rather than in
//! `rental.rs`.
//!
//! Owning two shows the dearer one. The ladder only goes up, so the highest
//! is the one a buyer would pick anyway, and picking it in the query is what
//! lets the whole feature ship with no equip flow and no slot column.

use anyhow::Result;
use serde_json::Value;
use tokio_postgres::Client;
use uuid::Uuid;

/// `marketplace_items.item_kind` for a burn milestone.
///
/// Deliberately not `badge`: migration 148 retires every `badge` row and
/// invites re-running its INSERT shape for new ones, so a milestone seeded as
/// a badge would be switched off by the next badge anyone adds.
pub const MILESTONE_BADGE_ITEM_KIND: &str = "milestone_badge";

/// The emoji a milestone item payload carries. `None` on a malformed
/// payload, so a bad row renders nothing rather than an empty glyph.
pub fn emoji_from_payload(payload: &Value) -> Option<String> {
    payload
        .get("emoji")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|emoji| !emoji.is_empty())
        .map(str::to_string)
}

pub struct MilestoneBadge;

impl MilestoneBadge {
    /// The dearest milestone every owner holds, one row per user. Feeds the
    /// flair directory's startup seed.
    pub async fn highest_for_all(client: &Client) -> Result<Vec<(Uuid, String)>> {
        let rows = client
            .query(
                "SELECT DISTINCT ON (p.user_id)
                        p.user_id,
                        i.payload
                 FROM user_purchases p
                 JOIN marketplace_items i ON i.id = p.item_id
                 WHERE i.item_kind = $1
                 ORDER BY p.user_id, i.price_chips DESC, i.sku ASC",
                &[&MILESTONE_BADGE_ITEM_KIND],
            )
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let payload: Value = row.get("payload");
                emoji_from_payload(&payload).map(|emoji| (row.get("user_id"), emoji))
            })
            .collect())
    }

    /// The dearest milestone one user holds. Scoped to `user_id` in the query
    /// itself, like every other read over user-owned rows.
    pub async fn highest_for_user(client: &Client, user_id: Uuid) -> Result<Option<String>> {
        let row = client
            .query_opt(
                "SELECT i.payload
                 FROM user_purchases p
                 JOIN marketplace_items i ON i.id = p.item_id
                 WHERE p.user_id = $1
                   AND i.item_kind = $2
                 ORDER BY i.price_chips DESC, i.sku ASC
                 LIMIT 1",
                &[&user_id, &MILESTONE_BADGE_ITEM_KIND],
            )
            .await?;
        Ok(row.and_then(|row| {
            let payload: Value = row.get("payload");
            emoji_from_payload(&payload)
        }))
    }
}
