use anyhow::{Context, Result};
use late_core::telemetry::TracedExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// The model backing @bot's grounded chat/news replies. Gemini 3.6 Flash beats
/// 3.1 Pro on coding/agentic benchmarks while costing less and running faster;
/// Pro only keeps an edge on the hardest reasoning benchmarks, which this bot
/// doesn't need.
pub const AI_MODEL: &str = "gemini-3.6-flash";

#[derive(Debug, Clone)]
pub struct AiService {
    client: Client,
    api_key: Option<String>,
    enabled: bool,
}

#[derive(Serialize)]
struct GeminiRequest<'a> {
    #[serde(rename = "systemInstruction", skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent<'a>>,
    contents: Vec<GeminiContent<'a>>,
    #[serde(rename = "generationConfig")]
    generation_config: GeminiConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiTool>>,
}

#[derive(Serialize)]
struct GeminiContent<'a> {
    parts: Vec<GeminiPart<'a>>,
}

#[derive(Serialize)]
struct GeminiPart<'a> {
    text: &'a str,
}

#[derive(Serialize)]
struct GeminiConfig {
    temperature: f32,
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: u32,
    #[serde(rename = "responseMimeType", skip_serializing_if = "Option::is_none")]
    response_mime_type: Option<String>,
    /// A JSON schema Gemini must conform the output to. Only honored when no
    /// tools are attached (grounding and schema enforcement are mutually
    /// exclusive), which is exactly why the schema path is ungrounded.
    #[serde(rename = "responseSchema", skip_serializing_if = "Option::is_none")]
    response_schema: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct GeminiTool {
    #[serde(rename = "googleSearch")]
    google_search: GeminiGoogleSearch,
}

#[derive(Serialize)]
struct GeminiGoogleSearch {}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiResponseContent>,
}

#[derive(Deserialize)]
struct GeminiResponseContent {
    parts: Option<Vec<GeminiResponsePart>>,
}

#[derive(Deserialize)]
struct GeminiResponsePart {
    text: Option<String>,
}

/// How much of an unusable Gemini body to log. Enough to carry `finishReason`,
/// `promptFeedback`, and the safety ratings; short enough that a response
/// padded with grounding metadata can't flood the log.
const RAW_RESPONSE_LOG_LIMIT: usize = 4096;

/// Pull the reply text out of a Gemini response body, logging the raw body
/// whenever there isn't one.
///
/// By the time a `None` reaches a caller it is indistinguishable from "AI is
/// switched off", so an API-side refusal arrives as silence: the news pipeline
/// reported `AI failed to return extraction` from eight frames away, naming no
/// cause. The body holds the answer (`finishReason`, `promptFeedback`) and was
/// previously parsed and dropped. This is the only place that can still see it.
fn first_text(call: &str, body_text: &str) -> Result<Option<String>> {
    let body: GeminiResponse = serde_json::from_str(body_text)?;
    if let Some(candidates) = body.candidates
        && let Some(first) = candidates.into_iter().next()
        && let Some(content) = first.content
        && let Some(parts) = content.parts
        && let Some(part) = parts.into_iter().next()
        && let Some(text) = part.text
    {
        return Ok(Some(text));
    }

    tracing::warn!(
        call = %call,
        model = %AI_MODEL,
        raw_response = %body_text.chars().take(RAW_RESPONSE_LOG_LIMIT).collect::<String>(),
        "gemini returned no usable text"
    );
    Ok(None)
}

/// Slice the JSON object out of a reply. Grounded calls can't use JSON
/// response mode (see `generate_json_with_search`), and asked via the prompt
/// alone the model fences its JSON, prefixes prose, or appends grounding
/// notes. Taking the first `{` through the last `}` survives all of those;
/// bare JSON passes through untouched. A reply with no object comes back
/// trimmed and fails at the caller's parse, which callers must tolerate.
fn extract_json_object(text: &str) -> &str {
    let trimmed = text.trim();
    match (trimmed.find('{'), trimmed.rfind('}')) {
        (Some(start), Some(end)) if start < end => &trimmed[start..=end],
        _ => trimmed,
    }
}

