//! Chat message translation: the one place a message body meets the
//! translation model.
//!
//! Every request funnels through [`TranslationService::request`]:
//! single-flight dedupe (N sessions rendering the same new message cost one
//! call), DB cache first (`message_translations`, so cost scales with
//! messages written rather than readers), a global daily call cap as the
//! runaway backstop, and a small concurrency gate so a lively minute queues
//! instead of tripping API rate limits. Results fan out on a broadcast
//! channel; sessions drain it in `ChatState::tick` like every other service.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use chrono::{NaiveDate, Utc};
use late_core::db::Db;
use late_core::models::message_translation::{
    CachedTranslation, CachedTranslationRow, MessageTranslation, TranslateLang,
    translation_source_text,
};
use serde::Deserialize;
use tokio::sync::{Semaphore, broadcast};
use tracing::Instrument;
use uuid::Uuid;

use super::svc::{AI_MODEL, AiService};
use crate::metrics::{self, TranslationResult};

/// Hard ceiling on translation API calls per UTC day, across the process.
/// This is the runaway backstop, not a budget anyone should ever reach:
/// at chat-message sizes it caps worst-case spend around a dollar a day.
/// Legitimate traffic lives orders of magnitude below it; hitting the cap
/// means a bug or abuse, and translations degrade to "unavailable" until
/// the UTC day rolls over.
pub const TRANSLATE_DAILY_CAP: u32 = 20_000;

/// Concurrent API calls allowed; excess requests queue on the semaphore so a
/// burst of messages trickles through instead of erroring on rate limits.
const MAX_CONCURRENT_CALLS: usize = 4;

const EVENT_CHANNEL_CAP: usize = 256;

#[derive(Clone, Debug)]
pub struct TranslationEvent {
    pub message_id: Uuid,
    pub room_id: Uuid,
    pub target: TranslateLang,
    pub outcome: TranslationOutcome,
    /// The author asked for this translation to be shown to everyone reading
    /// `target` (the "translate my messages to English" opt-in). Sessions
    /// display these without auto mode or a `t`; private results still only
    /// show to sessions that asked.
    pub author_shared: bool,
}

#[derive(Clone, Debug)]
pub enum TranslationOutcome {
    Translated(String),
    /// The model judged the message already written in the target language.
    /// A real, broadcast-worthy answer: sessions cache it as "render
    /// nothing" so the message is never re-requested.
    SameLanguage,
    Failed,
}

#[derive(Deserialize)]
struct TranslationReply {
    translation: String,
    same_language: bool,
}

#[derive(Clone)]
pub struct TranslationService {
    db: Db,
    ai: AiService,
    event_tx: broadcast::Sender<TranslationEvent>,
    inflight: Arc<Mutex<HashSet<(Uuid, TranslateLang)>>>,
    daily_spend: Arc<Mutex<(NaiveDate, u32)>>,
    api_gate: Arc<Semaphore>,
}

