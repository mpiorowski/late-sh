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
    Ja,
    Es,
    Fr,
    Pt,
    De,
    It,
    Pl,
    Ru,
    Uk,
    Tr,
    Vi,
    Id,
    Th,
    Hi,
}

impl TranslateLang {
    /// Settings-row cycle order. Every variant must appear exactly once;
    /// `cycle` walks this list.
    pub const ALL: [Self; 17] = [
        Self::En,
        Self::ZhHans,
        Self::Ko,
        Self::Ja,
        Self::Es,
        Self::Fr,
        Self::Pt,
        Self::De,
        Self::It,
        Self::Pl,
        Self::Ru,
        Self::Uk,
        Self::Tr,
        Self::Vi,
        Self::Id,
        Self::Th,
        Self::Hi,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::ZhHans => "zh-hans",
            Self::Ko => "ko",
            Self::Ja => "ja",
            Self::Es => "es",
            Self::Fr => "fr",
            Self::Pt => "pt",
            Self::De => "de",
            Self::It => "it",
            Self::Pl => "pl",
            Self::Ru => "ru",
            Self::Uk => "uk",
            Self::Tr => "tr",
            Self::Vi => "vi",
            Self::Id => "id",
            Self::Th => "th",
            Self::Hi => "hi",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim() {
            "en" => Some(Self::En),
            "zh-hans" => Some(Self::ZhHans),
            "ko" => Some(Self::Ko),
            "ja" => Some(Self::Ja),
            "es" => Some(Self::Es),
            "fr" => Some(Self::Fr),
            "pt" => Some(Self::Pt),
            "de" => Some(Self::De),
            "it" => Some(Self::It),
            "pl" => Some(Self::Pl),
            "ru" => Some(Self::Ru),
            "uk" => Some(Self::Uk),
            "tr" => Some(Self::Tr),
            "vi" => Some(Self::Vi),
            "id" => Some(Self::Id),
            "th" => Some(Self::Th),
            "hi" => Some(Self::Hi),
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
            Self::Ja => "Japanese 日本語",
            Self::Es => "Spanish Español",
            Self::Fr => "French Français",
            Self::Pt => "Portuguese Português",
            Self::De => "German Deutsch",
            Self::It => "Italian Italiano",
            Self::Pl => "Polish Polski",
            Self::Ru => "Russian Русский",
            Self::Uk => "Ukrainian Українська",
            Self::Tr => "Turkish Türkçe",
            Self::Vi => "Vietnamese Tiếng Việt",
            Self::Id => "Indonesian Bahasa Indonesia",
            Self::Th => "Thai ไทย",
            Self::Hi => "Hindi हिन्दी",
        }
    }

    /// The language name handed to the translation model.
    pub fn prompt_name(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::ZhHans => "Simplified Chinese",
            Self::Ko => "Korean",
            Self::Ja => "Japanese",
            Self::Es => "Spanish",
            Self::Fr => "French",
            Self::Pt => "Portuguese",
            Self::De => "German",
            Self::It => "Italian",
            Self::Pl => "Polish",
            Self::Ru => "Russian",
            Self::Uk => "Ukrainian",
            Self::Tr => "Turkish",
            Self::Vi => "Vietnamese",
            Self::Id => "Indonesian",
            Self::Th => "Thai",
            Self::Hi => "Hindi",
        }
    }

    /// The script the pre-flight check treats as "already this language".
    /// `None` means the check can never clear a message locally: the Latin
    /// roster (English included) shares one script, and Russian and
    /// Ukrainian share Cyrillic, so a same-script body may or may not
    /// already be in the target; those targets send everything scripted to
    /// the model, which reports same-language bodies instead of translating
    /// them, and the cache absorbs the cost either way. English used to
    /// claim Latin as a cost shortcut, but that silently refused to
    /// translate French/Spanish/German for English readers, the most common
    /// real request. Japanese claims Kana; a kanji-only Japanese message
    /// counts as Han and gets one redundant (cached) call, the price of
    /// keeping Chinese translatable for Japanese readers.
    fn script(self) -> Option<Script> {
        match self {
            Self::En => None,
            Self::ZhHans => Some(Script::Han),
            Self::Ko => Some(Script::Hangul),
            Self::Ja => Some(Script::Kana),
            Self::Es
            | Self::Fr
            | Self::Pt
            | Self::De
            | Self::It
            | Self::Pl
            | Self::Tr
            | Self::Vi
            | Self::Id => None,
            Self::Ru | Self::Uk => None,
            Self::Th => Some(Script::Thai),
            Self::Hi => Some(Script::Devanagari),
        }
    }

    pub fn cycle(self, forward: bool) -> Self {
        let idx = Self::ALL
            .iter()
            .position(|lang| *lang == self)
            .expect("every variant is in ALL");
        let next = if forward {
            (idx + 1) % Self::ALL.len()
        } else {
            (idx + Self::ALL.len() - 1) % Self::ALL.len()
        };
        Self::ALL[next]
    }
}