impl AiService {
    pub fn new(enabled: bool, api_key: Option<String>) -> Self {
        // Every caller funnels through this one client, and several hold a
        // scarce resource across the call (the translation API gate, spawned
        // summary tasks). reqwest has no default timeout, so a hung Gemini
        // call would pin those forever; 120s is far past any legitimate
        // generation.
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("reqwest client construction cannot fail with these options");
        Self {
            client,
            api_key,
            enabled,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled && self.api_key.is_some()
    }

    pub fn model(&self) -> &str {
        AI_MODEL
    }

    pub async fn generate_reply(
        &self,
        system_prompt: &str,
        history: &str,
    ) -> Result<Option<String>> {
        // The default reply is grounded with Google Search and allowed a large
        // output; correct for news/mentions that may need to look things up.
        self.generate(system_prompt, history, true, 8192).await
    }

    /// A cheap reply for short in-character lines (a tavern welcome, a
    /// one-liner): no Google Search grounding, so it skips the ~8-15s a
    /// grounded lookup costs. The output cap is generous rather than tight —
    /// on a thinking model the reasoning tokens count against `maxOutputTokens`
    /// too, and a cap sized only for the visible reply (e.g. 256) gets consumed
    /// mid-thought and hands back a sentence sheared off partway. The line
    /// itself stays short (the caller sanitizes it down to a couple of lines);
    /// the headroom just keeps the model from running out of budget before it
    /// starts writing.
    pub async fn generate_short_reply(
        &self,
        system_prompt: &str,
        history: &str,
    ) -> Result<Option<String>> {
        self.generate(system_prompt, history, false, 2048).await
    }

    /// An ungrounded reply with a full-size output budget: no Google Search
    /// (the answer is entirely in the prompt), but room for a multi-paragraph
    /// result plus a thinking model's reasoning tokens, which count against
    /// `maxOutputTokens` too. Used by the chat catch-up summarizer, whose
    /// input is large (a room's unread backlog) and whose output is prose.
    pub async fn generate_ungrounded(
        &self,
        system_prompt: &str,
        prompt: &str,
    ) -> Result<Option<String>> {
        self.generate(system_prompt, prompt, false, 8192).await
    }

    async fn generate(
        &self,
        system_prompt: &str,
        history: &str,
        grounded: bool,
        max_output_tokens: u32,
    ) -> Result<Option<String>> {
        if !self.is_enabled() {
            return Ok(None);
        }

        let api_key = self.api_key.as_ref().context("missing api key")?;
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            AI_MODEL, api_key
        );

        let req = GeminiRequest {
            system_instruction: Some(GeminiContent {
                parts: vec![GeminiPart {
                    text: system_prompt,
                }],
            }),
            contents: vec![GeminiContent {
                parts: vec![GeminiPart { text: history }],
            }],
            generation_config: GeminiConfig {
                temperature: 0.8,
                max_output_tokens,
                response_mime_type: None,
                response_schema: None,
            },
            tools: grounded.then(|| {
                vec![GeminiTool {
                    google_search: GeminiGoogleSearch {},
                }]
            }),
        };

        let res = self.client.post(&url).json(&req).send_traced().await?;
        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await.unwrap_or_default();
            anyhow::bail!("Gemini API error: {} - {}", status, text);
        }

        let body_text = res.text().await?;
        tracing::debug!(
            raw_response_len = body_text.len(),
            "received Gemini API response"
        );
        first_text("generate", &body_text)
    }

    /// A grounded (Google Search) call whose reply is expected to be JSON.
    /// Grounding and JSON response mode don't mix on gemini-3.6-flash:
    /// attaching the `googleSearch` tool together with
    /// `responseMimeType: application/json` gets a 200 whose body has no
    /// `candidates` at all (the model thinks, then emits nothing). So this
    /// path requests JSON purely through the prompt and slices the object out
    /// of the fence and prose the model wraps it in despite being told not
    /// to. The shape is still prompt-enforced only; callers must tolerate a
    /// parse failure.
    pub async fn generate_json_with_search(
        &self,
        system_prompt: &str,
        prompt: &str,
    ) -> Result<Option<String>> {
        if !self.is_enabled() {
            return Ok(None);
        }

        let api_key = self.api_key.as_ref().context("missing api key")?;
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            AI_MODEL, api_key
        );

        let req = GeminiRequest {
            system_instruction: Some(GeminiContent {
                parts: vec![GeminiPart {
                    text: system_prompt,
                }],
            }),
            contents: vec![GeminiContent {
                parts: vec![GeminiPart { text: prompt }],
            }],
            generation_config: GeminiConfig {
                temperature: 0.8,
                max_output_tokens: 8192,
                response_mime_type: None,
                response_schema: None,
            },
            tools: Some(vec![GeminiTool {
                google_search: GeminiGoogleSearch {},
            }]),
        };

        let res = self.client.post(&url).json(&req).send_traced().await?;
        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await.unwrap_or_default();
            anyhow::bail!("Gemini API error: {} - {}", status, text);
        }

        let body_text = res.text().await?;
        tracing::debug!(raw_response = %body_text, "Full Gemini API response");
        match first_text("generate_json_with_search", &body_text)? {
            Some(text) => Ok(Some(extract_json_object(&text).to_string())),
            None => Ok(None),
        }
    }

    /// A JSON reply Gemini must conform to `schema`, ungrounded (no Google
    /// Search). Because no tool is attached, the schema is hard-enforced, so
    /// the output is always well-formed JSON matching the shape — no fences, no
    /// stray quotes, nothing to repair. Use for structured bot decisions that
    /// answer from their own prompt rather than the live web. The cap is
    /// generous so a thinking model's reasoning tokens don't crowd out the
    /// (small) JSON payload.
    ///
    /// `model` is explicit rather than defaulting to `AI_MODEL`: callers on
    /// this path (e.g. the bartender's order flow) may need a different model
    /// tier than @bot's chat/news model.
    pub async fn generate_json(
        &self,
        model: &str,
        system_prompt: &str,
        prompt: &str,
        schema: serde_json::Value,
    ) -> Result<Option<String>> {
        if !self.is_enabled() {
            return Ok(None);
        }

        let api_key = self.api_key.as_ref().context("missing api key")?;
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            model, api_key
        );

        let req = GeminiRequest {
            system_instruction: Some(GeminiContent {
                parts: vec![GeminiPart {
                    text: system_prompt,
                }],
            }),
            contents: vec![GeminiContent {
                parts: vec![GeminiPart { text: prompt }],
            }],
            generation_config: GeminiConfig {
                temperature: 0.8,
                max_output_tokens: 4096,
                response_mime_type: Some("application/json".to_string()),
                response_schema: Some(schema),
            },
            tools: None,
        };

        let res = self.client.post(&url).json(&req).send_traced().await?;
        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await.unwrap_or_default();
            anyhow::bail!("Gemini API error: {} - {}", status, text);
        }

        let body_text = res.text().await?;
        tracing::debug!(raw_response = %body_text, "Full Gemini API response");
        first_text("generate_json", &body_text)
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
