use anyhow::{Context, Result};
use late_core::telemetry::TracedExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct AiService {
    client: Client,
    api_key: Option<String>,
    model: String,
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

impl AiService {
    pub fn new(enabled: bool, api_key: Option<String>, model: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model,
            enabled,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled && self.api_key.is_some()
    }

    pub fn model(&self) -> &str {
        &self.model
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
            self.model, api_key
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
        let body: GeminiResponse = serde_json::from_str(&body_text)?;
        if let Some(candidates) = body.candidates
            && let Some(first) = candidates.into_iter().next()
            && let Some(content) = first.content
            && let Some(parts) = content.parts
            && let Some(part) = parts.into_iter().next()
        {
            return Ok(part.text);
        }

        Ok(None)
    }

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
            self.model, api_key
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
                response_mime_type: Some("application/json".to_string()),
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
        let body: GeminiResponse = serde_json::from_str(&body_text)?;
        if let Some(candidates) = body.candidates
            && let Some(first) = candidates.into_iter().next()
            && let Some(content) = first.content
            && let Some(parts) = content.parts
            && let Some(part) = parts.into_iter().next()
        {
            return Ok(part.text);
        }

        Ok(None)
    }

    /// A JSON reply Gemini must conform to `schema`, ungrounded (no Google
    /// Search). Because no tool is attached, the schema is hard-enforced, so
    /// the output is always well-formed JSON matching the shape — no fences, no
    /// stray quotes, nothing to repair. Use for structured bot decisions that
    /// answer from their own prompt rather than the live web. The cap is
    /// generous so a thinking model's reasoning tokens don't crowd out the
    /// (small) JSON payload.
    ///
    /// `model` is explicit rather than defaulting to `self.model`: callers on
    /// this path (e.g. the bartender's order flow) may need a different model
    /// tier than the configured chat/news model.
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
        let body: GeminiResponse = serde_json::from_str(&body_text)?;
        if let Some(candidates) = body.candidates
            && let Some(first) = candidates.into_iter().next()
            && let Some(content) = first.content
            && let Some(parts) = content.parts
            && let Some(part) = parts.into_iter().next()
        {
            return Ok(part.text);
        }

        Ok(None)
    }
}
