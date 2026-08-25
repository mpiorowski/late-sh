//! The rental shape every timed Shop item shares: username effects, badge and
//! flag rentals, and titles.
//!
//! One place holds the two windows we sell (24h and 30 days), the copy that
//! quotes them, and the payload readers each rental kind needs. Every rental
//! lands as a user-scoped `shop_consumable_effects` row (`room_id IS NULL`)
//! whose `effect_kind` names the slot it fills, so one active row per user per
//! kind, a rebuy replaces the live row and resets its clock, and expiry is
//! read-time only (`ends_at > current_timestamp` in the queries that read it).

use serde_json::Value;

use super::marketplace::{CHAT_BADGE_SLOT, CHAT_FLAG_SLOT};

/// The day tier: 24 hours.
pub const RENTAL_DAY_SECS: i64 = 86_400;

/// The month tier: 30 days, priced at 30x the day tier.
pub const RENTAL_MONTH_SECS: i64 = 2_592_000;

/// `marketplace_items.item_kind` for a rented chat badge or flag. Unlike the
/// legacy permanent `badge` kind, these carry no `slot` column (nothing is
/// equipped); the slot they fill lives in the payload and reaches the label
/// query through the effect row.
pub const BADGE_RENTAL_ITEM_KIND: &str = "badge_rental";

/// `marketplace_items.item_kind` for a rented title.
pub const TITLE_RENTAL_ITEM_KIND: &str = "title_rental";

/// `shop_consumable_effects.effect_kind` for the rented title: the short text
/// printed after the username in chat and the clubhouse.
pub const TITLE_EFFECT_KIND: &str = "title";

/// The longest title we render, curated or custom.
pub const TITLE_MAX_LEN: usize = 20;

/// Shop copy for how long a bought rental runs: "30 days" once the duration is
/// a whole number of days past one, "24 hours" for the day tier (which reads in
/// hours, matching how the shop counts it down).
pub fn duration_label(duration_secs: i64) -> String {
    let hours = duration_secs / 3_600;
    match hours {
        hours if hours > 24 && hours % 24 == 0 => format!("{} days", hours / 24),
        hours => format!("{hours} hours"),
    }
}

/// The compact tag purchase banners and #lounge lines carry: "24h", "30d".
pub fn duration_tag(duration_secs: i64) -> String {
    let hours = duration_secs / 3_600;
    match hours {
        hours if hours > 24 && hours % 24 == 0 => format!("{}d", hours / 24),
        hours => format!("{hours}h"),
    }
}

/// How long the rental an item payload sells runs. `fallback` is required from
/// the caller rather than defaulted here: a malformed payload must land on the
/// window its own kind considers shortest, never on a shared guess.
pub fn duration_secs(payload: &Value, fallback: i64) -> i64 {
    payload
        .get("duration_secs")
        .and_then(|value| value.as_i64())
        .unwrap_or(fallback)
}

/// Which of the two chat-label slots a badge rental fills. The strings are the
/// legacy `marketplace_items.slot` values, reused verbatim as effect kinds so
/// the chat label query reads one name per slot whichever path filled it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeSlot {
    Badge,
    Flag,
}

impl BadgeSlot {
    pub fn effect_kind(self) -> &'static str {
        match self {
            Self::Badge => CHAT_BADGE_SLOT,
            Self::Flag => CHAT_FLAG_SLOT,
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            CHAT_BADGE_SLOT => Some(Self::Badge),
            CHAT_FLAG_SLOT => Some(Self::Flag),
            _ => None,
        }
    }
}

/// A badge rental as its item payload spells it. `None` on a payload missing
/// the emoji or naming a slot we do not render, so the purchase transaction
/// fails loudly instead of charging for an effect nothing can show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BadgeRental {
    pub emoji: String,
    pub slot: BadgeSlot,
}

impl BadgeRental {
    pub fn from_payload(payload: &Value) -> Option<Self> {
        let emoji = payload
            .get("emoji")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|emoji| !emoji.is_empty())?;
        let slot = payload.get("slot").and_then(|value| value.as_str())?;
        Some(Self {
            emoji: emoji.to_string(),
            slot: BadgeSlot::parse(slot)?,
        })
    }
}

/// The title text an item payload or a live effect row carries, trimmed and
/// clamped to what the renderers have room for. `None` on a blank title, so a
/// malformed row renders no title rather than an empty `, ` after the name.
pub fn title_from_payload(payload: &Value) -> Option<String> {
    payload
        .get("text")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(|text| text.chars().take(TITLE_MAX_LEN).collect())
}

#[cfg(test)]
#[path = "rental_test.rs"]
mod rental_test;