/// Writing systems the pre-flight check can classify. This is a script
/// detector, not a language detector: it exists to answer "could this
/// message already be in the viewer's language?" cheaply and locally, so
/// clearly-foreign-script targets never burn a call on a matching body.
/// All Latin-script languages are deliberately lumped together and no
/// target claims Latin as "already readable"; the model's same-language
/// verdict covers what the script check cannot (see `TranslateLang::script`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Script {
    Latin,
    Han,
    Hangul,
    Kana,
    Cyrillic,
    Thai,
    Devanagari,
}

const SCRIPT_ROSTER: [Script; 7] = [
    Script::Latin,
    Script::Han,
    Script::Hangul,
    Script::Kana,
    Script::Cyrillic,
    Script::Thai,
    Script::Devanagari,
];

fn char_script(c: char) -> Option<Script> {
    match c as u32 {
        // CJK Unified Ideographs, extension A, and compatibility ideographs.
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF => Some(Script::Han),
        // Hangul syllables, jamo, and compatibility jamo.
        0xAC00..=0xD7AF | 0x1100..=0x11FF | 0x3130..=0x318F => Some(Script::Hangul),
        // Hiragana and katakana.
        0x3040..=0x30FF => Some(Script::Kana),
        // Cyrillic and its supplement.
        0x0400..=0x052F => Some(Script::Cyrillic),
        // Thai.
        0x0E00..=0x0E7F => Some(Script::Thai),
        // Devanagari.
        0x0900..=0x097F => Some(Script::Devanagari),
        // Latin Extended Additional carries most Vietnamese diacritics.
        0x1E00..=0x1EFF => Some(Script::Latin),
        _ if c.is_ascii_alphabetic() || matches!(c as u32, 0xC0..=0x24F) => Some(Script::Latin),
        _ => None,
    }
}

/// The part of a stored chat body that translation should look at: the
/// composer stores a reply as `> @author: preview\nreply text`, and the
/// quoted first line is someone else's message, already rendered (and
/// translatable) on its own. Translating it again inside the reply would
/// put re-worded text in the quoted author's mouth and pollute the reply's
/// translation, so every translation surface (the worth-translating check,
/// the model prompt, the cached text) works on this slice. The same `> `
/// convention guards Drunk Text (`slur.rs`). Bodies that are only a quote
/// line pass through whole rather than vanishing.
pub fn translation_source_text(body: &str) -> &str {
    match body.split_once('\n') {
        Some((first_line, rest))
            if first_line.trim_start().starts_with("> ") && !rest.trim().is_empty() =>
        {
            rest
        }
        _ => body,
    }
}

