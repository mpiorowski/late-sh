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

    pub async fn generate_reply(
        &self,
        system_prompt: &str,
        history: &str,
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
                max_output_tokens: 8192,
                response_mime_type: None,
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
}
