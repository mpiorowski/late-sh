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

/// The month tier: 30 days, priced at 40x the day tier (migration 153).
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
/// retired `marketplace_items.slot` values, reused verbatim as effect kinds so
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

/// Whether this title item sells a text the buyer writes rather than one the
/// catalog carries. A custom SKU has no `text` key at all: the title does not
/// exist until someone types it, so nothing can read one out of the payload.
pub fn is_custom_title(payload: &Value) -> bool {
    payload
        .get("custom")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

/// Why a buyer's own title text was refused. Each variant is its own refusal
/// line, so the prompt says what to change instead of "invalid title".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustomTitleError {
    /// Nothing but whitespace.
    Empty,
    /// Longer than `TITLE_MAX_LEN` after trimming. Refused rather than
    /// clamped: the buyer pays for the text they typed, so a title that would
    /// be silently cut short is not the one they agreed to.
    TooLong,
    /// A control, zero-width, or bidi-override character. Those do not render
    /// as a title, they rewrite the line around it.
    Unprintable,
    /// Contains `@`. Titles reach the #lounge feed, whose bodies never carry a
    /// mention, and a title that reads as a handle impersonates its owner.
    Mention,
}

impl CustomTitleError {
    /// Sentence-case banner copy, the one place a refusal is worded.
    pub fn message(self) -> &'static str {
        match self {
            Self::Empty => "Type a title first",
            Self::TooLong => "Titles are capped at 20 characters",
            Self::Unprintable => "That title has characters chat cannot print",
            Self::Mention => "Titles cannot contain @",
        }
    }
}

/// A buyer-written title that has passed every rule a rendered title must
/// obey. The purchase path takes this rather than a `&str`, so there is no way
/// to charge for text nobody checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomTitle(String);

impl CustomTitle {
    /// Parse raw buyer input into a title, or say why it cannot be one.
    /// Surrounding whitespace goes, internal runs collapse to a single space
    /// (a title renders inline, so a tab or a double space would break the
    /// row), and everything else is refused rather than repaired.
    pub fn parse(input: &str) -> Result<Self, CustomTitleError> {
        if input.contains('@') {
            return Err(CustomTitleError::Mention);
        }
        if input.chars().any(is_unprintable_in_a_title) {
            return Err(CustomTitleError::Unprintable);
        }
        let text = input.split_whitespace().collect::<Vec<_>>().join(" ");
        match text.chars().count() {
            0 => Err(CustomTitleError::Empty),
            len if len > TITLE_MAX_LEN => Err(CustomTitleError::TooLong),
            _ => Ok(Self(text)),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

/// Characters a chat row cannot survive: C0/C1 controls (newlines included,
/// which `split_whitespace` would otherwise fold into a space), zero-width
/// joins and spaces, the bidi overrides that reverse the text after them, and
/// the byte-order mark.
fn is_unprintable_in_a_title(ch: char) -> bool {
    ch.is_control()
        || matches!(ch,
            '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FEFF}')
}

#[cfg(test)]
#[path = "rental_test.rs"]
mod rental_test;