impl TranslationService {
    pub fn new(db: Db, ai: AiService) -> Self {
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAP);
        Self {
            db,
            ai,
            event_tx,
            inflight: Arc::new(Mutex::new(HashSet::new())),
            daily_spend: Arc::new(Mutex::new((Utc::now().date_naive(), 0))),
            api_gate: Arc::new(Semaphore::new(MAX_CONCURRENT_CALLS)),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TranslationEvent> {
        self.event_tx.subscribe()
    }

    /// Translate `body` into `target`, cache first, API second.
    /// Fire-and-forget: the result arrives as a [`TranslationEvent`] for
    /// every subscriber, including the single-flight losers' sessions.
    pub fn request(&self, message_id: Uuid, room_id: Uuid, body: String, target: TranslateLang) {
        self.request_inner(message_id, room_id, body, target, false);
    }

    /// Same pipeline, but on behalf of the author's "translate my messages"
    /// opt-in: the cache row is marked author_shared and the event carries
    /// the flag, so every session reading `target` displays the result.
    pub fn request_shared(
        &self,
        message_id: Uuid,
        room_id: Uuid,
        body: String,
        target: TranslateLang,
    ) {
        self.request_inner(message_id, room_id, body, target, true);
    }

    fn request_inner(
        &self,
        message_id: Uuid,
        room_id: Uuid,
        body: String,
        target: TranslateLang,
        author_shared: bool,
    ) {
        {
            let mut inflight = self.inflight.lock().expect("inflight lock poisoned");
            if !inflight.insert((message_id, target)) {
                return;
            }
        }
        let service = self.clone();
        tokio::spawn(
            async move {
                let (outcome, author_shared) = service
                    .resolve(message_id, body, target, author_shared)
                    .await;
                service
                    .inflight
                    .lock()
                    .expect("inflight lock poisoned")
                    .remove(&(message_id, target));
                let _ = service.event_tx.send(TranslationEvent {
                    message_id,
                    room_id,
                    target,
                    outcome,
                    author_shared,
                });
            }
            .instrument(tracing::info_span!(
                "chat.translate",
                message_id = %message_id,
                target = %target.as_str()
            )),
        );
    }

    /// Bulk cache-only lookup for messages already on screen (every session
    /// entering a room; auto mode pre-expands hits, other sessions display
    /// only author-shared rows). Hits broadcast like live translations;
    /// misses stay silent, since history is translated on demand only.
    pub fn load_cached(&self, room_id: Uuid, message_ids: Vec<Uuid>, target: TranslateLang) {
        if message_ids.is_empty() {
            return;
        }
        let service = self.clone();
        tokio::spawn(async move {
            let client = match service.db.get().await {
                Ok(client) => client,
                Err(error) => {
                    tracing::error!(error = ?error, "translation cache lookup could not get db client");
                    return;
                }
            };
            let cached = match MessageTranslation::get_many(&client, &message_ids, target).await {
                Ok(cached) => cached,
                Err(error) => {
                    tracing::error!(error = ?error, "translation cache lookup failed");
                    return;
                }
            };
            for (message_id, row) in cached {
                metrics::record_chat_translation(TranslationResult::CacheHit);
                let outcome = match row.verdict {
                    CachedTranslation::Translated(body) => TranslationOutcome::Translated(body),
                    CachedTranslation::SameLanguage => TranslationOutcome::SameLanguage,
                };
                let _ = service.event_tx.send(TranslationEvent {
                    message_id,
                    room_id,
                    target,
                    outcome,
                    author_shared: row.author_shared,
                });
            }
        });
    }

    /// The full resolution pipeline for one message. This is the single
    /// match listing every way a translation request can end. Returns the
    /// outcome plus the author_shared flag the event should carry: the
    /// request's own flag, except a cache hit reports what the row says
    /// (a private `t` on an author-shared message still shares the event).
    async fn resolve(
        &self,
        message_id: Uuid,
        body: String,
        target: TranslateLang,
        author_shared: bool,
    ) -> (TranslationOutcome, bool) {
        match self
            .resolve_inner(message_id, &body, target, author_shared)
            .await
        {
            Ok(Resolution::CacheHit(row)) => {
                metrics::record_chat_translation(TranslationResult::CacheHit);
                let outcome = match row.verdict {
                    CachedTranslation::Translated(text) => TranslationOutcome::Translated(text),
                    CachedTranslation::SameLanguage => TranslationOutcome::SameLanguage,
                };
                (outcome, row.author_shared || author_shared)
            }
            Ok(Resolution::Translated(text)) => {
                metrics::record_chat_translation(TranslationResult::Translated);
                (TranslationOutcome::Translated(text), author_shared)
            }
            Ok(Resolution::SameLanguage) => {
                metrics::record_chat_translation(TranslationResult::SameLanguage);
                (TranslationOutcome::SameLanguage, author_shared)
            }
            Ok(Resolution::CapExhausted) => {
                metrics::record_chat_translation(TranslationResult::CapExhausted);
                tracing::warn!("translation daily cap exhausted; refusing until utc rollover");
                (TranslationOutcome::Failed, author_shared)
            }
            Ok(Resolution::NoText) => {
                metrics::record_chat_translation(TranslationResult::Failed);
                tracing::warn!("translation model returned no usable text");
                (TranslationOutcome::Failed, author_shared)
            }
            Ok(Resolution::Stale) => {
                metrics::record_chat_translation(TranslationResult::Stale);
                tracing::info!("translation discarded; message edited mid-flight");
                (TranslationOutcome::Failed, author_shared)
            }
            Err(error) => {
                metrics::record_chat_translation(TranslationResult::Failed);
                tracing::error!(error = ?error, "translation request failed");
                (TranslationOutcome::Failed, author_shared)
            }
        }
    }

    async fn resolve_inner(
        &self,
        message_id: Uuid,
        body: &str,
        target: TranslateLang,
        author_shared: bool,
    ) -> anyhow::Result<Resolution> {
        // Acquire and release the client around the cache check: requests
        // queue on the API gate below, and a queued request holding a pooled
        // connection would starve the rest of the app under a burst (the news
        // summarizer scopes its client the same way).
        {
            let client = self.db.get().await?;
            let mut cached = MessageTranslation::get_many(&client, &[message_id], target).await?;
            if let Some(cached) = cached.remove(&message_id) {
                return Ok(Resolution::CacheHit(cached));
            }
        }

        if !self.spend_from_daily_cap() {
            return Ok(Resolution::CapExhausted);
        }

        // The model sees the reply-quote-free text: a reply's quoted first
        // line is someone else's message, translated (or not) on its own.
        let source_text = translation_source_text(body);
        let _permit = self.api_gate.acquire().await?;
        let system_prompt = format!(
            "You translate chat messages for a cozy terminal clubhouse. Translate the \
             user's message into {}. Keep the tone, slang, and formatting. Leave URLs, \
             @mentions, `code spans`, emoji, and /commands exactly as written. If the \
             message is already written in {}, or contains no translatable natural \
             language (URLs, numbers, emoji), set same_language to true and return the \
             message unchanged. Reply with only the translation.",
            target.prompt_name(),
            target.prompt_name()
        );
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "translation": { "type": "string" },
                "same_language": { "type": "boolean" }
            },
            "required": ["translation", "same_language"]
        });
        let reply = self
            .ai
            .generate_json(AI_MODEL, &system_prompt, source_text, schema)
            .await?;
        let Some(reply) = reply else {
            return Ok(Resolution::NoText);
        };
        let parsed: TranslationReply = serde_json::from_str(&reply)?;
        let text = parsed.translation.trim().to_string();
        if text.is_empty() {
            return Ok(Resolution::NoText);
        }
        // An echoed body means nothing needed translating even when the
        // model forgot to say so (URL-only messages tend to come back
        // verbatim with same_language false). Rendering it would just
        // duplicate the message in dim italics.
        let verdict = if parsed.same_language || text == source_text.trim() {
            CachedTranslation::SameLanguage
        } else {
            CachedTranslation::Translated(text)
        };

        // The staleness guard compares the full stored body: that is what
        // an edit rewrites, quote line included.
        let client = self.db.get().await?;
        let written = MessageTranslation::upsert_if_current(
            &client,
            message_id,
            target,
            body,
            &verdict,
            author_shared,
        )
        .await?;
        if !written {
            return Ok(Resolution::Stale);
        }
        match verdict {
            CachedTranslation::Translated(text) => Ok(Resolution::Translated(text)),
            CachedTranslation::SameLanguage => Ok(Resolution::SameLanguage),
        }
    }

    /// One API call's worth of the daily cap, reset on UTC day rollover.
    /// True when the call may proceed.
    fn spend_from_daily_cap(&self) -> bool {
        let today = Utc::now().date_naive();
        let mut spend = self.daily_spend.lock().expect("daily spend lock poisoned");
        if spend.0 != today {
            *spend = (today, 0);
        }
        if spend.1 >= TRANSLATE_DAILY_CAP {
            return false;
        }
        spend.1 += 1;
        true
    }
}

enum Resolution {
    CacheHit(CachedTranslationRow),
    Translated(String),
    /// The model judged the message already in the target language (or the
    /// echo guard did); cached so nobody pays for the call again.
    SameLanguage,
    CapExhausted,
    NoText,
    /// The message was edited while the call was in flight; the result
    /// described text that no longer exists and was not cached.
    Stale,
}

#[cfg(test)]
#[path = "translate_test.rs"]
mod translate_test;
