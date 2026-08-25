//! Screening for text a player writes and everyone else has to read.
//!
//! Today that is the Shop's custom title (SHOP.md phase 1b): 20 characters the
//! buyer types and then wears after their name in every message they send. The
//! call is ungrounded and schema-enforced (`AiService::generate_json`), so the
//! verdict comes back as JSON in a shape Gemini guarantees, the same trade the
//! bartender's order flow makes.
//!
//! Two rules govern this module, and both point the same way:
//!
//! - **Never charge for a no-op.** The screen runs before the purchase
//!   transaction opens. A refusal costs nothing.
//! - **Fail closed.** Anything short of a clear allow is a refusal: the model
//!   off, the call unusable, the JSON unreadable. Text nobody screened must
//!   never reach a chat row, so "we could not check" and "we said no" have the
//!   same outcome.

use anyhow::Result;
use serde_json::json;
use std::time::Duration;

use super::svc::AiService;

/// The screen answers from its own instructions, not the web, so it runs on
/// the same model tier as the rest of the house bots.
const SCREEN_MODEL: &str = super::svc::AI_MODEL;

/// A buyer is waiting on this with a modal open. Well past a normal
/// ungrounded call, short enough that a hung API does not hold the prompt.
const SCREEN_TIMEOUT: Duration = Duration::from_secs(30);

/// How much of a model-written refusal reason reaches the buyer's banner. Long
/// enough to name the problem, short enough that it cannot flood the row.
const REASON_MAX_LEN: usize = 80;

/// What the house decided about a piece of player-written text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TitleScreen {
    /// Safe to sell and to render.
    Allowed,
    /// The screen read it and said no. `reason` is buyer-facing banner copy.
    Refused { reason: String },
    /// No verdict exists: AI is switched off, or the call came back with
    /// nothing usable. Callers must treat this as a refusal, never as an
    /// allow.
    Unavailable,
}

const SCREEN_PERSONA: &str = "You screen short display titles for a late-night terminal chat community. \
    A title is up to 20 characters that a paying member wears after their username in every message, like \"mira, the night clerk\". \
    The house register is Blade Runner noir: wry, weary, self-deprecating, a little seedy. That register is the point, so allow it generously.";

/// Refuse for a reason the buyer can act on. Kept house-side rather than
/// model-written so a refusal never leaks the screening instructions back.
const HOUSE_REFUSAL: &str = "That title did not pass the house screen";

/// Screen one buyer-written title. `Err` means the call itself broke (network,
/// API error) and is the caller's to log; every other outcome is a verdict.
pub async fn screen_custom_title(ai: &AiService, title: &str) -> Result<TitleScreen> {
    if !ai.is_enabled() {
        return Ok(TitleScreen::Unavailable);
    }

    let system_prompt = format!(
        "{SCREEN_PERSONA}\n\n\
        ALLOW: noir flavour, jokes, brags, self-deprecation, nonsense words, in-jokes, mild profanity, \
        anything that just reads as a person naming themselves.\n\
        REFUSE only for: slurs or hate aimed at a group; harassment, threats, or naming another member; \
        sexual content; impersonating staff, moderators, admins, the house bots (@bot, @graybeard, @bartender), or a system message; \
        advertising, links, or invites to somewhere else.\n\n\
        When it is borderline, allow it. This is a bar, not a boardroom.\n\n\
        Return a verdict as JSON. \"reason\" is one short phrase (under 12 words) the buyer will read, \
        naming what is wrong; leave it empty when you allow."
    );
    let prompt = format!("Title to screen, between the markers:\n<<<{title}>>>");

    let reply = match tokio::time::timeout(
        SCREEN_TIMEOUT,
        ai.generate_json(SCREEN_MODEL, &system_prompt, &prompt, title_screen_schema()),
    )
    .await
    {
        Ok(Ok(Some(reply))) => reply,
        Ok(Ok(None)) => return Ok(TitleScreen::Unavailable),
        Ok(Err(error)) => return Err(error),
        Err(_) => return Err(anyhow::anyhow!("custom title screen timed out")),
    };

    Ok(parse_title_screen(&reply))
}

/// The response schema Gemini must conform the verdict to. Enforced
/// server-side (only possible ungrounded), so the reply is always valid JSON
/// in this exact shape.
fn title_screen_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "allowed": { "type": "boolean" },
            "reason": { "type": "string" }
        },
        "required": ["allowed", "reason"],
        "propertyOrdering": ["allowed", "reason"]
    })
}

#[derive(serde::Deserialize)]
struct TitleScreenRaw {
    allowed: bool,
    reason: Option<String>,
}

/// Turn the model's JSON into a verdict. Unreadable JSON is a refusal, not an
/// allow: the whole point of the screen is that untested text never ships.
fn parse_title_screen(raw: &str) -> TitleScreen {
    let cleaned = strip_code_fence(raw);
    match serde_json::from_str::<TitleScreenRaw>(cleaned) {
        Ok(verdict) if verdict.allowed => TitleScreen::Allowed,
        Ok(verdict) => TitleScreen::Refused {
            reason: refusal_line(verdict.reason.as_deref()),
        },
        Err(error) => {
            tracing::warn!(error = ?error, "custom title screen returned unreadable json");
            TitleScreen::Refused {
                reason: HOUSE_REFUSAL.to_string(),
            }
        }
    }
}

/// Banner copy for a refusal: the model's own phrase when it wrote a usable
/// one, the house line otherwise. Model text is squeezed onto one line and
/// capped, because it lands in a UI row.
fn refusal_line(reason: Option<&str>) -> String {
    let Some(reason) = reason else {
        return HOUSE_REFUSAL.to_string();
    };
    let squeezed = reason.split_whitespace().collect::<Vec<_>>().join(" ");
    match squeezed.chars().count() {
        0 => HOUSE_REFUSAL.to_string(),
        len if len > REASON_MAX_LEN => HOUSE_REFUSAL.to_string(),
        _ => format!("{HOUSE_REFUSAL}: {squeezed}"),
    }
}

/// Strip a wrapping markdown code fence. Schema mode should never produce one,
/// but the bartender's order flow sees them anyway, so the same guard is worth
/// the six lines here rather than reaching into that module for it.
fn strip_code_fence(raw: &str) -> &str {
    let trimmed = raw.trim();
    let Some(rest) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    let rest = rest.strip_prefix("json").unwrap_or(rest);
    rest.trim().strip_suffix("```").unwrap_or(rest).trim()
}

#[cfg(test)]
#[path = "screen_test.rs"]
mod screen_test;
