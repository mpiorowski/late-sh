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
use late_core::models::message_translation::{MessageTranslation, TranslateLang};
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
}

#[derive(Clone, Debug)]
pub enum TranslationOutcome {
    Translated(String),
    Failed,
}

#[derive(Deserialize)]
struct TranslationReply {
    translation: String,
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
        {
            let mut inflight = self.inflight.lock().expect("inflight lock poisoned");
            if !inflight.insert((message_id, target)) {
                return;
            }
        }
        let service = self.clone();
        tokio::spawn(
            async move {
                let outcome = service.resolve(message_id, body, target).await;
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
                });
            }
            .instrument(tracing::info_span!(
                "chat.translate",
                message_id = %message_id,
                target = %target.as_str()
            )),
        );
    }

    /// Bulk cache-only lookup for messages already on screen (auto mode
    /// entering a room). Hits broadcast like live translations; misses stay
    /// silent, since history is translated on demand only.
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
            for (message_id, body) in cached {
                metrics::record_chat_translation(TranslationResult::CacheHit);
                let _ = service.event_tx.send(TranslationEvent {
                    message_id,
                    room_id,
                    target,
                    outcome: TranslationOutcome::Translated(body),
                });
            }
        });
    }

    /// The full resolution pipeline for one message. This is the single
    /// match listing every way a translation request can end.
    async fn resolve(
        &self,
        message_id: Uuid,
        body: String,
        target: TranslateLang,
    ) -> TranslationOutcome {
        match self.resolve_inner(message_id, &body, target).await {
            Ok(Resolution::CacheHit(text)) => {
                metrics::record_chat_translation(TranslationResult::CacheHit);
                TranslationOutcome::Translated(text)
            }
            Ok(Resolution::Translated(text)) => {
                metrics::record_chat_translation(TranslationResult::Translated);
                TranslationOutcome::Translated(text)
            }
            Ok(Resolution::CapExhausted) => {
                metrics::record_chat_translation(TranslationResult::CapExhausted);
                tracing::warn!("translation daily cap exhausted; refusing until utc rollover");
                TranslationOutcome::Failed
            }
            Ok(Resolution::NoText) => {
                metrics::record_chat_translation(TranslationResult::Failed);
                tracing::warn!("translation model returned no usable text");
                TranslationOutcome::Failed
            }
            Ok(Resolution::Stale) => {
                metrics::record_chat_translation(TranslationResult::Stale);
                tracing::info!("translation discarded; message edited mid-flight");
                TranslationOutcome::Failed
            }
            Err(error) => {
                metrics::record_chat_translation(TranslationResult::Failed);
                tracing::error!(error = ?error, "translation request failed");
                TranslationOutcome::Failed
            }
        }
    }

    async fn resolve_inner(
        &self,
        message_id: Uuid,
        body: &str,
        target: TranslateLang,
    ) -> anyhow::Result<Resolution> {
        // Acquire and release the client around the cache check: requests
        // queue on the API gate below, and a queued request holding a pooled
        // connection would starve the rest of the app under a burst (the news
        // summarizer scopes its client the same way).
        {
            let client = self.db.get().await?;
            let cached = MessageTranslation::get_many(&client, &[message_id], target).await?;
            if let Some(text) = cached.into_iter().next().map(|(_, text)| text) {
                return Ok(Resolution::CacheHit(text));
            }
        }

        if !self.spend_from_daily_cap() {
            return Ok(Resolution::CapExhausted);
        }

        let _permit = self.api_gate.acquire().await?;
        let system_prompt = format!(
            "You translate chat messages for a cozy terminal clubhouse. Translate the \
             user's message into {}. Keep the tone, slang, and formatting. Leave URLs, \
             @mentions, `code spans`, emoji, and /commands exactly as written. Reply \
             with only the translation.",
            target.prompt_name()
        );
        let schema = serde_json::json!({
            "type": "object",
            "properties": { "translation": { "type": "string" } },
            "required": ["translation"]
        });
        let reply = self
            .ai
            .generate_json(AI_MODEL, &system_prompt, body, schema)
            .await?;
        let Some(reply) = reply else {
            return Ok(Resolution::NoText);
        };
        let parsed: TranslationReply = serde_json::from_str(&reply)?;
        let text = parsed.translation.trim().to_string();
        if text.is_empty() {
            return Ok(Resolution::NoText);
        }

        let client = self.db.get().await?;
        let written =
            MessageTranslation::upsert_if_current(&client, message_id, target, body, &text).await?;
        if !written {
            return Ok(Resolution::Stale);
        }
        Ok(Resolution::Translated(text))
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
    CacheHit(String),
    Translated(String),
    CapExhausted,
    NoText,
    /// The message was edited while the call was in flight; the result
    /// described text that no longer exists and was not cached.
    Stale,
}

#[cfg(test)]
#[path = "translate_test.rs"]
mod translate_test;