/// Whether `body` is worth translating for a viewer reading `target`: its
/// reply-quote-free text (see [`translation_source_text`]) has enough
/// scripted characters to mean something, its dominant script differs from
/// the target's, and it fits the length cap. Numbers, emoji, URLs-only and
/// other unscripted bodies never qualify. Targets without a script of their
/// own (English and the Latin roster, and Russian/Ukrainian on shared
/// Cyrillic) translate every scripted body: the check can't prove a
/// same-script message is already in the target language, so the model
/// decides (reporting same-language bodies instead of translating them)
/// and the cache remembers.
pub fn needs_translation(body: &str, target: TranslateLang) -> bool {
    let body = translation_source_text(body);
    if body.chars().count() > TRANSLATE_MAX_BODY_CHARS {
        return false;
    }
    let mut counts = [0usize; SCRIPT_ROSTER.len()];
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
    let dominant = SCRIPT_ROSTER
        .into_iter()
        .max_by_key(|script| counts[*script as usize])
        .expect("script roster is non-empty");
    match target.script() {
        Some(script) => dominant != script,
        None => true,
    }
}

/// One cached model verdict for a (message, target) pair. Same-language is
/// a real, cacheable answer, not an absence: it cost a call to learn and it
/// renders as nothing, so it must be distinguishable from both a cache miss
/// (which may trigger a call) and a translation (which renders a `↳` line).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CachedTranslation {
    Translated(String),
    SameLanguage,
}

/// One cached row as read back: the verdict plus whether the message's
/// author chose to share it. Author-shared rows (written by the "translate
/// my messages to English" opt-in) display to every viewer reading the
/// target language; private rows (a reader's `t`) display only to sessions
/// that asked.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CachedTranslationRow {
    pub verdict: CachedTranslation,
    pub author_shared: bool,
}

pub struct MessageTranslation;

impl MessageTranslation {
    /// Cached translations for `message_ids` into `target`, keyed by message.
    pub async fn get_many(
        client: &impl GenericClient,
        message_ids: &[Uuid],
        target: TranslateLang,
    ) -> Result<HashMap<Uuid, CachedTranslationRow>> {
        if message_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = client
            .query(
                "SELECT message_id, body, same_language, author_shared FROM message_translations
                 WHERE message_id = ANY($1) AND target_lang = $2",
                &[&message_ids, &target.as_str()],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let verdict = if row.get::<_, bool>("same_language") {
                    CachedTranslation::SameLanguage
                } else {
                    CachedTranslation::Translated(row.get("body"))
                };
                let cached = CachedTranslationRow {
                    verdict,
                    author_shared: row.get("author_shared"),
                };
                (row.get("message_id"), cached)
            })
            .collect())
    }

    /// Cache `verdict` for `message_id` into `target`, but only while the
    /// message's body is still `source_body`. Translation calls are slow; an
    /// edit can land mid-flight, and its row delete runs before this row
    /// exists, so an unconditional write would cache the old body's
    /// translation against the new text forever. `FOR SHARE` makes the check
    /// wait out a concurrent edit's row lock and re-evaluate against the
    /// committed body. Returns whether the row landed; a `false` means the
    /// verdict described text that no longer exists and was discarded.
    pub async fn upsert_if_current(
        client: &impl GenericClient,
        message_id: Uuid,
        target: TranslateLang,
        source_body: &str,
        verdict: &CachedTranslation,
        author_shared: bool,
    ) -> Result<bool> {
        // A same-language row stores the judged text in body, so the row
        // always says exactly what was cached about what. The conflict arm
        // ORs author_shared: once the author shared a translation it stays
        // shared, even if a reader's private request rewrites the row.
        let (body, same_language) = match verdict {
            CachedTranslation::Translated(text) => (text.as_str(), false),
            CachedTranslation::SameLanguage => (translation_source_text(source_body), true),
        };
        let written = client
            .execute(
                "INSERT INTO message_translations
                     (message_id, target_lang, body, same_language, author_shared)
                 SELECT id, $2, $3, $5, $6 FROM chat_messages WHERE id = $1 AND body = $4 FOR SHARE
                 ON CONFLICT (message_id, target_lang)
                 DO UPDATE SET body = EXCLUDED.body, same_language = EXCLUDED.same_language,
                     author_shared = message_translations.author_shared OR EXCLUDED.author_shared",
                &[
                    &message_id,
                    &target.as_str(),
                    &body,
                    &source_body,
                    &same_language,
                    &author_shared,
                ],
            )
            .await?;
        Ok(written > 0)
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
