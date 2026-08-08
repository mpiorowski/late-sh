//! Cached chat-message translations and the language roster behind them.
//!
//! One row per (message, target language). The cache is the load-bearing
//! wall of the translation feature: cost scales with messages written, not
//! readers, because the first viewer's API call lands here and everyone
//! after reads the row. Rows die with their message (FK cascade); edits
//! delete rows explicitly inside the edit transaction so a translation
//! never outlives the text it translated.

use anyhow::Result;
use deadpool_postgres::GenericClient;
use std::collections::HashMap;
use uuid::Uuid;

/// Bodies longer than this are never sent for translation. Chat messages
/// are short; anything past the cap is pasted content that would only burn
/// tokens for a wall of text nobody expects translated inline.
pub const TRANSLATE_MAX_BODY_CHARS: usize = 1_500;

/// The closed roster of translation targets. A new language is a new
/// variant: `as_str` is the settings/DB key, `label` the settings row text,
/// `prompt_name` what the model is asked to translate into, and `script`
/// what the source-script check compares against.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TranslateLang {
    En,
    ZhHans,
    Ko,
}

impl TranslateLang {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::ZhHans => "zh-hans",
            Self::Ko => "ko",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim() {
            "en" => Some(Self::En),
            "zh-hans" => Some(Self::ZhHans),
            "ko" => Some(Self::Ko),
            _ => None,
        }
    }

    /// Settings-row label: English name plus the native name where they
    /// differ, so a user hunting for their own language finds it.
    pub fn label(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::ZhHans => "Chinese 简体中文",
            Self::Ko => "Korean 한국어",
        }
    }

    /// The language name handed to the translation model.
    pub fn prompt_name(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::ZhHans => "Simplified Chinese",
            Self::Ko => "Korean",
        }
    }

    fn script(self) -> Script {
        match self {
            Self::En => Script::Latin,
            Self::ZhHans => Script::Han,
            Self::Ko => Script::Hangul,
        }
    }

    pub fn cycle(self, forward: bool) -> Self {
        match (self, forward) {
            (Self::En, true) => Self::ZhHans,
            (Self::ZhHans, true) => Self::Ko,
            (Self::Ko, true) => Self::En,
            (Self::En, false) => Self::Ko,
            (Self::ZhHans, false) => Self::En,
            (Self::Ko, false) => Self::ZhHans,
        }
    }
}

/// Writing systems the pre-flight check can classify. This is a script
/// detector, not a language detector: it exists to answer "could this
/// message already be in the viewer's language?" cheaply and locally, so
/// same-script messages never reach the API. Latin-script languages other
/// than English are deliberately lumped together; the feature targets
/// CJK↔EN chat, where script alone separates the two sides.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Script {
    Latin,
    Han,
    Hangul,
    Kana,
}

fn char_script(c: char) -> Option<Script> {
    match c as u32 {
        // CJK Unified Ideographs, extension A, and compatibility ideographs.
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF => Some(Script::Han),
        // Hangul syllables, jamo, and compatibility jamo.
        0xAC00..=0xD7AF | 0x1100..=0x11FF | 0x3130..=0x318F => Some(Script::Hangul),
        // Hiragana and katakana.
        0x3040..=0x309F | 0x30A0..=0x30FF => Some(Script::Kana),
        _ if c.is_ascii_alphabetic() || matches!(c as u32, 0xC0..=0x24F) => Some(Script::Latin),
        _ => None,
    }
}

/// Whether `body` is worth translating for a viewer reading `target`: it has
/// enough scripted text to mean something, its dominant script differs from
/// the target's, and it fits the length cap. Numbers, emoji, URLs-only and
/// other unscripted bodies never qualify. Kana counts as foreign for every
/// current target, so Japanese messages translate even though Japanese is
/// not yet a target language.
pub fn needs_translation(body: &str, target: TranslateLang) -> bool {
    if body.chars().count() > TRANSLATE_MAX_BODY_CHARS {
        return false;
    }
    let mut counts = [0usize; 4];
    for c in body.chars() {
        if let Some(script) = char_script(c) {
            counts[script as usize] += 1;
        }
    }
    let total: usize = counts.iter().sum();
    // A couple of stray letters (a laugh, "ok", a username) are not a
    // message in a language; require a handful before calling it text.
    if total < 3 {
        return false;
    }
    let dominant = [Script::Latin, Script::Han, Script::Hangul, Script::Kana]
        .into_iter()
        .max_by_key(|script| counts[*script as usize])
        .expect("script roster is non-empty");
    dominant != target.script()
}

pub struct MessageTranslation;

impl MessageTranslation {
    /// Cached translations for `message_ids` into `target`, keyed by message.
    pub async fn get_many(
        client: &impl GenericClient,
        message_ids: &[Uuid],
        target: TranslateLang,
    ) -> Result<HashMap<Uuid, String>> {
        if message_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = client
            .query(
                "SELECT message_id, body FROM message_translations
                 WHERE message_id = ANY($1) AND target_lang = $2",
                &[&message_ids, &target.as_str()],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.get("message_id"), row.get("body")))
            .collect())
    }

    pub async fn upsert(
        client: &impl GenericClient,
        message_id: Uuid,
        target: TranslateLang,
        body: &str,
    ) -> Result<()> {
        client
            .execute(
                "INSERT INTO message_translations (message_id, target_lang, body)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (message_id, target_lang) DO UPDATE SET body = EXCLUDED.body",
                &[&message_id, &target.as_str(), &body],
            )
            .await?;
        Ok(())
    }

    /// Drop every cached translation of a message. Called inside the edit
    /// transaction: the cached text describes the pre-edit body.
    pub async fn delete_for_message(client: &impl GenericClient, message_id: Uuid) -> Result<u64> {
        Ok(client
            .execute(
                "DELETE FROM message_translations WHERE message_id = $1",
                &[&message_id],
            )
            .await?)
    }
}
