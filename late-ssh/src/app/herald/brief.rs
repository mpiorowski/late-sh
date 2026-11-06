//! The brief: one text call at pick time that compresses the operator's
//! `#announcements` of the day into the words of an announcement post
//! (`state::ANNOUNCEMENT_MAX_CHARS`), the full note being on the edition's
//! page the post links. Returns the words or an error; `svc.rs` logs and
//! counts, and posts an error as the note as written, cut
//! (`state::Brief::Cut`). AI off is `None`, without a call.

use anyhow::{Context, bail};
use serde::Deserialize;
use serde_json::json;

use super::state::ANNOUNCEMENT_MAX_CHARS;
use crate::app::ai::svc::{AI_MODEL, AiService};

/// The prompt's opening: what the call is for.
const OPENING: &str = "You write the one short post with which late.sh, a command-line clubhouse reached over SSH, announces its news on Mastodon and Bluesky. You are given the day's release note as its operator wrote it.";

/// The rules. The note's commands and names are the one thing a reader
/// will type, so they travel verbatim; nothing is added, since the post
/// goes out under the official account unread. The voice is a person
/// telling a friend what changed: short, plain, no sell.
const RULES: &str = r#"Write two or three short paragraphs, one or two plain sentences each, a blank line between them, as late.sh, in the note's own voice. Lead with the one change a reader would care about most, then the next one or two. Keep every command, key, path, flag and product name exactly as the note writes it. Never state a feature, a number, a date or a command the note does not contain. Name nobody. No markdown, no headings, no bullet points, no emoji. No hashtags, no links and no @ mentions: the link to the full note is added after your text. The post is read on a phone, so the whole text stays under the character limit given.

Sound like a person telling a friend what changed, not like a product launch or a press release. Use short, everyday words: "fixed", "added", "now", "no longer". Never "seamless", "elevate", "unlock", "empower", "robust", "effortless", "supercharge", "experience", "journey", "exciting", "delighted", "thrilled". No exclamation marks, no dashes used as punctuation (no em dash, no en dash: use a comma, a colon or a full stop), no calls to action, no "we're excited to announce", no "stay tuned", no rhetorical questions. Say what changed and what it means for the reader, then stop."#;

/// The model's answer as the schema holds it.
#[derive(Deserialize)]
struct Answer {
    text: String,
}

/// Ask the model for the words of `note`. AI off is `None` without a
/// call; a call that fails or answers nothing usable is an error.
pub(crate) async fn brief(ai: &AiService, note: &str) -> anyhow::Result<Option<String>> {
    if !ai.is_enabled() {
        return Ok(None);
    }
    let prompt = format!(
        "Write the post for this note, under {ANNOUNCEMENT_MAX_CHARS} characters.\n\n{note}"
    );
    let answer = ai
        .generate_json(
            AI_MODEL,
            &format!("{OPENING}\n\n{RULES}"),
            &prompt,
            schema(),
        )
        .await
        .context("asking the model")?;
    match answer {
        Some(text) => parse(&text).map(Some),
        None => bail!("the model gave no answer"),
    }
}

/// The answer's shape.
pub(crate) fn schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "text": { "type": "string" },
        },
        "required": ["text"],
    })
}

/// The answer's words, trimmed; empty is refused, never posted.
pub(crate) fn parse(text: &str) -> anyhow::Result<String> {
    let answer: Answer = serde_json::from_str(text).context("decoding the brief's answer")?;
    let words = answer.text.trim();
    if words.is_empty() {
        bail!("the brief is empty");
    }
    Ok(words.to_string())
}
